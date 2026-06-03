# rundale-bench TODO

> Backlog for the eval pipeline. Separate from `/TODO.md` (game demo loop) and the gemma4-rundale training plan.
> Last updated: 2026-05-28 (post round-4 sweep launch).

## P0 — blocks correctness or wastes wall-time

- [ ] **Pydantic on Bundle / Result / Summary shapes.** This session's failures (None-in-format crash, `bench_bug=true` with nonzero axes, rubric_sha256 mismatch, missing required field) all caught at write-time instead of ingest-time. Big surface coverage from one ~1-day refactor.
  - `validate_result` already exists in `judge_bundle.py`; promote to `BundleSchema` / `ResultSchema` / `SummarySchema` Pydantic models. Validate at every write boundary (orchestrator → pending, subagent → done, ingest → judgments). Reject malformed before fanout.
  - Acceptance: 5 failure modes from this session become impossible by construction (not just caught earlier).
- [ ] **Round-4 drain.** Re-launched sweep (bjj7xl994) after `metric_from_summary` None-handling fix. When it lands: dispatch wave-N subagents per slice across all 5 models, ingest --finalize, regen leaderboard + bench-site, commit. Pattern mirrors round-3 commit 4e4fda2b.
- [ ] **bench-bug = 0 axes invariant** enforced in `judge_bundle.py validate_item`. Currently a comment-level convention — three round-3 wave-1 done files violated it, blocked ingest, required manual fixes. Pydantic above subsumes this, but add as standalone validator if Pydantic refactor slips.

## P1 — quality + signal

- [ ] **Outlines / lm-format-enforcer integration for MLX serve.** Constrains generation to a per-slice JSON grammar. Eliminates ~30% of observed bench-bugs (truncated mid-JSON metadata, format-meta, chain-of-thought leaks bypassing JSON extraction). Targets: Phi-3.5-mini (currently ~90% bench-bug rate → would become benchmark-able), DS-R1 distills (currently 100% bench-bug → would salvage scoring). Trade: ~10-20% throughput hit + grammar files per slice.
  - Grammar files needed: `grammars/dialogue.lark`, `grammars/reaction.lark`, `grammars/tier-sim.lark`, `grammars/gaeilge.lark`, `grammars/intent.lark`.
  - Acceptance: re-bench Phi-3.5-mini + DS-R1-Distill-Llama-8B; bench-bug rate drops below 20%; overall scores produce real numbers.
- [ ] **Subagent post-validate wrapper.** Agent tool sometimes prints JSON in reply text instead of calling Write (DS-R1-Qwen tier3-sim, round 3). Orchestrator should: (1) check expected output file exists after Agent return, (2) if missing, regex-extract JSON from reply text, (3) write file, (4) retry agent with corrective prompt if extraction fails. Saves a manual recovery step per ~5-10 dispatches.
- [ ] **`code-switch` slice** (new). Measures bidirectional Irish/English register-switch per `docs/plans/gemma4-rundale-training-plan.md` Phase 2 prep:
  - Player switches mid-conversation → NPC continues in matching language.
  - NPC Irish quality in switched mode (reuses gaeilge judge axes).
  - NPC Irish-idiom-drops in English mode.
  - Refusal/clarification when player addresses Irish-only NPC in English (and reverse).
  - Pre-register slice + judge BEFORE Phase 2 training to avoid post-hoc gaming.
- [ ] **Gaeilge slice expansion.** Currently 10 prompts. Add (a) multi-turn conversational, (b) Connacht-marked vs Munster-marked register distinction, (c) period-1820 idiom subset. EuroLLM scores 4.02 on current 10-prompt slice — need wider/harder to differentiate top tier.

## P2 — infra + ergonomics

- [ ] **`MLX_VENV` env-var documented in README.** This session repeatedly hit "psutil missing — run inside .venv-mlx". Right answer is `MLX_VENV=/Users/.../vllm-mlx`. Add to `rundale-bench/README.md` setup section + auto-detect in `local_runner.py` (search common uv tools paths if `.venv-mlx` symlink missing).
- [ ] **Runtime RAM-cap kill switch.** `local_runner.py`'s `RamSampler` measures but doesn't enforce. Add `--max-ram-gb` flag: if peak RSS exceeds N, SIGKILL the mlx_lm.server and skip the candidate. Defends against OOM that kills Claude Code remotely (per round-3 user note).
- [ ] **Bundled-slice metric surfaces `pending_judge` warning.** `metric_from_summary` now prefixes ` (pending_judge)` when summary flags it (round-4 fix). Add equivalent on leaderboard.md row — current rows show 0.00 for pending without indication, looks like a failed run.
- [ ] **Tokenizer audit script** (`tokenizer_audit.py`). Measure tokens/char on Hyde Irish-side + Brooke Irish-side across candidate bases (Gemma 4 9B, Qwen3-14B, EuroLLM-9B, OLMo-2-13B, Mistral-Small-24B-2501). Gates base-model pick per Phase 1 plan. Cheap (no fine-tune needed, just tokenize + count).
- [ ] **Per-slice cost ledger.** Currently `cost.usd` on cloud rows, $0.0000 on local. Bundled-judge rows hide the Sonnet-subagent compute cost (it's not $0 in reality, it's amortised against the Claude Code session). Surface `judge_compute_minutes` or similar.

## P3 — nice-to-have

- [ ] **HF preflight script.** Mechanical: given an mlx-community repo URL, check (a) architecture is not VL/multimodal, (b) `chat_template` present, (c) approximate weights size. Round-3 + round-4 this was done ad-hoc via subagent; codify as `rundale-bench/preflight.py <repo>` + manifest cache.
- [ ] **Disk-cleanup discipline as TOML metadata.** Add `delete_after_bench: bool` per candidate in `candidates_local_mlx.toml`. Honor it in `local_runner.py` finalisation — `huggingface_hub.delete_cache` for the slot. Currently this is done manually post-sweep, error-prone.
- [ ] **Round 5 / round 6 candidate pre-registration.** When current backlog clears, next sweeps target: (a) Qwen3-VL-disabled variants if mlx-community ships them, (b) ExaONE-Deep when MLX upload appears, (c) Gemma-3 text-only variants if released, (d) Marco-o1 family, (e) Yi-1.5-34B if RAM cap allows.
- [ ] **Bench-site model-detail page**: add "bench-bug rate" column. Reading "DS-R1-Llama-8B: 0.00 overall" doesn't communicate WHY (chain-of-thought leak). Surfacing "10/10 bench-bugs" makes the failure mode legible.
- [ ] **Reproducibility manifest.** Per-sweep, capture (`harness_sha`, `mlx_lm version`, `vllm-mlx version`, `MLX runtime version`, model SHA from HF) into a single file alongside `local_<stamp>.json`. Right now `harness_sha` alone isn't enough to reproduce a run year-over-year.

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

| ID     | Category             | Severity | Location                                                | Description                                                                                                                                                                                                                                                                                                                         |
| ------ | -------------------- | -------- | ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Complexity           | P1       | `rundale_bench.py:1-1163`                               | The main orchestrator owns CLI parsing, target/catalog resolution, all slice runners, judge dispatch, artifact writing, ingest/finalize, and aggregation in one file. Split per-slice runners, CLI commands, artifact I/O, and ingest/finalize into modules before adding more benchmark phases.                                    |
| TD-002 | Complexity           | P2       | `build_site_data.py:1-753`                              | Static-site data aggregation mixes artifact discovery, proof-run fallback discovery, model/provider/family/price enrichment, demo-profile cost shaping, and leaderboard row generation in one file. Split source discovery, catalog enrichment, profile enrichment, and output shaping so the site data contract is easier to test. |
| TD-003 | Generated Data Drift | P2       | `bench-site/src/data/bench.json:1-11690`                | The checked-in site data is a large generated artifact with no cheap freshness gate in the normal unit-test path. Add a deterministic `build_site_data.py --check` or test fixture that fails when committed site data is stale relative to its intended input directories.                                                         |
| TD-004 | Config Schema        | P2       | `candidates_local_mlx.toml:1-730`                       | The local MLX fleet catalog carries model IDs, quantization metadata, RAM estimates, and skip thresholds by hand. Add a schema/consistency test for unique IDs, required fields, positive RAM estimates, and model-name/provider compatibility before more local candidates are added.                                              |
| TD-005 | Weak Tests           | P2       | `local_runner.py:1-465`                                 | The MLX runner performs memory headroom checks, starts/stops `mlx_lm.server`, polls readiness, skips overlarge models, and appends local leaderboard rows, but no unit tests import it. Extract pure planning/readiness/result-shaping helpers so OOM-safety and skip behavior can be tested without launching MLX.                 |
| TD-006 | Stale Docs           | P3       | `README.md:11`, `AGENTS.md:45`, `v1/MANIFEST.json:4-67` | Docs still say the v1-dev dataset has 155 prompts, but `MANIFEST.json` currently records 309 dev+holdout records. Update the prose or generate the count from the manifest so benchmark status notes stay trustworthy.                                                                                                              |

### In Progress

_(none)_

### Done

_(none)_

### Progress Log

- 2026-05-25 - Initialized the rundale-bench debt ledger after scanning the Python bench runner, static-site pipeline, v1 dataset/config files, local MLX runner, and bench-site data artifact.
