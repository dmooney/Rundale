# Acceptance criteria — bench metadata strip (follow-up to #994)

## Background

The rebench surfaced that the bench sends raw model output to the judge,
including the `---` + JSON metadata block the runtime strips before
showing to the player. Grok-4.3 (as judge) penalises that scaffolding
as anachronistic, depressing scores by ~+5 points per record on
identical advice. See `docs/proofs/994-bench-prompt-mirror/rebench-findings.md`.

## Goal

Bench scores the dialogue the player actually sees — not the raw
model envelope. Same fix applies to both runtime output formats.

## Observable criteria

1. `grade.py` exports `extract_dialogue_for_judging(reply: str) -> str`
   that returns the player-visible dialogue line(s) from any of three
   reply shapes:
   - **Mod-template `---`/JSON format**: text before `\n---` is the
     dialogue; anything from `\n---` onward is metadata and is stripped.
   - **Runtime Rust-builder JSON-first format**: reply is a JSON object
     with a `"dialogue"` field; helper parses and returns that field.
   - **Legacy plain text** (OLD prompt era): no envelope detected;
     reply returned verbatim.
   The helper never raises — malformed JSON or unexpected shapes fall
   through to verbatim return so an unparseable envelope is judged as
   the model emitted it (which is also what the player would see if
   the runtime parser failed).

2. `grade_dialogue` and `score_multiaxis` send the extracted dialogue
   to the judge, not the raw reply. The `non_latin_chars` check still
   scans the full raw reply so non-Latin script leaks in metadata are
   still caught.

3. Unit tests in `test_grade.py` cover all three input shapes plus the
   edge cases:
   - `---` with no trailing JSON
   - JSON-first with no `dialogue` key
   - JSON-first with `dialogue` containing escaped quotes
   - Reply that starts with `---` (no preamble — return empty)
   - Trailing whitespace / multiple `---` lines

4. Re-running `score_multiaxis` against the NEW grok-4.3 cache
   produces a total ≥ 8.0 (matches the manually-stripped run in the
   #994 rebench bundle, ~8.21). Without the fix the same cache scored
   7.45 under grok-4.3 because of the scaffolding penalty.

## Out of scope

- Reconciling the two runtime envelope formats (mod template uses
  `---`/JSON; Rust builder uses JSON-first). Helper handles both
  because the bench reads the mod template but the runtime emits
  JSON-first. Aligning the two is a separate task.

- Re-baselining historic leaderboard rows. The old caches (OLD prompt
  era) had no envelope so their scores are unchanged; only NEW-prompt
  caches see the lift.
