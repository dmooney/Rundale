# rundale-bench

Frozen, reproducible benchmark for in-character 1820 Irish gameplay inference. Drives model + provider choices in [`parish-config::presets::preset_models()`](../../crates/parish-config/src/presets.rs) and the broader cloud / local inference selection.

Spec + roadmap: [`docs/plans/rundale-bench.md`](../../../docs/plans/rundale-bench.md).

## Status

`v1-dev` — in-development. Phases 1-7 of the rollout have landed structurally but the dataset is intentionally undersized (155 prompts total vs the plan's 1100 target). The framework is complete and produces measured per-slice scores; freeze (`MANIFEST.json::frozen=true` + `git tag rundale-bench-v1.0`) waits until the corpus is grown to the planned size and the leaderboard has at least three independently-evaluated targets (one frontier cloud, one open-weight, one local MLX) on the holdout split.

| Phase | Status | Notes |
|---|---|---|
| 1 — Dataset freeze | landed | dialogue slice frozen with sha256-verified loader |
| 2 — Intent + grader | landed | 30 records (target 200); deterministic Jaccard grader; 70% label match on `gpt-oss-120b:free` smoke |
| 3 — Dialogue extend + pinned judge | landed (partial corpus) | 50 records added (target 400); `judge_v1` pinned at Claude Sonnet 4.6 with rubric_sha256 |
| 4 — Reaction + sim slices | landed (partial corpus) | 30 + 30 + 15 records (target 200 + 200 + 100); `judge_reaction_v1`, `judge_sim_v1` pinned |
| 5 — Holdout split | landed | deterministic id-hash split; `core` tier preserved in dev; encryption deferred to v1.0 freeze |
| 6 — Leaderboard | landed (seed row only) | append-only table; one seed row pre-split; needs broader sweep |
| 7 — v1.0 freeze | deferred | requires corpus growth + 3+ leaderboard rows before tag |

## Layout

```
parish/testing/rundale-bench/
├── README.md                   — this file
├── build_manifest.py           — rebuild MANIFEST.json after dataset edits
├── split_holdout.py            — deterministic dev/holdout split (20% via SHA-256 of id)
├── grade.py                    — graders (intent, schema, dialogue, reaction, simulation)
├── rundale_bench.py            — single-entry orchestrator
├── test_grade.py               — unit tests; run via `python3 test_grade.py`
└── v1/
    ├── MANIFEST.json           — slice hashes + Merkle root
    ├── dialogue.jsonl          — 150 dialogue records (Phase 1 + Phase 3 extension)
    ├── dialogue.holdout.jsonl  — 20% sealed-by-convention holdout
    ├── intent.jsonl            — 30 records with JSON gold labels (Phase 2)
    ├── intent.holdout.jsonl
    ├── reaction.jsonl          — 30 records, persona-varied first encounters (Phase 4)
    ├── reaction.holdout.jsonl
    ├── tier2-sim.jsonl         — 30 records, short-scene JSON simulation (Phase 4)
    ├── tier2-sim.holdout.jsonl
    ├── tier3-sim.jsonl         — 15 records, 6+ hour NPC batch JSON (Phase 4)
    ├── tier3-sim.holdout.jsonl
    ├── judge_v1.json           — pinned LLM judge for dialogue (rubric_sha256-verified)
    ├── judge_reaction_v1.json  — pinned judge for reaction in-character score
    └── judge_sim_v1.json       — pinned judge for sim plausibility
```

## Running the bench

```sh
# One slice on a target's dev split:
python3 parish/testing/rundale-bench/rundale_bench.py \
    --target 'openai/gpt-oss-120b:free@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \
    --suite v1 --slice intent --split dev --limit 30

# Full sweep — every slice, dev split:
python3 parish/testing/rundale-bench/rundale_bench.py \
    --target '<spec>' --suite v1 --slice all --split dev

# Holdout (gates leaderboard submission):
python3 parish/testing/rundale-bench/rundale_bench.py \
    --target '<spec>' --suite v1 --slice all --split holdout
```

Outputs land in `docs/proofs/rundale-bench/run_<target>_<slice>_<UTC>.json`. Append a row to `docs/proofs/rundale-bench/leaderboard.md` for each holdout sweep.

## Slice record schema

Each line of a `*.jsonl` slice is a JSON object:

```jsonc
{
  "id":       "dialogue-0001",         // stable, never reused; <slice>-NNNN
  "tier":     "core" | "extended",     // optional grouping (e.g. minimal smoke-test subset)
  "persona":  "brigid-midwife",        // who the model is being asked to be
  "prompt":   "I have been having…",   // user message exactly as sent to the model
  "schema":   { … },                    // optional JSON Schema the response must satisfy
  "gold":     { … }                     // optional ground-truth answer (for deterministic graders)
}
```

`schema` and `gold` are present for slices that have a deterministic or hybrid grader. The `dialogue` slice has neither — it is LLM-judge-only.

## Tiers

- **`core`** — minimal subset that must be re-run on every benchmark sweep (cheap smoke test).
- **`extended`** — full dataset for the slice; reported separately from `core`.

`gen_dlg.py` loads only `core`; `flaw_scan.py` loads the full slice. Both go through `eval_lib.load_slice`, which verifies the file's SHA-256 against `MANIFEST.json` on every load — drift surfaces as a `RuntimeError` rather than a silent score change.

## Manifest

`MANIFEST.json` records per-slice byte count, record count, and SHA-256, plus a single `merkle_root_sha256` over the sorted list of per-slice hashes. Rebuild after any intentional dataset edit:

```sh
python3 parish/testing/rundale-bench/build_manifest.py v1
```

Then commit `MANIFEST.json` alongside the slice change so reviewers can see the hash delta.

## Editing the dataset (before freeze)

While `frozen=false` (i.e. `v1-dev`), prompts can still be added/changed:

1. Edit the slice `*.jsonl`. New records use the next `id` in `<slice>-NNNN` sequence — never reuse an `id`.
2. Re-run the manifest builder (above) to refresh hashes.
3. Run the smoke probe to confirm scoring is sane:
   ```sh
   python3 parish/scripts/local-eval/flaw_scan.py \
       --target '<your target>' --prompts 25 --workers 2
   ```
4. Commit the slice change + manifest change in the same PR.

After `frozen=true`, edits require a new version directory (`v1.1/`, `v2/`).

## Contribution rules

- New prompts must stay in-character for 1820 rural Ireland. No anachronisms, no modern terms.
- Prompts must be safe to leak (the dev split is public). Truly hard probes belong in the sealed holdout once Phase 5 ships.
- One record per line, no embedded newlines (use `\n` in JSON strings).
- Run `python3 -c 'import json; [json.loads(l) for l in open(<path>)]'` on the slice before committing — a malformed line poisons every downstream grader.

## What's not yet here

- Holdout splits (Phase 5).
- Deterministic graders (`grade.py`, Phase 2+).
- Pinned LLM-judge (Phase 3).
- Bench orchestrator (`rundale_bench.py`, Phase 4).
- Leaderboard (Phase 6).

Each of these lands as its own incremental PR — see the [plan doc](../../../docs/plans/rundale-bench.md) for the phased rollout.
