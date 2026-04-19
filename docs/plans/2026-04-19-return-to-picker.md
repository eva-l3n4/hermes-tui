# Return-to-Picker Implementation Plan

> **For Hermes:** Execute task-by-task. Run tests after each. Commit after each.

**Goal:** Let the user return to the session picker from within a chat session, pick another session (or create a new one), without restarting Kaishi.

**Architecture:**
- App already has a `Screen::Picker` state and a fully-working picker flow used on startup. Re-enter it by flipping the `screen` field + clearing per-session chat state, then re-fetching the session list via existing `acp.list_sessions()` → `AppEvent::SessionsLoaded` flow.
- One-way transition for v1: dropping current chat state, no warm resume. Simpler, and selecting the same session in the picker resumes it via existing code.
- Keybind: `Ctrl+B` (for "back") when in chat with empty input. Rationale: `Ctrl+B` is currently unused; empty-input Esc is already taken (approvals, modal dismiss); Ctrl+B is single-step and discoverable via status bar hint + /help.
- Also expose via slash command `/sessions` and palette entry.

**Tech Stack:** Rust, ratatui, tokio, existing ACP client.

**Out of scope (next plans):** picker delete/filter, Ctrl+F conversation search.

---

## Task 1: Add `return_to_picker` method on App (state reset logic)

**Objective:** Pure state-reset method. Testable in isolation.

**Files:**
- Modify: `src/app.rs` — add method on `impl App` near the other
  session-management methods (around line 1330, near `/new` handler).

**Step 1: Write failing test**

Add to bottom of `src/app.rs` (no existing test module — create one):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn chat_app_fixture() -> App {
        let mut app = App::new(vec![]);
        app.screen = Screen::Chat;
        app.session_id = Some("sess-abc".into());
        app.session_title = Some("Old title".into());
        app.messages.push(ChatMessage {
            role: Role::User,
            content: "hello".into(),
            tokens: None,
        });
        app.pending_response = "streaming…".into();
        app.pending_thought = "thinking…".into();
        app.scroll_offset = 42;
        app.total_input_tokens = 100;
        app.total_output_tokens = 50;
        app.prompt_count = 3;
        app.context_used = 1234;
        app.undo_checkpoints.push(1);
        app
    }

    #[test]
    fn return_to_picker_switches_screen() {
        let mut app = chat_app_fixture();
        app.return_to_picker();
        assert_eq!(app.screen, Screen::Picker);
    }

    #[test]
    fn return_to_picker_clears_session_state() {
        let mut app = chat_app_fixture();
        app.return_to_picker();
        assert!(app.session_id.is_none());
        assert!(app.session_title.is_none());
        assert!(app.messages.is_empty());
        assert_eq!(app.pending_response, "");
        assert_eq!(app.pending_thought, "");
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn return_to_picker_resets_counters() {
        let mut app = chat_app_fixture();
        app.return_to_picker();
        assert_eq!(app.total_input_tokens, 0);
        assert_eq!(app.total_output_tokens, 0);
        assert_eq!(app.prompt_count, 0);
        assert_eq!(app.context_used, 0);
        assert!(app.undo_checkpoints.is_empty());
    }

    #[test]
    fn return_to_picker_resets_picker_selection() {
        let mut app = chat_app_fixture();
        app.picker_selected = 5;
        app.picker_scroll_offset = 100;
        app.return_to_picker();
        assert_eq!(app.picker_selected, 0);
        assert_eq!(app.picker_scroll_offset, 0);
    }
}
```

**Step 2: Run tests to verify failure**

```bash
cd /home/opus/hermes-tui && cargo test return_to_picker 2>&1 | tail -20
```

Expected: FAIL — `no method named 'return_to_picker' found`.

**Step 3: Write minimal implementation**

Add to `impl App` in `src/app.rs`, placed near the `/new` handler (roughly line 1345, just after that match arm's closing brace — inside `impl App` but outside `handle_local_command`):

```rust
/// Exit the current chat session and return to the picker.
/// Clears per-session state; keeps app-level settings (effort, yolo, verbose, input history).
/// Caller is responsible for triggering a session list refresh via
/// `AppEvent::SessionsLoaded` if desired.
pub fn return_to_picker(&mut self) {
    self.screen = Screen::Picker;
    self.modal = ModalState::None;

    // Clear per-session chat state
    self.session_id = None;
    self.session_title = None;
    self.messages.clear();
    self.line_cache.clear();
    self.pending_response.clear();
    self.pending_thought.clear();
    self.scroll_offset = 0;
    self.status = AgentStatus::Idle;
    self.active_tools.clear();
    self.tool_msg_map.clear();

    // Reset per-session counters
    self.total_input_tokens = 0;
    self.total_output_tokens = 0;
    self.prompt_count = 0;
    self.context_used = 0;
    self.undo_checkpoints.clear();

    // Reset picker scroll to top
    self.picker_selected = 0;
    self.picker_scroll_offset = 0;

    // Clear history pagination state
    self.history_total = 0;
    self.history_loaded = 0;
    self.loading_more_history = false;
}
```

**Step 4: Run tests to verify pass**

```bash
cd /home/opus/hermes-tui && cargo test return_to_picker 2>&1 | tail -20
```

Expected: 4 tests passed.

**Step 5: Commit**

```bash
cd /home/opus/hermes-tui
git add src/app.rs
git commit -m "feat(app): add return_to_picker state reset method

Pure state transition from Chat → Picker. Clears per-session
state (messages, tokens, session_id) while preserving app-level
settings (effort, yolo, input history). Caller triggers a
session list refresh separately."
```

---

## Task 2: Wire Ctrl+B keybind in chat mode

**Objective:** `Ctrl+B` with empty input in chat triggers return-to-picker and refreshes session list.

**Files:**
- Modify: `src/app.rs` — add keybind handler in `handle_chat_key`
  match block (around line 622, near Ctrl+P).

**Step 1: Write failing test**

Add to the test module in `src/app.rs`:

```rust
#[test]
fn return_to_picker_is_idempotent() {
    // Safety: calling it twice should not panic
    let mut app = chat_app_fixture();
    app.return_to_picker();
    app.return_to_picker();
    assert_eq!(app.screen, Screen::Picker);
}
```

Run: `cargo test return_to_picker_is_idempotent` → should PASS immediately (not a new behavior, just a safety invariant).

No new failing test for this task — it's a keybind wire-up, verified manually.

**Step 2: Locate and add the keybind**

Find this block in `src/app.rs` around line 622:

```rust
            // Ctrl+P: command palette
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
```

Immediately BEFORE it, insert:

```rust
            // Ctrl+B: back to session picker (only when input is empty)
            (KeyModifiers::CONTROL, KeyCode::Char('b')) if self.input.is_empty() => {
                self.return_to_picker();
                // Refresh session list in background
                if let Some(tx) = &self.event_tx {
                    let tx = tx.clone();
                    let acp = acp.clone();
                    tokio::spawn(async move {
                        if let Ok(sessions) = acp.list_sessions().await {
                            let _ = tx.send(crate::event::AppEvent::SessionsLoaded(sessions));
                        }
                    });
                }
            }
```

**Step 3: Verify compile**

```bash
cd /home/opus/hermes-tui && cargo build 2>&1 | tail -10
```

Expected: clean build, no warnings.

**Step 4: Manual smoke test**

```bash
cd /home/opus/hermes-tui && cargo run --release
```

Steps to verify:
1. Pick an existing session from the picker, Enter.
2. Once in chat, press `Ctrl+B` with empty input.
3. Should see the session picker with the just-exited session visible and selection at top ("New Session" card highlighted).
4. Select the same session → should resume normally.
5. Press `Ctrl+B` with non-empty input → nothing happens (input preserved).

**Step 5: Commit**

```bash
cd /home/opus/hermes-tui
git add src/app.rs
git commit -m "feat(chat): Ctrl+B returns to session picker

Only triggers when input is empty (avoids interfering with
text editing). Refreshes session list in background so the
picker reflects any sessions created elsewhere while we were
chatting."
```

---

## Task 3: Add `/sessions` slash command and palette entry

**Objective:** Discoverability — users who don't know Ctrl+B can type `/sessions` or use Ctrl+P.

**Files:**
- Modify: `src/app.rs` — `SLASH_COMMANDS` constant, `handle_local_command`, `build_palette_entries`, `Keybind` action dispatch.

**Step 1: Add to slash commands list**

Find line 12-16 in `src/app.rs`:

```rust
const SLASH_COMMANDS: &[&str] = &[
    "/clear", "/compact", "/context", "/effort", "/exit", "/help", "/model",
    "/new", "/quit", "/reset", "/save", "/title", "/tools", "/usage",
    "/verbose", "/version", "/yolo",
];
```

Add `"/sessions"` alphabetically:

```rust
const SLASH_COMMANDS: &[&str] = &[
    "/clear", "/compact", "/context", "/effort", "/exit", "/help", "/model",
    "/new", "/quit", "/reset", "/save", "/sessions", "/title", "/tools",
    "/usage", "/verbose", "/version", "/yolo",
];
```

**Step 2: Handle `/sessions` in `handle_local_command`**

Find the `"/new" =>` arm (line 1330). Immediately after its closing brace (line 1345), add:

```rust
            "/sessions" | "/switch" => {
                self.return_to_picker();
                if let Some(tx) = &self.event_tx {
                    let tx = tx.clone();
                    let acp_cloned = acp.clone();
                    tokio::spawn(async move {
                        if let Ok(sessions) = acp_cloned.list_sessions().await {
                            let _ = tx.send(crate::event::AppEvent::SessionsLoaded(sessions));
                        }
                    });
                }
                true
            }
```

**Step 3: Add palette entry**

Find `build_palette_entries` (line 364). Add as the second entry (after "New session"):

```rust
            PaletteEntry { label: "Switch session".into(), keybind: Some("Ctrl+B".into()), action: PaletteAction::SlashCommand("/sessions".into()) },
```

**Step 4: Update /help text**

Find the `/help` handler (around line 1359). In the "Local commands" block, add after the `/new` line:

```
                     /sessions        Return to session picker\n\
```

And in the "Keys" block, add after `Esc Esc: Undo last turn`:

```
                     Ctrl+B:   Back to session picker\n\
```

**Step 5: Verify build**

```bash
cd /home/opus/hermes-tui && cargo build 2>&1 | tail -5
```

Expected: clean build.

**Step 6: Manual smoke test**

1. Launch Kaishi, enter a session.
2. Type `/sessions` + Enter → returns to picker.
3. Re-enter a session, press `Ctrl+P` → see "Switch session" entry with "Ctrl+B" hint.
4. Select it → returns to picker.
5. Type `/help` → verify both new lines present.
6. Type `/` then Tab → cycles through `/sessions`.

**Step 7: Commit**

```bash
cd /home/opus/hermes-tui
git add src/app.rs
git commit -m "feat(commands): add /sessions command and palette entry

Exposes return-to-picker via three surfaces: Ctrl+B keybind,
/sessions slash command (also /switch alias), and Ctrl+P
palette entry. Updates /help text."
```

---

## Task 4: Add status bar hint for Ctrl+B

**Objective:** Discoverability via ambient UI — the bottom-row hints should mention `Ctrl+B` when in chat.

**Files:**
- Modify: `src/ui.rs` — locate the status-bar hint row (likely in
  `draw_chat` or a helper).

**Step 1: Locate hint text**

```bash
cd /home/opus/hermes-tui && rg -n "Ctrl\+P|/help" src/ui.rs | head
```

Find where the existing hint strings are rendered (status bar or footer in chat screen).

**Step 2: Add Ctrl+B hint**

Where "Ctrl+P palette" is mentioned, add " · Ctrl+B sessions" (or fit the existing format). Keep concise — chat status bar has limited room.

If the hint is too crowded, rotate Ctrl+B in only when messages.len() > 1 (i.e., actively in a session worth leaving).

**Step 3: Verify build**

```bash
cd /home/opus/hermes-tui && cargo build 2>&1 | tail -5
```

**Step 4: Manual verification**

Launch, confirm status bar shows Ctrl+B hint somewhere.

**Step 5: Commit**

```bash
cd /home/opus/hermes-tui
git add src/ui.rs
git commit -m "feat(ui): status bar hint for Ctrl+B sessions shortcut"
```

---

## Task 5: Update roadmap and ship

**Files:**
- Modify: `docs/roadmap-0.6-0.7.md` — move return-to-picker from v0.9 wishlist to shipped.
- Modify: `Cargo.toml` — bump to `0.8.2`.

**Step 1: Update roadmap**

In `docs/roadmap-0.6-0.7.md`, add a new section after v0.8.0:

```markdown
## v0.8.2 — Navigation ✓

- ✓ **Return to session picker (Ctrl+B)** — `/sessions` command,
  palette entry, one-way state reset with background list refresh.
```

**Step 2: Bump version**

In `Cargo.toml`, change `version = "0.8.1"` → `version = "0.8.2"`.

**Step 3: Verify and tag**

```bash
cd /home/opus/hermes-tui
cargo build --release 2>&1 | tail -5
cargo test 2>&1 | tail -5
```

Expected: clean build, all tests pass.

**Step 4: Commit and tag**

```bash
cd /home/opus/hermes-tui
git add Cargo.toml Cargo.lock docs/roadmap-0.6-0.7.md
git commit -m "chore: bump to v0.8.2 — return-to-picker"
git tag v0.8.2
```

**Step 5: Push (Eva's call — mention before pushing)**

```bash
# Only if Eva OKs:
git push origin main
git push origin v0.8.2
```

---

## Verification Checklist

- [ ] `cargo test` passes (4 new tests + idempotency)
- [ ] `cargo build --release` clean, no warnings
- [ ] Ctrl+B with empty input in chat → picker
- [ ] Ctrl+B with non-empty input → no-op (input preserved)
- [ ] `/sessions` command → picker
- [ ] Ctrl+P → "Switch session" entry visible
- [ ] Session list refreshes on return (new sessions created elsewhere appear)
- [ ] Picker selection resets to top ("New Session")
- [ ] Re-entering same session resumes correctly
- [ ] `/help` mentions both `/sessions` and Ctrl+B
