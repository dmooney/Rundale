# Demo non-inference fixes — proof evidence

Evidence type: live gameplay transcript
Date: 2026-05-17
Branch: claude/confident-allen-93f272
PR: #986

## Scope

Five non-inference bugs surfaced by an earlier 10-turn `just demo` audit. The fixes
land as five distinct commits. The user explicitly deferred inference-layer bugs
(JSON envelope leaks in player/NPC text, anachronisms, modern-register player
prose, hallucinated Gaelic) to a separate session — those remain visible in this
transcript and are not in scope.

Bugs fixed:

1. `#13` `--demo-max-turns` hung the process instead of exiting.
2. `#10` Addressing a co-located NPC by a name that isn't present routed to the wrong NPC.
3. `#14` Markov simulator output for the `reaction` category surfaced in the chat bubble.
4. `#11` (regression guard) `apply_movement` arrival messages must carry `source: "system"`.
5. `#12` Status-bar clock flickered every LLM call because it froze on `inference_paused`.
6. `#9` TOCTOU #283 fired the "World shifted while your words were in the air." message on most intents.

## 1. Workspace test suite

```
$ cargo test --workspace --no-fail-fast
cargo test: 2788 passed, 15 ignored (67 suites, 13.08s)
```

New tests added by this PR:

| Crate | Test |
|-------|------|
| `parish-core` | `ipc::handlers::tests::resolve_npc_targets_named_but_absent_returns_empty` |
| `parish-core` | `ipc::handlers::tests::resolve_npc_targets_no_names_falls_back_to_first_present` |
| `parish-core` | `game_session::tests::stream_reaction_texts_skips_llm_when_client_is_simulator` |
| `parish-core` | `game_session::tests::apply_movement_arrival_messages_are_system_sourced` |
| `apps/ui` (vitest) | `demo-player.test.ts > dispatches /quit when CLI demo reaches max_turns` |
| `apps/ui` (vitest) | `demo-player.test.ts > does not dispatch /quit for UI-launched demos at max_turns` |

## 2. Frontend gates

```
$ npx vitest run
PASS (401) FAIL (0)

$ npm run check  # svelte-check
0 ERRORS 1 WARNINGS 1 FILES_WITH_PROBLEMS
```

The one remaining warning is a pre-existing CSS `user-select` standard-property
hint in `InputField.svelte`; untouched by this PR.

## 3. Live demo run

`just demo 2 3` exits via `--demo-max-turns 3`. The background launch terminated
with exit code 0 (Fix #13). No manual `pkill` was needed.

```
$ just demo 2 3 > /tmp/demo-verify.log 2>&1 &
$ wait        # background pid
$ echo $?
0
```

Counts from the demo log:

```
$ grep -c "World shifted\|TOCTOU #283" /tmp/demo-verify.log
0
$ grep -ciE "bridget|new collection|saints blush|God help|Pat Morrissey|Father Clancy|drainage situation" /tmp/demo-verify.log
0
$ grep -ciE "ERROR|panic" /tmp/demo-verify.log
0
```

For comparison, the pre-fix 10-turn audit produced four `World shifted` warnings
(approximately 40 % of intents) and surfaced the simulator corpus phrase
"bridget from the new collection... God help us" in the chat.

### Transcript excerpt

`demo turn` lines show the LLM-chosen player action; `chat [npc]` lines show
the NPC reply. Movement narration arrives as `chat source=system`.

```
2026-05-17T01:45:44.331149Z  INFO demo turn: LLM chose action location=The Hurling Green action=go to the Crossroads
2026-05-17T01:45:44.333934Z  INFO chat [player] input=go to the Crossroads
2026-05-17T01:45:44.335236Z  INFO chat source=system text=You walk along an old lane between settlements. (4 minutes on foot)
2026-05-17T01:45:47.928169Z  INFO demo turn: LLM chose action location=The Crossroads action=Good day, folks! I'm just passing through. This is a beautiful spot.
2026-05-17T01:45:47.929433Z  INFO chat [player] input=Good day, folks! I'm just passing through. This is a beautiful spot.
2026-05-17T01:45:57.423403Z  INFO chat [npc] npc=Peig Hannigan reply=Good day indeed, stranger! Passing through, ye say? ...
2026-05-17T01:46:26.212443Z  INFO npc-reaction npc=Peig Hannigan emoji=😊
2026-05-17T01:46:30.791647Z  INFO demo turn: LLM chose action location=The Crossroads action=I'm here to learn and perhaps share a tale or two. What stories are the people of Roscommon telling these days?
2026-05-17T01:47:00.815290Z  INFO chat [npc] npc=Peig Hannigan reply={"dialogue": "Ah, so ye're here to learn and share. ..."}
2026-05-17T01:47:21.925462Z  INFO chat [npc] npc=Tommy O'Brien reply=Ah, tales of the sídhe and the piseogs, indeed. ...
```

System narration carries `source=system` (Fix #11 regression-guard test verifies
this at the message-construction boundary). NPC reply lines carry the speaker's
display name and never contain simulator-corpus phrases (Fix #14).

### Deferred (inference layer)

These two artefacts are visible in the excerpt above; they are not in scope and
will be addressed in a follow-up session:

- Turn 3 NPC replies begin with a literal `{"dialogue": "..."` JSON envelope —
  the dialogue parser failed to strip the surrounding JSON before forwarding to
  the chat stream.
- "the famine" and "the great frost of '95" references — the world year is 1820,
  the Great Famine is 1845+; the frost reference is plausible only if read as
  "1795" but the surrounding context implies recency.

## 4. Per-fix correspondence

| Fix | Commit | Evidence path |
|-----|--------|--------------|
| #13 `--demo-max-turns` clean exit | `eeb3ee16` | exit code 0 above; new vitest cases |
| #10 NPC addressing | `97afd67a` | two new `resolve_npc_targets` tests |
| #14 simulator → canned for reactions | `b165acb4` | zero simulator-corpus phrase hits; new `stream_reaction_texts_skips_llm_when_client_is_simulator` |
| #11 arrival message source guard | `b165acb4` | new `apply_movement_arrival_messages_are_system_sourced` |
| #12 clock flicker | `4a66cadd` | visual-only, behaviour follows from code review |
| #9 TOCTOU #283 | `ab4f80ef` | zero `World shifted` emissions (was 4 per 10 turns) |
