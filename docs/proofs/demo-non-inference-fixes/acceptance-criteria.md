# Acceptance Criteria: demo-non-inference-fixes

## Task

A 10-turn `just demo` audit produced six non-inference bugs. Land each as
its own commit and verify behaviour change in a live run. The visible
differences after this change:

- `just demo 2 N` actually exits when `N` turns are reached, instead of
  leaving the Tauri process alive.
- Markov simulator output never appears as readable text in the chat
  panel during a demo (was leaking the "bridget / new collection / God
  help us" corpus phrases via arrival reactions).
- The TOCTOU race banner "The world shifted while your words were in the
  air." stops firing on routine multi-second LLM calls.
- Addressing a co-located NPC by a name that is not actually present
  returns "No one here answers to that name just now." rather than
  silently routing to whoever happens to be there.
- Arrival narration after movement carries `source: "system"`, never an
  NPC display name, so it never renders as a dialogue bubble.
- The status-bar clock keeps ticking visually during transient
  `inference_paused` holds; only an explicit user `/pause` freezes the
  digits and shows the "⏸ Paused" indicator.

## Criteria

- A `just demo 2 3` invocation returns exit code 0 within seconds of
  the third turn's NPC reply — observable via: shell exit code from the
  background `just demo` process.
- The demo log contains zero lines matching
  `bridget|new collection|saints blush|God help us|Pat Morrissey|Father Clancy|drainage situation`
  — observable via: `grep -ciE` against `/tmp/demo-verify.log`.
- The demo log contains zero lines matching
  `World shifted|TOCTOU #283` — observable via: `grep -c`.
- `resolve_npc_targets` returns empty when an addressed name is absent,
  triggering the existing "No one here answers to that name" branch —
  observable via: new unit test
  `resolve_npc_targets_named_but_absent_returns_empty`.
- Every `GameMessage` from `apply_movement` carries `source == "system"`
  — observable via: new unit test
  `apply_movement_arrival_messages_are_system_sourced`.
- The status-bar component freezes only on `snap.paused`, not on
  `snap.inference_paused` — observable via: code diff in
  `parish/apps/ui/src/components/StatusBar.svelte`.

## Verification

```sh
just demo 2 3 > /tmp/demo-verify.log 2>&1   # foreground or background; capture exit
echo $?                                      # must be 0
grep -c "World shifted\|TOCTOU #283" /tmp/demo-verify.log
grep -ciE "bridget|new collection|saints blush|God help|Pat Morrissey|Father Clancy|drainage situation" /tmp/demo-verify.log
cargo test -p parish-core --lib ipc::handlers::tests::resolve_npc_targets
cargo test -p parish-core --lib game_session::tests::apply_movement_arrival_messages_are_system_sourced
cargo test -p parish-core --lib game_session::tests::stream_reaction_texts_skips_llm_when_client_is_simulator
cd parish/apps/ui && npx vitest run src/lib/demo-player.test.ts
```

Expected signals in output:

- `echo $?` → `0`
- `grep -c "World shifted"` → `0`
- `grep -ciE "bridget|..."` → `0`
- `cargo test ... resolve_npc_targets` → `3 passed`
- `cargo test ... apply_movement_arrival_messages_are_system_sourced` → `1 passed`
- `cargo test ... stream_reaction_texts_skips_llm_when_client_is_simulator` → `1 passed`
- `vitest demo-player.test.ts` → `6 passed`

## Scope note

Inference-layer bugs (JSON envelope leaks in player/NPC text,
anachronisms like "the famine" in an 1820 setting, modern player
register, hallucinated Gaelic) are deferred to a separate session per
the user's direction. They remain visible in the transcript and are
intentionally not part of these acceptance criteria.
