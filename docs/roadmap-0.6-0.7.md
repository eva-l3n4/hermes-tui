# Kaishi Roadmap: v0.6.0 – v0.8.0+

Feature parity targets against Claude Code, OpenCode, and Hermes CLI.

## v0.6.0 — Polish & Visual Leap ✓

All shipped. See git tag `v0.6.0`.

1. ✓ Terminal bell on turn completion
2. ✓ Ctrl+L clear screen
3. ✓ Word-level cursor movement (Alt+Left/Right)
4. ✓ Tab completion for /commands
5. ✓ Link rendering in inline markdown
6. ✓ /compact feedback
7. ✓ Syntax highlighting in code blocks (syntect, base16-eighties.dark)

---

## v0.7.0 — Power User Features

### Quick Wins (trivial–small)

8. **`!` shell escape prefix**
   Type `!npm test` to run a command directly without the AI. Detect `!`
   prefix in input, spawn shell subprocess, display output as a system
   message. Essential for quick checks mid-conversation.

9. **Permission mode indicator + toggle**
   Show current approval mode in status bar. Keybinding (e.g., Shift+Tab)
   to cycle Normal → Auto-Accept → Plan. Kaishi has the approval modal
   but no way to toggle YOLO mode from within the TUI.

10. **Thinking toggle (Alt+T)**
    Quick toggle for extended thinking on/off. Distinct from Ctrl+O
    (expand/collapse thinking *display*) — this controls whether thinking
    is *requested* on subsequent prompts.

11. **`/compact` with focus hint**
    Support `/compact focus on auth logic` — targeted compaction that
    preserves specific context. Pass focus text as parameter to ACP
    compact call. Nearly free if ACP supports it.

### Medium Effort

12. **Context window health indicator**
    Show context fill level in the status bar (e.g., colored bar
    `[████████░░░░] 67% · 134k/200k`). Requires ACP to expose context
    usage — check `PromptDone`, `session/update`, or
    `_hermes/get_session_info`. Warn at 70% and 85% thresholds.

13. **Diff view for file edits**
    When the agent writes/patches files, show a colored unified diff
    inline rather than just "✓ patched file.rs". Syntect infrastructure
    is already in place for highlighting. Parse tool results for diff
    content, render with +/- coloring (green/red).

14. **Session export (/save)**
    Dump current conversation as markdown to a file. Iterate
    `self.messages`, format by role, write to
    `~/kaishi-export-{timestamp}.md` or a user-specified path.
    Hermes CLI has this as `/save`.

15. **Markdown table rendering**
    Detect pipe-delimited tables and render with aligned columns.
    Currently passes through as raw text. Even basic column alignment
    would be a big readability win.

16. **External editor for input (Ctrl+G)**
    Open `$EDITOR` with the current prompt text, return saved content as
    input. Standard in shells and Claude Code. Essential for long/complex
    prompts. Needs temporary file + subprocess + TUI suspend/restore.

17. **Ctrl+R reverse history search**
    Incremental search through input history. Complements existing
    Up/Down history navigation. Type to filter, Enter to select.

### Larger Features (scope carefully)

18. **Effort / reasoning control**
    `/effort low|medium|high` or keybinding. Store as session-local
    setting, pass as param on `session/prompt`. Depends on ACP
    supporting effort params.

19. **Session deletion from picker**
    `d` or `Delete` key on a session in the picker to delete it. Needs
    ACP method (`session/delete` or `_hermes/delete_session`). Confirm
    with a mini-modal before deleting.

20. **Search within conversation (Ctrl+F)**
    Search overlay: input bar, highlight matches in messages, `n`/`N` to
    jump between. Needs search state struct, match index tracking, and
    scroll-to-match logic.

21. **Image paste / attach**
    Ctrl+V or `/image <path>` to include images in prompts. ACP supports
    content blocks with image data. File path is the safer starting
    point — terminal image paste varies by emulator.

22. **File path autocomplete (`@` prefix)**
    Type `@` to trigger filesystem autocomplete with popup/dropdown.
    Needs async directory scanning, filtered popup widget, Enter/Tab to
    confirm. Significant UI work — standalone mini-project.

---

## v0.8.0+ — Aspirational

23. **Agent team display**
    When Hermes spawns subagents, show nested agent activity inline —
    expandable/collapsible per-agent sections with their own tool call
    summaries. Similar to Claude Code's tmux teammate view but
    integrated into the TUI.

24. **`/rewind` — undo last turn**
    Revert to previous conversation checkpoint. Would need ACP support
    or local message stack manipulation. Claude Code: Esc Esc.

25. **Side questions (`/btw`)**
    Ask something without it counting toward context cost. Needs ACP
    support for ephemeral prompts.

26. **`#` quick memory**
    Type `# Always use 2-space indent` to save to project/session
    memory. Needs local notes file or Hermes memory integration.

27. **Vim keybindings toggle**
    Normal/Insert mode state machine for input. Config flag to enable.

28. **Theme / skin support**
    `--theme` flag or config for different color palettes. Current
    terminal-native approach works well with Catppuccin remapping, but
    explicit light-terminal support would help others.

29. **Notification badges on session picker**
    Show which sessions have unread/new activity. Relevant for
    background tasks or multi-session workflows.

---

## Current State (v0.6.0)

For reference, what's already shipped:

- Markdown rendering (headings, bold, italic, inline code, fenced code
  blocks with box-drawing, bullets, numbered lists, blockquotes, HR)
- Syntax highlighting in code blocks (syntect, 200+ languages)
- Animated spinner with shimmer, stall detection, phase word bank
- Tool call smart summaries (20+ tools recognized)
- Turn completion divider with per-turn token deltas + elapsed time
- Thinking collapse/expand (Ctrl+O)
- Status bar: model, active tool, cumulative tokens, CWD
- Approval modal (Enter/Esc/j/k)
- Session picker with title, timestamps, source badges, message counts
- Lazy history pagination (scroll-up to load more)
- Slash command passthrough to ACP server
- Tab completion for /commands (bash-style cycle-through)
- Local commands: /quit /clear /new /verbose /usage /help /title /reset
- Input: multiline (Ctrl+J, Shift+Enter), cursor movement
  (Ctrl+A/E/W/K, Alt+Left/Right), input history (Up/Down), placeholder
- Terminal bell on turn completion
- Ctrl+L clear screen
- Link rendering in markdown ([text](url) → underlined)
- /compact feedback ("Compressing context…")
- Mouse scroll, keyboard scroll (PgUp/PgDn, Ctrl+U)
- ACP reconnect on crash (Disconnected screen → Enter to respawn)
- CLI args: --profile, --cwd, --session, --help
- Word-level wrapping with continuation indent
- 4266 LOC across 7 source files
