# Fix Token Counting — Show What Matters

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Replace misleading session-cumulative token counters with useful per-turn and context-aware metrics.

**Architecture:** The AIAgent already tracks `last_prompt_tokens` (current context size) and per-API-call usage including cache hits. The fix is to expose these properly instead of only summing everything into opaque cumulative counters.

**Tech Stack:** Python (hermes-agent), Rust (Kaishi TUI)

---

## The Problem

### What's broken

1. **`session_input_tokens` and `session_prompt_tokens` are misleading.** They `+=` every API call's full input, including re-sent context. A 6-prompt session with tool calls accumulates 3M+ "input tokens" when the actual context is ~30K. This looks like a billing disaster but isn't — it's just bad accounting.

2. **Two counter sets diverge.** `session_prompt_tokens` (OpenAI-style, from raw API response) ≠ `session_input_tokens` (canonical, from normalized usage). The ACP adapter reports one; `/usage` shows the other. Hence 1.8M vs 3.1M for the same session.

3. **No per-turn visibility.** After each response, there's no way to see "this turn used X input, Y output" — only the ever-growing cumulative total.

4. **Cache hits are invisible.** Prompt caching means most re-sent context is essentially free. The counters don't distinguish cached vs. uncached reads.

### What users want

- **Current context size** — "how full is my context window?" (already tracked as `last_prompt_tokens`)
- **Per-turn usage** — "how much did this response cost?"
- **Cache efficiency** — "how much was a cache hit?"
- **Session totals that make sense** — total *new* tokens, not total *re-sent* tokens

---

## Design

### New per-turn tracking in AIAgent (run_agent.py)

Track per-turn deltas alongside the existing cumulative counters. At the start of each `run_conversation()` call, snapshot the current cumulative values. At the end, the delta is the per-turn usage.

```python
# At start of run_conversation():
_turn_start_input = self.session_input_tokens
_turn_start_output = self.session_output_tokens
_turn_start_cache_read = self.session_cache_read_tokens
_turn_start_cache_write = self.session_cache_write_tokens
_turn_start_reasoning = self.session_reasoning_tokens
_turn_start_api_calls = self.session_api_calls

# In the result dict at end:
result["turn_input_tokens"] = self.session_input_tokens - _turn_start_input
result["turn_output_tokens"] = self.session_output_tokens - _turn_start_output
result["turn_cache_read_tokens"] = self.session_cache_read_tokens - _turn_start_cache_read
result["turn_cache_write_tokens"] = self.session_cache_write_tokens - _turn_start_cache_write
result["turn_reasoning_tokens"] = self.session_reasoning_tokens - _turn_start_reasoning
result["turn_api_calls"] = self.session_api_calls - _turn_start_api_calls
```

### Unify counter sets

Eliminate the confusing dual-counter situation. The result dict should use ONE set of names consistently. Recommendation: keep the canonical names (`input_tokens`, `output_tokens`) and drop the OpenAI-style aliases (`prompt_tokens`, `completion_tokens`) from the result dict. The raw values are still accumulated internally for cost estimation, but consumers see one consistent set.

### ACP adapter fix (server.py)

The ACP adapter currently reads `prompt_tokens` and `completion_tokens` from the result — the OpenAI-style cumulative values. Change to read the per-turn values:

```python
# Before (cumulative, wrong counter set):
usage = Usage(
    input_tokens=result.get("prompt_tokens", 0),
    output_tokens=result.get("completion_tokens", 0),
    ...
)

# After (per-turn, correct counter set):
usage = Usage(
    input_tokens=result.get("turn_input_tokens", 0),
    output_tokens=result.get("turn_output_tokens", 0),
    total_tokens=result.get("turn_input_tokens", 0) + result.get("turn_output_tokens", 0),
    thought_tokens=result.get("turn_reasoning_tokens"),
    cached_read_tokens=result.get("turn_cache_read_tokens"),
)
```

### CLI /usage display fix

Show useful breakdowns instead of opaque totals:

```
  📊 Session Token Usage
  ────────────────────────────────────
  Model:                     azure/claude-opus-4-7
  Current context:           28,431 / 200,000 (14%)
  
  This session (6 turns, 47 API calls):
    Input tokens (new):          12,340
    Input tokens (cached):      156,200
    Output tokens:                8,910
    Reasoning tokens:             2,100
  
  Estimated cost:              ~$0.0842
  ────────────────────────────────────
```

The key insight: "input tokens (new)" = `session_input_tokens - session_cache_read_tokens`. That's the actual non-cached work. "Input tokens (cached)" = `session_cache_read_tokens`. Together they show the real picture.

### Status bar update

The status bar already shows `context_tokens/context_length (%)`. No change needed — this is already the most useful number.

---

## Tasks

### Task 1: Add per-turn delta tracking to run_conversation()

**Objective:** Snapshot cumulative counters at turn start, compute deltas at turn end.

**Files:**
- Modify: `run_agent.py` (~line 8900 for turn start, ~line 11395 for result dict)

**Step 1: Snapshot at turn start**

Find the beginning of `run_conversation()` (after the method signature and initial setup, before the main loop). Add:

```python
# Per-turn usage tracking: snapshot cumulative values at turn start
_turn_start_input = self.session_input_tokens
_turn_start_output = self.session_output_tokens
_turn_start_cache_read = self.session_cache_read_tokens
_turn_start_cache_write = self.session_cache_write_tokens
_turn_start_reasoning = self.session_reasoning_tokens
_turn_start_api_calls = self.session_api_calls
_turn_start_cost = self.session_estimated_cost_usd
```

**Step 2: Add per-turn deltas to result dict**

In the result dict construction (~line 11383), add after the existing cumulative fields:

```python
# Per-turn deltas (what THIS turn consumed)
"turn_input_tokens": self.session_input_tokens - _turn_start_input,
"turn_output_tokens": self.session_output_tokens - _turn_start_output,
"turn_cache_read_tokens": self.session_cache_read_tokens - _turn_start_cache_read,
"turn_cache_write_tokens": self.session_cache_write_tokens - _turn_start_cache_write,
"turn_reasoning_tokens": self.session_reasoning_tokens - _turn_start_reasoning,
"turn_api_calls": self.session_api_calls - _turn_start_api_calls,
"turn_estimated_cost_usd": self.session_estimated_cost_usd - _turn_start_cost,
```

**Step 3: Verify**

Run a test prompt and check the result dict contains both cumulative and per-turn values. Per-turn values should be smaller and make intuitive sense.

**Step 4: Commit**

```bash
git add run_agent.py
git commit -m "feat: add per-turn token delta tracking to run_conversation result"
```

### Task 2: Fix ACP adapter to report per-turn usage

**Objective:** The TUI receives per-turn token counts instead of session-cumulative.

**Files:**
- Modify: `acp_adapter/server.py` (~line 505-513)

**Step 1: Update Usage construction**

Replace the current Usage construction:

```python
# Before:
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
# After — report per-turn deltas:
usage = None
turn_in = result.get("turn_input_tokens", 0)
turn_out = result.get("turn_output_tokens", 0)
if turn_in or turn_out:
    usage = Usage(
        input_tokens=turn_in,
        output_tokens=turn_out,
        total_tokens=turn_in + turn_out,
        thought_tokens=result.get("turn_reasoning_tokens"),
        cached_read_tokens=result.get("turn_cache_read_tokens"),
    )
```

**Step 2: Verify via TUI**

Connect Kaishi, send a multi-tool prompt, check that the turn summary shows reasonable per-turn numbers (not millions).

**Step 3: Commit**

```bash
git add acp_adapter/server.py
git commit -m "fix: report per-turn token deltas instead of session-cumulative in ACP"
```

### Task 3: Fix /usage display in CLI

**Objective:** Show meaningful breakdowns: context size, new vs cached input, per-turn stats.

**Files:**
- Modify: `cli.py` (~line 6517-6570, the `_show_usage` method)

**Step 1: Rewrite the display section**

Replace the raw counter dump with a structured display that separates:
- Current context window status (from `compressor.last_prompt_tokens` / `compressor.context_length`)
- Session totals with cache breakdown: new input = `session_input_tokens - session_cache_read_tokens`, cached = `session_cache_read_tokens`
- API call count, session duration, cost

```python
# ── Session token usage ─────────────────────────────────────
input_tokens = getattr(agent, "session_input_tokens", 0) or 0
output_tokens = getattr(agent, "session_output_tokens", 0) or 0
cache_read = getattr(agent, "session_cache_read_tokens", 0) or 0
cache_write = getattr(agent, "session_cache_write_tokens", 0) or 0
reasoning = getattr(agent, "session_reasoning_tokens", 0) or 0
calls = getattr(agent, "session_api_calls", 0) or 0

# New (non-cached) input = total input minus cache reads
new_input = max(0, input_tokens - cache_read)

compressor = agent.context_compressor
last_prompt = compressor.last_prompt_tokens
ctx_len = compressor.context_length
pct = min(100, (last_prompt / ctx_len * 100)) if ctx_len else 0

print("  📊 Session Token Usage")
print(f"  {'─' * 40}")
print(f"  Model:                     {agent.model}")
print(f"  Current context:           {last_prompt:>10,} / {ctx_len:,} ({pct:.0f}%)")
print(f"  {'─' * 40}")
print(f"  Input tokens (new):        {new_input:>10,}")
print(f"  Input tokens (cached):     {cache_read:>10,}")
print(f"  Output tokens:             {output_tokens:>10,}")
if reasoning:
    print(f"  Reasoning tokens:          {reasoning:>10,}")
print(f"  API calls:                 {calls:>10,}")
# ... cost display unchanged ...
```

**Step 2: Verify**

Run `/usage` in a session with a few turns. Numbers should make sense — "new input" should be small (actual new content), "cached" should be large (re-sent context that hit cache).

**Step 3: Commit**

```bash
git add cli.py
git commit -m "fix: /usage shows new vs cached input breakdown, current context size"
```

### Task 4: Remove Kaishi per-turn delta subtraction (now unnecessary)

**Objective:** Since the server now sends per-turn values, the TUI no longer needs to compute deltas client-side.

**Files:**
- Modify: `~/hermes-tui/src/app.rs` (remove `total_input_tokens` / `total_output_tokens` tracking)

**Step 1: Simplify handle_prompt_done**

The `handle_prompt_done` currently subtracts running totals to get per-turn values. Since the server now sends per-turn values directly, simplify to use them as-is:

```rust
// Before: computing deltas client-side
let turn_in = u.input_tokens.saturating_sub(self.total_input_tokens);
let turn_out = u.output_tokens.saturating_sub(self.total_output_tokens);
self.total_input_tokens = u.input_tokens;
self.total_output_tokens = u.output_tokens;

// After: server sends per-turn values directly
// Just use u.input_tokens and u.output_tokens as-is
```

**Step 2: Remove unused fields**

Remove `total_input_tokens` and `total_output_tokens` from the App struct if no longer needed. Keep `prompt_count` for session stats.

**Step 3: Verify**

Build and test with the updated server. Turn summaries should show reasonable per-turn numbers.

**Step 4: Commit**

```bash
cd ~/hermes-tui
git add src/app.rs
git commit -m "fix: use server-provided per-turn token values directly"
```

---

## Verification

After all tasks:

1. `/usage` shows current context size prominently, with new vs cached breakdown
2. End-of-turn display (CLI and TUI) shows per-turn token counts that match intuition (~30K input for a normal turn, not millions)
3. The two different consumer paths (CLI `/usage` and ACP→TUI) show consistent numbers
4. Cost estimation remains accurate (it already uses the cumulative values internally)

## Notes

- The cumulative counters are NOT removed — they're still needed for cost estimation and the session-level total. We're adding per-turn deltas alongside them.
- `last_prompt_tokens` (context compressor) is already the best "how big is my context" metric — it comes from the actual API response, not estimation.
- Cache hit ratio = `session_cache_read_tokens / session_input_tokens` — useful for understanding caching efficiency.
