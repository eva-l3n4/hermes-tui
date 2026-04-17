mod acp;
mod app;
mod event;
mod ui;
mod ui_modal;
mod ui_picker;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use event::EventLoop;
use ratatui::prelude::*;
use std::io;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse args
    let profile = std::env::var("HERMES_PROFILE").ok();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Event loop + ACP client
    let mut events = EventLoop::new(250);
    let event_tx = events.sender();

    let acp = Arc::new(acp::AcpClient::spawn(event_tx.clone(), profile.as_deref()).await?);

    // Create app immediately — show picker with "Connecting..." while ACP initializes
    let mut app = App::new(vec![]);
    app.event_tx = Some(event_tx.clone());

    // Initialize ACP + fetch sessions in background
    let acp_init = Arc::clone(&acp);
    let event_tx_init = event_tx.clone();
    tokio::spawn(async move {
        // Initialize handshake
        match acp_init.initialize().await {
            Ok(init) => {
                if let Some(model) = init
                    .get("agentInfo")
                    .or_else(|| init.get("agent_info"))
                    .and_then(|s| s.get("name"))
                    .and_then(|m| m.as_str())
                {
                    let _ = event_tx_init.send(event::AppEvent::SlashCommandResponse(
                        format!("__model_name:{}", model),
                    ));
                }
            }
            Err(e) => {
                let _ = event_tx_init.send(event::AppEvent::AcpError(
                    format!("ACP initialize failed: {}", e),
                ));
            }
        }

        // Fetch sessions for the picker
        match acp_init.list_sessions().await {
            Ok(sessions) => {
                let _ = event_tx_init.send(event::AppEvent::SessionsLoaded(sessions));
            }
            Err(e) => {
                let _ = event_tx_init.send(event::AppEvent::AcpError(
                    format!("Failed to list sessions: {}", e),
                ));
            }
        }

        let _ = event_tx_init.send(event::AppEvent::AcpReady);
    });

    let result = run(&mut terminal, &mut app, &mut events, acp.clone(), &cwd).await;

    // Cleanup
    acp.shutdown().await;
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    events: &mut EventLoop,
    acp: Arc<acp::AcpClient>,
    cwd: &str,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        match events.next().await? {
            event::AppEvent::Key(key) => {
                app.handle_key(key, &acp, cwd).await?;
            }
            event::AppEvent::Tick => {
                app.tick();
            }
            event::AppEvent::MouseScroll(delta) => {
                app.handle_scroll(delta);
            }
            event::AppEvent::Resize(_, _) => {}

            // ACP events
            event::AppEvent::AgentMessage(text) => {
                app.handle_agent_message(&text);
            }
            event::AppEvent::AgentThought(text) => {
                app.handle_agent_thought(&text);
            }
            event::AppEvent::ToolCallStart { id, name, kind } => {
                app.handle_tool_start(&id, &name, kind.as_deref());
            }
            event::AppEvent::ToolCallUpdate {
                id,
                status,
                content,
            } => {
                app.handle_tool_update(&id, &status, content.as_deref());
            }
            event::AppEvent::PromptDone { stop_reason, usage } => {
                app.handle_prompt_done(&stop_reason, usage);
            }
            event::AppEvent::ApprovalRequest {
                request_id,
                command,
                options,
            } => {
                app.show_approval_modal(request_id, command, options);
            }
            event::AppEvent::AcpError(err) => {
                app.sys_msg(format!("ACP error: {}", err));
                app.status = app::AgentStatus::Idle;
            }
            event::AppEvent::SessionCreated(sid) => {
                app.session_id = Some(sid);
                app.status = app::AgentStatus::Idle;
                app.sys_msg("Session ready.");
            }
            event::AppEvent::SessionResumed(sid) => {
                app.session_id = Some(sid);
                app.status = app::AgentStatus::Idle;
                app.sys_msg("Session resumed.");
            }
            event::AppEvent::SessionsLoaded(sessions) => {
                app.sessions = sessions;
            }
            event::AppEvent::AcpReady => {
                // ACP is ready — picker can now accept Enter
            }
            event::AppEvent::SlashCommandResponse(text) => {
                // Hack: model name arrives via this channel from init
                if let Some(model) = text.strip_prefix("__model_name:") {
                    app.model_name = model.to_string();
                } else {
                    app.sys_msg(text);
                }
            }
        }

        if app.should_quit() {
            return Ok(());
        }
    }
}
