# Judge verdict — rundale-bench v1-dev Phase 1 (dataset freeze)

Verdict: sufficient

Technical debt: clear

The PR ships Phase 1 of the [rundale-bench plan](../../plans/rundale-bench.md): a frozen, content-addressed JSONL artifact for the dialogue prompt corpus, plus the loader plumbing and tamper-detection contract that subsequent phases (graders, judge pinning, leaderboard) will build on. No scoring change, no behavioural change — the probes load the same prompts they always did, from a single source of truth instead of two parallel inline lists.

## What was claimed and verified

1. **Single-source-of-truth corpus.** Both `flaw_scan.py` (100 prompts) and `gen_dlg.py` (5-prompt core set) now derive their `PROMPTS` from `parish/testing/rundale-bench/v1/dialogue.jsonl` via `eval_lib.load_slice`. The pre-freeze duplication is gone. `flaw_scan.PROMPTS[:5] == gen_dlg.PROMPTS` is asserted in the smoke run.
2. **Content-addressed integrity.** `MANIFEST.json` records SHA-256 + record count + byte count per slice, plus a Merkle-style root over the sorted slice hashes (`b2adfb38…c647e9`). `load_slice` verifies bytes-on-disk against the manifest before yielding records. A tampered slice raises `RuntimeError: rundale-bench/v1/dialogue.jsonl sha256 mismatch` rather than silently changing the probe corpus.
3. **Reproduction parity.** The PR #958 probe (`openai/gpt-oss-120b:free`, 25 prompts) replayed through the frozen JSONL produces 25/25 successful, 0 non-Latin leaks — at least as good as the pre-freeze 24/25 (the prior 1 flag was a transient API blip).
4. **Versioning path.** `build_manifest.py` refuses to mutate a frozen suite (`frozen=true` + hash drift = `SystemExit`). The contribution rules in `parish/testing/rundale-bench/README.md` lay out the dev workflow before freeze and the new-version-cut workflow after.
5. **Tier semantics.** Records carry `tier: "core" | "extended"`. `gen_dlg.py` loads only `core` (the canonical 5-prompt blind-judge set); `flaw_scan.py` loads the full slice. The split is enforced at import (`assert len(PROMPTS) == 5` in `gen_dlg.py`).

## Independent verification

- `parish/testing/rundale-bench/v1/MANIFEST.json` — `merkle_root_sha256` reproduces from `python3 parish/testing/rundale-bench/build_manifest.py v1`.
- `parish/scripts/local-eval/eval_lib.py::load_slice` — resolves `BENCH_ROOT` from `Path(__file__).resolve().parents[2] / "testing" / "rundale-bench"`. Verified by running the script from outside its own directory and observing the loader finds the slice.
- `parish/scripts/local-eval/flaw_scan.py` — `PROMPTS = [r["prompt"] for r in load_slice("dialogue", version="v1")]` followed by `assert len(PROMPTS) >= 100`. The previous 100-line inline list is removed; this is a pure relocation.
- `parish/scripts/local-eval/gen_dlg.py` — `PROMPTS = [r["prompt"] for r in load_slice("dialogue", version="v1", tier="core")]` followed by `assert len(PROMPTS) == 5`.
- Live smoke run produced `docs/proofs/rundale-bench/post_freeze_flaw_scan.md`: 25/25 successful, 0/25 flagged, $0.00 cost (OpenRouter free tier).

## Known limits

- `v1-dev` suite is **not yet frozen**. The plan's Phase 7 flips `frozen=true` and tags `rundale-bench-v1.0`. Until then, prompts can still be added or edited as long as the manifest is rebuilt in the same commit. Reviewers should treat manifest deltas as material changes.
- Only the `dialogue` slice exists. `intent`, `reaction`, `tier2-sim`, and `tier3-sim` land in Phase 2-4 as separate PRs.
- No deterministic grader yet (Phase 2). The dialogue slice will remain LLM-judge-only even at v1.0 freeze.
- No holdout split (Phase 5). Today's prompts are all public, so contamination risk is non-zero for any model trained after this commit lands.
- No bench orchestrator (`rundale_bench.py`) — `flaw_scan.py` and `gen_dlg.py` continue to be the user-facing entry points until Phase 4.

## Risk

Low. The change is a pure relocation of an existing prompt corpus into a hashed artifact, plus a verifying loader. No scoring change, no behavioural change. The largest concrete risk is path-resolution drift if `eval_lib.py` is later moved — `BENCH_ROOT` depends on the file's relative position in the tree, and would need to be re-validated. Mitigated by the smoke run that exercises `load_slice` from script imports.

## Approved.
