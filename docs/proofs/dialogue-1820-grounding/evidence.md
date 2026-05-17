Evidence type: live gameplay transcript

# Proof — NPC dialogue prompt grounding (1820 facts, Gaelic guardrail, persona lane-keeping)

Follow-up to `docs/proofs/982-dialogue-fixes/`. The earlier slice fixed
formula-closer recycling, max_tokens truncation, lore drift, and emoji
reaction coverage. This slice fixes three remaining axes flagged by a
live demo run and a Grok-4.3 dialogue judge pass:

## Regressions and fixes

| # | Symptom (live evidence)                                                                                                                                                                                                                            | Fix                                                                                  | File                                                                                                              |
|---|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------|
| 1 | Aoife (schoolmistress) in turns 6 + 7: "we must conduct classes in secret as teaching in Irish is outlawed" / "Liam recited a passage in Irish from memory, despite the danger of it". The Penal Laws against Catholic and Irish-language education were repealed in 1782. By 1820 hedge schools operated openly. | New `WORLD FACTS — 1820 rural Roscommon` preamble listing Penal-Law repeal date, pending Emancipation, pending Famine, and Daniel O'Connell's status. | `parish/crates/parish-npc/src/lib.rs` (build_tier1_system_prompt) + `mods/rundale/prompts/tier1_system.txt`        |
| 2 | Aoife emitted `"Slán abhaile go connachtú (Safe home and good journey)."`. `Slán abhaile` is correct Irish; `go connachtú` is hallucinated. The 14B model doesn't know Irish reliably past a small whitelist. | New `ALLOWED IRISH PHRASES` block — verbatim-only whitelist of eight phrases plus an explicit "Do NOT invent or extend Irish phrases. Do NOT improvise Irish grammar" guard. Hiberno-English fallback markers (`ye`, `yer`, `'tis`, `mornin'`, `Mayhap`, `Aye`, `sure`) listed as the safe alternative. | same files                                                                                                        |
| 3 | Grok-4.3 dialogue judge scored midwife dialogue-0005 (prompt "I lost a sheep last night. Could it be more than a wolf?") at 2/5: midwife replied "Let us see if we can track the beast or man that took your poor sheep" — reads as farmer/tracker. dialogue-0003 (prompt "My mother is taken with a bad cough") at 2/5 for modern-register "healing properties". | New `STAY IN YOUR LANE` clause + `REGISTER` blacklist. Lane clause names midwife/farmer/priest/teacher and instructs redirect ("Ye'd best ask the right person hereabouts") when out of scope. Register clause lists eight explicit modern-register negative examples ("healing properties", "fascinating", "taking in the sights", etc.) with period equivalents. | same files                                                                                                        |

## Patched prompt structure

`build_tier1_system_prompt` in `parish/crates/parish-npc/src/lib.rs`
now opens with persona, then immediately lane-keeping, then 1820
world facts, then the existing historical/cultural blocks, then the
Irish-phrase whitelist, then the register blacklist, then the
existing fresh-phrasing anti-formula clause, then personality/mood.
The mod-loadable mirror at `mods/rundale/prompts/tier1_system.txt`
is updated in lockstep so live-reload via the editor surfaces the
same text.

### Clause 1 — STAY IN YOUR LANE (Patch C, persona binding)

```text
STAY IN YOUR LANE: a midwife knows herbs, births, sickness, and women's
matters — she does NOT track livestock predators, hunt, or speak as a
farmer would. A farmer talks of land, beasts, and weather — not
deliveries. A priest speaks of souls and gossip, not arithmetic. A
teacher speaks of pupils and books, not midwifery. If asked about
something outside your knowledge, redirect — "Ye'd best ask the right
person hereabouts" — or admit ye don't know.
```

### Clause 2 — WORLD FACTS preamble (Patch A, 1820 grounding)

```text
WORLD FACTS — 1820 rural Roscommon:
- Penal Laws against Catholic and Irish-language education were
  repealed in 1782. Hedge schools operate openly; teaching in Irish
  is tolerated. Do NOT claim it is illegal or in secret.
- Catholic Emancipation: pending in 1829. Has NOT happened yet.
- Great Famine: 1845. Has NOT happened yet. The potato is a staple
  but the blight has not struck.
- The British Crown rules Ireland. Daniel O'Connell is active but
  not yet famous.
```

### Clause 3 — ALLOWED IRISH PHRASES (Patch B, Gaelic guardrail)

```text
ALLOWED IRISH PHRASES — you MAY use these verbatim, and ONLY these:
- "Slán abhaile" (safe home)
- "Slán leat" (goodbye)
- "Dia dhuit" (hello, lit. God to you)
- "Go raibh maith agat" (thank you)
- "Céad míle fáilte" (hundred thousand welcomes)
- "Sláinte" (cheers / health)
- "mo chara" (my friend)
- "sídhe" (the fairies)
Do NOT invent or extend Irish phrases. Do NOT improvise Irish grammar.
If unsure, stay in Hiberno-English. Sprinkle dialect markers ("ye",
"yer", "'tis", "mornin'", "Mayhap", "Aye", "sure") instead of
confabulating Irish.
```

This is intentionally narrower than the pre-existing `GA_IE_PHRASE_GUIDE`
appended by `language_directive`. The earlier guide is appended only
when `native = "ga-IE"`; it lists ~25 phrases including
exclamations like "Mhuise" and "Bedambut" that 14B models confabulate
into. The Tier 1 system prompt now carries a smaller whitelist with an
explicit improvisation guard that fires *unconditionally*, regardless
of native-language setting. The two are complementary: the system
prompt sets the verbatim-only contract; the directive adds optional
extra phrases when a native locale is configured.

### Clause 4 — REGISTER blacklist (Patch C, modern-word negative examples)

```text
REGISTER: avoid 21st-century words. Do NOT use: fascinating, amazing,
definitely, totally, decided to visit, healing properties, taking in
the sights. Use period equivalents: a thing of interest, a fine sight,
surely, mayhap, a tea of thyme will ease her chest.
```

## Why the lane-keeping clause is verbatim (no `{appropriate_role}` substitution)

The user spec offered `{appropriate_role}` if the template engine
supports such substitution, otherwise the literal string `"the right
person hereabouts"`. The current Tier 1 system prompt template uses
flat `{name}`, `{age}`, `{occupation}`, `{personality}`, `{mood}`,
`{intel_guidance}`, `{tone_guidance}`, `{improv_section}` placeholders;
there is no NPC-roster lookup at prompt-render time, so the role of
"the right person hereabouts" cannot be resolved per-NPC without a new
data plumb. The literal fallback ships.

Future work: per-occupation referrals (`{midwife_name}`,
`{teacher_name}` derived from the live NPC roster) would let
Maeve-the-midwife redirect *by name* — "Ye'd best ask Aoife at the
hedge school" — but that's a roster-injection seam, not a prompt edit.
Out of scope for this fix.

## Live evidence — CLI script run

The `mods/*` path is runtime-shipping per `agent-check.sh`. Mod loading
fires at startup of every entry point (Tauri, server, CLI). The
modified `mods/rundale/prompts/tier1_system.txt` is parsed via
`PromptTemplates::tier1_system` and made available to the editor's
live-reload path. A startup failure (malformed text, missing
placeholder definition, broken template) surfaces immediately — the
process refuses to come up.

The CLI script harness exercises the live mod-loading and game-loop
wiring without an LLM dependency. Transcript captured in
[`cli_script.txt`](./cli_script.txt):

```
$ cargo run -p parish -- --script testing/fixtures/test_anachronism.txt
{"command":"go to crossroads","result":"moved","to":"The Crossroads",...
{"command":"go to pub","result":"moved","to":"Darcy's Pub",...
{"command":"hello there","result":"npc_not_available",...,"new_log_lines":["Padraig Darcy 😊"]}
{"command":"how are you today","result":"npc_not_available",...}
{"command":"tell me about the parish","result":"npc_not_available",...,"new_log_lines":["Niamh Darcy 👀"]}
{"command":"what news from the market","result":"npc_not_available",...,"new_log_lines":["Padraig Darcy 👀"]}
{"command":"can I use the telephone","result":"npc_not_available",...}
...
{"command":"/quit","result":"quit",...}
```

What the live run proves:

- The patched `mods/rundale/prompts/tier1_system.txt` parses through
  `PromptFile` → `PromptTemplates` cleanly at startup (no exception,
  no malformed-prompt panic).
- Mod load includes the modified prompt path — the manifest entry
  `tier1_system = "prompts/tier1_system.txt"` is read by
  `read_text(&manifest.prompts.tier1_system)` (see
  `parish/crates/parish-core/src/game_mod.rs:565`).
- The CLI process boots, the world graph loads, NPCs are placed at
  their schedule-resolved locations, scripted commands return valid
  JSON envelopes — none of which would happen if the modified files
  broke any structural invariant.

The harness intentionally does not call the LLM (Tier 1 NPC dialogue
is skipped — note the `npc_not_available` result on the chat lines —
because no LLM provider is wired into the scripted-CLI mode).
Validating the *content* of the new prompt clauses (does the model
actually stop confabulating Irish, does Aoife stop saying classes are
secret, does the midwife stop tracking wolves) requires a real
LLM-backed run with grok-4.3 or the local 14B model. That follow-up
is the same blind-judge pass described in the user's spec ("re-cache
+ re-judge with grok-4.3"); the prompt edits land first, the
score-lift confirmation lands separately if needed.

## Unit-test contract

`test_tier1_system_no_unsubstituted_placeholders` in
`parish/crates/parish-npc/src/lib.rs` now also asserts each new clause
is present:

- `STAY IN YOUR LANE` (lane-keeping)
- `Penal Laws` and `1782` (1820 fact preamble)
- `ALLOWED IRISH PHRASES`, `Slán abhaile`, `Do NOT invent or extend
  Irish phrases` (Gaelic whitelist + improvisation guard)
- `REGISTER:` and `healing properties` (modern-register blacklist)

A future edit that strips any of those clauses fails the unit test
intentionally.

## Tests

- `cargo test -p parish-npc` → all passed (`test_tier1_system_no_unsubstituted_placeholders` includes the new contract assertions).
- `cargo test -p parish-core` → 430 passed, 4 ignored. Mod-load
  contract (`mod_artefact_malformed_input`) green — the patched
  `tier1_system.txt` parses fine.
- `cargo run -p parish -- --script testing/fixtures/test_anachronism.txt`
  captured in `cli_script.txt` — live process boots, mod loads,
  scripted commands return well-formed JSON.

## Files changed (2)

- `parish/crates/parish-npc/src/lib.rs` — three new prompt clauses in
  `build_tier1_system_prompt`, plus four new contract assertions in
  the unit test.
- `mods/rundale/prompts/tier1_system.txt` — mirror of the Rust prompt
  changes for the editor-live-reload path.
