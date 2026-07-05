# Rundale-Bench Plan

> Status: In progress · Updated: 2026-05-25 · [Docs Index](../index.md)

> Back to [Docs Index](../index.md) | Siblings: [LLM Quality Evals Plan](llm-quality-evals.md), [Promptfoo Pentest Plan](promptfoo-pentest-plan.md)

## Goal

Promote the ad-hoc `parish/scripts/local-eval/` probes into a **public, frozen, reproducible benchmark** for in-character 1820 Irish gameplay inference — `rundale-bench-v1` — so model and provider choices in `parish-config::presets::preset_models()` (and downstream `InferenceCategory` swaps) can be defended with data instead of guesses.

Modelled on the contract that makes SWE-bench and HLE useful: fixed dataset, held-out split, deterministic-where-possible grader, pinned judge, append-only leaderboard, SemVer-frozen task spec.

## Status

Proposed. Lands incrementally — each phase ships as a small PR.

Predecessor: [PR #958](https://github.com/dmooney/Rundale/pull/958) generalised the local-eval scripts behind a `Target` abstraction so any OpenAI-compatible endpoint can be evaluated. That's the _probe_ — useful for one-off swap decisions, not yet a benchmark.

## What's missing today

| Property              | Probe (today)                                            | Real benchmark                                                    |
| --------------------- | -------------------------------------------------------- | ----------------------------------------------------------------- |
| Dataset versioning    | Prompts mutate in-place in `flaw_scan.py` / `gen_dlg.py` | Frozen JSONL, Merkle root committed, SemVer                       |
| Held-out split        | None                                                     | 80% dev (public) / 20% holdout (sealed)                           |
| Grading               | LLM-judge only (Opus, stochastic)                        | Hybrid: deterministic where possible + pinned-LLM judge for taste |
| Statistical N         | 5 (dialogue) / 100 (flaw scan)                           | 200-500 per slice                                                 |
| Reproducibility       | "Run the script and eyeball"                             | Same target → same score within rubric noise (±0.3)               |
| Leaderboard           | Scattered across `docs/proofs/local-perf/`               | Append-only canonical table                                       |
| Spec                  | None                                                     | Versioned task contract + citation BibTeX                         |
| Contamination control | None                                                     | Holdout sealed; not crawled                                       |

## Scope

Five slices, mirroring `InferenceCategory`:

| Slice       | Grader        | Gold style                                        |
| ----------- | ------------- | ------------------------------------------------- |
| `intent`    | deterministic | exact-match intent label + target F1 vs gold JSON |
| `reaction`  | hybrid        | schema-valid + LLM-judge rubric                   |
| `tier2-sim` | hybrid        | schema-valid + delta plausibility (LLM)           |
| `tier3-sim` | hybrid        | schema-valid + count + LLM-judge                  |
| `dialogue`  | LLM-judge     | frozen 5-axis rubric + non-Latin script rule      |

Out of scope for v1: agent-loop benchmarks (multi-turn dialogue with memory), latency budgets (already covered by `inf_bench.rs`), or training-data benchmarks. Those are v2 candidates.

## Deliverables

1. **Dataset.** `rundale-bench/v1/<slice>.jsonl`, one record per line:

   ```jsonl
   {"id": "intent-0001", "prompt": "...", "schema": {...}, "gold": {"intent": "talk", "target": "Padraig", "dialogue": "I saw his cow"}}
   ```

   200 prompts per slice. Hand-authored or hand-graded (intent / reaction); LLM-graded for tier2/tier3 once a pinned judge exists.

2. **Holdout split.** 20% reserved as `<slice>.holdout.jsonl`. Sealed: encrypted-at-rest in repo (age key in CI secret) or hosted externally. Never decrypted in interactive sessions; only the `rundale_bench.py` CI runner sees the plaintext. Hashes of holdout prompts checked in alongside ciphertext so contamination can be audited.

3. **Graders.** `rundale-bench/grade.py`:

   - `grade_intent(pred, gold)` — exact-match label + Jaccard on optional fields, returns 0.0-1.0
   - `grade_schema(pred, schema)` — JSON-Schema validation, returns boolean
   - `grade_dialogue(pred, rubric, judge)` — calls pinned judge model with frozen rubric prompt, returns the 5-axis scores + overall
   - `grade_simulation(pred, schema, judge)` — schema-validate then plausibility-judge

4. **Harness.** `rundale-bench/rundale_bench.py`:

   ```sh
   python3 -m rundale_bench --target '<spec>' --suite v1 --split dev
   python3 -m rundale_bench --target '<spec>' --suite v1 --split holdout    # CI-only
   ```

   Single entry point. Emits per-slice scores, aggregate overall, per-1k-task USD, p50/p95 latency. Outputs JSON + appends a leaderboard row.

5. **Pinned judge.** `judge_v1` snapshot: model id + base_url + temperature + seed + rubric prompt hash. Captured in `docs/agent/rundale-bench-v1.md` and verified at runtime (`grade.py` aborts if the judge response signature deviates from the pinned hash).

6. **Spec doc.** `docs/agent/rundale-bench-v1.md`:

   - task definitions (one section per slice with prompt template + grading rule)
   - dataset cards (size, sourcing, annotator notes, known biases)
   - grading rules + judge pin
   - submission protocol + leaderboard schema
   - citation BibTeX
   - errata + version history

7. **Leaderboard.** `rundale-bench/artifacts/leaderboard.md` — generated Markdown table:

   ```text
   | Date (UTC)        | Target                                                     | Dev / Holdout overall | Intent | Reaction | Tier2 | Tier3 | Dialogue | $/1k | p50 ms | Harness SHA |
   ```

   Every CI run appends one row. Reproducibility check: re-running the same target on the same harness SHA must yield within ±0.3 overall.

## Phased rollout

### Phase 1 — Dataset freeze

Land first; smallest blast radius.

- Move `flaw_scan.PROMPTS` (100 dialogue probes) into `v1/dialogue.jsonl` with `id` field. No schema or gold yet — judge will score later.
- Move the 5-prompt `gen_dlg.py` set into the same file, marked as `core/extended` tier.
- Hash + commit; print Merkle root in spec doc.
- `flaw_scan.py` reads from the JSONL (back-compat: defaults reproduce today's output).

Acceptance: re-running the gpt-oss-120b proof bundle against the frozen dataset produces the same 24/25 result.

### Phase 2 — Intent slice with gold labels

- Author `v1/intent.jsonl`: 200 prompts spanning the `move/talk/look/interact/examine/unknown` taxonomy, with gold `{intent, target, dialogue}`. Manually graded for ambiguous cases.
- `grade.py::grade_intent` implements exact-match + Jaccard.
- `rundale_bench.py` runs intent-only at this point and reports per-target accuracy.

Acceptance: scoring `gpt-5.4-nano` on intent reaches > 95% (today's preset claim must hold up).

### Phase 3 — Reaction + dialogue slices with pinned judge

- Add `v1/reaction.jsonl` and `v1/dialogue.jsonl` (latter already present from Phase 1, extended to 500).
- Pin `judge_v1` = Claude Opus 4.7 at a fixed snapshot date, with frozen rubric prompt + temperature 0 + seed.
- `grade.py::grade_dialogue` invokes pinned judge; aborts if the judge response signature deviates.
- Aggregate scoring per slice.

Acceptance: re-running the same target twice produces overall scores within ±0.3.

### Phase 4 — Tier2 + Tier3 simulation slices

- `v1/tier2-sim.jsonl` and `v1/tier3-sim.jsonl` with full JSON schema attached per record.
- `grade.py::grade_simulation` first schema-validates then asks the pinned judge for plausibility (mood + relationship delta sanity).

Acceptance: schema-valid rate > 90% for current production preset; plausibility ≥ 4.0/5.

### Phase 5 — Holdout split + sealing

- Pull 20% of each slice into `<slice>.holdout.jsonl`, age-encrypt with a CI-only key.
- Update spec doc: dev set is public, holdout is sealed.
- CI runs the harness against both splits; only holdout scores feed the leaderboard.
- Local runs can only target dev split.

Acceptance: a CI run on `gpt-oss-120b:free` produces a leaderboard row; the dev set remains inspectable for prompt debugging.

### Phase 6 — Leaderboard + submission protocol

- Author `rundale-bench/artifacts/leaderboard.md` with starter rows for the current `preset_models()` picks.
- Spec out the submission protocol: PR adds a leaderboard row, CI re-runs the harness to verify the score.
- Optional: external mirror so the leaderboard is searchable without cloning the repo.

Acceptance: at least three targets evaluated (one cloud frontier, one cloud free, one local MLX), with scores publicly visible.

### Phase 7 — v1.0 cut

- Freeze the dataset Merkle root + spec hash + judge pin.
- Tag `rundale-bench-v1.0`.
- Any later prompt fix is `v1.1` (errata); any new slice is `v2`.

## Open design questions

1. **Judge stability.** Pinning a Claude model at a date assumes Anthropic doesn't silently degrade the snapshot. Mitigation: include a 20-prompt regression set scored by the pinned judge at v1.0 freeze; CI re-scores at every harness run and alerts on drift.
2. **Cost of judging.** 500 dialogue prompts × 5 axes × 1 judge call ≈ $5-15 per evaluated target (Opus rates). Affordable for occasional sweeps; potentially expensive on every PR. Tier the harness: cheap (intent only, deterministic) on PR; full sweep on a weekly cron.
3. **Holdout contamination.** Even sealed prompts may leak into training data over time if any researcher exfiltrates them. Mitigation: rotate 20% of holdout into a new v1.1 each year; track per-target score deltas to catch sudden jumps.
4. **Public vs private leaderboard.** Public draws more contributors but invites Goodharting. Private avoids that but lacks credibility. Start private (internal) and graduate to public after Phase 7.
5. **Multi-language.** Dataset is Hiberno-English with optional Irish (ga-IE). v1 stays monolingual; multi-mod world content (e.g. a German parish mod) is a v2 concern.

## Why bother

Today every `preset_models()` swap is a vibes-decision. With `rundale-bench`:

- `Provider::Vllm` ([PR #957](https://github.com/dmooney/Rundale/pull/957)) presets become measurable instead of "Qwen2.5 looks fine".
- Frontier vs cheap tiers become a defensible cost-per-quality tradeoff curve.
- Players who fork Rundale for their own settings (Yorkshire 1920, Andalusia 1492) inherit a benchmark template to validate their own prompt drift.
- "We support model X" becomes "Model X scores Y on rundale-bench-v1" — the only honest answer.

## References

- [SWE-bench](https://www.swebench.com/) — task contract + held-out split discipline.
- [Humanity's Last Exam](https://lastexam.ai/) — expert-graded, contamination-controlled, leaderboard-driven.
- [HELM](https://crfm.stanford.edu/helm/) — multi-axis scoring, transparency, cost reporting.
- [PR #958](https://github.com/dmooney/Rundale/pull/958) — `Target` abstraction this plan builds on.
- [`docs/plans/llm-quality-evals.md`](llm-quality-evals.md) — earlier proposal for in-repo quality sensors; `rundale-bench` is the public face of that work.
