# Evidence — bench strips metadata envelope before judging

Evidence type: gameplay transcript

## Background

The #994 rebench surfaced a measurement bug: bench sent the raw model
output (dialogue + `---` + JSON metadata) to the judge, but the runtime
strips the metadata before showing the player. Grok-4.3 (as judge)
penalised the JSON scaffolding as anachronistic, depressing every NEW
prompt score by ~+0.76 points.

## Fix

`grade.py::extract_dialogue_for_judging(reply)` — handles both runtime
envelope formats (mod-template `---`/JSON and Rust-builder JSON-first)
plus the legacy plain-text fallback. Wired into:

- `grade.grade_dialogue` — strips before sending to judge_v1.
- `grade.grade_reaction` — same.
- `grade.grade_pairwise` — strips both replies before A/B compare.
- `score_multiaxis.score_one` — strips before multi-axis judging.
- `rubric_lab.absolute_judge` and `rubric_lab.pairwise_judge` — strips
  in both rubric-iteration modes.

`_non_latin` still scans the full raw reply so non-Latin script leaks
inside metadata are still flagged.

## Criterion → transcript mapping

Full transcript: [transcript.txt](transcript.txt).

### Criterion 1 — helper handles all three reply shapes

10 unit tests cover every shape and edge case:

```
OK   test_extract_dialogue_dash_marker
OK   test_extract_dialogue_dash_marker_trailing_whitespace
OK   test_extract_dialogue_dash_marker_no_trailing_block
OK   test_extract_dialogue_dash_at_start_returns_empty
OK   test_extract_dialogue_json_first
OK   test_extract_dialogue_json_first_with_escaped_quotes
OK   test_extract_dialogue_json_first_missing_dialogue_field
OK   test_extract_dialogue_legacy_plain_text
OK   test_extract_dialogue_malformed_json_falls_through
OK   test_extract_dialogue_empty_input
OK   test_extract_dialogue_multiple_dash_lines

38/38 passed
```

Pre-existing tests (28) still pass — no regressions in
`grade_dialogue`, `grade_reaction`, `grade_pairwise`, etc.

### Criterion 2 — judge sees stripped dialogue; non_latin still scans raw

The `extract_dialogue_for_judging` call inserted on the user-facing
prompt line; `_non_latin(reply)` still operates on the raw reply.
Verified by reading the diff at `grade.py::grade_dialogue` and
`grade.py::grade_reaction`.

### Criterion 3 — unit tests cover the edge cases

See criterion 1 transcript.

### Criterion 4 — rebench under the fix produces total ≥ 8.0

```
n=15  total=8.29
  character=8.13
  authenticity=8.93
  language=8.40
  responsiveness=8.20
  craft=7.80
```

Compared to the #994 bundle's three reference runs (same NEW cache,
same grok-4.3 judge):

| Run                                | Total | Notes |
|------------------------------------|-------|-------|
| OLD raw (#994 bundle, n=15)        | 8.09  | Pre-#994 prompt; no envelope. |
| NEW raw, **pre-fix** (#994 bundle) | 7.45  | Judge penalised JSON scaffolding. |
| NEW manual-stripped (#994 bundle)  | 8.21  | One-off strip script for comparison. |
| **NEW auto-stripped, post-fix**    | **8.29** | This bundle. Bench now does the strip. |

`8.29 ≥ 8.0` — criterion met. Recovers the +0.84 lift the scaffolding
penalty was hiding, and now slightly exceeds the OLD prompt score
(8.29 vs 8.09 = +0.20) — first apples-to-apples evidence that the new
runtime tier-1 grounding outperforms the old hardcoded bench prompt
under a discriminative judge.

### Spot-check: `dialogue-0011` (toothache case)

Same prompt, same model, same NEW-prompt reply — judged before and after
the fix:

| Stage           | Total | Judge's stated reason |
|-----------------|-------|------------------------|
| Pre-fix (raw)   | 3.4   | "Modern JSON metadata ruins period immersion and dialect is absent." |
| Post-fix (auto) | 6.6   | "Plausible remedies but lacks Hiberno-English idiom or midwife voice." |

Judge now grades the dialogue line; the +3.2-point swing is the
measurement artifact this fix removes. The 6.6 post-fix is itself a
real critique (no Hiberno-English idiom in the spoken text) that the
content of the reply earns on its own merits.
