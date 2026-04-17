# Claude Code-Style UI Overhaul & Bug Fixes

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Fix three user-reported UI bugs and bring the visual language closer to Claude Code's clean, minimal aesthetic — merged icons, compact tool calls, muted palette.

**Architecture:** All changes are in `src/ui.rs` and `src/app.rs`. No wire protocol changes. No new dependencies. The rendering pipeline stays the same (messages → render_message → pre_wrap → cache → Paragraph::scroll). We're changing *what* gets rendered, not *how*.

**Tech Stack:** Rust, ratatui 0.29+, crossterm, unicode-width crate (already a dep)

**Repo:** `/home/opus/hermes-tui`

---

## Summary of Issues

### User-Reported Bugs

| # | Bug | Root Cause | Fix Location |
|---|-----|-----------|--------------|
| B1 | Role indicators (◆, ○, ❯) appear one line *above* their content | `render_message()` puts icon on a header `Line`, then content starts on the next line | `src/ui.rs:302-368` |
| B2 | Tool calls are a UX nightmare — messy, no context, cryptic names | Tools render as bare `⚙ name` one-liners; name extraction is fragile (whitespace split after stripping icon prefixes) | `src/ui.rs:304-319`, `src/app.rs:758-820` |
| B3 | Interleaved thinking doesn't appear between tool calls despite latency proving it's happening | **Server-side**: ACP adapter may not set `reasoning_callback`. TUI-side flush logic is correct. | Server: `acp_adapter/server.py` (upstream) |

### Code Bugs Found During Review

| # | Bug | Root Cause | Fix Location |
|---|-----|-----------|--------------|
| C1 | `truncate()` will panic on multi-byte chars at the cut point | Slices by byte index: `&s[..max]` doesn't respect char boundaries | `src/ui.rs:725-731` |
| C2 | `pre_wrap_lines` can overflow on CJK/wide chars | Wraps by `.take(max_width)` char count, but CJK chars are 2 columns wide | `src/ui.rs:280-300` |

### Claude Code-Style Visual Improvements

| # | Change | Description |
|---|--------|-------------|
| V1 | Inline role indicators | Merge `◆`/`❯`/`○` onto the same line as the first content line (fixes B1 simultaneously) |
| V2 | Compact tool calls | Claude Code `⎿` nesting style with tool name, brief arg preview, and clean status transitions |
| V3 | Muted palette | Reduce color saturation — mostly white/gray text, accent colors only for role differentiation |
| V4 | Streaming indicator cleanup | Remove the separate `(streaming…)` label; just show content arriving with a cursor block |
| V5 | Thought display refinement | Compact `○ thinking…` with expandable content, no separate header line |

---

## Task Breakdown

### Task 1: Fix `truncate()` char boundary panic (C1)

**Objective:** Make string truncation safe for multi-byte characters (emoji, CJK, accented chars).

**Files:**
- Modify: `src/ui.rs:725-731`

**Step 1: Fix the truncate function**

Replace the current byte-slicing `truncate()` with a char-boundary-aware version:

```rust
fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}
```

Note: this truncates by *character count*, not display width. For display-width-aware truncation (needed for status bar), use the `unicode-width` crate — but that's a separate concern handled in `draw_status_bar` already. This function is only used for session hints and error messages where char count is good enough.

**Step 2: Verify build**

```bash
cd /home/opus/hermes-tui && cargo build 2>&1 | tail -5
```

**Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "fix: truncate() respects char boundaries — prevents panic on multi-byte"
```

---

### Task 2: Fix `pre_wrap_lines` for wide characters (C2)

**Objective:** Use display width (not char count) when wrapping lines, so CJK and emoji don't overflow.

**Files:**
- Modify: `src/ui.rs:280-300`

**Step 1: Rewrite pre_wrap_lines to use display width**

```rust
fn pre_wrap_lines(lines: Vec<Line<'static>>, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return lines;
    }
    let mut result = Vec::with_capacity(lines.len());
    for line in lines {
        if line.width() <= max_width {
            result.push(line);
            continue;
        }
        let style = line.spans.first().map(|s| s.style).unwrap_or_default();
        let full: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        // Wrap by display width, not char count
        let mut current = String::new();
        let mut current_width = 0;
        for ch in full.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if current_width + ch_width > max_width && !current.is_empty() {
                result.push(Line::from(Span::styled(
                    std::mem::take(&mut current),
                    style,
                )));
                current_width = 0;
            }
            current.push(ch);
            current_width += ch_width;
        }
        if !current.is_empty() {
            result.push(Line::from(Span::styled(current, style)));
        }
    }
    result
}
```

Add the import at the top of `ui.rs` if not already present (it is — `use unicode_width::UnicodeWidthStr;` is on line 9, but we also need `UnicodeWidthChar`):

The `UnicodeWidthChar` trait is in the same crate but needs explicit use. However, calling it as `unicode_width::UnicodeWidthChar::width(ch)` works without a use statement.

**Step 2: Verify build**

```bash
cargo build 2>&1 | tail -5
```

**Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "fix: pre_wrap_lines uses display width — CJK/emoji no longer overflow"
```

---

### Task 3: Merge role indicators inline with content (B1 + V1)

**Objective:** Put the role icon on the same line as the first content line instead of a separate header. This fixes the "indicator one line too high" bug and matches Claude Code's inline style.

**Files:**
- Modify: `src/ui.rs:302-368` (`render_message` function)

**Step 1: Rewrite render_message for inline icons**

The key change: instead of pushing a header line with just the icon, then pushing content lines separately, we prepend the icon span to the *first* content line.

```rust
fn render_message(
    lines: &mut Vec<Line>,
    msg: &ChatMessage,
    width: usize,
    verbose: bool,
    narrow: bool,
) {
    // Tool messages: compact single line with status icon
    if msg.role == Role::Tool {
        let (icon, color) = if msg.content.starts_with('✓') {
            ("  ✓ ", Color::Green)
        } else if msg.content.starts_with('✗') {
            ("  ✗ ", Color::Red)
        } else {
            ("  ⚙ ", Color::DarkGray)
        };
        let name = msg
            .content
            .trim_start_matches(['✓', '✗', '⚙', ' '])
            .to_string();
        lines.push(Line::from(vec![
            Span::styled(icon, Style::default().fg(color)),
            Span::styled(name, Style::default().fg(color)),
        ]));
        return;
    }

    let (icon, icon_color) = match msg.role {
        Role::User => ("❯ ", Color::Cyan),
        Role::Assistant => ("◆ ", Color::Magenta),
        Role::System => ("● ", Color::Yellow),
        Role::Tool => unreachable!(),
        Role::Thought => ("○ ", Color::DarkGray),
    };

    // Build usage suffix if present
    let usage_span = msg.tokens.as_ref().map(|u| {
        Span::styled(
            format!(" [{}→{}]", u.input_tokens, u.output_tokens),
            Style::default().fg(Color::DarkGray),
        )
    });

    match msg.role {
        Role::Thought => {
            if verbose {
                let thought_lines: Vec<&str> = msg.content.lines().collect();
                if thought_lines.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {}", icon),
                            Style::default().fg(icon_color),
                        ),
                        Span::styled(
                            "thinking…",
                            Style::default().fg(Color::DarkGray).italic(),
                        ),
                    ]));
                } else {
                    // First line gets the icon
                    let mut first = vec![
                        Span::styled(
                            format!("  {}", icon),
                            Style::default().fg(icon_color),
                        ),
                        Span::styled(
                            thought_lines[0].to_string(),
                            Style::default().fg(Color::DarkGray).italic(),
                        ),
                    ];
                    if let Some(u) = usage_span {
                        first.push(u);
                    }
                    lines.push(Line::from(first));

                    // Remaining lines indented
                    for &tl in &thought_lines[1..] {
                        lines.push(Line::from(Span::styled(
                            format!("{}{}", indent(narrow), tl),
                            Style::default().fg(Color::DarkGray).italic(),
                        )));
                    }
                }
            } else {
                let line_count = msg.content.lines().count();
                let mut spans = vec![
                    Span::styled(
                        format!("  {}", icon),
                        Style::default().fg(icon_color),
                    ),
                    Span::styled(
                        format!("({} lines — /verbose to expand)", line_count),
                        Style::default().fg(Color::DarkGray).italic(),
                    ),
                ];
                if let Some(u) = usage_span {
                    spans.push(u);
                }
                lines.push(Line::from(spans));
            }
        }
        Role::Assistant => {
            // Render markdown; prepend icon to the first line
            let before = lines.len();
            render_markdown_lines(lines, &msg.content, width, narrow);
            // Prepend icon to whatever the first rendered line is
            if lines.len() > before {
                let first = &mut lines[before];
                let mut new_spans = vec![Span::styled(
                    format!("  {}", icon),
                    Style::default().fg(icon_color),
                )];
                new_spans.extend(first.spans.clone());
                if let Some(u) = usage_span {
                    // Append usage to the LAST line of this message, not first
                    // Actually, append to first line for visibility
                }
                *first = Line::from(new_spans);
            } else {
                // Empty content — just show icon
                let mut spans = vec![Span::styled(
                    format!("  {}", icon),
                    Style::default().fg(icon_color),
                )];
                if let Some(u) = usage_span {
                    spans.push(u);
                }
                lines.push(Line::from(spans));
            }
            // Add usage as a subtle line at the end of assistant messages
            if let Some(u) = &msg.tokens {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}[{}→{} tokens]",
                        indent(narrow),
                        u.input_tokens,
                        u.output_tokens
                    ),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
        _ => {
            // User and System: inline icon with first content line
            let content_lines: Vec<&str> = msg.content.lines().collect();
            if content_lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", icon),
                    Style::default().fg(icon_color),
                )));
            } else {
                // First line gets icon
                let mut first_spans = vec![Span::styled(
                    format!("  {}", icon),
                    Style::default().fg(icon_color),
                )];
                first_spans.extend(parse_inline_spans(content_lines[0]));
                lines.push(Line::from(first_spans));

                // Remaining lines indented to align with content after icon
                for &cl in &content_lines[1..] {
                    let mut spans = vec![Span::raw(indent(narrow).to_string())];
                    spans.extend(parse_inline_spans(cl));
                    lines.push(Line::from(spans));
                }
            }
        }
    }
}
```

**Step 2: Update streaming indicator (V4)**

In `draw_messages()`, update the pending response rendering (around line 194-215) to also use inline icon:

```rust
// Render the in-progress streaming response
if !app.pending_response.is_empty() {
    let before = all_lines.len();
    render_markdown_lines(&mut all_lines, &app.pending_response, inner_width, narrow);
    // Prepend icon to first rendered line
    if all_lines.len() > before {
        let first = &mut all_lines[before];
        let mut new_spans = vec![Span::styled(
            "  ◆ ",
            Style::default().fg(Color::Magenta),
        )];
        new_spans.extend(first.spans.clone());
        *first = Line::from(new_spans);
    }

    // Blinking cursor at end
    if app.tick % 4 < 2 {
        if let Some(last) = all_lines.last_mut() {
            let mut spans = last.spans.clone();
            spans.push(Span::styled("█", Style::default().fg(Color::Magenta)));
            *last = Line::from(spans);
        }
    }
    all_lines.push(Line::from(""));
}
```

**Step 3: Update pending thought indicator (V5)**

```rust
// Show pending thought (inline icon)
if !app.pending_thought.is_empty() {
    let mut first_spans = vec![
        Span::styled("  ○ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "thinking…",
            Style::default().fg(Color::DarkGray).italic(),
        ),
    ];
    all_lines.push(Line::from(first_spans));
    if app.verbose {
        for line in app.pending_thought.lines() {
            all_lines.push(Line::from(Span::styled(
                format!("{}{}", indent(narrow), line),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    all_lines.push(Line::from(""));
}
```

**Step 4: Verify build**

```bash
cargo build 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add src/ui.rs
git commit -m "fix: merge role indicators inline with content — icons no longer float above text"
```

---

### Task 4: Redesign tool call rendering (B2 + V2)

**Objective:** Replace the cryptic `⚙ name` tool display with Claude Code-style compact tool calls that show what's happening.

**Files:**
- Modify: `src/app.rs:758-820` (tool event handlers)
- Modify: `src/ui.rs:304-319` (tool rendering)
- Modify: `src/event.rs:47-56` (ToolCallStart needs more data)

**Step 1: Extend ToolCallStart event to carry input preview**

In `src/event.rs`, the `ToolCallStart` variant already has `id`, `name`, `kind`. No change needed here — but we need to capture `rawInput` from the wire.

In `src/acp.rs:289-305`, the `tool_call` handler already reads `title` and `kind`. Add `rawInput` capture:

```rust
"tool_call" => {
    let id = params
        .get("toolCallId")
        .or_else(|| params.get("tool_call_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name = params
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let kind = params
        .get("kind")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let input = params
        .get("rawInput")
        .or_else(|| params.get("raw_input"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let _ = event_tx.send(AppEvent::ToolCallStart { id, name, kind, input });
}
```

Update `AppEvent::ToolCallStart` in `src/event.rs`:

```rust
ToolCallStart {
    id: String,
    name: String,
    kind: Option<String>,
    input: Option<String>,
},
```

Update the match in `src/main.rs:206-208`:

```rust
event::AppEvent::ToolCallStart { id, name, kind, input } => {
    app.handle_tool_start(&id, &name, kind.as_deref(), input.as_deref());
}
```

**Step 2: Improve tool message content in app.rs**

Update `handle_tool_start` to store a richer message:

```rust
pub fn handle_tool_start(&mut self, id: &str, name: &str, _kind: Option<&str>, input: Option<&str>) {
    self.flush_pending_thought();
    if !self.pending_response.is_empty() {
        self.flush_pending_response(None);
    }

    self.active_tools.push((id.to_string(), name.to_string()));

    // Build a compact tool description with input preview
    let preview = input
        .map(|s| {
            // Try to extract a meaningful preview from the raw input
            let clean = s.trim();
            if clean.len() > 80 {
                format!("({}…)", &clean[..clean.char_indices().nth(80).map(|(i,_)|i).unwrap_or(clean.len())])
            } else {
                format!("({})", clean)
            }
        })
        .unwrap_or_default();

    let idx = self.messages.len();
    self.messages.push(ChatMessage {
        role: Role::Tool,
        content: format!("⚙ {} {}", name, preview),
        tokens: None,
    });
    self.tool_msg_map.insert(id.to_string(), idx);
    self.scroll_offset = 0;
}
```

**Step 3: Fix tool name extraction in handle_tool_update**

The current name extraction (`split_whitespace().next()`) is fragile. Store the name separately or parse more carefully:

```rust
pub fn handle_tool_update(&mut self, id: &str, status: &str, content: Option<&str>) {
    if status == "completed" || status == "error" {
        self.active_tools.retain(|(tid, _)| tid != id);
    }

    if let Some(&msg_idx) = self.tool_msg_map.get(id) {
        if msg_idx < self.messages.len() {
            // Extract the tool name from active_tools or from the stored message
            let name = self.active_tools.iter()
                .find(|(tid, _)| tid == id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| {
                    // Fallback: parse from message content "⚙ name (...)"
                    self.messages[msg_idx].content
                        .trim_start_matches(['✓', '✗', '⚙', '⏳', ' '])
                        .split(|c: char| c == ' ' || c == '(')
                        .next()
                        .unwrap_or("")
                        .to_string()
                });

            let new_content = match status {
                "completed" => format!("✓ {}", name),
                "error" => {
                    let detail = content
                        .map(|t| {
                            let preview = if t.len() > 100 {
                                &t[..t.char_indices().nth(100).map(|(i,_)|i).unwrap_or(t.len())]
                            } else {
                                t
                            };
                            format!(" — {}", preview)
                        })
                        .unwrap_or_default();
                    format!("✗ {}{}", name, detail)
                }
                _ => format!("⚙ {}", name), // still running
            };

            self.messages[msg_idx].content = new_content;

            if msg_idx < self.line_cache.len() {
                self.line_cache.truncate(msg_idx);
            }
        }

        if status == "completed" || status == "error" {
            self.tool_msg_map.remove(id);
        }
    }
}
```

**Step 4: Improve tool rendering in ui.rs**

Update the tool message rendering to be more visually structured:

```rust
// In render_message, the Role::Tool branch:
if msg.role == Role::Tool {
    let (icon, color) = if msg.content.starts_with('✓') {
        ("✓", Color::Green)
    } else if msg.content.starts_with('✗') {
        ("✗", Color::Red)
    } else {
        ("⚙", Color::DarkGray)
    };

    // Parse: "ICON name (preview)" or "ICON name — error"
    let rest = msg.content
        .trim_start_matches(['✓', '✗', '⚙', ' '])
        .to_string();

    // Split name from detail at first space-paren or space-dash
    let (name, detail) = if let Some(paren_idx) = rest.find(" (") {
        (&rest[..paren_idx], Some(&rest[paren_idx..]))
    } else if let Some(dash_idx) = rest.find(" — ") {
        (&rest[..dash_idx], Some(&rest[dash_idx..]))
    } else {
        (rest.as_str(), None)
    };

    let mut spans = vec![
        Span::styled(format!("  {} ", icon), Style::default().fg(color)),
        Span::styled(
            name.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ];

    if let Some(d) = detail {
        spans.push(Span::styled(
            d.to_string(),
            Style::default().fg(Color::DarkGray),
        ));
    }

    lines.push(Line::from(spans));
    return;
}
```

**Step 5: Verify build**

```bash
cargo build 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add src/ui.rs src/app.rs src/acp.rs src/event.rs src/main.rs
git commit -m "feat: redesign tool calls — compact display with input preview and bold names"
```

---

### Task 5: Mute the color palette (V3)

**Objective:** Shift from saturated colors to a more muted, Claude Code-inspired palette. White/gray base, accents only for differentiation.

**Files:**
- Modify: `src/ui.rs` (multiple locations)

**Step 1: Define a palette at the top of ui.rs**

Add a palette section after the imports for easy tuning:

```rust
// ─── Palette (Claude Code-inspired) ────────────────────────────
mod palette {
    use ratatui::style::Color;

    pub const TEXT: Color = Color::White;
    pub const DIM: Color = Color::DarkGray;
    pub const ACCENT_USER: Color = Color::Cyan;
    pub const ACCENT_ASSISTANT: Color = Color::Rgb(180, 140, 255); // soft purple
    pub const ACCENT_SYSTEM: Color = Color::Yellow;
    pub const ACCENT_THOUGHT: Color = Color::DarkGray;
    pub const ACCENT_TOOL: Color = Color::DarkGray;
    pub const SUCCESS: Color = Color::Green;
    pub const ERROR: Color = Color::Red;
    pub const CODE_FG: Color = Color::Rgb(130, 200, 130);  // soft green
    pub const CODE_BG: Color = Color::Rgb(30, 30, 30);
    pub const BORDER: Color = Color::Rgb(60, 60, 60);      // subtle borders
    pub const STATUS_BG: Color = Color::Rgb(40, 40, 40);   // near-black status bar
    pub const QUOTE: Color = Color::Rgb(100, 140, 200);    // soft blue
}
```

**Step 2: Replace hardcoded colors throughout ui.rs**

Go through all `Color::` references and replace with `palette::` equivalents. Key locations:

- `draw_status_bar`: `Color::DarkGray` → `palette::STATUS_BG`, `Color::White` → `palette::TEXT`
- `draw_messages` block border: `Color::DarkGray` → `palette::BORDER`
- `render_message` role colors: use `palette::ACCENT_*`
- `render_markdown_lines` code blocks: `Color::Green` → `palette::CODE_FG`
- `draw_input` border: `Color::Cyan` → `palette::ACCENT_USER`
- Inline code bg: `Color::Rgb(40, 40, 40)` → `palette::CODE_BG`

This is a sweep — touch every `Color::` in ui.rs and replace with the appropriate palette constant. The palette module makes future theme tweaks trivial.

**Step 3: Update status bar to muted style**

```rust
let style = match &app.status {
    AgentStatus::Idle => Style::default().bg(palette::STATUS_BG).fg(palette::TEXT),
    AgentStatus::Thinking => Style::default().bg(palette::STATUS_BG).fg(palette::ACCENT_ASSISTANT),
    AgentStatus::Error(_) => Style::default().bg(palette::STATUS_BG).fg(palette::ERROR),
};
```

The current design uses bright `Color::Blue` bg for thinking — too loud. Muting to the same bg with a colored fg is subtler.

**Step 4: Verify build**

```bash
cargo build 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add src/ui.rs
git commit -m "style: muted palette — Claude Code-inspired colors with soft accents"
```

---

### Task 6: Clean up status bar text (Claude Code style)

**Objective:** Make the status bar more information-dense and less emoji-heavy.

**Files:**
- Modify: `src/ui.rs:70-151` (`draw_status_bar`)

**Step 1: Simplify status bar content**

Replace the current `🌸 Hanami | model | N msgs` with something cleaner:

```rust
fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let narrow = area.width < 60;
    let model = if app.model_name.is_empty() {
        "hermes"
    } else {
        &app.model_name
    };

    let session_hint = if narrow {
        String::new()
    } else if let Some(ref title) = app.session_title {
        format!(" > {}", truncate(title, 30))
    } else if let Some(ref sid) = app.session_id {
        let short = if sid.len() > 8 { &sid[..8] } else { sid };
        format!(" > {}", short)
    } else {
        String::new()
    };

    let status_text = match &app.status {
        AgentStatus::Idle => {
            format!(" {}{}", model, session_hint)
        }
        AgentStatus::Thinking => {
            let spinner = SPINNER[(app.tick as usize) % SPINNER.len()];
            let tool_hint = if let Some((_, name)) = app.active_tools.last() {
                format!(" {}", name)
            } else {
                " thinking…".to_string()
            };
            format!(" {}{}{}", spinner, tool_hint, session_hint)
        }
        AgentStatus::Error(e) => {
            format!(" ⚠ {}", truncate(e, 50))
        }
    };

    // ... rest of status bar layout stays the same, but use palette colors
    let style = match &app.status {
        AgentStatus::Idle => Style::default().bg(palette::STATUS_BG).fg(palette::DIM),
        AgentStatus::Thinking => Style::default().bg(palette::STATUS_BG).fg(palette::ACCENT_ASSISTANT),
        AgentStatus::Error(_) => Style::default().bg(palette::STATUS_BG).fg(palette::ERROR),
    };

    let help = if narrow { " ? " } else { " Esc quit | /help " };
    // ... rest unchanged
}
```

**Step 2: Verify build and commit**

```bash
cargo build 2>&1 | tail -5
git add src/ui.rs
git commit -m "style: clean up status bar — less emoji, more information density"
```

---

### Task 7: Visual integration test

**Objective:** Build, run, and verify all changes work together. No automated tests for TUI rendering — this is a manual visual check.

**Step 1: Build release**

```bash
cd /home/opus/hermes-tui
cargo build --release 2>&1 | tail -5
```

**Step 2: Run and verify**

```bash
./target/release/hermes-tui --profile hanami
```

Check:
- [ ] Role indicators (◆, ❯, ○) appear on the same line as content
- [ ] Tool calls show name in bold with input preview in gray
- [ ] Completed tools show green ✓, errors show red ✗
- [ ] Color palette is muted — no bright blue status bar
- [ ] Status bar shows model name without 🌸
- [ ] Streaming shows ◆ inline with first text line + block cursor
- [ ] Thinking shows ○ inline, expandable with /verbose
- [ ] No panics on emoji-heavy messages (truncate fix)
- [ ] Code blocks still render with soft green

**Step 3: Final commit**

```bash
git add -A
git commit -m "chore: visual integration verification pass"
```

---

## Deferred: Server-Side Fixes

These require changes to the forked Hermes upstream and are **not part of this plan**:

### D1: `reasoning_callback` not wired in ACP adapter

**File:** `acp_adapter/server.py` (in the forked hermes-agent repo)

The ACP adapter sets `agent.thinking_callback = thinking_cb` but does NOT set `agent.reasoning_callback`. This means actual model reasoning content (extended thinking, chain-of-thought) never fires as `agent_thought_chunk` wire events. The TUI receives nothing between tool calls.

**Fix:**
```python
# acp_adapter/server.py — in the prompt handler, alongside thinking_callback
agent.thinking_callback = None         # Suppress kawaii spinner text
agent.reasoning_callback = thinking_cb  # Real reasoning → agent_thought_chunk
```

### D2: `HERMES_INTERACTIVE` not set in ACP adapter

The approval system checks `os.getenv("HERMES_INTERACTIVE")` — if unset AND not a gateway session, it auto-approves everything. The ACP adapter should set this in `__init__`.

### D3: Extended thinking intermittent

Sometimes reasoning_callback fires, sometimes it doesn't. May be related to model routing (some models support extended thinking, others don't) or to the API response format varying.

---

## Task Dependency Graph

```
Task 1 (truncate fix) ──────┐
Task 2 (pre_wrap fix) ──────┤
Task 3 (inline indicators) ─┤── Task 7 (visual test)
Task 4 (tool calls) ────────┤
Task 5 (palette) ───────────┤
Task 6 (status bar) ────────┘
```

Tasks 1-6 are independent of each other and can be done in any order (or in parallel by subagents). Task 7 depends on all of them.
