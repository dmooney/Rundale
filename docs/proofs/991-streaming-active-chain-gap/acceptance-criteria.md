# Acceptance Criteria: 991-streaming-active-chain-gap

## Task

Issue #991: in `just demo 2 5` on slow inference, the demo loop fires the
next player turn before the prior NPC dialogue stream completes. Five
player turns can land in ~18 seconds with zero `chat [npc]` lines logged
because the auto-player's `waitForFalse(streamingActive)` resolves during
a transient `streamingActive=false` window between conversation turns.

Root cause (traced from current `main`, not the #990 hypothesis):
`run_npc_turn` spawns/cancels its own loading animation per NPC turn, so
within a single `handle_npc_conversation` chain — addressed NPCs in
phase 1, then the autonomous follow-up chain in phase 2 — every per-turn
cancel emits a `loading {active:false}` event. The frontend's `onLoading`
handler in `+page.svelte` then flips `streamingActive` to false because
`pendingTurnCount === 0 && !hasPendingEndHints()` (no `stream-end` has
fired yet, but the pump has already drained the prior turn's tokens).
The autonomous chain's `spawn_loading` is `|| None`, so no compensating
`loading=true` re-arrives until the next phase-1 NPC starts — and there
may be no such NPC.

The visible difference after this change:

- During a multi-NPC conversation chain, `streamingActive` stays true
  from the first `loading=true` until the chain's `stream-end` fires
  and the token pump has drained. It does not flicker false between
  per-turn `loading=false` events.
- The demo auto-player consequently waits for the full chain (all
  addressed NPCs + autonomous follow-ups) before firing its next turn.
- `just demo 2 N` produces approximately `N` `chat [npc]` lines on a
  slow inference backend, not zero.

## Criteria

- The frontend stream-manager exposes a `chainInProgress` flag that
  goes true on the first `stream-token` of a chain and back to false in
  `finishNpcStream` after `stream-end` drains. While the flag is true,
  a `loading {active:false}` event does NOT clear `streamingActive` —
  observable via: new vitest unit test in
  `parish/apps/ui/src/lib/setup/stream-manager.test.ts`.
- `runDemoTurn` does not return until both `loading=false` has been
  received and the chain's `stream-end` has fired. Specifically, given
  a scripted event sequence of `loading(true) → stream-token →
  loading(false) → stream-token → stream-turn-end → stream-end`, the
  demo loop resolves only after the final `stream-end` — observable
  via: new vitest test in
  `parish/apps/ui/src/lib/demo-player.test.ts` named
  `waits_through_per_turn_loading_false_within_chain`.
- A live `just demo 2 3` run against a real LLM backend produces at
  least one `chat [npc]` log line per `chat [player]` line on average
  (≥3 NPC replies for 3 player turns) — observable via:
  `grep -c "chat \[npc\]" /tmp/demo-991.log` ≥
  `grep -c "chat \[player\]" /tmp/demo-991.log`.
- Adjacent `chat [player]` log lines are separated by at least
  `turn_pause_secs` plus the inference duration of the intervening NPC
  reply — observable via: the timestamp delta between consecutive
  `chat [player]` lines in `/tmp/demo-991.log` is ≥ 5 seconds on any
  cloud or local backend whose Tier-1 inference takes ≥ 3 seconds.
- All existing vitest + cargo tests stay green — observable via:
  `pnpm --dir parish/apps/ui run test` and `cargo test -p parish-core`
  exit 0.

## Verification

```sh
# Unit tests for the frontend state machine + demo loop fix
pnpm --dir parish/apps/ui run test -- stream-manager demo-player

# Live demo run (requires configured LLM backend)
cd parish && rm -f saves/parish_001.db* && just demo 2 3 > /tmp/demo-991.log 2>&1
grep -c "chat \[player\]" /tmp/demo-991.log    # → 3
grep -c "chat \[npc\]"    /tmp/demo-991.log    # → ≥ 3

# Backend tests unaffected
cargo test -p parish-core
```

Expected signals in output:

- `vitest stream-manager` → new `chainInProgress` test passes.
- `vitest demo-player` → new `waits_through_per_turn_loading_false_within_chain` passes.
- `chat [player]` count == `chat [npc]` count (within ±1 for empty
  replies) in `/tmp/demo-991.log`.
- Inter-turn timestamp gap in `/tmp/demo-991.log` ≥ 5 s.
- `cargo test -p parish-core` → all green.

## Scope note

This fix lives entirely in the frontend stream-manager state machine
and the demo-player wait condition. The backend per-turn
`spawn_loading` / `cancel` flow is unchanged — addressing it there
would touch `parish-tauri`, `parish-server`, `parish-core/game_loop`
and risk live-proof regressions in non-demo paths. The frontend
already has the right pump-drain bookkeeping (`pendingNpcTurns`,
`pendingStreamEndHints`); we extend it with a single chain-in-progress
flag to suppress mid-chain `loading=false` clearing.
