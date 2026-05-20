Judge for bench-strip-metadata fix (#994 follow-up)

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Criterion 1 — helper exports + handles three reply shapes

`grade.py:26` defines `extract_dialogue_for_judging(reply: str) -> str`. The
independent one-liner exercising the helper returned `'hello'`, `'hi there'`,
`'plain text legacy reply'` — the exact expected outputs for the dash-marker,
JSON-first, and legacy plain-text shapes. Unit tests `test_extract_dialogue_dash_marker`,
`test_extract_dialogue_json_first`, and `test_extract_dialogue_legacy_plain_text`
(in `parish/testing/rundale-bench/test_grade.py`) cover the three required
shapes; malformed-JSON and empty-input cases confirm the helper never raises
and falls through verbatim.

## Criterion 2 — judge sees stripped dialogue; non_latin still scans raw

Reading `grade.py::grade_dialogue` (lines 306-314) confirms the user payload
sent to the judge is built from `dialogue = extract_dialogue_for_judging(reply)`
(line 309) while `nl = _non_latin(reply)` (line 314) still operates on the
raw reply. `grade_reaction` (line 357) and `grade_pairwise` (lines 415-416)
follow the same pattern. `score_multiaxis.score_one` (line 88) and
`rubric_lab.absolute_judge` / `pairwise_judge` (lines 77, 106-107) likewise
strip before judging — all 5 caller sites verified.

## Criterion 3 — unit tests cover edge cases

`python3 parish/testing/rundale-bench/test_grade.py` printed `38/38 passed`,
including the 10 new `test_extract_dialogue_*` cases that cover dash-with-no-
trailing-block, dash-at-start (empty), escaped quotes in JSON, missing
`dialogue` field, malformed JSON, empty input, trailing whitespace, and
multiple dash lines.

## Criterion 4 — rebench total ≥ 8.0

`multiaxis_NEW_grokjudge_postfix.json::aggregates."x-ai/grok-4.3".total_mean`
parses to 8.2933 (n=15), comfortably above the 8.0 threshold and a +0.84
recovery over the pre-fix 7.45. Subscores all plausible (character 8.13,
authenticity 8.93, language 8.40, responsiveness 8.20, craft 7.80).

## Independent re-checks

- `python3 parish/testing/rundale-bench/test_grade.py` → `38/38 passed`.
- One-liner against `extract_dialogue_for_judging` for all three envelopes →
  `'hello'`, `'hi there'`, `'plain text legacy reply'` as required.
- `grep -n "extract_dialogue_for_judging\|grade_dialogue\|grade_reaction\|grade_pairwise\|score_one\|absolute_judge\|pairwise_judge"`
  on `grade.py`, `score_multiaxis.py`, `rubric_lab.py` showed strip-calls at
  every user-facing payload line: `grade.py:309`, `grade.py:357`,
  `grade.py:415-416`, `score_multiaxis.py:88`, `rubric_lab.py:77, 106-107`.
  No raw `reply` reaches the judge.
- Read `grade.py::grade_dialogue` (lines 280-334) directly: stripped dialogue
  in the user prompt, raw reply in `_non_latin`. Matches AC.
- Parsed `multiaxis_NEW_grokjudge_postfix.json` aggregates →
  `total_mean = 8.293` ≥ 8.0.

Nothing flagged.
