# Hermes CLI Feature Parity — Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Reach feature parity with the Hermes CLI's in-session experience — all slash commands the ACP adapter supports should work, the TUI should display session metadata properly, and unknown `/commands` should forward to the server instead of being silently swallowed.

**Architecture:** Most missing features require zero new server-side work — the ACP adapter already handles `/model <name>`, `/tools`, `/context`, `/reset`, `/compact`, `/version`, `/title`, `/yolo`. The TUI just needs to stop intercepting unrecognized commands locally and instead send them as prompts via ACP. A few features (usage tracking, session title display) need small client-side additions.

**Tech Stack:** Rust (ratatui, crossterm, tokio, serde_json), Python (ACP adapter — read-only for this plan)

**Repo:** `~/hermes-tui`

---

## Current State

### TUI slash commands (local):
- `/quit`, `/exit`, `/q` — exit
- `/clear` — clear screen
- `/new` — new session
- `/model` — show model (no args, display only)
- `/verbose`, `/v` — toggle verbose mode
- `/help`, `/h`, `/?` — show help

### ACP adapter slash commands (server-side, already implemented):
- `/help` — list commands
- `/model [name]` — show or **switch** model
- `/tools` — list available tools
- `/context` — conversation stats (message counts by role)
- `/reset` — clear conversation history
- `/compact` — compress context
- `/version` — show Hermes version
- `/title [name]` — set or show session title
- `/yolo` — toggle approval bypass

### Gap: 6 server commands unreachable from TUI, `/model` only partially works, unknown commands silently dropped.

---

## Task 1: Remove local `/model` override, let server handle it

**Objective:** The TUI's local `/model` handler only displays the model name. The server's `/model [name]` both displays AND switches models. Remove the local override so `/model` (with or without args) goes to the server. Also verify that all other server commands already work via the existing forwarding path.

**Context:** The TUI already forwards unrecognized commands to the ACP server (lines 370-381 in `app.rs`). The `_ => false` catch-all returns false, causing the caller to send the text as a prompt. So `/tools`, `/context`, `/compact`, `/version`, `/title`, `/yolo`, `/reset` should already work. The only problem is `/model` — the local handler intercepts it before it can reach the server.

**Files:**
- Modify: `src/app.rs:673-680` (remove local `/model` handler)

**Steps:**

1. **Delete** the `/model` arm from `handle_local_command`:
   ```rust
   // DELETE this entire arm:
   "/model" => {
       if self.model_name.is_empty() {
           self.sys_msg("Model: (unknown — set via ACP initialize)");
       } else {
           self.sys_msg(format!("Model: {}", self.model_name));
       }
       true
   }
   ```

2. **UX consideration:** When slash commands are forwarded as prompts, they appear as a user message bubble (e.g., ❯ `/tools`) and the status briefly shows "Thinking…" before the server responds. This is acceptable for now — the response comes back nearly instantly. A future refinement could suppress the user bubble for recognized server commands, or add a `SlashCommandResponse` event type. Not blocking for this plan.

3. When the server processes `/model <name>` and switches models, we should update `self.model_name` to reflect the new model. The server response text is "Model switched to: X\nProvider: Y". We could parse this, but a cleaner approach: update `model_name` when we see a successful model switch response. For now, the simplest path is to let the server response display as a system message — the status bar will still show the old model name until the next `PromptDone` (which carries model info). This is acceptable for v1; refinement can come later.

4. Verify: `cargo clippy --all-targets -- -D warnings`

5. **Test all server commands** (these should already work via forwarding, but verify):
   - `/model` → server shows "Current model: ... Provider: ..."
   - `/model anthropic/claude-sonnet-4` → server switches and confirms
   - `/tools` → list of available tools
   - `/context` → message counts by role
   - `/compact` → compresses context
   - `/version` → "Hermes Agent vX.Y.Z"
   - `/title my-session` → sets title
   - `/yolo` → toggles approval mode
   - `/reset` → clears server history

5. Commit:
```bash
git add src/app.rs
git commit -m "feat: remove local /model override — server handles display + switching

All server-side commands (/model, /tools, /context, /reset,
/compact, /version, /title, /yolo) now reach the ACP adapter
via the existing command-forwarding path."
```

---

## Task 2: Update `/help` to show all available commands

**Objective:** The TUI's `/help` currently lists only 5 local commands. Update it to show both local commands and server-side commands, clearly grouped.

**Files:**
- Modify: `src/app.rs:690-707` (`/help` handler)

**Steps:**

1. Replace the `/help` handler content with a comprehensive command list:

   ```rust
   "/help" | "/h" | "/?" => {
       self.sys_msg(
           "Local commands:\n\
            \n\
            /new             Start a new session\n\
            /clear           Clear the screen\n\
            /verbose         Toggle tool call details\n\
            /quit            Exit (also Ctrl+D)\n\
            \n\
            Server commands:\n\
            \n\
            /model [name]    Show or switch model\n\
            /tools           List available tools\n\
            /context         Show conversation stats\n\
            /compact         Compress conversation context\n\
            /reset           Clear conversation history\n\
            /title [name]    Set or show session title\n\
            /version         Show Hermes version\n\
            /yolo            Toggle approval bypass\n\
            \n\
            Keys:\n\
            \n\
            Scroll: PgUp/PgDn, mouse wheel\n\
            Cancel: Ctrl+C during generation\n\
            Newline: Ctrl+J\n\
            History: Up/Down arrows\n\
            \n\
            Unrecognized /commands are forwarded to the server."
               .to_string(),
       );
       true
   }
   ```

2. Verify: `cargo clippy --all-targets -- -D warnings`

3. Commit:
```bash
git add src/app.rs
git commit -m "feat: comprehensive /help — local, server, and key bindings"
```

---

## Task 3: Display session title in status bar

**Objective:** When a session has a title (set via `/title` or by the server), show it in the status bar alongside the model name.

**Files:**
- Modify: `src/app.rs` (add `session_title` update from server responses)
- Modify: `src/ui.rs` (status bar rendering)

**Steps:**

1. The `App` struct already has `session_title: Option<String>`. Check if it's being populated from session data. Search for where `session_title` is set:
   - On session resume, the picker's `SessionInfo` might carry a title
   - On `/title my-name` the server responds with text but doesn't update `session_title`

2. Add title extraction from `/title` server responses. When the server responds to a `/title` command, the response text starts with "Session title set: ". We could parse this, but a cleaner approach is to update `session_title` whenever we see a title in session data.

   For now, the simplest approach: when the user sends `/title <name>`, also set `session_title` locally before forwarding:
   ```rust
   // In handle_local_command, add a new arm BEFORE the catch-all:
   cmd if cmd == "/title" => {
       // Extract the title argument if present
       if let Some(title) = parts.get(1) {
           self.session_title = Some(title.to_string());
       }
       // Don't return true — let it fall through to the server
       false
   }
   ```

   Wait — this won't work with the current match structure since we need to return `false` to forward AND still capture the arg. Instead, handle it as a pre-processing step before the match:

   ```rust
   // Before the match block in handle_local_command:
   if cmd == "/title" {
       if let Some(title) = parts.get(1) {
           self.session_title = Some(title.to_string());
       }
       // Fall through to server
       return false;
   }
   ```

3. Update the status bar in `src/ui.rs` to show the title. Find the status bar rendering section and add the title:
   ```rust
   // In the status bar, after model name:
   // Format: "model-name • session-title" or just "model-name"
   let status_left = if let Some(title) = &app.session_title {
       format!("{} • {}", app.model_name, title)
   } else {
       app.model_name.clone()
   };
   ```

4. Also populate `session_title` from `SessionInfo` when resuming a session. In the session resume handler, check if the selected session has a title and set it.

5. Verify: `cargo clippy --all-targets -- -D warnings`

6. Test:
   - Start a session → status bar shows model name only
   - `/title my-project` → status bar updates to "model • my-project"
   - Resume a titled session → title appears in status bar

7. Commit:
```bash
git add src/app.rs src/ui.rs
git commit -m "feat: display session title in status bar"
```

---

## Task 4: Track and display token usage

**Objective:** Track cumulative token usage from `PromptDone` events and display via `/usage` command. Show running totals in the status bar.

**Files:**
- Modify: `src/app.rs` (add usage tracking fields + `/usage` command)
- Modify: `src/ui.rs` (status bar — optional token count)

**Steps:**

1. Add usage tracking fields to `App`:
   ```rust
   // Token tracking
   pub total_input_tokens: u64,
   pub total_output_tokens: u64,
   pub prompt_count: u32,
   ```
   Initialize all to 0 in `App::new()`.

2. Update the `PromptDone` handler to accumulate tokens:
   ```rust
   // In handle_prompt_done or wherever PromptDone events are processed:
   if let Some(usage) = &usage {
       self.total_input_tokens += usage.input_tokens;
       self.total_output_tokens += usage.output_tokens;
       self.prompt_count += 1;
   }
   ```

3. Add `/usage` as a local command:
   ```rust
   "/usage" | "/u" => {
       let total = self.total_input_tokens + self.total_output_tokens;
       self.sys_msg(format!(
           "Session usage ({} prompts):\n  Input:  {} tokens\n  Output: {} tokens\n  Total:  {} tokens",
           self.prompt_count,
           self.total_input_tokens,
           self.total_output_tokens,
           total,
       ));
       true
   }
   ```

4. Optionally show token count in status bar (right-aligned):
   ```rust
   // Right side of status bar:
   let total_tokens = app.total_input_tokens + app.total_output_tokens;
   let status_right = if total_tokens > 0 {
       format!("{}↑ {}↓", app.total_input_tokens, app.total_output_tokens)
   } else {
       String::new()
   };
   ```

5. Update `/help` to include `/usage`.

6. Verify: `cargo clippy --all-targets -- -D warnings`

7. Test:
   - Send a prompt → PromptDone should update counters
   - `/usage` → shows accumulated tokens
   - Status bar → shows token counts after first prompt

8. Commit:
```bash
git add src/app.rs src/ui.rs
git commit -m "feat: track and display token usage (/usage, status bar)"
```

---

## Task 5: Investigate D3 — extended thinking not appearing

**Objective:** Opus 4.7 is ET-only, but reasoning content sometimes doesn't appear in the TUI. Diagnose whether the issue is in the ACP adapter callback, the wire format, or the TUI's event parsing.

**Files:**
- Read: `~/.hermes/hermes-agent/acp_adapter/server.py` (callback wiring)
- Read: `src/acp.rs` (event parsing for `agent_thought_chunk`)
- Read: `src/app.rs` (thought handling)

**Steps:**

1. **Verify the callback chain:**
   - `server.py:444` sets `agent.reasoning_callback = thinking_cb`
   - Trace `make_thinking_cb` to see what it sends on the wire
   - Confirm the wire event method name and payload format

2. **Add diagnostic logging to the TUI:**
   Add a temporary debug log file that captures all incoming JSON-RPC messages:
   ```rust
   // In the ACP reader task, before dispatch:
   if std::env::var("HERMES_TUI_DEBUG").is_ok() {
       use std::io::Write;
       if let Ok(mut f) = std::fs::OpenOptions::new()
           .create(true).append(true)
           .open("/tmp/hermes-tui-debug.jsonl")
       {
           let _ = writeln!(f, "{}", line);
       }
   }
   ```

3. **Run with debug logging:**
   ```bash
   HERMES_TUI_DEBUG=1 cargo run
   ```
   Send a prompt that should trigger reasoning, then examine `/tmp/hermes-tui-debug.jsonl`:
   - Look for `agent_thought_chunk` notifications
   - Check if they exist but are being dropped by the parser
   - Check the method name format (could be `session_update` with nested `agent_thought_chunk`, or a top-level notification)

4. **Check the reasoning callback in the ACP adapter:**
   ```bash
   # In server.py, find make_thinking_cb:
   grep -A 30 "def make_thinking_cb" ~/.hermes/hermes-agent/acp_adapter/server.py
   ```
   Verify it actually sends a wire event. Some models return reasoning in the response object but don't fire the callback.

5. **Check if the model routing affects it:**
   Since this goes through Headroom → Azure AI Foundry / Fireworks, verify that the response format preserves reasoning content. Some proxy layers strip `thinking` blocks.

6. Document findings. If the issue is:
   - **Callback not firing:** Fix in `server.py` — may need to check `response.reasoning` in addition to the callback
   - **Wire event format mismatch:** Fix parsing in `acp.rs`
   - **Proxy stripping thinking blocks:** Fix in model routing config
   - **Model doesn't send reasoning for short queries:** Document as expected behavior

7. Remove the debug logging (or gate it properly).

8. Commit:
```bash
git add src/acp.rs
git commit -m "fix: diagnose and resolve extended thinking display (D3)"
```

---

## Task 6: Clean up `/new` vs `/reset` semantics

**Objective:** The TUI has `/new` (creates a brand new session) but the server has `/reset` (clears history within the same session). These are different operations. Support both.

**Files:**
- Modify: `src/app.rs` (add `/reset` pre-processing)

**Steps:**

1. `/reset` should forward to the server AND clear the TUI's local message display:
   ```rust
   // Add before the catch-all in handle_local_command:
   "/reset" => {
       // Clear local display
       self.messages.clear();
       self.line_cache.clear();
       self.scroll_offset = 0;
       self.total_input_tokens = 0;
       self.total_output_tokens = 0;
       self.prompt_count = 0;
       // Fall through to server to clear server-side history
       return false;
   }
   ```

   Wait — returning `false` means the caller sends it as a prompt. But we also want to clear the local state. The problem is `handle_local_command` returns a bool and doesn't have a "handled locally AND forward" option.

   Better approach: do the local cleanup, then explicitly forward:
   ```rust
   "/reset" => {
       self.messages.clear();
       self.line_cache.clear();
       self.scroll_offset = 0;
       self.total_input_tokens = 0;
       self.total_output_tokens = 0;
       self.prompt_count = 0;
       // Don't return true — let it forward to server
       false
   }
   ```

   This works because `false` means "not fully handled locally" → caller sends as prompt → server handles `/reset` → response comes back as system message.

2. Update `/help` to distinguish the two:
   ```
   /new             Start a new session (new session ID)
   /reset           Clear history (same session)
   ```

3. Verify: `cargo clippy --all-targets -- -D warnings`

4. Test:
   - `/reset` → clears screen AND server responds "Conversation history cleared."
   - `/new` → creates new session ID, clears screen

5. Commit:
```bash
git add src/app.rs
git commit -m "feat: /reset clears local + server history (distinct from /new)"
```

---

## Task Summary

| # | Task | Type | Complexity |
|---|------|------|------------|
| 1 | Forward unrecognized slash commands to server | Architecture | Low — verify existing flow, remove `/model` override |
| 2 | Update `/help` with all commands | Polish | Trivial |
| 3 | Session title in status bar | Feature | Low |
| 4 | Token usage tracking + `/usage` | Feature | Low-Medium |
| 5 | Investigate D3 (extended thinking) | Debug | Medium — diagnostic, outcome unknown |
| 6 | `/reset` vs `/new` semantics | Feature | Low |

**Estimated total:** ~2-3 hours. Tasks 1-2 are the biggest bang-for-buck. Task 5 is investigative.

**Dependency graph:**
```
Task 1 (forward commands) ──┐
Task 2 (help text) ─────────┤── independent, any order
Task 3 (title in status) ───┤   (but Task 2 should update
Task 4 (usage tracking) ────┤    after Tasks 3, 4, 6 add
Task 6 (/reset semantics) ──┘    new commands)

Task 5 (D3 investigation) ──── independent
```

Tasks 1-4 and 6 are independent and can be done in parallel by subagents. Task 2's help text should be finalized last (after all new commands are added). Task 5 is standalone investigation.
