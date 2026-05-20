# Acceptance criteria — NPC dialogue prompt grounding (1820 facts + Gaelic guardrail + persona)

Task: edit the Tier 1 NPC dialogue system prompt so it:

1. Grounds dialogue in 1820 historical facts (Penal Laws repealed 1782,
   Emancipation pending 1829, Famine pending 1845, Daniel O'Connell
   active but not yet famous).
2. Whitelists exactly eight verbatim Irish phrases and forbids
   improvising new Irish grammar.
3. Binds personas into their lane (midwife / farmer / priest / teacher)
   and blacklists eight modern-register words (`fascinating`, `amazing`,
   `definitely`, `totally`, `decided to visit`, `healing properties`,
   `taking in the sights`).

The patches must land in both the hardcoded runtime prompt and the
mod-shipped mirror so editor live-reload stays consistent.

## Observable criteria

### A1 — 1820 fact preamble present in rendered prompt

The string `WORLD FACTS — 1820 rural Roscommon:` and the four
sub-bullets (Penal Laws / Emancipation / Famine / British Crown) must
appear in the output of
`build_tier1_system_prompt(npc, false, &lang)`.

Verifier: unit test
`test_tier1_system_no_unsubstituted_placeholders` asserts
`prompt.contains("Penal Laws")` and `prompt.contains("1782")`.

### A2 — Irish-phrase whitelist + improvisation guard present

The string `ALLOWED IRISH PHRASES`, the anchor phrase `Slán abhaile`,
and the literal guard `Do NOT invent or extend Irish phrases` must all
appear in the rendered prompt.

Verifier: unit test asserts all three substrings present.

### A3 — STAY IN YOUR LANE clause present

The string `STAY IN YOUR LANE` must appear in the rendered prompt.

Verifier: unit test asserts `prompt.contains("STAY IN YOUR LANE")`.

### A4 — Modern-register blacklist present

The strings `REGISTER:` and `healing properties` (one of the eight
negative examples) must both appear in the rendered prompt.

Verifier: unit test asserts both substrings present.

### A5 — Mod-shipped prompt mirror updates with no parse failure

`mods/rundale/prompts/tier1_system.txt` is updated in lockstep. A live
process boot (`cargo run -p parish -- --script
testing/fixtures/test_anachronism.txt`) loads the modified manifest
without error.

Verifier: `cli_script.txt` shows the process boots, mod loads, the
world graph initialises, scripted commands return well-formed JSON
including `npc_not_available` greetings (which only fire after NPCs
have been schedule-placed against the loaded location graph).

### A6 — Pre-existing contract unchanged

The unit test still asserts `Acts of Union` and `CULTURAL GUIDELINES`
appear. The pre-existing `FRESH PHRASING` anti-formula clause from
PR #984 is preserved.

Verifier: unit test continues to pass the original assertions.

## Out of scope

- A live Grok-4.3 re-judge of the affected dialogue samples. The
  user's spec marked this as "optional, skip if it takes more than 2
  min". The prompt edits land; the score-lift confirmation is a
  separate slice when a provider with cached samples is online.
- Per-occupation `{appropriate_role}` substitution. The current
  template engine has no NPC-roster lookup at render time, so the
  redirect line ships as the literal "Ye'd best ask the right person
  hereabouts" fallback the user spec explicitly allowed.
- The pre-existing `GA_IE_PHRASE_GUIDE` in `language_directive` —
  retained as the optional broader list appended only when
  `native = "ga-IE"`. The new whitelist is unconditional and narrower;
  the two are complementary.
