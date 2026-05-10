Evidence type: gameplay transcript + LLM A/B comparison

## Summary

Background simulation (Tier 2 + Tier 3) used to feed the LLM cryptic
`INT[V4 A3 E5 P2 W3 C4]` intelligence codes and raw numeric NPC IDs
(`NPC 42 (0.7)`) in its relationship lists. The model has no way to know
what those codes mean, so they functioned as noise. This change converts
those encodings to plain English while keeping numeric `[id]` anchors
for the JSON response parser (`Tier2Response` / `Tier3Response` fields
are unchanged).

### Code changes

- `parish/crates/parish-npc/src/types.rs` — new
  `Intelligence::adjective_summary()` (compact form for Tier 3 fan-out).
- `parish/crates/parish-npc/src/ticks.rs` —
  - `NpcSnapshot`: `intelligence_tag` -> `intelligence_prose`,
    `relationship_context` -> `relationship_summary`.
  - `Tier3Snapshot`: adds `intelligence_adjectives` (compact),
    swaps `relationship_context` -> `relationship_summary`.
  - New `format_relationships_natural()` helper.
  - `build_tier2_prompt` and `build_tier3_prompt` rewritten with
    natural-language bodies and `[id]` anchors.
  - Snapshot constructors take a `npc_names: &HashMap<NpcId, String>`
    so names are resolved at snapshot time.
- Dispatch sites in `parish-tauri/src/setup.rs`,
  `parish-cli/src/headless.rs`, `parish-npc/src/manager.rs`, and the
  `tier2_llm_integration.rs` integration tests updated to build and
  pass the names map.

## Before / after, same scenario

Three NPCs at The Pub, 14:30, drizzling rain:

- Padraig Mullen, blacksmith, content. V4 A3 E5 P4 W3 C2.
  Friends with Mary (0.7), cool toward Sean (-0.3).
- Mary McKenna, washerwoman, tired. V2 A2 E4 P5 W4 C2.
- Sean Doyle, labourer, irritable. V3 A4 E2 P3 W2 C3.

### OLD Tier 2 prompt

```
You are simulating background interactions between characters in a small Irish parish in 1820.

Location: The Pub
Time: 14:30
Weather: drizzling. People are commenting on the weather.

Characters present:
- Padraig Mullen (blacksmith), mood: content, INT[V4 A3 E5 P4 W3 C2]
- Mary McKenna (washerwoman), mood: tired, INT[V2 A2 E4 P5 W4 C2]
- Sean Doyle (labourer), mood: irritable, INT[V3 A4 E2 P3 W2 C3]

Generate a brief (1-2 sentence) summary...
```

(No relationship context. Intelligence is an opaque six-character code.)

### NEW Tier 2 prompt

```
You are simulating background interactions between characters in a small Irish parish in 1820.

Location: The Pub
Time: 14:30
Weather: drizzling. People are commenting on the weather.

Dramatis personae (id in brackets — reuse these in your JSON):
- [1] Padraig Mullen, blacksmith. Currently content. Well-spoken with
  a good vocabulary... Reads people like a book... Resourceful and
  sensible... Unimaginative and humourless... friendly with Mary
  McKenna, cool to Sean Doyle.
- [2] Mary McKenna, washerwoman. Currently tired. Speaks plainly with
  a limited vocabulary... Perceptive about people's feelings...
  Extraordinarily resourceful... friendly with Padraig Mullen,
  acquainted with Sean Doyle.
- [3] Sean Doyle, labourer. Currently irritable. Sharp-minded...
  Blunt and socially clumsy... Impulsive and prone to poor judgment...
  cool to Padraig Mullen, acquainted with Mary McKenna.

Write a 1-2 sentence summary of what they are doing and saying to each
other. Note any mood changes or relationship shifts.

Respond with JSON, using the bracketed ids:
{"summary":"...",
 "mood_changes":[{"npc_id":<id>,"new_mood":"..."}],
 "relationship_changes":[{"from":<id>,"to":<id>,"delta":<-0.1..0.1>}]}
```

## Blind LLM comparison (Haiku 4.5 stand-in for local Ollama)

Each prompt was handed to a separate Haiku agent role-playing as a
small background LLM. Neither agent saw the other's prompt or the
implementation; they were asked to produce the requested JSON and to
flag in MODEL NOTES anything that felt unclear.

### OLD prompt — model output and notes

> Summary: "Padraig remarks on the drizzle affecting his metalwork
> while Mary commiserates, exhausted from wringing out wet laundry;
> Sean grumbles that the weather won't stop the day's labour, earning
> a weary nod from Mary but drawing a slight smirk from Padraig."

Model self-critique (verbatim):

> The `INT[V4 A3 E5 P4 W3 C2]` encoding is completely opaque. I
> interpreted it as some kind of attribute vector ... but without a
> schema definition, I cannot actually use it to inform the
> interaction. I simply ignored it and relied on the mood labels
> instead.

> No NPC IDs were provided, so I guessed 1, 2, 3 in order. If the
> system uses different IDs, the JSON will be misaligned.

### NEW prompt — model output and notes

> Summary: "Padraig and Mary commiserate about the drizzle ... Sean
> enters irritably, complaining the wet will ruin the day's work,
> and when Padraig suggests waiting it out, Sean snaps that some of
> them don't have that luxury — Mary shoots Padraig a look that says
> she understands Sean's frustration even if his tone was harsh."

Model self-critique: no complaints about parsing the prompt. Concerns
were about scene-craft (mood persistence semantics, dialect
authenticity), not about decoding cryptic fields.

JSON in both cases used numeric `npc_id` matching the `[id]` anchors —
the parser stays untouched.

## What the new prompts unlock

1. **Intelligence shapes behaviour.** OLD: model "simply ignored"
   `INT[...]`. NEW: Padraig's high emotional + Sean's bluntness drive
   the central beat (Sean snaps, Mary reads the room).
2. **Relationships drive plot.** Tier 2 currently sends no relationship
   info at all; the new prompt's "cool to Sean Doyle" became the
   tension axis with a well-motivated mild +0.03 delta from Mary -> Sean.
3. **Numeric ID safety.** Bracketed `[id]` anchors + an explicit
   "reuse these in your JSON" instruction keep the response parser
   unchanged.

## Token cost (rough)

- Tier 2, 3 NPCs: ~155 -> ~430 tokens (+275). Tier 2 fires per
  location per 5 game-minutes; the dialogue benefit justifies it.
- Tier 3, 30 NPCs with adjective summary: ~1280 -> ~1950 tokens
  (+670). `adjective_summary()` returns `""` for average NPCs, so
  the prompt naturally compresses for unremarkable agents.
- The adaptive split (full prose Tier 2, adjective list Tier 3)
  saves ~1350 tokens vs full prose everywhere.

## Verification

```
cargo test -p parish-npc
   ...
   test result: ok. 411 passed; 0 failed; 0 ignored
   (lib)
   test result: ok. 6 passed; 0 failed (tier2_llm_integration)
   test result: ok. 3 passed; 0 failed (gossip_integration)
   test result: ok. 3 passed; 0 failed (doctests)

cargo test --workspace
   all suites green

cargo clippy --workspace --all-targets
   no warnings

cargo fmt --all
   clean
```

New tests:

- `test_intelligence_adjective_summary_*` (5 tests)
- `test_format_relationships_natural_empty`
- `test_format_relationships_natural_known_names`
- `test_format_relationships_natural_unknown_name_fallback`
- `test_npc_snapshot_uses_prose_not_codes`
- `test_tier3_snapshot_uses_adjectives_not_codes`
- `test_tier2_prompt_omits_intelligence_when_average`

Updated tests: `test_build_tier2_prompt`,
`test_build_tier2_prompt_rain_commentary`,
`test_tier3_prompt_construction`, `test_tier3_batching`,
`test_run_tier2_solo_npc_template`,
`test_tier3_snapshot_from_npc_*`, and the lib.rs
`test_tier2_system_no_unsubstituted_placeholders` — all green.
