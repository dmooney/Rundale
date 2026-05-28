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
