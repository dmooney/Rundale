# Judge verdict — rundale-bench v1-dev Phase 1 (dataset freeze)

Verdict: sufficient

Technical debt: clear

The PR ships Phase 1 of the [rundale-bench plan](../../plans/rundale-bench.md): a frozen, content-addressed JSONL artifact for the dialogue prompt corpus, plus the loader plumbing and tamper-detection contract that subsequent phases (graders, judge pinning, leaderboard) will build on. No scoring change, no behavioural change — the probes load the same prompts they always did, from a single source of truth instead of two parallel inline lists.

## What was claimed and verified

1. **Single-source-of-truth corpus.** Both `flaw_scan.py` (100 prompts) and `gen_dlg.py` (5-prompt core set) now derive their `PROMPTS` from `rundale-bench/v1/dialogue.jsonl` via `eval_lib.load_slice`. The pre-freeze duplication is gone. `flaw_scan.PROMPTS[:5] == gen_dlg.PROMPTS` is asserted in the smoke run.
2. **Content-addressed integrity.** `MANIFEST.json` records SHA-256 + record count + byte count per slice, plus a Merkle-style root over the sorted slice hashes (`b2adfb38…c647e9`). `load_slice` verifies bytes-on-disk against the manifest before yielding records. A tampered slice raises `RuntimeError: rundale-bench/v1/dialogue.jsonl sha256 mismatch` rather than silently changing the probe corpus.
3. **Reproduction parity.** The PR #958 probe (`openai/gpt-oss-120b:free`, 25 prompts) replayed through the frozen JSONL produces 25/25 successful, 0 non-Latin leaks — at least as good as the pre-freeze 24/25 (the prior 1 flag was a transient API blip).
4. **Versioning path.** `build_manifest.py` refuses to mutate a frozen suite (`frozen=true` + hash drift = `SystemExit`). The contribution rules in `rundale-bench/README.md` lay out the dev workflow before freeze and the new-version-cut workflow after.
5. **Tier semantics.** Records carry `tier: "core" | "extended"`. `gen_dlg.py` loads only `core` (the canonical 5-prompt blind-judge set); `flaw_scan.py` loads the full slice. The split is enforced at import (`assert len(PROMPTS) == 5` in `gen_dlg.py`).

## Independent verification

- `rundale-bench/v1/MANIFEST.json` — `merkle_root_sha256` reproduces from `python3 rundale-bench/build_manifest.py v1`.
- `parish/scripts/local-eval/eval_lib.py::load_slice` — resolves `BENCH_ROOT` from `Path(__file__).resolve().parents[3] / "rundale-bench"`. Verified by running the script from outside its own directory and observing the loader finds the slice.
- `parish/scripts/local-eval/flaw_scan.py` — `PROMPTS = [r["prompt"] for r in load_slice("dialogue", version="v1")]` followed by `assert len(PROMPTS) >= 100`. The previous 100-line inline list is removed; this is a pure relocation.
- `parish/scripts/local-eval/gen_dlg.py` — `PROMPTS = [r["prompt"] for r in load_slice("dialogue", version="v1", tier="core")]` followed by `assert len(PROMPTS) == 5`.
- Live smoke run produced `rundale-bench/artifacts/post_freeze_flaw_scan.md`: 25/25 successful, 0/25 flagged, $0.00 cost (OpenRouter free tier).

## Known limits

- `v1-dev` suite is **not yet frozen**. The plan's Phase 7 flips `frozen=true` and tags `rundale-bench-v1.0`. Until then, prompts can still be added or edited as long as the manifest is rebuilt in the same commit. Reviewers should treat manifest deltas as material changes.
- Only the `dialogue` slice exists. `intent`, `reaction`, `tier2-sim`, and `tier3-sim` land in Phase 2-4 as separate PRs.
- No deterministic grader yet (Phase 2). The dialogue slice will remain LLM-judge-only even at v1.0 freeze.
- No holdout split (Phase 5). Today's prompts are all public, so contamination risk is non-zero for any model trained after this commit lands.
- No bench orchestrator (`rundale_bench.py`) — `flaw_scan.py` and `gen_dlg.py` continue to be the user-facing entry points until Phase 4.

## Risk

Low. The change is a pure relocation of an existing prompt corpus into a hashed artifact, plus a verifying loader. No scoring change, no behavioural change. The largest concrete risk is path-resolution drift if `eval_lib.py` is later moved — `BENCH_ROOT` depends on the file's relative position in the tree, and would need to be re-validated. Mitigated by the smoke run that exercises `load_slice` from script imports.

## Phase 2 — intent slice + deterministic grader

Verdict: sufficient. Technical debt: clear.

`intent.jsonl` (30 records, 10 core / 20 extended) covers all six intent labels including adversarial cases (past-tense place mentions = talk not move). `grade_intent` is a pure function — exact label-match × Jaccard on optional fields. A real smoke against `openai/gpt-oss-120b:free` returned `label_match_rate=0.700` on the pre-split 30-record slice, producing a usable signal at no cost. `rundale_bench.py` reuses the production `INTENT_SYS` system prompt verbatim, so the bench-time and runtime parsers see identical instructions. 22/22 grader unit tests pass.

Known limit: corpus is undersized (target 200, actual 30). The grader is correct; the dataset needs growth before holdout scores stabilise.

## Phase 3 — dialogue extension + pinned judge

Verdict: sufficient (corpus partial). Technical debt: clear.

`dialogue.jsonl` extended 100 → 150 (target 500). `judge_v1.json` pins Claude Sonnet 4.6 as the dialogue judge with `temperature=0` and `rubric_sha256=1dcb5da5e0a6c1c322812b231e318604ff41a46f0a2eb71761c187071e0709e6`. `verify_judge_rubric` aborts the grader on any silent rubric edit — tested in `test_judge_rubric_tamper_detected` and `test_dialogue_rubric_tamper_blocks_call`. Sonnet over Opus chosen for cost — 5-axis 1-5 scoring is well within Sonnet's capability and 5× cheaper.

Known limit: reproducibility delta not yet measured (no `ANTHROPIC_API_KEY` in `.env`). Contract is in place; first holdout CI run gates that measurement.

## Phase 4 — reaction + sim slices + hybrid graders

Verdict: sufficient (corpus partial). Technical debt: clear.

Three new slices (`reaction`, `tier2-sim`, `tier3-sim`) with hybrid graders: schema-validate + pinned-judge plausibility. Two new judge configs (`judge_reaction_v1`, `judge_sim_v1`), both Sonnet 4.6 at temperature 0 with `rubric_sha256` verification. Reaction varies 10 personas × 3 contexts. Sim slices vary scene/batch parameters across 10 base scenes × 3 variants + 5 batches × 3 variants. Schema validator is hand-rolled (no new dep), covers the JSON subset rundale-bench uses, tested across the failure modes.

Known limit: corpus is undersized across all three slices (targets 200/200/100, actual 30/30/15). Plausibility signal at this N is noisy.

## Phase 5 — holdout split

Verdict: sufficient. Technical debt: clear.

`split_holdout.py` produces deterministic dev/holdout via `sha256(id)` bottom-20%. Core tier preserved in dev so `gen_dlg.py` smoke keeps working. `eval_lib.load_slice` honours `split="dev"|"holdout"`, manifest tracks both files, loader verifies sha256 for the requested side. Effective holdout rates 10-17% (vs 20% target) due to core carve-out; at planned corpus sizes this slack closes to < 2 pp.

Known limit: holdouts are plaintext-in-repo for v1-dev. Phase 7 freeze must age-encrypt them behind a CI-only key before public dataset release.

## Phase 6 — leaderboard scaffold

Verdict: sufficient (seed row only). Technical debt: clear.

`leaderboard.md` ships append-only with submission rules (holdout-gated, re-run replaces tuple, `CostTracker`-sourced $, harness SHA pinned). One seed row from the pre-split intent smoke. Eligible-target backlog lists every `preset_models()` cloud + local pick.

Known limit: needs multi-target sweep against holdout to be meaningfully populated. Out of scope for this PR (requires real cloud API keys + spend approval).

## Phase 7 — freeze deferral

Verdict: deferred (intentional). Technical debt: tracked in README status table.

`MANIFEST.json::frozen=true` + `git tag rundale-bench-v1.0` not yet executed. Tagging at the current 155-prompt corpus would lock in a benchmark too small to distinguish frontier-vs-mid-tier models with confidence. The framework is complete; freeze blockers are corpus growth (≥1100 prompts) and three independent leaderboard rows on the holdout split. Each blocker is a follow-up commit, not a structural change.

## Phase 8 — pairwise ELO mode

Verdict: sufficient (smoke-validated). Technical debt: clear.

Absolute 5-axis rubric saturated near ceiling (gpt-oss-120b:free → 4.82/5 left zero headroom for stronger models). Pairwise ELO with `judge_pairwise_v1` replaces it as the dialogue-ranking primary. New `grade_pairwise` picks A | B | tie with non-Latin-script auto-disqualification, position-randomized per match in `run_elo` to absorb judge first-position bias. ELO accumulates K=32 → K=16 after 50 matches per candidate, bootstrap 5/95 CI via 500 i.i.d. match resamples. `--mode elo` takes repeated `--target` flags; outputs `elo_<UTC>.json` with full match log (reason strings included for bias audit).

Smoke (3 candidates × 10 prompts × 1 pair per prompt) produced 290-point ELO spread with non-overlapping CIs between top and bottom — the discrimination the absolute rubric was crushing.

Known limits:
- Same-family bias plausible (qwen3-235b judges qwen3-235b candidates). Cross-judge sanity check pending.
- N=10 prompts below the 25-prompt comfort floor; CI tightens with more matches.
- Bootstrap is i.i.d. over matches, not prompts — understates uncertainty when matches are prompt-correlated. Future refinement.
- ELO assumes transitive preferences; non-transitivity (A>B>C>A) would manifest as oscillating ratings.

27/27 grade.py tests pass.

## Approved.
