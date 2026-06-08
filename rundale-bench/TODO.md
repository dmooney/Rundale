# rundale-bench TODO

> Backlog for the eval pipeline. Separate from `/TODO.md` (game demo loop) and the gemma4-rundale training plan.
> Last updated: 2026-06-07 (epic #1206 harness-hardening sweep).

## P0 — blocks correctness or wastes wall-time

- [x] **Pydantic on Bundle / Result / Summary shapes.** Done in #1213 — `schemas.py` ships `BundleSchema` / `ResultSchema` / `SummarySchema` / `ResultItemSchema`; `judge_bundle.write_pending` validates `BundleSchema` and `validate_result` validates `ResultSchema`. All 5 failure modes (None-in-format, bench_bug-with-nonzero-axes, rubric_sha256 mismatch/absence, missing required field, malformed shape) are enforced by construction. Covered by `test_schemas.py` (25 tests).
- [x] **Round-4 drain.** Re-launched sweep (bjj7xl994) after `metric_from_summary` None-handling fix. When it lands: dispatch wave-N subagents per slice across all 5 models, ingest --finalize, regen leaderboard + bench-site, commit. Pattern mirrors round-3 commit 4e4fda2b.
- [x] **bench-bug = 0 axes invariant** enforced in `judge_bundle.py validate_item`. Currently a comment-level convention — three round-3 wave-1 done files violated it, blocked ingest, required manual fixes. Pydantic above subsumes this, but add as standalone validator if Pydantic refactor slips.

## P1 — quality + signal

- [ ] **Outlines / lm-format-enforcer integration for MLX serve.** _Deferred (epic #1206): needs a live MLX serve + grammar authoring + re-bench of Phi-3.5/DS-R1; no MLX runtime in the hardening sandbox._ Constrains generation to a per-slice JSON grammar. Eliminates ~30% of observed bench-bugs (truncated mid-JSON metadata, format-meta, chain-of-thought leaks bypassing JSON extraction). Targets: Phi-3.5-mini (currently ~90% bench-bug rate → would become benchmark-able), DS-R1 distills (currently 100% bench-bug → would salvage scoring). Trade: ~10-20% throughput hit + grammar files per slice.
  - Grammar files needed: `grammars/dialogue.lark`, `grammars/reaction.lark`, `grammars/tier-sim.lark`, `grammars/gaeilge.lark`, `grammars/intent.lark`.
  - Acceptance: re-bench Phi-3.5-mini + DS-R1-Distill-Llama-8B; bench-bug rate drops below 20%; overall scores produce real numbers.
- [x] **Subagent post-validate wrapper.** Done (#1206) — `judge_bundle.recover_result(done_path, reply_text)`: if the done file is absent/unparseable it regex-extracts the outermost JSON object from the reply text (via `extract_json`), writes it so the queue is consistent for ingest, and raises `ValueError` when nothing is recoverable so the caller can retry the agent. Covered by `test_judge_bundle.py` (6 tests).
- [ ] **`code-switch` slice** (new). _Deferred (epic #1206): net-new Irish-language corpus authoring needing a fluent-Irish review pass — a content-design task, out of scope for tooling hardening. The slice/judge plumbing (judge-config shape) already supports adding it._ Measures bidirectional Irish/English register-switch per `docs/plans/gemma4-rundale-training-plan.md` Phase 2 prep:
  - Player switches mid-conversation → NPC continues in matching language.
  - NPC Irish quality in switched mode (reuses gaeilge judge axes).
  - NPC Irish-idiom-drops in English mode.
  - Refusal/clarification when player addresses Irish-only NPC in English (and reverse).
  - Pre-register slice + judge BEFORE Phase 2 training to avoid post-hoc gaming.
- [ ] **Gaeilge slice expansion.** _Deferred (epic #1206): net-new Irish-language corpus authoring (multi-turn, Connacht/Munster register, 1820 idiom) needing a fluent-Irish review pass — content design, not tooling._ Currently 11 prompts. Add (a) multi-turn conversational, (b) Connacht-marked vs Munster-marked register distinction, (c) period-1820 idiom subset. EuroLLM scores 4.02 on current slice — need wider/harder to differentiate top tier.

## P2 — infra + ergonomics

- [x] **`MLX_VENV` env-var documented in README.** Done (#1206) — `README.md` "Local MLX sweep" section now has an "MLX venv (`MLX_VENV`)" subsection documenting the env-var override (the code already read it at `local_runner.py:62`; the docs lagged). Auto-detect of uv tools paths is intentionally NOT added — explicit `MLX_VENV` is the deterministic answer and a path-search heuristic risks selecting the wrong venv silently. Covered by `test_doc_drift.py::test_mlx_venv_documented_in_readme`.
- [x] **Runtime RAM-cap kill switch.** Done (#1206) — `local_runner.py --max-ram-gb N`: `ram_cap_exceeded()` predicate + `RamSampler(max_ram_gb=, on_breach=)` latches `breached` and SIGKILLs the server the instant a live sample crosses the ceiling; the sweep loop then skips the candidate's remaining slices. Default disabled. Covered by `test_local_runner.py` (`ram_cap_exceeded` + `RamSampler` breach-latch tests).
- [x] **Bundled-slice metric surfaces `pending_judge` warning.** Done in #1273 — `build_leaderboard_page.py` renders `(pending_judge)` on the leaderboard row (`build_leaderboard_page.py:302`) and `metric_from_summary` prefixes it on the local row.
- [x] **Tokenizer audit script** (`tokenizer_audit.py`). Done (#1206) — `tokenizer_audit.py` with pure `tokens_per_char` + corpus-weighted `summarize_audit` aggregators (tested) plus a `count_tokens`/`audit_corpora` path that lazy-imports `transformers` and degrades gracefully when it (or HF auth) is absent. Running the live tokenize across the candidate bases still needs `transformers` + model downloads (external infra). Covered by `test_tokenizer_audit.py`.
- [x] **Per-slice cost ledger.** Done (#1206) — `local_runner.slice_cost_ledger(rows)` sums `cost_usd` per slice (+ a `total` rollup) and surfaces `judge_compute_minutes` when rows carry it, so the otherwise-invisible Sonnet-subagent compute is legible. Emitted as a `cost_ledger` block in the per-sweep `local_<stamp>.json`. Covered by `test_local_runner.py` (sums + judge-minutes tests).

## P3 — nice-to-have

- [x] **HF preflight script.** Done (#1206) — `preflight.py` with a pure, offline `classify_repo(config, files)` returning `{ok, is_multimodal, has_chat_template, reasons}` (rejects VL/multimodal architectures + missing `chat_template`) plus an optional `fetch_repo` network path that lazy-imports `huggingface_hub`. CLI: `python3 rundale-bench/preflight.py <repo>`. Approximate weights-size gating is left to the existing `peak_ram_gb_est` fitness check. Covered by `test_preflight.py`.
- [x] **Disk-cleanup discipline as TOML metadata.** Done (#1206) — `candidates_schema.py` accepts an optional `delete_after_bench: bool` per candidate and exposes `candidates_to_delete(rows)`; `local_runner.py` records the flagged repos in the per-sweep `local_<stamp>.json` (`delete_after_bench` list) so a finaliser can honour them. Covered by `test_candidates_schema.py`. (The actual `huggingface_hub.delete_cache` eviction runs only inside a live sweep.)
- [ ] **Round 5 / round 6 candidate pre-registration.** _Deferred (epic #1206):_ a planning note (which HF repos to target next), not code. When current backlog clears, next sweeps target: (a) Qwen3-VL-disabled variants if mlx-community ships them, (b) ExaONE-Deep when MLX upload appears, (c) Gemma-3 text-only variants if released, (d) Marco-o1 family, (e) Yi-1.5-34B if RAM cap allows.
- [x] **Bench-site model-detail page**: bench-bug rate. Done in #1273 — `build_site_data.py` emits `bench_bug_rate` per model (`build_site_data.py:636`) and `build_leaderboard_page.py` renders a bench-bug-rate column. (Any further Svelte detail-page styling is front-end polish in `bench-site/`.)
- [x] **Reproducibility manifest.** Done in #1272 — `repro_manifest.py` captures `harness_sha` + tool/runtime versions + model SHA per sweep. Covered by `test_repro_manifest.py`.

## 2026-05-28 hardening: Sonnet-only judging enforced

User caught that `local_runner.py` was defaulting `--judge judge_v1` (= Qwen3-235B via OpenRouter), inflating round-4 dialogue scores by ~1.5 points vs Sonnet's strict calibration. Round 4 OLMo-2-7B was 4.78 under Qwen; Sonnet scored 3.28 on the same replies. Other round-4 deltas: Tulu-3 4.60 → 2.82, Ministral-8B 4.44 → 2.86, OLMo-2-13B 4.56 → 2.96.

Locked down to make it impossible:

- `rundale_bench.load_judge` HARD-FAILS at load time if `judge_via != "claude-code-subagent"` OR `model != "claude-sonnet-4-6"` OR `api_key_env != null`. Every code path that judges goes through `load_judge`. One chokepoint.
- `local_runner.py --judge` default now `judge_sonnet_v1`.
- `_JUDGE_ALIASES` map dropped the `qwen → judge_v1` alias; added `dialogue|reaction|sim|gaeilge` aliases that all resolve to Sonnet-subagent configs.
- HTTP-API judge configs `judge_v1.json` and `judge_pairwise_v1.json` renamed to `.disabled-2026-05-28-sonnet-only` so they can't be loaded even if someone re-types the id.
- Round-4 dialogue run files patched in-place: Sonnet axes overwrite the inline Qwen scores. Older redundant dialogue runs for the same model deleted.

Tests added (informal — run from `python -c`):

- `load_judge('judge_sonnet_v1', 'v1')` → OK
- `load_judge('judge_v1', 'v1')` → FileNotFoundError (file disabled)
- A synthetic non-subagent config with `judge_via='http'` → ValueError REFUSED.

## Done — rounds 1-4 reference

- [x] **Round 1** — first 9-model MLX sweep, set up the pipeline (commit a20c9fad).
- [x] **Round 2** — 7-model sweep with subagent-only judges (commit 4abacf1f).
- [x] **Round 3** — 8-model sweep, slightly larger tier (commit 4e4fda2b). Surfaced DS-R1 + Phi-3.5-mini bench-bug patterns.
- [x] **Round 4** — 5-model sweep, recent smaller/quantized (in-flight 2026-05-28; will commit on land).
- [x] Subagent judges across all 5 slices (commit a20c9fad). Sonnet-subagent = $0 bench-it.
- [x] Leaderboard reads `run_*_all_*.json` (same shape local + cloud).
- [x] Local model logos + pretty names on bench-site.
- [x] `metric_from_summary` None-handling fix (round-4 hotfix; pending its own commit).

## Notes

- **Judge is Sonnet 4.6 via subagent in every mode.** No HTTP-API judging in rundale-bench.
- **OOM kills Claude Code remotely.** Conservative `peak_ram_gb_est` overstated vs actual. `--headroom-gb 26` default for the 48 GB M5 Pro (= 22 GB available cap).
- **Pre-existing HF cache models** (`Qwen2.5-14B`, `Qwen3.6-27B` at session start) **preserved**. Round-N fresh downloads deleted post-bench to stay under disk + RAM caps.
- **Phase 1 / Phase 2 training plan** lives in `docs/plans/gemma4-rundale-training-plan.md`. Bench feature work that supports the plan (gaeilge expansion, code-switch slice, tokenizer audit) is scheduled in P1 / P2 above.

## Technical debt ledger

Structured tracking layer parallel to the priority backlog above. Items here are architectural debt that doesn't block any single sweep but compounds with each new feature.

### Open

| ID     | Category   | Severity | Location                  | Description                                                                                                                                                                                                                                                                                      |
| ------ | ---------- | -------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| TD-001 | Complexity | P1       | `rundale_bench.py:1-1163` | The main orchestrator owns CLI parsing, target/catalog resolution, all slice runners, judge dispatch, artifact writing, ingest/finalize, and aggregation in one file. Split per-slice runners, CLI commands, artifact I/O, and ingest/finalize into modules before adding more benchmark phases. |

### In Progress

_(none)_

### Done

| ID     | Category             | Resolution                                                                                                                                                                                                                                                                             |
| ------ | -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-004 | Config Schema        | #1206 — `candidates_schema.py` (`validate_candidates`) checks unique `hf_repo`, required fields, positive `peak_ram_gb_est`, valid `slot`; `local_runner.py` runs it at startup and refuses a malformed fleet. Covered by `test_candidates_schema.py`.                                 |
| TD-005 | Weak Tests           | #1206 — `local_runner.py` now imports cleanly without psutil (lazy import), and the pure helpers (`ram_cap_exceeded`, `slice_cost_ledger`, `fitness_check`, `metric_from_summary`) plus the `RamSampler` breach latch are unit-tested in `test_local_runner.py` without launching MLX. |
| TD-006 | Stale Docs           | #1206 — README + AGENTS now state the live total (309 records = 270 dev + 39 holdout, sourced from `MANIFEST.json`) instead of the stale 155, and `test_doc_drift.py` parses the manifest and fails if the docs drift again.                                                           |
| TD-002 | Complexity           | #1284 — obsolete: `build_site_data.py` was deleted when the v1 bench-site was retired. The v2 site (`promptfoo/bench-site/`) reads `leaderboard.jsonl` directly, no Python aggregation step to split.                                                                                  |
| TD-003 | Generated Data Drift | #1284 — obsolete: the large generated `bench-site/src/data/bench.json` no longer exists. The v2 site reads the append-only `leaderboard.jsonl` at build time, so there is no committed derived-data artifact to drift.                                                                 |

### Progress Log

- 2026-05-25 - Initialized the rundale-bench debt ledger after scanning the Python bench runner, static-site pipeline, v1 dataset/config files, local MLX runner, and bench-site data artifact.

_2026-06-04 audit: 2 of 14 unchecked items verified done (Round-4 drain via commit c3bcd609; bench-bug=0 axes invariant enforced in judge_bundle.py:209-223). 1 item marked partial (MLX_VENV env-var in local_runner.py but not in README). Remaining 11 items still open._

- **2026-06-06**: Re-audit vs current code. Resolved->Done: none. Still open: TD-001 (rundale_bench.py 1771 LOC), TD-002 (build_site_data.py 1080 LOC), TD-003 (bench.json freshness gate), TD-004 (candidates_local_mlx.toml schema test), TD-005 (local_runner.py RamSampler/fitness_check untested), TD-006 (README/AGENTS say 155 prompts vs MANIFEST 309).
- **2026-06-07** (#1284): Retired the v1 bench-site. TD-002 + TD-003 are obsolete — `build_site_data.py` and the generated `bench-site/src/data/bench.json` are gone; the v2 site (`promptfoo/bench-site/`) reads `promptfoo/leaderboard/leaderboard.jsonl` directly. Only TD-001 remains open.

- **2026-06-07** (epic #1206 harness-hardening sweep): Resolved -> Done: TD-004 (`candidates_schema.py` + startup validation), TD-005 (`local_runner.py` lazy psutil import + pure-helper/`RamSampler` unit tests), TD-006 (doc count fixed to 309 + `test_doc_drift.py` guard). Also shipped, from the priority backlog: subagent post-validate wrapper (`recover_result`), runtime RAM-cap kill switch (`--max-ram-gb`), per-slice cost ledger, `MLX_VENV` README docs, HF preflight (`preflight.py`), tokenizer-audit scaffold (`tokenizer_audit.py`), and `delete_after_bench` TOML metadata. Verified already-done from prior PRs: Pydantic schemas (#1213), pending_judge + bench-bug-rate surfacing (#1273), reproducibility manifest (#1272). Still open: TD-001/TD-002/TD-003 (large structural refactors / generated-data freshness gate); Outlines/lm-format-enforcer + code-switch slice + Gaeilge expansion + round-5/6 pre-registration (need live MLX / Irish-corpus authoring / planning — all annotated `Deferred (epic #1206)` above).

## Issue tracking

2026-06-04 audit: open items tracked under epic #1206 (rundale-bench harness hardening).

2026-06-06 re-audit: all six TD items still open, tracked under epic #1206 (rundale-bench harness hardening), which remains open and now lists them explicitly.

2026-06-07: epic #1206 harness-hardening sweep cleared the tractable backlog (see progress log). TD-001/TD-002/TD-003 + the live-MLX / Irish-corpus items remain open under #1206.
