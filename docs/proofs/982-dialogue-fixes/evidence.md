Evidence type: gameplay transcript

# Proof — #982 dialogue prompt, max_tokens, lore, and reaction coverage

Follow-up to the observability/routing bundle in
`docs/proofs/982-inference-routing/`. That earlier slice added the
`chat [npc]` and `npc-reaction` log lines that made the five
dialogue-quality regressions in issue #982 visible. This slice fixes
the regressions themselves.

## Regressions and fixes

| # | Symptom (from issue #982)                                   | Fix                                                                                                          | File                                                                            |
|---|-------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------|
| 1 | Stock closer "if I might ask it so bold" recurs across NPCs | New `FRESH PHRASING` block in `build_tier1_system_prompt`                                                     | `parish/crates/parish-npc/src/lib.rs` (CULTURAL GUIDELINES section)             |
| 2 | NPC reply truncates mid-sentence (`max_tokens` cap)         | `TIER1_DIALOGUE_MAX_TOKENS = 512` passed explicitly on the dialogue inference call                           | `parish/crates/parish-core/src/game_loop/npc_turn.rs`                            |
| 3 | "St. Tíobán" attribution is unhistorical                    | Lore corrected to `Cill Taobháin` / `Teevan` per [research note 6](../../research/irish-language.md) and Wikipedia's "Kilteevan" article | `mods/rundale/{world,npcs,pronunciations}.json`, `parish-core::game_mod` fixture |
| 4 | Zero emoji reactions across a 5-turn demo                   | `KEYWORD_REACTIONS` expanded with palette-resident chitchat cues (greetings, weather, parish, family, music) | `parish/crates/parish-npc/src/reactions/emoji_reactions.rs`                      |
| 5 | Two NPCs reply with identical closer in the same turn       | Same anti-formula clause as #1 — it forbids recycling a prior closer or another NPC's phrasing               | `parish/crates/parish-npc/src/lib.rs`                                            |

## 1 + 5. Stock-phrase and identical-closer fix

Added a `FRESH PHRASING` section to the Tier 1 system prompt
unconditionally (improv on or off). The clause cites the offending
templates so the model has explicit negative examples:

```rust
// parish/crates/parish-npc/src/lib.rs (excerpt from build_tier1_system_prompt)
FRESH PHRASING: Do not close with stock politeness templates such as
"if I might ask it so bold," "if ye don't mind my asking," or similar
repeated softeners. Every reply must use distinct wording — never
recycle the closer of any earlier turn in the conversation, and never
echo another NPC's phrasing. End on a concrete observation, question,
or action rooted in your character, not a formula.
```

## 2. Truncation fix

Tier 1 dialogue previously sent `max_tokens: None` and inherited the
provider default, which truncated mid-sentence under vllm-mlx with the
14B Qwen model. Now the cap is explicit:

```rust
// parish/crates/parish-core/src/game_loop/npc_turn.rs
pub const TIER1_DIALOGUE_MAX_TOKENS: u32 = 512;
//
queue.send(
    req_id, model.to_string(), setup.context, Some(setup.system_prompt),
    Some(token_tx),
    Some(TIER1_DIALOGUE_MAX_TOKENS),
    Some(0.7),
    crate::inference::InferencePriority::Interactive,
    true,
)
```

512 fits a 2-4 sentence reply plus the structured JSON envelope
(`dialogue`, `action`, `mood`, `internal_thought`, `language_hints`)
with headroom; smaller caps regress when an NPC pauses to enumerate
items.

## 3. Cill Taobháin lore correction

Wikipedia's *Kilteevan* article and `townlands.ie` agree the Irish
form is *Cill Taobháin* — "Teevan's Church" — not *Cill Tíobáin* and
not associated with a Saint Tíobán. This was already flagged in
`docs/research/irish-language.md`, note [6], but the mod data still
referenced the saint. Three world-text touchpoints, one test fixture:

| File                                  | Before                          | After                                                  |
|---------------------------------------|---------------------------------|--------------------------------------------------------|
| `mods/rundale/pronunciations.json`    | `Cill Tíobáin — church of St. Tíobán` | `Cill Taobháin — Teevan's Church`                       |
| `mods/rundale/world.json` (Kilteevan) | `Cill Tíobáin, the church of St. Tíobán` | `Cill Taobháin, Teevan's Church, named for ... Taobhán` |
| `mods/rundale/world.json` (Holy Well) | `St. Tíobán's Well`             | `the holy well of Kilteevan ... the spring that gave Cill Taobháin its name` |
| `mods/rundale/npcs.json` (Brigid)     | `nothing to do with St. Tíobán` | `nothing to do with the Church`                        |
| `parish/crates/parish-core/src/game_mod.rs` (test fixture) | `church of St. Tíobán` | `Cill Taobháin — Teevan's Church`                       |

Verification:

```
$ grep -rl 'Tíobán\|Tíobáin' parish/ mods/ docs/
docs/research/irish-language.md   # only the correction note itself
```

## 4. Reaction-coverage fix — live transcript

Demo prompt steers the player into chitchat ("greet people, ask about
lives, land, events"). Old `KEYWORD_REACTIONS` only covered charged
topics (death, landlord, ghosts, gold), so a full session emitted zero
`npc-reaction` events.

`KEYWORD_REACTIONS` now adds palette-resident chitchat cues. The
12-entry `REACTION_PALETTE` is unchanged; new keyword groups all map
to existing emoji so the LLM validator, UI, and reaction-log context
renderer keep working without further edits.

Captured live, with the script harness driving the simulator inference
client (deterministic, no LLM needed to validate the rule path):

```
$ cargo run -p parish -- --script testing/fixtures/test_anachronism.txt 2>/dev/null
{"command":"go to crossroads","result":"moved","to":"The Crossroads", ...}
{"command":"go to pub","result":"moved","to":"Darcy's Pub", ...}
{"command":"hello there",            ..., "new_log_lines":["Padraig Darcy 😊","Niamh Darcy 😊"]}
{"command":"how are you today",      ..., "new_log_lines":[]}
{"command":"tell me about the parish",..., "new_log_lines":["Padraig Darcy 😊","Niamh Darcy 😊"]}
{"command":"what news from the market",..., "new_log_lines":["Padraig Darcy 👀"]}
{"command":"can I use the telephone",..., "new_log_lines":[]}
...
```

Trip counts on this 9-input run:

- `hello there` → both NPCs smile (matches `hello` keyword, 😊).
- `tell me about the parish` → both NPCs smile (matches `parish`, 😊).
- `what news from the market` → Padraig raises an eyebrow (matches
  `news`, 👀; Niamh's 60 % gate did not fire this turn — that is the
  intended sparing-accent behaviour and matches the rule-path doc
  comment).
- `how are you today`, anachronism inputs, and the closing `look`
  produce no reaction — none of those phrases match a keyword, so the
  rule path returns `None`.

Per `docs/agent/scaling-rules.md`, the 60 % probabilistic gate is
intentionally preserved — reactions are an accent, not a per-turn
emission. The fix widens *what* counts as a candidate, not how often
candidates fire.

## Test runs

```
$ cargo test -p parish-npc 2>&1 | tail
cargo test: 433 passed (5 suites, 0.99s)

$ cargo test -p parish-core 2>&1 | tail
cargo test: 424 passed, 4 ignored (9 suites, 5.62s)

$ cargo clippy -p parish-npc -p parish-core --all-targets
cargo clippy: No issues found

$ cargo fmt --check
(clean)
```

The previously-passing `generate_rule_reaction_no_match` test used
`"Good morning to you"` as a negative case; that phrase now matches
the new `good morning` keyword (correctly), so the test was updated
to `"Just walking by here"` which still matches nothing.

## Files changed

- `parish/crates/parish-npc/src/lib.rs` — anti-formula clause in `build_tier1_system_prompt`.
- `parish/crates/parish-npc/src/reactions/emoji_reactions.rs` — keyword expansion + test input swap.
- `parish/crates/parish-core/src/game_loop/npc_turn.rs` — explicit `max_tokens` cap.
- `parish/crates/parish-core/src/game_mod.rs` — test fixture lore.
- `mods/rundale/pronunciations.json`, `world.json`, `npcs.json` — lore.

## What the harness cannot prove

The simulator path validates the rule-based reaction expansion and
the lore data. It does not exercise the dialogue prompt change or the
`max_tokens` cap — both require a live LLM. Those two land as
"prompt-engineering" changes whose validation lives in the next
`just demo 1 5` run on macOS hardware, which the previous proof
bundle (`docs/proofs/982-inference-routing/`) showed how to capture.
