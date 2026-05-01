# LLM Quality Evals Plan

> Back to [Docs Index](../index.md) | Sibling: [Promptfoo Pentest Plan](promptfoo-pentest-plan.md)

## Goal

Continuous, **quantitative** quality regression sensors over Tier 1 / Tier 2 NPC output — independent of, and complementary to, the existing pentest plan. The pentest plan red-teams for security; this plan tracks output *quality* over model swaps, prompt edits, and gameplay changes.

This is the deferred LLM-as-judge piece from Phase 3 of the harness-engineering plan ([PR #538](https://github.com/dmooney/Rundale/pull/538)). When that PR lands, Phase 3 introduces capture-on-green snapshot baselines + structural rubrics in `crates/parish-cli/tests/eval_baselines.rs`; those are computational sensors. This plan adds the inferential sensors the article calls out as the hardest piece of the Behaviour harness.

## Status

Proposed. No code yet. Once accepted, lands as a small standalone PR with a `just eval-quality` recipe and a starter rubric.

## Scope

### In scope

- **Tier 1 dialogue** (`mods/rundale/prompts/tier1_*.txt`) — judge dialogue against rubrics like:
  - "Sounds plausibly 1820 rural Ireland (no anachronistic words, idioms, or concepts)."
  - "Mood / relationship cues from the prompt are reflected in the reply."
  - "NPC stays in character (occupation, age, personality) across multiple turns."
  - "JSON sidecar is well-formed and the `action` field is a valid enum value."
- **Tier 2 simulation** (`mods/rundale/prompts/tier2_system.txt`) — JSON validity, mood enum validity, summary plausibility.
- **Intent parsing** (the `parse_intent` LLM call in `parish-input`) — exact-match accuracy on a curated corpus of player utterances.
- A **`just eval-quality`** recipe and a **`/eval-quality`** skill (sister to `/rubric`) to run the suite ad-hoc.
- A leaderboard JSON in `testing/evals/quality/leaderboard.json` capturing each (model, prompt-template-version, rubric) → score, so model swaps and prompt edits are visible.

### Out of scope

- Pentest / red-teaming — covered by [`promptfoo-pentest-plan.md`](promptfoo-pentest-plan.md).
- Function-calling output format change — see [ADR-020 NPC Tool Use](../adr/020-npc-tool-use.md). Once accepted, this plan's rubrics will move from "JSON is well-formed" to "tool call args validate against schema."
- Long-term-memory retrieval quality — see [ADR-021 NPC Memory Retrieval](../adr/021-npc-memory-retrieval.md). When that ships, the corpus will need scenarios that exercise recall.

## Approach

Promptfoo is already on the table for the pentest plan; reuse it. The two suites live side-by-side:

```
testing/evals/
├── baselines/                # Phase 3 — structural snapshots (already shipped)
│   ├── test_movement_errors.json
│   ├── test_walkthrough.json
│   └── test_all_locations.json
├── pentest/                  # promptfoo-pentest-plan.md
│   └── …
└── quality/                  # this plan
    ├── tier1.yaml            # Promptfoo config: prompts × providers × rubrics
    ├── tier2.yaml
    ├── intent.yaml
    ├── corpus/               # Curated player utterances + reference outputs
    │   ├── tier1-dialogue.jsonl
    │   ├── tier2-summary.jsonl
    │   └── intent-classification.jsonl
    └── leaderboard.json      # Append-only scoreboard
```

### Rubric judge

Use a frontier model (e.g. Claude Sonnet 4.6) as the judge by default. Rubrics live in YAML next to the prompts; each rubric is a single-line judge prompt plus a numeric threshold. Failures emit a one-line summary plus the offending sample in the run artifact.

### Determinism + reproducibility

- **Fixed seed on the model under test** when supported. When not (most cloud providers), run N=5 and report mean ± stdev.
- **Provider/model recorded** in every leaderboard row so a regression can be attributed.
- **Prompt template version** = git short-SHA of the prompt file.

## Plan

### Phase A — Wiring (1 PR, ~half a day)

1. `testing/evals/quality/` directory + `tier1.yaml` with a 10-sample corpus and one rubric ("plausible 1820 voice").
2. `just eval-quality` recipe that runs Promptfoo against `tier1.yaml`, writes a row into `leaderboard.json`, and exits non-zero if the rubric score drops below baseline.
3. CI job `quality-evals` (optional — may stay manual until cost is understood; if added, gate on a small fixed corpus only).
4. `.agents/skills/eval-quality/SKILL.md` mirroring `/rubric` but for inferential checks.

### Phase B — Coverage (incremental)

Add `tier2.yaml`, `intent.yaml`, expand corpus, add rubrics as you find recurring quality issues. Each rubric should answer "is there a kind of regression I keep catching by re-reading the JSON output myself?" and turn it into a judge prompt.

### Phase C — Leaderboard tooling

A small CLI (`scripts/eval-leaderboard.py` or extend `parish-cli`) that prints the leaderboard sorted by score, filters by model/prompt version, and diffs any two runs. Useful for prompt iteration.

## Critical files

- New: `testing/evals/quality/tier1.yaml`, `corpus/`, `leaderboard.json`
- New: `.agents/skills/eval-quality/SKILL.md`
- Edit: `justfile` (add `eval-quality` recipe)
- Edit (after [PR #538](https://github.com/dmooney/Rundale/pull/538) lands): `crates/parish-cli/tests/eval_baselines.rs` (cross-reference in module doc)
- Reference: `mods/rundale/prompts/tier1_system.txt`, `tier1_context.txt`, `tier2_system.txt`

## Verification

- `just eval-quality` — runs to completion in under a minute against the local provider; non-zero exit on rubric regression.
- Smoke: deliberately introduce an anachronism in `tier1_system.txt` ("smartphone"); rerun; confirm the rubric flags it and the leaderboard records the drop.
- `just check-doc-paths` — every path cited above exists by the time the plan ships.

## Related

- [ADR-008 Structured JSON LLM Output](../adr/008-structured-json-llm-output.md) — current output format the rubrics validate against.
- [ADR-018 NPC Multidimensional Intelligence](../adr/018-npc-intelligence-dimensions.md) — characterisation rubrics will lean on this.
- [ADR-020 NPC Tool Use](../adr/020-npc-tool-use.md) — if accepted, rubrics shift from JSON-shape to tool-args-shape.
- [Promptfoo Pentest Plan](promptfoo-pentest-plan.md) — sibling effort; share the harness wiring but keep corpora separate.
- `crates/parish-cli/tests/eval_baselines.rs` — Phase 3 structural sensors this plan complements.
