mod api;
mod app;
mod event;
mod ui;

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

#[tokio::main]
async fn main() -> Result<()> {
    // Read config from env / args
    let api_url =
        std::env::var("HERMES_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8642".into());
    let api_key = std::env::var("HERMES_API_KEY").unwrap_or_default();
    let model = std::env::var("HERMES_MODEL").unwrap_or_else(|_| "hermes-agent".into());

    // Check for a session to resume from CLI arg
    let resume_session = std::env::args().nth(1);

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // App + event loop
    let client = api::HermesClient::new(&api_url, &api_key, &model);
    let mut app = App::new(client);
    let mut events = EventLoop::new(250);
    app.event_tx = Some(events.sender());

    // Fetch server status (model name, etc.)
    app.fetch_status().await;

    // Resume session if requested
    if let Some(session_prefix) = resume_session {
        match app.client.get_session_messages(&session_prefix).await {
            Ok((resolved_id, api_messages)) => {
                app.messages.clear();
                app.session_id = resolved_id.clone();
                let count = api_messages.len();
                for m in api_messages {
                    let role = match m.role.as_str() {
                        "user" => app::Role::User,
                        "assistant" => app::Role::Assistant,
                        _ => app::Role::System,
                    };
                    app.messages.push(app::ChatMessage {
                        role,
                        content: m.content,
                        tokens: None,
                    });
                }
                app.messages.push(app::ChatMessage {
                    role: app::Role::System,
                    content: format!("✓ Resumed session ({} messages loaded)", count),
                    tokens: None,
                });
            }
            Err(e) => {
                app.messages.push(app::ChatMessage {
                    role: app::Role::System,
                    content: format!("Failed to resume session: {}", e),
                    tokens: None,
                });
            }
        }
    }

    let result = run(&mut terminal, &mut app, &mut events).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    result
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    events: &mut EventLoop,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        match events.next().await? {
            event::AppEvent::Key(key) => {
                app.handle_key(key).await?;
            }
            event::AppEvent::Tick => {
                app.tick();
            }
            event::AppEvent::MouseScroll(delta) => {
                app.handle_scroll(delta);
            }
            event::AppEvent::StreamDelta(delta) => {
                app.handle_stream_delta(&delta);
            }
            event::AppEvent::StreamDone(usage) => {
                app.handle_stream_done(usage);
            }
            event::AppEvent::StreamError(err) => {
                app.handle_stream_error(&err);
            }
            event::AppEvent::Resize(_, _) => {}
        }

        if app.should_quit() {
            return Ok(());
        }
    }
}
