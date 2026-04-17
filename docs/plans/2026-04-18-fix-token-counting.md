# Fix Token Counting — Show What Matters

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Make the Kaishi TUI turn summary show meaningful token counts instead of inflated cumulative totals.

**Architecture:** The AIAgent already tracks `last_prompt_tokens` (current context window size from the most recent API call) and per-API-call output tokens. The fix is to expose these in the result dict and have the ACP adapter send them to the TUI.

**Tech Stack:** Python (hermes-agent), Rust (Kaishi TUI)

---

## The Problem

### What the user sees

The Kaishi TUI turn summary line (`── 1.8M in · 567 out · 12s ──`) shows wildly inflated numbers. A session with ~30K actual context reports 1.8M "input tokens" after 6 prompts.

### Root cause

The ACP adapter (`server.py:506-509`) reads `prompt_tokens` and `completion_tokens` from the result dict. These are `session_prompt_tokens` and `session_completion_tokens` — **session-cumulative** totals that `+=` every API call's full input, including re-sent context.

If one turn has 10 tool iterations, each re-sending 30K context, `session_prompt_tokens` grows by 300K for that single turn. The TUI does delta subtraction to get per-turn values, but even the per-turn delta (300K) is misleading — the actual context is only 30K.

### What users want

- **Current context window size** — "how full is my window?" → `last_prompt_tokens` (already tracked)
- **Actual output** — "how much did the model write?" → per-turn output token delta
- **Not** the sum of context re-sent across every API call within a turn

---

## Design

### What to send from the server

The result dict already contains `last_prompt_tokens` (from `context_compressor.last_prompt_tokens`) — the actual prompt size of the most recent API call, i.e. the current context window usage. This is the right "input" number to show.

For output, we want the per-turn delta: total new output tokens generated during this turn. This requires snapshotting `session_output_tokens` at turn start and computing the delta at turn end.

### ACP adapter change

Send `last_prompt_tokens` as `input_tokens` and the per-turn output delta as `output_tokens`.

### TUI change

The TUI's client-side delta subtraction becomes unnecessary — the server sends the right values directly.

---

## Tasks

### Task 1: Add per-turn output delta + expose last_prompt_tokens in result dict

**Objective:** The result dict includes the actual context size and per-turn output tokens.

**Files:**
- Modify: `run_agent.py` (~line 8900 for turn start snapshot, ~line 11383 for result dict)

**Step 1: Snapshot output tokens at turn start**

Find the beginning of `run_conversation()` (after initial setup, before the main loop). Add:

```python
# Per-turn usage tracking
_turn_start_output = self.session_output_tokens
_turn_start_cache_read = self.session_cache_read_tokens
_turn_start_reasoning = self.session_reasoning_tokens
```

**Step 2: Add per-turn fields to result dict**

In the result dict construction (~line 11383), add:

```python
# Per-turn values (what THIS turn actually consumed)
"turn_output_tokens": self.session_output_tokens - _turn_start_output,
"turn_cache_read_tokens": self.session_cache_read_tokens - _turn_start_cache_read,
"turn_reasoning_tokens": self.session_reasoning_tokens - _turn_start_reasoning,
```

Note: `last_prompt_tokens` is already in the result dict at line 11403.

**Step 3: Commit**

```bash
git add run_agent.py
git commit -m "feat: add per-turn output token delta to run_conversation result"
```

### Task 2: Fix ACP adapter to send context size + per-turn output

**Objective:** The TUI receives current context window size as `input_tokens` and per-turn output as `output_tokens`.

**Files:**
- Modify: `acp_adapter/server.py` (~line 505-513)

**Step 1: Update Usage construction**

Replace:

```python
usage = None
if any(result.get(key) is not None for key in ("prompt_tokens", "completion_tokens", "total_tokens")):
    usage = Usage(
        input_tokens=result.get("prompt_tokens", 0),
        output_tokens=result.get("completion_tokens", 0),
        total_tokens=result.get("total_tokens", 0),
        thought_tokens=result.get("reasoning_tokens"),
        cached_read_tokens=result.get("cache_read_tokens"),
    )
```

With:

```python
# Report context window size (not cumulative input) and per-turn output
context_size = result.get("last_prompt_tokens", 0)
turn_out = result.get("turn_output_tokens", 0)
usage = None
if context_size or turn_out:
    usage = Usage(
        input_tokens=context_size,
        output_tokens=turn_out,
        total_tokens=context_size + turn_out,
        thought_tokens=result.get("turn_reasoning_tokens"),
        cached_read_tokens=result.get("turn_cache_read_tokens"),
    )
```

**Step 2: Verify via TUI**

Connect Kaishi, send a multi-tool prompt. Turn summary should show ~30K input (context size), not 300K+ (re-sent total).

**Step 3: Commit**

```bash
git add acp_adapter/server.py
git commit -m "fix: send context window size and per-turn output to ACP clients"
```

### Task 3: Simplify Kaishi token display (now unnecessary delta math)

**Objective:** Remove client-side delta subtraction since server sends correct values.

**Files:**
- Modify: `~/hermes-tui/src/app.rs`

**Step 1: Simplify handle_prompt_done**

Remove the delta subtraction logic — use `u.input_tokens` and `u.output_tokens` directly since the server now sends context size and per-turn output.

**Step 2: Remove `total_input_tokens` and `total_output_tokens` from App struct**

These tracked running totals for client-side delta computation. No longer needed.

**Step 3: Commit**

```bash
cd ~/hermes-tui
git add src/app.rs
git commit -m "fix: use server-provided context size and per-turn output directly"
```

---

## Verification

After all tasks:

1. TUI turn summary shows `── 30k in · 2.1k out · 12s ──` (context size, actual output)
2. Numbers match intuition — "in" is the context window, "out" is what the model wrote this turn
3. Cost estimation is unaffected (cumulative counters remain for internal use)

## Notes

- Cumulative counters are NOT removed — still needed for cost estimation and `/usage`.
- `last_prompt_tokens` comes from the actual API response, not estimation — it's the most accurate context size metric.
- The "input" label in the TUI now means "context window size" not "tokens consumed." This is a semantic change but matches what users care about.
- `/usage` in the CLI is a separate concern — it still shows cumulative values. Can be improved later if desired.
