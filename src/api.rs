use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::event::{AppEvent, Usage};

/// Minimal OpenAI-compatible API client for Hermes.
#[derive(Clone)]
pub struct HermesClient {
    base_url: String,
    api_key: String,
    model: String,
    http: Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Serialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// A single SSE chunk from streaming chat completions.
#[derive(Deserialize, Debug)]
struct StreamChunk {
    choices: Option<Vec<StreamChoice>>,
    usage: Option<StreamUsage>,
}

#[derive(Deserialize, Debug)]
struct StreamChoice {
    delta: Option<Delta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Delta {
    content: Option<String>,
}

#[derive(Deserialize, Debug)]
struct StreamUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

/// Session summary from /v1/sessions.
#[derive(Deserialize, Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub source: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub started_at: Option<f64>,
    pub last_active: Option<f64>,
    pub message_count: Option<u32>,
    pub preview: Option<String>,
}

/// Session message from /v1/sessions/{id}/messages.
#[derive(Deserialize, Debug, Clone)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
}

/// Server status from /v1/status.
#[derive(Deserialize, Debug, Clone)]
pub struct ServerStatus {
    pub model: String,
    pub model_display: String,
}

impl HermesClient {
    pub fn new(base_url: &str, api_key: &str, model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            http: Client::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    pub fn api_key(&self) -> &str {
        &self.api_key
    }
    pub fn model(&self) -> &str {
        &self.model
    }

    fn auth_header(&self) -> Option<String> {
        if self.api_key.is_empty() {
            None
        } else {
            Some(format!("Bearer {}", self.api_key))
        }
    }

    /// Send a chat completions request with SSE streaming.
    /// Deltas and completion events are pushed to `event_tx`.
    pub async fn stream_chat(
        &self,
        messages: Vec<Message>,
        session_id: Option<&str>,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
        };

        let mut req = self
            .http
            .post(&url)
            .json(&body);

        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        if let Some(sid) = session_id {
            req = req.header("X-Hermes-Session-Id", sid);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            let _ = event_tx.send(AppEvent::StreamError(format!(
                "API error {}: {}",
                status, body_text
            )));
            return Ok(());
        }

        // Read the SSE stream line by line
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk).replace('\0', ""));

            // Process complete lines
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.starts_with("data: [DONE]") {
                    let _ = event_tx.send(AppEvent::StreamDone(None));
                    return Ok(());
                }

                if let Some(json_str) = line.strip_prefix("data: ") {
                    if let Ok(chunk) = serde_json::from_str::<StreamChunk>(json_str) {
                        if let Some(choices) = &chunk.choices {
                            for choice in choices {
                                if let Some(delta) = &choice.delta {
                                    if let Some(content) = &delta.content {
                                        if !content.is_empty() {
                                            let _ = event_tx
                                                .send(AppEvent::StreamDelta(content.clone()));
                                        }
                                    }
                                }
                                if choice.finish_reason.is_some() {
                                    let usage = chunk.usage.as_ref().map(|u| Usage {
                                        input_tokens: u.prompt_tokens.unwrap_or(0),
                                        output_tokens: u.completion_tokens.unwrap_or(0),
                                    });
                                    let _ = event_tx.send(AppEvent::StreamDone(usage));
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Stream ended without [DONE]
        let _ = event_tx.send(AppEvent::StreamDone(None));
        Ok(())
    }

    /// List recent sessions from the server.
    pub async fn list_sessions(&self, limit: u32) -> Result<Vec<SessionInfo>> {
        let url = format!("{}/v1/sessions?limit={}", self.base_url, limit);
        let mut req = self.http.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to list sessions: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let data = body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let sessions: Vec<SessionInfo> = data
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        Ok(sessions)
    }

    /// Get messages for a session (supports prefix matching).
    pub async fn get_session_messages(
        &self,
        session_id: &str,
    ) -> Result<(String, Vec<SessionMessage>)> {
        let url = format!("{}/v1/sessions/{}/messages", self.base_url, session_id);
        let mut req = self.http.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get session messages ({}): {}", status, text);
        }
        let body: serde_json::Value = resp.json().await?;
        let resolved_id = body
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or(session_id)
            .to_string();
        let data = body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let messages: Vec<SessionMessage> = data
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        Ok((resolved_id, messages))
    }

    /// Get server status (model name, etc.).
    pub async fn get_status(&self) -> Result<ServerStatus> {
        let url = format!("{}/v1/status", self.base_url);
        let mut req = self.http.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to get status: {}", resp.status());
        }
        let status: ServerStatus = resp.json().await?;
        Ok(status)
    }

    /// Approve a pending dangerous command.
    pub async fn approve(&self, session_id: &str) -> Result<()> {
        let url = format!("{}/v1/approve", self.base_url);
        let body = serde_json::json!({ "session_id": session_id });
        let mut req = self.http.post(&url).json(&body);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        req.send().await?;
        Ok(())
    }

    /// Deny a pending dangerous command.
    pub async fn deny(&self, session_id: &str) -> Result<()> {
        let url = format!("{}/v1/deny", self.base_url);
        let body = serde_json::json!({ "session_id": session_id });
        let mut req = self.http.post(&url).json(&body);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        req.send().await?;
        Ok(())
    }
}
