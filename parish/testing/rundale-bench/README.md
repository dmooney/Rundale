# rundale-bench

Frozen, reproducible benchmark for in-character 1820 Irish gameplay inference. Drives model + provider choices in [`parish-config::presets::preset_models()`](../../crates/parish-config/src/presets.rs) and the broader cloud / local inference selection.

Spec + roadmap: [`docs/plans/rundale-bench.md`](../../../docs/plans/rundale-bench.md).

## Status

`v1-dev` — in-development. Phase 1 of the rollout (dataset freeze) is in progress; the dataset is **not** yet frozen. Once Phase 7 completes, `MANIFEST.json::frozen` flips to `true` and the repo is tagged `rundale-bench-v1.0`. Any change after that ships under a new version.

## Layout

```
parish/testing/rundale-bench/
├── README.md           — this file
└── v1/
    ├── MANIFEST.json   — slice hashes + Merkle root (sha256-of-concatenated-sha256s, filenames sorted)
    ├── dialogue.jsonl  — 100 prompts for the Dialogue slice (5 core + 95 extended)
    ├── intent.jsonl    — (Phase 2) 200 prompts with gold intent labels
    ├── reaction.jsonl  — (Phase 3)
    ├── tier2-sim.jsonl — (Phase 4)
    └── tier3-sim.jsonl — (Phase 4)
```

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
