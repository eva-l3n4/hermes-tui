# Picker & Popup Scroll Fixes Implementation Plan

> **For Hermes:** Execute task-by-task. Run `cargo test` and `cargo build` after each. Commit per task. Use subagent-driven-development.

**Goal:** Fix three navigation-scroll bugs in Kaishi's TUI:

1. Session picker opens focused on the *bottom* of the list (oldest session), not the top where `"+ New Session"` is selected.
2. Command palette (Ctrl+P) doesn't scroll when selection moves past the visible window.
3. File autocomplete popup (`@`) hard-caps rendering at 8 entries; cursor can go off-screen silently.

**Architecture:** All three are scroll-follow bugs with the same underlying fix: keep the *selected index* in view by computing a scroll offset that tracks the cursor. Currently none of them do that.

- **Picker** uses its own scroll math in `ui_picker.rs` with a hand-rolled `Paragraph::scroll(…)`. The math is inverted — `scroll_offset = 0` currently maps to `scroll_pos = max_scroll` (bottom), not `0` (top). Fix: invert + add cursor-follow.
- **Palette** renders a plain `List::new(items)` with no state. Fix: switch to `render_stateful_widget` with a `ListState` that holds the selected index — ratatui auto-scrolls the viewport to keep it visible.
- **File popup** renders `entries.iter().take(8)`. Fix: same `ListState` pattern, drop the hard `take(8)`, let the popup height + ListState handle windowing.

**Tech Stack:** Rust, ratatui 0.29 (`ListState::with_selected` and `render_stateful_widget` are the key APIs).

**Files touched:**
- `src/ui_picker.rs` (scroll math + cursor-follow)
- `src/ui_palette.rs` (ListState)
- `src/ui_file_popup.rs` (ListState + height sizing)
- `src/app.rs` (picker_scroll_offset now derived, not mouse-driven; tests)
- `Cargo.toml` (version bump)
- `docs/roadmap-0.6-0.7.md` (v0.8.3 entry)

**Out of scope:** picker delete/filter, Ctrl+F conversation search (separate plans).

---

## Task 1: Fix picker opens at bottom → opens at top

**Objective:** When sessions load, the picker shows `"+ New Session"` selected at the top. Currently the viewport starts pinned to the bottom because of an inverted `max_scroll.saturating_sub(...)` in the scroll math.

**Files:**
- Modify: `src/ui_picker.rs:82-90` (fix `scroll_pos` formula)

**Step 1: Reproduce manually**

```bash
cd /home/opus/hermes-tui && cargo run --release
```

On a list with enough sessions to overflow the viewport, confirm the picker opens with the oldest session visible at the bottom and `"+ New Session"` somewhere above the top edge (off-screen).

**Step 2: Replace the scroll math**

In `src/ui_picker.rs`, replace the block at lines 82–90:

```rust
    // Scrolling
    let total_lines = lines.len() as u16;
    let visible_height = area.height;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll_pos = max_scroll.saturating_sub(scroll_offset.min(max_scroll));

    let paragraph = Paragraph::new(Text::from(lines)).scroll((scroll_pos, 0));
```

with:

```rust
    // Scrolling — scroll_offset counts from the top, 0 = top of list
    let total_lines = lines.len() as u16;
    let visible_height = area.height;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll_pos = scroll_offset.min(max_scroll);

    let paragraph = Paragraph::new(Text::from(lines)).scroll((scroll_pos, 0));
```

And update the scrollbar state block immediately below to use `scroll_pos` directly (it already does, but double-check nothing else referenced the old inverted value).

**Step 3: Verify build**

```bash
cd /home/opus/hermes-tui && cargo build 2>&1 | tail -10
```

Expected: clean build.

**Step 4: Manual smoke test**

```bash
cargo run --release
```

- Picker opens → `"+ New Session"` is selected and visible at top.
- Scroll wheel down → list moves up (older sessions come into view).
- Scroll wheel up → returns to top.

**Step 5: Commit**

```bash
cd /home/opus/hermes-tui
git add src/ui_picker.rs
git commit -m "fix(picker): scroll_offset 0 means top, not bottom

The scroll_pos formula was inverted (max_scroll.saturating_sub),
so the picker opened showing the oldest session pinned to the
bottom of the viewport with '+ New Session' selected off-screen
above. Straighten the math so offset 0 = top."
```

---

## Task 2: Picker selection follows cursor

**Objective:** Pressing `j`/`Down` past the bottom of the visible window scrolls the list so the selected card stays on-screen. Same for `k`/`Up` past the top.

Currently `picker_scroll_offset` is only touched by the mouse wheel (`handle_scroll`); keyboard navigation moves `picker_selected` but never `picker_scroll_offset`, so selection can disappear off either edge.

**Files:**
- Modify: `src/ui_picker.rs` (expose `CARD_HEIGHT` constant, return visible rows)
- Modify: `src/app.rs` (add `ensure_picker_visible` helper, call it after Up/Down)

**Approach:** The cleanest fix without restructuring ratatui's line-based layout is to treat each card as occupying a known number of render lines (3: title + meta + blank) and compute "is the selected card's top line within `[scroll_offset, scroll_offset + area.height)`?" after each Up/Down. If not, nudge `picker_scroll_offset`.

**Step 1: Extract the CARD_HEIGHT constant**

At the top of `src/ui_picker.rs` (after imports, before `draw_picker`):

```rust
/// Each session card (and the "New Session" card) occupies this many
/// rendered lines: [title/label, meta/empty, separator blank].
pub const CARD_HEIGHT: u16 = 3;
```

Nothing else needs to change inside `ui_picker.rs` for this task — the constant is just a single source of truth the `App` code can consume.

**Step 2: Write failing test**

Add to the test module in `src/app.rs`:

```rust
#[test]
fn picker_scroll_follows_cursor_down() {
    let mut app = App::new(vec![]);
    app.screen = Screen::Chat; // placeholder so new() doesn't panic
    // Simulate a picker with 20 cards, viewport 10 rows tall.
    app.picker_selected = 5; // card index 5 → top line at row 15
    app.ensure_picker_visible(10 /* visible_rows */);
    // Row 15 must be inside [offset, offset + 10). offset should ≥ 6.
    assert!(app.picker_scroll_offset >= 6);
    assert!(app.picker_scroll_offset <= 15);
}

#[test]
fn picker_scroll_follows_cursor_up() {
    let mut app = App::new(vec![]);
    app.picker_scroll_offset = 30;
    app.picker_selected = 2;
    app.ensure_picker_visible(10);
    // Top line of card 2 is row 6 → offset must be ≤ 6.
    assert!(app.picker_scroll_offset <= 6);
}

#[test]
fn picker_scroll_no_change_when_already_visible() {
    let mut app = App::new(vec![]);
    app.picker_scroll_offset = 5;
    app.picker_selected = 3; // top line at row 9, visible_rows 10 → in view
    app.ensure_picker_visible(10);
    assert_eq!(app.picker_scroll_offset, 5);
}
```

Run: `cargo test picker_scroll` → FAIL (`no method named ensure_picker_visible`).

**Step 3: Implement `ensure_picker_visible`**

In `src/app.rs`, add to `impl App` near `return_to_picker` (around line 1906):

```rust
/// Adjust `picker_scroll_offset` so the currently selected card is
/// within the visible viewport. `visible_rows` is the terminal-row
/// height of the picker list area (not including header/footer).
pub fn ensure_picker_visible(&mut self, visible_rows: u16) {
    use crate::ui_picker::CARD_HEIGHT;
    let top_row = (self.picker_selected as u16).saturating_mul(CARD_HEIGHT);
    let bot_row = top_row.saturating_add(CARD_HEIGHT);

    // Scrolled too far down: selected card is above viewport
    if top_row < self.picker_scroll_offset {
        self.picker_scroll_offset = top_row;
        return;
    }
    // Scrolled too far up: selected card is below viewport
    let viewport_bottom = self.picker_scroll_offset.saturating_add(visible_rows);
    if bot_row > viewport_bottom {
        self.picker_scroll_offset = bot_row.saturating_sub(visible_rows);
    }
}
```

And add `pub use ui_picker::CARD_HEIGHT;` or just `use crate::ui_picker::CARD_HEIGHT;` inside the method — method-local `use` is fine.

Run: `cargo test picker_scroll` → 3 PASS.

**Step 4: Call `ensure_picker_visible` after Up/Down**

Problem: `handle_picker_key` in `app.rs` doesn't know the picker viewport height. Solution: stash the last-known height from the draw call.

Add a field to `App` (near `picker_scroll_offset` at line 228):

```rust
    /// Last-observed height of the picker list area, stashed by the
    /// renderer so keyboard handlers can scroll-follow the cursor.
    pub picker_viewport_rows: u16,
```

Initialize in `App::new` (around line 304):

```rust
            picker_viewport_rows: 0,
```

In `src/ui.rs` line 225, update the draw call to pass `chunks[1].height` back. Actually simpler: have `draw_picker` stash it via a `&mut App` — but ui uses `&app`. Cleanest: compute in `ui.rs` before the call.

In `src/ui.rs`, find the picker draw block (around line 225) and change:

```rust
            ui_picker::draw_picker(frame, &app.sessions, app.picker_selected, app.picker_scroll_offset);
```

to:

```rust
            // Layout mirror so we know the picker list height for scroll-follow.
            let area = frame.area();
            let picker_chunks = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Vertical)
                .constraints([
                    ratatui::layout::Constraint::Length(3),
                    ratatui::layout::Constraint::Min(3),
                    ratatui::layout::Constraint::Length(2),
                ])
                .split(area);
            app.picker_viewport_rows = picker_chunks[1].height;
            ui_picker::draw_picker(frame, &app.sessions, app.picker_selected, app.picker_scroll_offset);
```

This requires `app: &mut App` in the call site — check the signature. If it's `&App`, change to `&mut App` all the way up. Alternative: use interior mutability (a `Cell<u16>`) if mutation is too invasive.

> **Pitfall:** If `ui.rs::draw` takes `&App` (not `&mut`), prefer `Cell<u16>` over widening the signature — propagating `&mut App` through ratatui render closures can tangle with borrows of other fields.

**Fallback using Cell:**

Change the field to `pub picker_viewport_rows: std::cell::Cell<u16>,` and init to `Cell::new(0)`. Then in `ui.rs`: `app.picker_viewport_rows.set(picker_chunks[1].height);`. In `app.rs` handlers: `self.picker_viewport_rows.get()`.

In `handle_picker_key` (lines 500–509), wrap the Up/Down arms to call `ensure_picker_visible`:

```rust
            (_, KeyCode::Up) | (_, KeyCode::Char('k'))
                if self.picker_selected > 0 =>
            {
                self.picker_selected -= 1;
                let rows = self.picker_viewport_rows.get();
                self.ensure_picker_visible(rows);
            }
            (_, KeyCode::Down) | (_, KeyCode::Char('j'))
                if self.picker_selected + 1 < total =>
            {
                self.picker_selected += 1;
                let rows = self.picker_viewport_rows.get();
                self.ensure_picker_visible(rows);
            }
```

(Adjust `.get()` to direct field access if you went with `&mut App` instead of `Cell`.)

**Step 5: Verify**

```bash
cd /home/opus/hermes-tui
cargo build 2>&1 | tail -10
cargo test picker_scroll 2>&1 | tail -10
```

Expected: clean build, 3 tests pass.

**Step 6: Manual smoke test**

1. Launch Kaishi with many sessions.
2. Press `j` repeatedly → selection scrolls down; when hitting the viewport bottom, list follows.
3. Press `k` back up → selection returns to top, list follows.
4. Mouse wheel still works independently (`handle_scroll` unchanged).

**Step 7: Commit**

```bash
cd /home/opus/hermes-tui
git add src/app.rs src/ui.rs src/ui_picker.rs
git commit -m "feat(picker): keyboard selection follows cursor

Tracks the picker viewport height via a Cell stashed by the
renderer so j/k Up/Down can adjust picker_scroll_offset to
keep the selected card on-screen. Extracts CARD_HEIGHT into
ui_picker as the shared source of truth."
```

---

## Task 3: Command palette follows cursor

**Objective:** `Ctrl+P` → typing a common prefix gives a long match list → `Down` past the visible window keeps selected in view.

**Files:**
- Modify: `src/ui_palette.rs` (switch to `ListState`, render_stateful_widget)

**Step 1: Replace plain List with stateful List**

In `src/ui_palette.rs`, change the imports line 2:

```rust
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
```

to:

```rust
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
```

Then at the bottom of `draw_command_palette` (line 113), replace:

```rust
    f.render_widget(List::new(items), layout[2]);
```

with:

```rust
    let mut list_state = ListState::default().with_selected(Some(selected));
    let list = List::new(items);
    f.render_stateful_widget(list, layout[2], &mut list_state);
```

ratatui's `List` with a `ListState` auto-scrolls so `selected` stays in the viewport.

> **Pitfall:** `List` applies its own selection highlight via `highlight_style` when a `ListState` is attached. We're already baking the selection style into each `ListItem` directly in the current code. Don't add `.highlight_style(...)` on the `List` — it would double up. The existing per-item styling is fine.

**Step 2: Verify build**

```bash
cd /home/opus/hermes-tui && cargo build 2>&1 | tail -10
```

Expected: clean build.

**Step 3: Manual smoke test**

1. Launch Kaishi, enter chat, press `Ctrl+P`.
2. The palette has ~15 entries; the visible height caps around 17 rows total (min(20, …)).
3. Press `Down` past the visible bottom → list scrolls, selected stays visible.
4. Press `Up` back → list scrolls back.
5. Typing to filter still works; `Enter` executes the right entry.

**Step 4: Commit**

```bash
cd /home/opus/hermes-tui
git add src/ui_palette.rs
git commit -m "fix(palette): scroll follows selection past viewport

Render the palette list via render_stateful_widget with a
ListState so ratatui auto-scrolls to keep the selected entry
in view. Previously Down past the visible window silently
moved the cursor off-screen."
```

---

## Task 4: File autocomplete popup follows cursor

**Objective:** Typing `@` and navigating through many file matches: the popup scrolls so the selected path stays visible. Also drop the hard-coded `take(8)` render cap.

**Files:**
- Modify: `src/ui_file_popup.rs` (dynamic height, ListState, no take(8))

**Step 1: Replace the render block**

In `src/ui_file_popup.rs`, replace the entire function body after the `let ModalState::FileAutocomplete { … }` destructure (current lines 17–53) with:

```rust
    let area = f.area();
    // Height: cap at 10 rows of entries (borders + list = 12), but also
    // never more than the number of entries we actually have.
    let max_items = entries.len().min(10) as u16;
    let height = (max_items + 2).max(3); // at least 3 for "Scanning…" state
    let width = area.width.min(60);
    let y = area.height.saturating_sub(height + 4); // above input
    let x = 2;
    let rect = Rect::new(x, y, width, height);

    f.render_widget(Clear, rect);

    let title = if *loading { " Scanning… " } else { " Files " };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::ACCENT_ASSISTANT));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let items: Vec<ListItem> = entries
        .iter()
        .map(|path| ListItem::new(format!(" {}", path)))
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .fg(Color::Black)
            .bg(palette::ACCENT_ASSISTANT),
    );
    let mut state = ListState::default().with_selected(Some(*selected));
    f.render_stateful_widget(list, inner, &mut state);
```

And update the imports at line 2 to include `ListState`:

```rust
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};
```

> **Note:** We moved the selection style from per-item `Style` to `List::highlight_style`. This is the idiomatic ratatui pattern and lets ListState drive scroll behavior. Visual output should match.

**Step 2: Verify build**

```bash
cd /home/opus/hermes-tui && cargo build 2>&1 | tail -10
```

Expected: clean build.

**Step 3: Manual smoke test**

```bash
cargo run --release
```

1. Enter a chat session in a directory with many files (e.g., `/home/opus/hermes-tui/src`).
2. Type `@` → popup appears; type a common letter like `a` or leave query empty.
3. With 20+ matches, press Down past row 10 → popup scrolls, selection stays visible.
4. Up scrolls back. Enter inserts the selected path.

**Step 4: Commit**

```bash
cd /home/opus/hermes-tui
git add src/ui_file_popup.rs
git commit -m "fix(file-popup): scroll follows selection

Drop the hardcoded take(8) cap and render via ListState so
the popup auto-scrolls to keep the selected path visible
when navigating past the viewport. Moves selection styling
from per-item to List::highlight_style (idiomatic)."
```

---

## Task 5: Bump to v0.8.3 + roadmap

**Files:**
- Modify: `Cargo.toml` (0.8.2 → 0.8.3)
- Modify: `docs/roadmap-0.6-0.7.md` (new v0.8.3 entry)

**Step 1: Bump version**

In `Cargo.toml`, change `version = "0.8.2"` → `version = "0.8.3"`.

**Step 2: Roadmap entry**

In `docs/roadmap-0.6-0.7.md`, add after the v0.8.2 entry:

```markdown
## v0.8.3 — Scroll fidelity ✓

- ✓ **Picker opens at top**, not bottom — fixed inverted scroll math.
- ✓ **Picker cursor-follow** — j/k past viewport edge scrolls the list.
- ✓ **Palette cursor-follow** — Down past viewport keeps selection visible.
- ✓ **File popup cursor-follow** — drops take(8) cap, scrolls via ListState.
```

**Step 3: Full check**

```bash
cd /home/opus/hermes-tui
cargo build --release 2>&1 | tail -5
cargo test 2>&1 | tail -10
cargo clippy --release -- -D warnings 2>&1 | tail -10
```

Expected: clean build, all tests pass, zero clippy warnings.

**Step 4: Commit and tag**

```bash
cd /home/opus/hermes-tui
git add Cargo.toml Cargo.lock docs/roadmap-0.6-0.7.md
git commit -m "chore: bump to v0.8.3 — scroll fidelity"
git tag v0.8.3
```

**Step 5: Push** (Eva's call — mention before pushing)

```bash
# Only after Eva approves:
git push origin main
git push origin v0.8.3
```

---

## Verification Checklist

- [ ] `cargo test` — all existing tests pass + 3 new picker-scroll tests
- [ ] `cargo build --release` — clean, no warnings
- [ ] `cargo clippy -- -D warnings` — clean
- [ ] Picker opens with `"+ New Session"` selected and visible at top
- [ ] Picker `j`/`k` past viewport edge scrolls the list
- [ ] Picker mouse wheel still scrolls independently
- [ ] Palette Down past viewport bottom keeps selection visible
- [ ] File popup (`@`) with 20+ matches scrolls past the 8th entry
- [ ] File popup Enter still inserts the correct path
- [ ] No visual regressions in selected-item highlight colors

---

## Gotchas log (for executor)

1. **Don't trust patch-tool lint output on Rust files** — spurious "async fn not in Rust 2015" errors appear. Trust `cargo build`.
2. **ratatui `List` + `ListState`:** set selection via `highlight_style` on the `List`, OR bake it into `ListItem` styles — not both, or you get double-highlighting.
3. **`App` borrow during render:** if widening `&App → &mut App` in `ui.rs::draw` fights the borrow checker, fall back to `Cell<u16>` for `picker_viewport_rows` (noted inline in Task 2).
4. **Card height is 3 lines** — verified by reading `render_new_session_card` (1 label + 1 blank) and `render_session_card` (1 title + 1 meta + 1 blank). If card layout ever changes, `CARD_HEIGHT` needs updating in one place.
