---
name: demo
description: Run the LLM auto-player (demo mode) to verify gameplay changes, surface bugs, and generate content. Use after implementing Tauri-facing features or NPC/world changes that the script harness cannot exercise.
disable-model-invocation: false
argument-hint: [turns] [pause_secs]
---

Run the LLM auto-player to prove a feature works in the live Tauri app.

## When to use

Use when `/prove` (headless harness) is not enough — Tauri IPC changes, NPC dialogue quality, frontend streaming behavior, or any feature you need to see running live rather than in JSON output.

## Running

```sh
just demo 2 5    # 5 turns, 2s pause — fast smoke test
just demo 4 20   # 20 turns, 4s pause — content generation / sustained observation
just demo 3      # unlimited turns
```

Capture logs to read the chat transcript:

```sh
just demo 2 5 > /tmp/demo.log 2>&1
grep -E "chat \[|demo turn|WARN" /tmp/demo.log
```

`chat [player]` and `chat [npc]` lines show the full conversation. `demo turn: LLM chose action` shows what the auto-player picked each turn.

## What to verify

- Player actions are single-line natural language — no reasoning preamble or JSON artifacts
- NPC dialogue is Irish-authentic and responds to what the player actually said
- `demo turn` fires each turn — if absent, the LLM call is hanging or failing
- No `waitForFalse timed out` warnings — streaming completed cleanly
- Clock advances between turns — game is not paused

## Common bugs demo surfaces that `/prove` misses

- Streaming freezes — UI input stays disabled, 30s timeout fires in log
- Thinking blocks leaking into player actions — action contains reasoning prose
- NPC says nothing — 429 rate limit or JSON field name typo in response
- Game clock paused throughout — demo auto-resumes, but check debug panel if it persists

## Does not replace

`just check`, `/prove`, `/verify`, or Playwright tests. Demo is observational — it surfaces live behavior, it does not assert.
