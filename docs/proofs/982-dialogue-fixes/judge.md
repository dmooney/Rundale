# Judge Verdict — #982 dialogue fixes

## Review scope

Single slice on `fix/982-dialogue-prompt-and-reactions` against
`origin/main`, fixing five regressions catalogued in issue #982:

1. Recurring stock-phrase closer
2. Mid-sentence truncation
3. Unhistorical "St. Tíobán" lore
4. Zero emoji reactions over a 5-turn demo
5. Two NPCs landing on the same closer in the same turn

Files touched (5):

- `parish/crates/parish-npc/src/lib.rs`
- `parish/crates/parish-npc/src/reactions/emoji_reactions.rs`
- `parish/crates/parish-core/src/game_loop/npc_turn.rs`
- `parish/crates/parish-core/src/game_mod.rs` (test fixture)
- `mods/rundale/{world,npcs,pronunciations}.json`

## Structural assessment

**Root-cause fix on each axis.**

- **Prompt anti-formula clause** addresses regressions #1 and #5
  with one negative-example block embedded in the always-on path of
  `build_tier1_system_prompt`. The clause quotes the exact offending
  template so the model has a concrete cue, and explicitly forbids
  recycling a previous closer or echoing another NPC. No model-side
  retraining required.
- **Explicit `max_tokens` cap.** The dialogue inference call
  previously sent `max_tokens: None` (rendered as the field being
  omitted entirely under `#[serde(skip_serializing_if = ...)]`), so
  every provider's default applied. `512` is the right size: enough
  for a 2-4 sentence reply plus the structured-output envelope, and
  far below the 14B Qwen context limit.
- **Lore correction** follows the existing research note that already
  acknowledged the error. Wikipedia ("Kilteevan") and `townlands.ie`
  agree on *Cill Taobháin* / "Teevan's Church". All four mod-data
  references and the test fixture are updated in one pass.
- **Reaction-coverage** expands `KEYWORD_REACTIONS` strictly within
  the existing 12-entry `REACTION_PALETTE`. The validator
  `infer_player_message_reaction` rejects emoji not in the palette,
  the `ReactionLog::context_string` formatter pulls descriptions from
  the same palette, and the UI emoji icon set is palette-keyed —
  staying inside the palette means no UI or test ripple. The 60 %
  probabilistic gate is preserved deliberately so reactions remain a
  sparing accent.

**Mode parity.** All edits are in `parish-core` and `parish-npc`,
both backend-agnostic. The dialogue inference call site is shared
by server, Tauri, and CLI runtimes via `run_npc_turn`. The
`KEYWORD_REACTIONS` table is consumed through the shared
`emit_npc_reactions` helper (cross-runtime since #696 slice 5). No
runtime-specific wiring required.

**Scope discipline.** No unrelated refactors, no dead code, no
abstractions introduced. The prompt change is a contiguous edit to an
existing format string; the `max_tokens` change is a one-line replace
plus a named constant; the lore changes are find-and-replace; the
keyword expansion is a single table addition.

## Risk

- **Prompt change is the riskiest item.** The fresh-phrasing clause
  adds ~80 tokens to every Tier 1 system prompt. Token-cost impact is
  bounded (one-time per turn, shared across many concurrent turns
  under prompt caching) and the literal "if I might ask it so bold"
  callout is a strong negative cue for the affected models. Worst
  case: the model interprets the clause as cuing the *same* closer at
  twice the rate — the live-demo follow-up captures that risk before
  merge.
- **`KEYWORD_REACTIONS` collision.** Inputs containing two keyword
  groups (e.g. "tell me about the parish family") will match the
  first group only (`news`/`👀`), since the loop returns on first
  match. Pre-existing behaviour, no change.
- **Lore continuity.** Existing saves reference Brigid's knowledge
  string verbatim only if a long-term memory ingested it. The mod-load
  path always re-reads the JSON, so new sessions see the corrected
  string immediately; in-flight memories are stale until they age out
  (bounded by `MAX_ENTRIES` per NPC).

## Testing

- `cargo test -p parish-npc`  → 433 passed.
- `cargo test -p parish-core` → 424 passed, 4 ignored.
- `cargo clippy -p parish-npc -p parish-core --all-targets` → clean.
- `cargo fmt --check` → clean.
- `cargo run -p parish -- --script testing/fixtures/test_anachronism.txt`
  captured in `evidence.md` — confirms the new keyword cues fire for
  `hello`, `parish`, and `news` lines, and stay silent on the
  unrelated anachronism probes.
- `generate_rule_reaction_no_match` test input swap: `"Good morning
  to you"` would now (correctly) match — replaced with `"Just walking
  by here"` so the test still asserts no match.

## Verdict

Verdict: sufficient

Technical debt: clear

Remaining live-LLM follow-up (the prompt and `max_tokens` changes
both require a real provider to validate) is the same `just demo 1 5`
run on macOS that the previous proof bundle described — not in scope
for the harness check.
