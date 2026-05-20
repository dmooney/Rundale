# rundale-bench v1-dev — Phase 1 (dataset freeze) evidence

Evidence type: gameplay transcript

Phase 1 of the [rundale-bench plan](../../plans/rundale-bench.md) promotes the ad-hoc dialogue prompt corpus into a versioned JSONL slice at `rundale-bench/v1/dialogue.jsonl` with content-addressed integrity via `MANIFEST.json`.

## What changed

- New `rundale-bench/v1/dialogue.jsonl` — 100 records (5 `core` + 95 `extended`), schema documented in [`rundale-bench/README.md`](../../../rundale-bench/README.md).
- New `rundale-bench/v1/MANIFEST.json` — per-slice SHA-256, byte count, record count, plus `merkle_root_sha256` over the sorted list of slice hashes. Suite version is `v1-dev`; `frozen=false`.
- New `rundale-bench/build_manifest.py` — rebuilds the manifest. Refuses to mutate a frozen suite.
- New `rundale-bench/README.md` — slice schema, tier semantics, contribution rules.
- `parish/scripts/local-eval/eval_lib.py` — adds `load_slice(slice_name, version, tier, verify=True)` which verifies the slice bytes against the manifest SHA-256 on every load. `RuntimeError` on mismatch.
- `parish/scripts/local-eval/flaw_scan.py` — drops the inline 100-prompt `PROMPTS` list, loads from `dialogue.jsonl`.
- `parish/scripts/local-eval/gen_dlg.py` — drops the inline 5-prompt list, loads `tier="core"` records from `dialogue.jsonl`.

## Why

Today the same 1820-Ireland dialogue prompts existed in two places (`flaw_scan.PROMPTS` and `gen_dlg.PROMPTS`) and were silently mutable. Any prompt drift between them would have been invisible at review time; any prompt change would have invalidated historical scores without trace. The freeze gives the prompt corpus a single source of truth, content-hashing for tamper detection, and a versioning path for principled future revisions.

This is the *smallest* phase of the rollout — no scoring change, no behavioural change, no new graders. It just teaches the existing probes to read from a frozen artifact.

## Manifest snapshot

```
merkle_root_sha256: b2adfb38834b760ba5ec33abd11eac4207fd700e777a64eca46293bf05c647e9
  dialogue.jsonl: 100 records, 13780 bytes, sha256=aa7265e93813a165…
```

## Reproduction parity

Re-running the same probe used to validate [PR #958](https://github.com/dmooney/Rundale/pull/958) (`openai/gpt-oss-120b:free` via OpenRouter, 25 prompts, 2 workers) now loads from the frozen JSONL:

```sh
python3 parish/scripts/local-eval/flaw_scan.py \
    --target 'openai/gpt-oss-120b:free@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \
    --output rundale-bench/artifacts/post_freeze_flaw_scan.md \
    --prompts 25 --workers 2
```

- Pre-freeze (PR #958): 24/25 successful, 1 transient API blip
- Post-freeze (this PR): 25/25 successful, 0 non-Latin script leaks, $0.00

Full transcript: [`post_freeze_flaw_scan.md`](post_freeze_flaw_scan.md).

The probe sources its 25 prompts from `load_slice("dialogue", version="v1")` and the loader verifies the on-disk hash before yielding records — any drift in the slice file (e.g. an editor adding a trailing newline) surfaces as a `RuntimeError: rundale-bench/v1/dialogue.jsonl sha256 mismatch` instead of a silent prompt change.

## Tamper-detection test

The smoke suite appends a stray byte to `dialogue.jsonl`, asserts that `load_slice("dialogue")` raises `RuntimeError` with `sha256 mismatch` in the message, and restores the original bytes from in-memory backup. Output:

```
sha256 mismatch detection: OK
flaw_scan.PROMPTS: 100
gen_dlg.PROMPTS:   5
```

`flaw_scan.PROMPTS[:5] == gen_dlg.PROMPTS` is asserted in the smoke as well, so the core/extended tier split is enforced at import time.

## Not yet done

- Other slices (`intent`, `reaction`, `tier2-sim`, `tier3-sim`) — Phase 2-4 of the plan.
- Holdout split — Phase 5.
- Pinned LLM-judge + bench orchestrator — Phase 3-4.
- Leaderboard — Phase 6.
- Freeze (`frozen=true`) — Phase 7. Suite is `v1-dev`; prompts can still be added/edited as long as the manifest is rebuilt in the same commit.
