# Judge verdict — NPC dialogue prompt grounding

## Review scope

Single slice on `worktree-agent-a2486a8d7cafdb452` against
`origin/main`, fixing three regression axes in NPC dialogue:

1. Historical drift (Aoife claimed teaching Irish was illegal in 1820)
2. Hallucinated Gaelic ("Slán abhaile go connachtú")
3. Persona drift (midwife replied as a wolf-tracker; modern register
   "healing properties")

Files changed (2):

- `parish/crates/parish-npc/src/lib.rs` —
  `build_tier1_system_prompt` gains four new clauses (lane-keeping,
  1820 fact preamble, Irish-phrase whitelist + improvisation guard,
  modern-register blacklist). Unit test
  `test_tier1_system_no_unsubstituted_placeholders` gains nine new
  contract assertions.
- `mods/rundale/prompts/tier1_system.txt` — mirror of the Rust prompt
  changes for the editor-live-reload path.

## Structural assessment

**Root-cause fix on each axis.**

- **Patch A (1820 fact preamble).** The pre-existing `HISTORICAL
  CONTEXT` paragraph mentioned the Acts of Union and "Catholic
  Emancipation has not yet been achieved" but said nothing about the
  Penal Laws against Catholic / Irish-language education. By 1820
  those laws had been repealed for 38 years, but the model defaulted
  to the common "Penal era" mental model and produced the secret-class
  framing. The new four-bullet preamble explicitly cites each date
  (1782, 1829, 1845) and names Daniel O'Connell's status, giving the
  model concrete dates to anchor against. This is dual-coding: the
  date *and* the political consequence, in the same bullet, so the
  reasoning chain stays grounded.
- **Patch B (Irish-phrase whitelist + improvisation guard).** The
  pre-existing `GA_IE_PHRASE_GUIDE` listed ~25 phrases and was
  appended only when `native = "ga-IE"`. The new guardrail fires
  unconditionally, lists exactly the eight phrases the user vetted,
  and adds an explicit "Do NOT invent or extend Irish phrases. Do NOT
  improvise Irish grammar" guard. Critically, the clause provides a
  *positive* fallback: Hiberno-English dialect markers (`ye`, `yer`,
  `'tis`, `mornin'`, `Mayhap`, `Aye`, `sure`). Without that
  alternative the model would silently confabulate to fill the
  flavour gap.
- **Patch C (lane-keeping + register blacklist).** The lane clause
  names the four roles in the active mod and gives a concrete redirect
  template. The register clause names eight specific modern words with
  period equivalents. Both are negative examples in the prompt — the
  same anti-formula technique that worked for the stock-closer fix in
  PR #984.

**Lockstep mod mirror.** The mod-shipped `tier1_system.txt` is updated
to the same text. The two are kept in sync because the editor live-
reload path (`parish/crates/parish-core/src/editor/live_reload.rs`)
reads from the mod file; if it drifted from the Rust hardcoded
template, an in-editor edit would silently disagree with the running
process.

**Test coverage.** The pre-existing unit test
`test_tier1_system_no_unsubstituted_placeholders` already asserted
contract clauses (`Acts of Union`, `CULTURAL GUIDELINES`). The patch
extends the same test with nine new substring assertions covering
every new clause. A future edit that strips any of the new content
trips the test intentionally — same pattern PR #984 established.

## Risks and follow-ups

- **No live LLM verification.** The user spec marked the Grok-4.3 re-
  judge as "optional, skip if it takes more than 2 min". A live run
  with the affected dialogue prompts ("teach the children", "lost a
  sheep last night", "my mother is taken with a bad cough", "safe
  home") would confirm the score lift from 2/5 toward 4/5. The
  current proof is structural (the prompt clauses ship into the
  rendered system prompt, the mod loads cleanly in a real CLI
  process, the unit tests pin the contract). A separate slice can
  add the score-lift transcript once a provider is online.
- **Prompt size budget.** The new clauses add ~80 lines of system-
  prompt text. Tier 1 dialogue uses prompt caching under most
  providers, so the steady-state cost is one-time per session, not
  per-turn. Token budget under `TIER1_DIALOGUE_MAX_TOKENS = 512`
  (PR #984) is unaffected — that cap is on *response*, not the
  system prompt.
- **`{appropriate_role}` substitution gap.** The lane clause ships
  with the literal "Ye'd best ask the right person hereabouts"
  redirect because the current template engine has no NPC-roster
  lookup at render time. A future enhancement could plumb a
  per-occupation referral table from the mod's `npcs.json`, but
  that's a separate data-plumb seam, not a prompt edit.
- **Pre-existing `GA_IE_PHRASE_GUIDE`.** Retained as the optional
  broader list appended only when `native = "ga-IE"`. The two are
  complementary: the new unconditional whitelist enforces the
  verbatim-only contract; the existing guide adds extra phrases when
  a native locale is configured. No conflict because the unconditional
  clause says "you MAY use these verbatim, and ONLY these" — narrowing
  the broader list when both apply.

## Acceptance criteria verification

Verifying each criterion from
[acceptance-criteria.md](./acceptance-criteria.md) against the
unit-test + live-CLI evidence:

- **A1 — 1820 fact preamble.** `cargo test -p parish-npc
  test_tier1_system_no_unsubstituted_placeholders` passes; the test
  asserts `Penal Laws` and `1782` appear in the rendered prompt.
  **Met.**
- **A2 — Irish-phrase whitelist + improvisation guard.** Same test
  asserts `ALLOWED IRISH PHRASES`, `Slán abhaile`, and `Do NOT invent
  or extend Irish phrases` all appear. **Met.**
- **A3 — STAY IN YOUR LANE.** Same test asserts substring
  `STAY IN YOUR LANE`. **Met.**
- **A4 — Modern-register blacklist.** Same test asserts `REGISTER:`
  and `healing properties`. **Met.**
- **A5 — Mod-shipped prompt mirror loads cleanly.** `cli_script.txt`
  shows the live process boots through mod-load, world-graph init,
  NPC schedule placement, scripted-command JSON envelopes. **Met.**
- **A6 — Pre-existing contract unchanged.** Same test continues to
  assert `Acts of Union` and `CULTURAL GUIDELINES`. The
  `FRESH PHRASING` anti-formula clause from PR #984 is preserved in
  the same `format!()` call. **Met.**

## Verdict

Verdict: sufficient

Acceptance criteria: met

Technical debt: clear

Remaining live-LLM follow-up (Grok-4.3 re-judge confirming the score
lift) is the same provider-online task the user's spec marked as
optional — not a debt in this slice.
