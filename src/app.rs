use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::api::{HermesClient, Message};
use crate::event::{AppEvent, Usage};

/// Visible role tag for messages in the conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A single message in the conversation view.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub tokens: Option<Usage>,
}

/// What the assistant is currently doing.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Streaming,
    Error(String),
}

/// Application state.
pub struct App {
    pub client: HermesClient,

    /// Full conversation history (rendered in the scroll view).
    pub messages: Vec<ChatMessage>,

    /// Input buffer (what the user is typing).
    pub input: String,
    /// Cursor position within `input`.
    pub cursor: usize,

    /// Scroll offset for the message viewport (0 = bottom / latest).
    pub scroll_offset: u16,

    /// Current agent status.
    pub status: AgentStatus,

    /// Accumulator for the in-progress assistant response.
    pub pending_response: String,

    /// Session ID for continuity.
    pub session_id: String,

    /// Resolved model name from server.
    pub model_name: String,
    /// Display-friendly model alias.
    pub model_display: String,

    /// Session title (if known, e.g. after resume).
    pub session_title: Option<String>,

    /// Event sender — lets us inject stream events from API tasks.
    pub event_tx: Option<mpsc::UnboundedSender<AppEvent>>,

    /// Spinner tick counter (for animation).
    pub tick: u64,

    /// Mouse event counter (diagnostic — shown in status bar).
    pub mouse_events: u64,

    /// Exit flag.
    quit: bool,
}

impl App {
    pub fn new(client: HermesClient) -> Self {
        let model_display = client.model().to_string();
        Self {
            client,
            messages: vec![ChatMessage {
                role: Role::System,
                content: "Welcome to Hermes TUI. Type a message or /help for commands.".into(),
                tokens: None,
            }],
            input: String::new(),
            cursor: 0,
            scroll_offset: 0,
            status: AgentStatus::Idle,
            pending_response: String::new(),
            session_id: uuid::Uuid::new_v4().to_string(),
            model_name: String::new(),
            model_display,
            session_title: None,
            event_tx: None,
            mouse_events: 0,
            tick: 0,
            quit: false,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Fetch server status on startup.
    pub async fn fetch_status(&mut self) {
        match self.client.get_status().await {
            Ok(status) => {
                self.model_name = status.model;
                self.model_display = status.model_display;
            }
            Err(_) => {
                // Fallback: use what we configured
                self.model_name = self.client.model().to_string();
            }
        }
    }

    /// Push a system message into the chat.
    fn sys_msg(&mut self, msg: impl Into<String>) {
        self.messages.push(ChatMessage {
            role: Role::System,
            content: msg.into(),
            tokens: None,
        });
        self.scroll_offset = 0;
    }

    /// Build the messages array for the API request.
    fn build_api_messages(&self) -> Vec<Message> {
        self.messages
            .iter()
            .filter(|m| m.role == Role::User || m.role == Role::Assistant)
            .map(|m| Message {
                role: match m.role {
                    Role::User => "user".into(),
                    Role::Assistant => "assistant".into(),
                    _ => "system".into(),
                },
                content: m.content.clone(),
            })
            .collect()
    }

    /// Handle a slash command. Returns true if the input was consumed.
    async fn handle_command(&mut self, text: &str) -> bool {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match cmd.as_str() {
            "/quit" | "/exit" | "/q" => {
                self.quit = true;
                true
            }
            "/clear" => {
                self.messages.clear();
                self.scroll_offset = 0;
                true
            }
            "/new" => {
                self.messages.clear();
                self.session_id = uuid::Uuid::new_v4().to_string();
                self.session_title = None;
                self.scroll_offset = 0;
                self.sys_msg("New session started.");
                true
            }
            "/sessions" | "/session" | "/s" => {
                self.sys_msg("Loading sessions…");
                match self.client.list_sessions(15).await {
                    Ok(sessions) => {
                        if sessions.is_empty() {
                            self.sys_msg("No sessions found.");
                        } else {
                            let mut lines = vec!["Recent sessions:".to_string()];
                            for (i, s) in sessions.iter().enumerate() {
                                let title = s.title.as_deref().unwrap_or("(untitled)");
                                let preview = s.preview.as_deref().unwrap_or("");
                                let short_id = if s.id.len() > 12 {
                                    &s.id[..12]
                                } else {
                                    &s.id
                                };
                                let source = s.source.as_deref().unwrap_or("?");
                                let count = s.message_count.unwrap_or(0);
                                let display = if title != "(untitled)" {
                                    title.to_string()
                                } else if !preview.is_empty() {
                                    preview.to_string()
                                } else {
                                    "(empty)".to_string()
                                };
                                lines.push(format!(
                                    "  {:>2}. [{}] {} ({}, {} msgs)",
                                    i + 1,
                                    short_id,
                                    display,
                                    source,
                                    count
                                ));
                            }
                            lines.push(String::new());
                            lines.push("Use /resume <id-prefix> to load a session.".to_string());
                            // Remove the "Loading…" message
                            if let Some(last) = self.messages.last() {
                                if last.content.contains("Loading") {
                                    self.messages.pop();
                                }
                            }
                            self.sys_msg(lines.join("\n"));
                        }
                    }
                    Err(e) => {
                        self.sys_msg(format!("Failed to list sessions: {}", e));
                    }
                }
                true
            }
            "/resume" | "/r" => {
                if arg.is_empty() {
                    self.sys_msg("Usage: /resume <session-id-or-prefix>");
                    return true;
                }
                self.sys_msg(format!("Resuming session {}…", arg));
                match self.client.get_session_messages(arg).await {
                    Ok((resolved_id, api_messages)) => {
                        self.messages.clear();
                        self.session_id = resolved_id.clone();
                        self.session_title = None;

                        let msg_count = api_messages.len();
                        for m in api_messages {
                            let role = match m.role.as_str() {
                                "user" => Role::User,
                                "assistant" => Role::Assistant,
                                _ => Role::System,
                            };
                            self.messages.push(ChatMessage {
                                role,
                                content: m.content,
                                tokens: None,
                            });
                        }

                        let short = if resolved_id.len() > 16 {
                            &resolved_id[..16]
                        } else {
                            &resolved_id
                        };
                        self.sys_msg(format!(
                            "✓ Resumed session {} ({} messages loaded)",
                            short, msg_count
                        ));
                        self.scroll_offset = 0;
                    }
                    Err(e) => {
                        self.sys_msg(format!("Failed to resume: {}", e));
                    }
                }
                true
            }
            "/model" => {
                if self.model_name.is_empty() {
                    self.sys_msg(format!("Model: {}", self.model_display));
                } else {
                    self.sys_msg(format!(
                        "Model: {} (display: {})",
                        self.model_name, self.model_display
                    ));
                }
                true
            }
            "/approve" => {
                let _ = self.client.approve(&self.session_id).await;
                self.sys_msg("✓ Approval sent.");
                true
            }
            "/deny" => {
                let _ = self.client.deny(&self.session_id).await;
                self.sys_msg("✗ Denial sent.");
                true
            }
            "/help" | "/h" | "/?" => {
                self.sys_msg(
                    "Commands:\n\
                     \n\
                     /new             Start a new session\n\
                     /sessions        List recent sessions\n\
                     /resume <id>     Resume a session by ID or prefix\n\
                     /model           Show current model\n\
                     /approve         Approve pending dangerous command\n\
                     /deny            Deny pending dangerous command\n\
                     /clear           Clear the screen\n\
                     /quit            Exit\n\
                     \n\
                     Scroll: PgUp/PgDn, Ctrl+U (up 10), Ctrl+E (down)"
                        .to_string(),
                );
                true
            }
            _ if cmd.starts_with('/') => {
                self.sys_msg(format!("Unknown command: {}. Type /help for commands.", cmd));
                true
            }
            _ => false,
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match (key.modifiers, key.code) {
            // Quit
            (KeyModifiers::CONTROL, KeyCode::Char('c'))
            | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.quit = true;
            }

            // Submit message
            (_, KeyCode::Enter) if self.status == AgentStatus::Idle => {
                let text = self.input.trim().to_string();
                if text.is_empty() {
                    return Ok(());
                }

                self.input.clear();
                self.cursor = 0;

                // Try slash command first
                if self.handle_command(&text).await {
                    return Ok(());
                }

                // Add user message
                self.messages.push(ChatMessage {
                    role: Role::User,
                    content: text,
                    tokens: None,
                });
                self.scroll_offset = 0;

                // Start streaming
                self.status = AgentStatus::Streaming;
                self.pending_response.clear();

                let api_messages = self.build_api_messages();
                let session_id = self.session_id.clone();
                let event_tx = self
                    .event_tx
                    .as_ref()
                    .expect("event_tx must be set before handling keys")
                    .clone();

                let client = self.client.clone();

                tokio::spawn(async move {
                    if let Err(e) = client
                        .stream_chat(api_messages, Some(&session_id), event_tx.clone())
                        .await
                    {
                        let _ = event_tx.send(AppEvent::StreamError(e.to_string()));
                    }
                });
            }

            // Scroll
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
            }
            (_, KeyCode::PageUp) => {
                self.scroll_offset = self.scroll_offset.saturating_add(20);
            }
            (_, KeyCode::PageDown) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(20);
            }

            // Cursor / editing with modifiers (must come before generic Char)
            (KeyModifiers::CONTROL, KeyCode::Char('a')) | (_, KeyCode::Home) => {
                self.cursor = 0;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('e')) | (_, KeyCode::End) => {
                self.cursor = self.input.len();
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
                // Delete word backward
                let before = &self.input[..self.cursor];
                let trimmed = before.trim_end();
                let new_end = trimmed
                    .rfind(|c: char| c.is_whitespace())
                    .map(|i| i + 1)
                    .unwrap_or(0);
                self.input.replace_range(new_end..self.cursor, "");
                self.cursor = new_end;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
                // Kill to end of line
                self.input.truncate(self.cursor);
            }

            // Text input
            (_, KeyCode::Char(c)) => {
                self.input.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            (_, KeyCode::Backspace) => {
                if self.cursor > 0 {
                    let prev = self.input[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.input.replace_range(prev..self.cursor, "");
                    self.cursor = prev;
                }
            }
            (_, KeyCode::Delete) => {
                if self.cursor < self.input.len() {
                    let next = self.input[self.cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor + i)
                        .unwrap_or(self.input.len());
                    self.input.replace_range(self.cursor..next, "");
                }
            }
            (_, KeyCode::Left) => {
                if self.cursor > 0 {
                    self.cursor = self.input[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                }
            }
            (_, KeyCode::Right) => {
                if self.cursor < self.input.len() {
                    self.cursor = self.input[self.cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor + i)
                        .unwrap_or(self.input.len());
                }
            }

            _ => {}
        }

        Ok(())
    }

    /// Handle mouse scroll: positive = scroll up (older), negative = scroll down (newer).
    pub fn handle_scroll(&mut self, delta: i16) {
        self.mouse_events += 1;
        if delta > 0 {
            self.scroll_offset = self.scroll_offset.saturating_add(delta as u16);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as u16);
        }
    }

    pub fn handle_stream_delta(&mut self, delta: &str) {
        self.pending_response.push_str(delta);
        self.scroll_offset = 0;
    }

    pub fn handle_stream_done(&mut self, usage: Option<Usage>) {
        let content = std::mem::take(&mut self.pending_response);
        if !content.is_empty() {
            self.messages.push(ChatMessage {
                role: Role::Assistant,
                content,
                tokens: usage,
            });
        }
        self.status = AgentStatus::Idle;
        self.scroll_offset = 0;
    }

    pub fn handle_stream_error(&mut self, err: &str) {
        if !self.pending_response.is_empty() {
            let content = std::mem::take(&mut self.pending_response);
            self.messages.push(ChatMessage {
                role: Role::Assistant,
                content,
                tokens: None,
            });
        }
        self.sys_msg(format!("⚠ Error: {}", err));
        self.status = AgentStatus::Idle;
    }
}
