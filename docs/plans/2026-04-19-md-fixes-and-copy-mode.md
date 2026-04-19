# Markdown Fixes + Copy Mode Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Fix three markdown rendering bugs (underscore emphasis, pre-rendered table misdetection, bold/text hugging) and add an in-app copy mode so multi-line selection pulls clean source text from messages instead of terminal-selecting the rendered frame (which grabs UI chrome and nulls).

**Architecture:** Inline parser gets a second emphasis branch for `_underscore_` / `__underscore__`. Table detector is tightened to require a GFM separator row before it trusts a `|...|` block — which also naturally rejects server-pre-rendered ASCII box tables. Copy mode is a new `ModalState::CopyMode` overlay driven by `arboard`; navigation picks a message (or code block inside one) and yanks the **raw `content` string** — never the rendered spans.

**Tech Stack:** Rust, ratatui + crossterm, `arboard` (new dep for system clipboard), existing `ModalState` pattern.

---

## Context for the implementer

All file paths are relative to `/home/opus/hermes-tui`. Run commands from that dir.

Key files:
- `src/ui.rs` — markdown rendering (`render_markdown_lines`, `parse_inline_spans`, `flush_table`)
- `src/app.rs` — `ModalState`, `ChatMessage`, key handlers
- `src/main.rs` — event loop
- `src/` — all ui_*.rs overlays (pattern to copy for ui_copy_mode.rs)

Build: `cargo build --release` (Kaishi runs the release binary — dev builds won't be what you test against).
Run: `cargo run --release` from inside this repo, or Eva's shell alias.
Lint: `cargo clippy --release -- -D warnings` must pass clean.

Reference sample that reproduces the table bug (from user):

```
Task                    │ Before                         ║
│    │ After                                 │ Rationale      ║
│                                                             ║
│                              ────────────────────────┼──────║
│    **Primary**             │ azure/claude-opus-4-7          ║
```

This is a **pre-rendered** table (the server already drew box art with `│` `║` `─ ┼`), then line-wrapped at ~65 cols. Kaishi's detector sees the `│` as pipes and re-tables. Fix: require a proper GFM separator row before trusting pipe-table mode; also treat lines containing box-drawing `│ ║ ─ ┼ ├ ┤ ┬ ┴ ┌ ┐ └ ┘` as preformatted.

---

## Task 1: Add `arboard` dependency and smoke-test clipboard

**Objective:** Wire the cross-platform clipboard crate and verify it works on Eva's environment (Linux, likely Wayland).

**Files:**
- Modify: `Cargo.toml`

**Step 1:** Add dep

```toml
# Under [dependencies]
arboard = { version = "3", default-features = false, features = ["wayland-data-control"] }
```

`wayland-data-control` gives native Wayland support without pulling in image-format deps.

**Step 2:** Build

```bash
cargo build --release
```

Expected: builds clean.

**Step 3:** Quick smoke test

Add a temporary `src/bin/clip_smoke.rs`:

```rust
fn main() -> anyhow::Result<()> {
    let mut cb = arboard::Clipboard::new()?;
    cb.set_text("hello from kaishi")?;
    println!("set; readback = {:?}", cb.get_text()?);
    Ok(())
}
```

Run: `cargo run --release --bin clip_smoke`
Expected: `set; readback = "hello from kaishi"` and `wl-paste` / middle-click-paste outside the terminal returns the same.

**Step 4:** Delete the smoke file

```bash
rm src/bin/clip_smoke.rs
# Also remove `[[bin]]` section if cargo added one
```

**Step 5:** Commit

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add arboard for system clipboard"
```

---

## Task 2: Underscore emphasis — add `_italic_` and `__bold__` to inline parser

**Objective:** Parse underscore-delimited emphasis identically to asterisk form so messages with `_unchanged_` render italic.

**Files:**
- Modify: `src/ui.rs` — function `parse_inline_spans` (starts ~line 1161)

**Step 1:** Read the asterisk handling to model the underscore branches on.

```bash
# Already familiar: see src/ui.rs lines 1195-1253 for ** and * handling.
```

**Step 2:** After the `*` italic branch (around line 1253, before the link branch at `ch == '['`), add underscore handling. The key subtlety: underscores inside words (`foo_bar_baz`, snake_case variables) should NOT be treated as emphasis. CommonMark calls this "intraword underscore" — require a non-alphanumeric character (or string boundary) on both sides of the delimiter.

Add (immediately after the italic `*` block closes):

```rust
// Bold underscores: __...__
// Only when not intraword (CommonMark intraword underscore rule).
if ch == '_' && text[i..].starts_with("__") {
    let before_is_word = spans
        .last()
        .map(|s| s.content.chars().last().map_or(false, |c| c.is_alphanumeric()))
        .unwrap_or(false)
        || current.chars().last().map_or(false, |c| c.is_alphanumeric());
    if !before_is_word {
        if !current.is_empty() {
            spans.push(Span::raw(std::mem::take(&mut current)));
        }
        chars.next();
        chars.next();
        let mut bold_text = String::new();
        let mut closed = false;
        while let Some(&(j, c)) = chars.peek() {
            if c == '_' && text[j..].starts_with("__") {
                // Closer must not be followed by alphanumeric either.
                let after = text[j + 2..].chars().next();
                if !after.map_or(false, |c| c.is_alphanumeric()) {
                    chars.next();
                    chars.next();
                    closed = true;
                    break;
                }
            }
            chars.next();
            bold_text.push(c);
        }
        if closed {
            spans.push(Span::styled(
                bold_text,
                Style::default().add_modifier(Modifier::BOLD),
            ));
        } else {
            current.push_str("__");
            current.push_str(&bold_text);
        }
        continue;
    }
}

// Italic underscore: _..._
if ch == '_' {
    let before_is_word = spans
        .last()
        .map(|s| s.content.chars().last().map_or(false, |c| c.is_alphanumeric()))
        .unwrap_or(false)
        || current.chars().last().map_or(false, |c| c.is_alphanumeric());
    if !before_is_word {
        if !current.is_empty() {
            spans.push(Span::raw(std::mem::take(&mut current)));
        }
        chars.next();
        let mut italic_text = String::new();
        let mut closed = false;
        while let Some(&(j, c)) = chars.peek() {
            if c == '_' {
                let after = text[j + 1..].chars().next();
                if !after.map_or(false, |c| c.is_alphanumeric()) {
                    chars.next();
                    closed = true;
                    break;
                }
            }
            chars.next();
            italic_text.push(c);
        }
        if closed {
            spans.push(Span::styled(
                italic_text,
                Style::default().add_modifier(Modifier::ITALIC),
            ));
        } else {
            current.push('_');
            current.push_str(&italic_text);
        }
        continue;
    }
}
```

**Step 3:** Sanity-run

```bash
cargo build --release
cargo clippy --release -- -D warnings
```

Expected: clean.

**Step 4:** Manual verification

Run Kaishi, paste this message into a session (use shell escape `!echo ...` or just have the agent echo):

```
Plain _italic_ and __bold__ and a snake_case_identifier and file_name.rs.
Mixed: _this is italic_, but file_name should stay plain.
```

Expected render:
- `italic` in italics
- `bold` in bold
- `snake_case_identifier` and `file_name` stay plain (no partial italicizing)

**Step 5:** Commit

```bash
git add src/ui.rs
git commit -m "fix(md): support _underscore_ emphasis (italic/bold) with intraword rule"
```

---

## Task 3: Tighten table detection — require GFM separator row

**Objective:** Stop Kaishi from rendering non-tables (especially server-pre-rendered ASCII box-art tables) as tables. Real GFM tables always have a `|---|---|` separator on the second line. Require it.

**Files:**
- Modify: `src/ui.rs` — state around `table_rows` in `render_markdown_lines` (lines ~1116-1135) and `flush_table`

**Step 1:** Read current state

Current logic (ui.rs:1117-1132) pushes any `|...|` line into `table_rows` and renders them via `flush_table`, skipping separator rows. That means even a single `|foo|` stray line becomes a table.

**Step 2:** Rework the accumulator state

Change the accumulator to track both candidate rows AND whether a separator has been seen. Replace the current `table_rows: Vec<Vec<String>>` with:

```rust
// Near the top of render_markdown_lines, replacing table_rows:
let mut table_buffer: Vec<Vec<String>> = Vec::new();
let mut table_has_separator = false;
```

Helper function (add above `render_markdown_lines`):

```rust
/// True iff `line` looks like a GFM table separator row: `| --- | :--: |` etc.
/// Requires at least one `---` cell and all cells match the pattern.
fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return false;
    }
    let cells: Vec<&str> = trimmed.trim_matches('|').split('|').collect();
    if cells.is_empty() {
        return false;
    }
    cells.iter().all(|cell| {
        let c = cell.trim();
        !c.is_empty()
            && c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
            && c.contains('-')
    })
}

/// True iff `line` contains box-drawing chars that indicate pre-rendered output.
/// We never try to markdown-table these.
fn has_box_drawing(line: &str) -> bool {
    line.chars().any(|c| matches!(c,
        '│' | '║' | '┃' | '─' | '━' | '┼' | '╫' | '╪' | '╬'
        | '├' | '┤' | '┬' | '┴' | '┌' | '┐' | '└' | '┘'
        | '╭' | '╮' | '╯' | '╰'
    ))
}
```

**Step 3:** Replace the pipe-table branch (ui.rs ~1117-1132) with:

```rust
// Pipe-delimited tables: require separator row on line 2 before committing to table mode.
// If we see a "|...|" line but not yet confirmed, buffer it. If the NEXT line is a
// valid GFM separator, we're in a real table. Otherwise flush the buffer as paragraphs.
if trimmed.starts_with('|') && trimmed.ends_with('|') && !has_box_drawing(trimmed) {
    if is_table_separator(trimmed) {
        // Separator arrives only as line 2 of a real table.
        if table_buffer.len() == 1 {
            table_has_separator = true;
            // don't push separator as a row
            continue;
        }
        // Stray separator outside table context — render as paragraph
        flush_table_buffer(&mut table_buffer, &mut table_has_separator, lines, narrow);
        let mut spans = vec![Span::raw(indent(narrow).to_string())];
        spans.extend(parse_inline_spans(raw_line.trim_start()));
        lines.push(Line::from(spans));
        continue;
    }
    let cells: Vec<String> = trimmed
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect();
    table_buffer.push(cells);
    continue;
}

// Non-table line: flush buffer. If it had no separator, render rows as paragraphs.
flush_table_buffer(&mut table_buffer, &mut table_has_separator, lines, narrow);
```

**Step 4:** Rename `flush_table` to `flush_table_buffer` and teach it to render-as-paragraphs when no separator was seen:

```rust
fn flush_table_buffer<'a>(
    rows: &mut Vec<Vec<String>>,
    has_separator: &mut bool,
    lines: &mut Vec<Line<'a>>,
    narrow: bool,
) {
    if rows.is_empty() {
        *has_separator = false;
        return;
    }

    if !*has_separator {
        // Not a real table — render each row as a plain line with inline markdown.
        let ind = indent(narrow);
        for row in rows.iter() {
            // Reconstruct with " | " separators so the user sees their original text.
            let joined = format!("| {} |", row.join(" | "));
            let mut spans = vec![Span::raw(ind.to_string())];
            spans.extend(parse_inline_spans(&joined));
            lines.push(Line::from(spans));
        }
        rows.clear();
        return;
    }

    // Real GFM table — existing render path (unchanged below)
    // ... (keep all the column-width + padded-cell code)
    *has_separator = false;
    rows.clear();
}
```

Keep the existing render body for the `has_separator == true` branch verbatim, just move it under the guard.

**Step 5:** Update all three `flush_table(...)` call sites inside `render_markdown_lines` to the new signature (pass `&mut table_has_separator`). Also update the final flush at the end of the function.

**Step 6:** Build + clippy

```bash
cargo build --release
cargo clippy --release -- -D warnings
```

Expected: clean.

**Step 7:** Verify with fixtures

Write a tiny ad-hoc runner to render markdown text and assert no table appears where there shouldn't be one. Use the user's pasted sample. Since `render_markdown_lines` is private, expose it for test via `pub(crate) fn render_markdown_lines_for_test` or copy the logic into an integration test. Simpler: run Kaishi manually, have the agent echo the exact user sample, and visually confirm:

- The box-art cascade no longer produces the ` ───┼─── ` grid
- A **real** markdown table with a proper `| --- | --- |` separator still renders correctly

For real-table verification, paste this and ask the agent to echo:

```
| Task | Before | After |
| --- | --- | --- |
| **Primary** | azure/claude-opus-4-7 | _unchanged_ |
```

Expected: renders as a bordered Kaishi table with bold/italic cells.

**Step 8:** Commit

```bash
git add src/ui.rs
git commit -m "fix(md): require GFM separator row for pipe-table detection"
```

---

## Task 4: Bold-hugging investigation + fix (scoped)

**Objective:** Reproduce the "bold hugs the next word" artifact and fix it.

**Files:**
- Likely: `src/ui.rs` — `parse_inline_spans`
- Possibly: `src/ui.rs` — `pre_wrap_lines` if it's a wrapping issue

**Step 1:** Repro

Run Kaishi, have agent echo each of these in separate messages:

```
A **bold** word then more.
**bold**word no space.
Text **bold**, comma.
A **bold** and _italic_ combo.
Bold **spans two words** right.
```

Inspect whether any case renders without the expected space between the bold end and the following char. Note the exact failing case.

**Step 2:** Likely root cause candidates

- (a) `pre_wrap_lines` merging adjacent same-style spans drops the separating space.
- (b) Wrap boundary eats trailing space on a line ending in bold.
- (c) The terminal's bold attribute visually connects to the next char on some fonts (not our bug).

**Step 3:** Fix once identified

If the parser loses a space: trace where `current` gets flushed. Ensure the push of `Span::raw(std::mem::take(&mut current))` happens before opening any styled span — check that `current` isn't being moved but then reused.

If wrap is the culprit: check `pre_wrap_lines` for `trim_end()` on word-break tokenization — replace with preserving one trailing space per line-internal word.

**Step 4:** Verify repros now render correctly.

**Step 5:** Commit

```bash
git add src/ui.rs
git commit -m "fix(md): preserve whitespace at bold/italic boundary"
```

If investigation shows there's no actual bug — e.g. the visual impression was a font rendering quirk — write a short note to Eva and skip the commit. Don't invent a fix.

---

## Task 5: Copy mode scaffolding — ModalState variant + key binding

**Objective:** Wire `Ctrl+Y` (yank) to open a new modal variant. Copy mode lists messages newest-first with a preview, navigable with arrow keys, Enter to copy, Esc to cancel.

**Files:**
- Modify: `src/app.rs` — add `ModalState::CopyMode`, handler method
- Create: `src/ui_copy_mode.rs`
- Modify: `src/ui.rs` — dispatch to copy mode draw
- Modify: `src/main.rs` — register module (add `mod ui_copy_mode;`)

**Step 1:** Add the modal variant in `src/app.rs` (find `pub enum ModalState`):

```rust
pub enum ModalState {
    // ...existing variants...
    CopyMode {
        selected: usize,
        scope: CopyScope,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CopyScope {
    /// Copy the entire raw message content.
    Message,
    /// Copy only the last fenced code block in the message.
    CodeBlock,
}
```

**Step 2:** Add an opener and handler on `App`:

```rust
pub fn open_copy_mode(&mut self) {
    // Select the most recent non-system message by default.
    let selected = self
        .messages
        .iter()
        .rposition(|m| !matches!(m.role, Role::System))
        .unwrap_or(self.messages.len().saturating_sub(1));
    self.modal = ModalState::CopyMode {
        selected,
        scope: CopyScope::Message,
    };
}
```

Key handler additions (inside `handle_key` where other modals are matched):

```rust
ModalState::CopyMode { selected, scope } => {
    let sel = *selected;
    let sc = scope.clone();
    match key.code {
        KeyCode::Esc => self.modal = ModalState::None,
        KeyCode::Up | KeyCode::Char('k') => {
            if sel > 0 {
                if let ModalState::CopyMode { selected, .. } = &mut self.modal {
                    *selected = sel - 1;
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if sel + 1 < self.messages.len() {
                if let ModalState::CopyMode { selected, .. } = &mut self.modal {
                    *selected = sel + 1;
                }
            }
        }
        KeyCode::Char('c') => {
            // Toggle scope: Message <-> CodeBlock
            let new_scope = match sc {
                CopyScope::Message => CopyScope::CodeBlock,
                CopyScope::CodeBlock => CopyScope::Message,
            };
            if let ModalState::CopyMode { scope, .. } = &mut self.modal {
                *scope = new_scope;
            }
        }
        KeyCode::Enter => {
            let result = self.perform_copy(sel, &sc);
            self.modal = ModalState::None;
            self.sys_msg(result);
        }
        _ => {}
    }
    return Ok(());
}
```

Registration in `Ctrl+Y` branch (in the chat-screen key handler):

```rust
KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    self.open_copy_mode();
    return Ok(());
}
```

Make sure this branch sits where other `Ctrl+X` handlers live — probably near `Ctrl+P` (palette) and `Ctrl+B` (back to picker).

**Step 3:** Add the perform_copy method:

```rust
pub fn perform_copy(&self, idx: usize, scope: &CopyScope) -> String {
    let Some(msg) = self.messages.get(idx) else {
        return "nothing to copy".to_string();
    };

    // Strip null bytes proactively — belt and suspenders.
    let raw: String = msg.content.chars().filter(|&c| c != '\0').collect();

    let text = match scope {
        CopyScope::Message => raw,
        CopyScope::CodeBlock => extract_last_code_block(&raw).unwrap_or(raw),
    };

    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text.clone())) {
        Ok(()) => format!("copied {} chars", text.chars().count()),
        Err(e) => format!("copy failed: {e}"),
    }
}
```

Add the helper (can live in `src/app.rs` or a new `src/text_utils.rs`):

```rust
/// Extract the content of the last fenced code block (without the fences).
fn extract_last_code_block(text: &str) -> Option<String> {
    let mut in_block = false;
    let mut last_block: Option<String> = None;
    let mut current = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_block {
                last_block = Some(std::mem::take(&mut current));
                in_block = false;
            } else {
                in_block = true;
                current.clear();
            }
        } else if in_block {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    // If still in an unclosed block, return what we have.
    if in_block && !current.is_empty() {
        last_block = Some(current);
    }
    last_block
}
```

**Step 4:** Create `src/ui_copy_mode.rs`:

```rust
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, CopyScope, Role};

pub fn draw_copy_mode(f: &mut Frame, app: &App, selected: usize, scope: &CopyScope) {
    let area = centered_rect(80, 70, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // title
            Constraint::Min(5),     // message list
            Constraint::Length(2),  // hints
        ])
        .split(area);

    let scope_label = match scope {
        CopyScope::Message => "message",
        CopyScope::CodeBlock => "code block",
    };
    let title = Paragraph::new(Line::from(vec![
        Span::styled("  Copy ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::styled(format!("({})", scope_label), Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = app
        .messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let role_icon = match m.role {
                Role::User => "›",
                Role::Assistant => "◆",
                Role::System => "·",
                Role::Tool => "▸",
                Role::Thought => "◌",
            };
            let preview: String = m
                .content
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .filter(|&c| c != '\0')
                .take(80)
                .collect();
            let nlines = m.content.lines().count();
            let line = Line::from(vec![
                Span::styled(format!(" {} ", role_icon), Style::default().fg(Color::DarkGray)),
                Span::raw(preview),
                Span::styled(
                    format!("  ({} lines)", nlines),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            let _ = i;
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(selected));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .highlight_style(Style::default().bg(Color::Magenta).fg(Color::Black));
    f.render_stateful_widget(list, chunks[1], &mut state);

    let hints = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓ select · enter copy · c toggle code/msg · esc cancel ",
            Style::default().fg(Color::DarkGray)),
    ]));
    f.render_widget(hints, chunks[2]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
```

**Step 5:** Wire into `src/main.rs`:

```rust
mod ui_copy_mode;
```

**Step 6:** Dispatch in `src/ui.rs` draw:

In the modal-dispatch chunk where `ModalState::Palette { .. }` etc. are drawn, add:

```rust
ModalState::CopyMode { selected, scope } => {
    ui_copy_mode::draw_copy_mode(frame, app, *selected, scope);
}
```

**Step 7:** Build + clippy

```bash
cargo build --release
cargo clippy --release -- -D warnings
```

Expected: clean.

**Step 8:** Manual test

Run Kaishi. In a session with some multi-line messages, press `Ctrl+Y`. Navigate with arrows. Press Enter on the most recent assistant message — should see `copied NNN chars` system message. Paste outside terminal → confirm it's the raw markdown (including `**`, `` ` ``, etc.) with no `│` or box chars and no null bytes.

Press `c` while in copy mode to toggle scope to "code block", navigate to a message with a code block, Enter — should copy just the code content.

**Step 9:** Update bottom-border hint

In `src/ui.rs` where the rounded input border's `bottom_hint` is computed — check the ratatui-scrollable-tui skill's "Rounded Input Border" section — the idle+empty state hint should now also mention `Ctrl+Y`. Careful: that row is already cramped per prior polish commits. Best option: don't add to the always-visible hint; add it only to `/help` command output.

Open `/help` text in `src/app.rs` (grep for `/save` or `Export session`). Add a line:

```
Ctrl+Y           Copy mode — copy messages or code blocks
```

**Step 10:** Commit

```bash
git add src/app.rs src/ui.rs src/ui_copy_mode.rs src/main.rs
git commit -m "feat(copy): add Ctrl+Y copy mode for messages and code blocks"
```

---

## Task 6: Strip null bytes defensively at intake

**Objective:** Make sure no null bytes sneak into `messages[].content` in the first place, so copy mode (and everything else) stays clean. The skill already flags this as ACP pitfall #5.

**Files:**
- Modify: `src/acp.rs` — wherever `agent_message_chunk` / `agent_thought_chunk` content is read and handed to the app
- Modify: `src/app.rs` — `submit_input` where user input is captured

**Step 1:** Find content extraction in `src/acp.rs`

```bash
rg -n "content.*text|as_str\(\).*content" src/acp.rs
```

**Step 2:** Add a helper at the top of `src/acp.rs` (or a shared util):

```rust
/// Strip ASCII NUL bytes that some terminal pastes / malformed streams inject.
pub fn scrub_nulls(s: &str) -> String {
    s.chars().filter(|&c| c != '\0').collect()
}
```

**Step 3:** Wrap each extracted text payload with `scrub_nulls()` before it enters the event pipeline. Do the same in `submit_input`:

```rust
let cleaned = scrub_nulls(&self.input);
// use `cleaned` as the message content
```

**Step 4:** Build + clippy

```bash
cargo build --release
cargo clippy --release -- -D warnings
```

**Step 5:** Commit

```bash
git add src/acp.rs src/app.rs
git commit -m "fix(acp): strip null bytes from inbound message chunks and user input"
```

---

## Task 7: Version bump + changelog line

**Files:**
- Modify: `Cargo.toml` (version)
- Modify: `README.md` if it has a version badge / compatibility line

**Step 1:** Bump to `0.9.0` (new minor — new feature + meaningful bug fixes)

```toml
version = "0.9.0"
```

**Step 2:** Build once more, run Kaishi, smoke-test: send a message with mixed `**bold**`, `_italic_`, `__bold__`, a real markdown table, and a pasted box-art table. Then Ctrl+Y, copy something, paste outside.

**Step 3:** Commit + tag

```bash
git add Cargo.toml Cargo.lock README.md
git commit -m "chore: bump to v0.9.0 — markdown fixes + copy mode"
git tag v0.9.0
```

Do NOT push / force-push — Eva handles that manually (Tirith blocks force-push for the agent).

---

## Verification checklist (final)

- [ ] `_italic_` and `__bold__` render with emphasis; `snake_case` and `file_name.rs` stay plain
- [ ] User's pasted box-art table no longer produces `─┼─` cascades — renders as plain lines
- [ ] Real GFM table (`| h | h |` + `| --- | --- |` + rows) still renders bordered/padded
- [ ] Bold-hugging case identified and fixed (or ruled non-bug with note to Eva)
- [ ] `Ctrl+Y` opens copy mode
- [ ] Arrow keys navigate; Enter copies; Esc cancels
- [ ] `c` toggles between message / code-block scope
- [ ] Pasted output contains raw markdown source — no box-drawing chars, no null bytes
- [ ] `cargo clippy --release -- -D warnings` clean
- [ ] `/help` lists `Ctrl+Y`

## Remember

```
Bite-sized tasks (2-5 min each)
Complete code, exact commands, frequent commits
Kaishi runs the RELEASE binary — rebuild with --release after every fix
Don't push; Eva handles that
```
