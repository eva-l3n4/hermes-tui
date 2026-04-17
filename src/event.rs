use anyhow::Result;
use crossterm::event::{Event, KeyEvent, MouseEventKind};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

/// Events the UI loop cares about.
#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    /// Mouse scroll delta: positive = up, negative = down.
    MouseScroll(i16),
    StreamDelta(String),
    StreamDone(Option<Usage>),
    StreamError(String),
    Resize(u16, u16),
}

#[derive(Debug, Clone)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub struct EventLoop {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    _tx: mpsc::UnboundedSender<AppEvent>,
}

impl EventLoop {
    pub fn new(tick_ms: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();

        // Async EventStream — works with EnableMouseCapture for mouse scroll
        tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            loop {
                let tick_delay = tokio::time::sleep(Duration::from_millis(tick_ms));
                tokio::select! {
                    maybe_event = reader.next() => {
                        match maybe_event {
                            Some(Ok(Event::Key(key))) => {
                                if event_tx.send(AppEvent::Key(key)).is_err() {
                                    break;
                                }
                            }
                            Some(Ok(Event::Mouse(mouse))) => {
                                let evt = match mouse.kind {
                                    MouseEventKind::ScrollUp => Some(AppEvent::MouseScroll(3)),
                                    MouseEventKind::ScrollDown => Some(AppEvent::MouseScroll(-3)),
                                    _ => None,
                                };
                                if let Some(e) = evt {
                                    if event_tx.send(e).is_err() {
                                        break;
                                    }
                                }
                            }
                            Some(Ok(Event::Resize(w, h))) => {
                                let _ = event_tx.send(AppEvent::Resize(w, h));
                            }
                            Some(Ok(_)) => {} // Focus, paste — ignore
                            Some(Err(_)) => break,
                            None => break,
                        }
                    }
                    _ = tick_delay => {
                        if event_tx.send(AppEvent::Tick).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Self { rx, _tx: tx }
    }

    pub async fn next(&mut self) -> Result<AppEvent> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("event channel closed"))
    }

    /// Get a sender handle for injecting stream events from the API task.
    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self._tx.clone()
    }
}
