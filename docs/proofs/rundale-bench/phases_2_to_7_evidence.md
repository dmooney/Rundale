# rundale-bench v1-dev Phases 2-7 evidence

Evidence type: gameplay transcript

Phase 1 (dataset freeze) shipped in [PR #960](https://github.com/dmooney/Rundale/pull/960) and froze the dialogue slice behind a sha256-verified loader. This bundle covers Phases 2-7 of the [rundale-bench plan](../../plans/rundale-bench.md): all four remaining slices, the pinned-judge contract, deterministic dev/holdout split, the leaderboard, and the freeze-deferral note.

Corpus is intentionally undersized vs the plan's 1100-prompt target (155 actual). The framework is complete; growing the corpus is the v1.0 freeze blocker. See README status table.

## Phase 2 — intent slice + deterministic grader

- `parish/testing/rundale-bench/v1/intent.jsonl` — 30 records (10 core, 20 extended), spanning move/talk/look/interact/examine/unknown. Adversarial cases included (past-tense place mentions = talk not move).
- `parish/testing/rundale-bench/grade.py::grade_intent` — exact-match intent label × Jaccard(target) × Jaccard(dialogue). Score is `label_match * (0.5 + 0.25*target_jaccard + 0.25*dialogue_jaccard)`; label mismatch zeros the score (parser bugs are not partial-credit).
- `parish/testing/rundale-bench/rundale_bench.py` — orchestrator with `--slice intent`. Production `INTENT_SYS` system prompt copied verbatim from `parish/scripts/local-eval/gen_samples.py`.

### Smoke run

```sh
python3 parish/testing/rundale-bench/rundale_bench.py \
    --target 'openai/gpt-oss-120b:free@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \
    --suite v1 --slice intent --limit 30
```

Result: `label_match_rate=0.700, mean_score=0.676` on 30 records, 12.8 min wall (rate-limited free tier), $0.00. Output JSON: `run_openai_gpt_oss_120b_free_intent_20260513T183247Z.json` (pre-split — 30 records, not 25).

## Phase 3 — dialogue extension + pinned judge

- `parish/testing/rundale-bench/v1/dialogue.jsonl` grown from 100 → 150 records. Added 50 new `extended` records spanning emotional/social (15), medical/herbal (15), folklore/supernatural (12), practical/everyday (8). Hand-authored; not LLM-generated, to avoid trivial vocabulary clustering.
- `parish/testing/rundale-bench/v1/judge_v1.json` — pinned LLM judge:
  - `model = claude-sonnet-4-6` (chosen over Opus to keep per-judge cost low; rubric is a 5-axis 1-5 scoring task that doesn't need Opus capability headroom)
  - `temperature = 0`
  - `rubric_sha256 = 1dcb5da5e0a6c1c322812b231e318604ff41a46f0a2eb71761c187071e0709e6`
- `grade.py::verify_judge_rubric` — computes `sha256(judge["rubric"])` and compares against `judge["rubric_sha256"]`. Raises `RuntimeError` on drift. Called from every judge-backed grader before any LLM invocation, so silent rubric edits become a hard failure rather than a quiet score change.

Reproducibility delta measurement deferred — running the full dialogue slice twice through `judge_v1` would cost ~$0.50 per repetition (Sonnet 4.6 input+output for 150 prompts × 2 judge axes), and the `PARISH_ANTHROPIC_API_KEY` is not in the local `.env`. The contract is in place; measurement lands when the first holdout sweep runs in CI.

## Phase 4 — reaction + sim slices + hybrid graders

Three new slices, each with a hybrid (schema + judge) grader:

| Slice | Records | Grader |
|---|---|---|
| `reaction.jsonl` | 30 (10 core, 20 extended) | non-Latin check + length [5, 400] + `judge_reaction_v1` in-character score (1-5) |
| `tier2-sim.jsonl` | 30 (10 core, 20 extended) | schema-validate + `judge_sim_v1` plausibility score (1-5) |
| `tier3-sim.jsonl` | 15 (5 core, 10 extended) | schema-validate + `judge_sim_v1` plausibility score (1-5) |

Reaction varies persona (publican / midwife / priest / farmer / blacksmith / barmaid / schoolteacher / soldier / landlord-agent / seanchaí) × context (location × weather × prior-acquaintance). Tier2-sim varies location × party size × event mix (uneventful 70% / mild incident 15% / mood pulse 15%). Tier3-sim varies NPC count (4-7) × season × event flavour.

Two new judge configs alongside `judge_v1`:
- `judge_reaction_v1.json` — 1-axis (in_character)
- `judge_sim_v1.json` — 1-axis (plausibility)

Both pinned at Claude Sonnet 4.6, `temperature=0`, with `rubric_sha256` verification.

Schema validator is hand-rolled — no `jsonschema` dep added. Supports the subset rundale-bench uses: `type` (incl. union), `enum`, `required`, `additionalProperties: false`, nested `properties` / `items`. Tested across happy + missing-required + bad-enum + extra-key + non-JSON-string paths.

## Phase 5 — holdout split

- `parish/testing/rundale-bench/split_holdout.py` — deterministic split by `sha256(id)` bottom-20%. Reproducible across machines + immune to record reordering.
- `core` tier preserved in dev — never moves to holdout, so `gen_dlg.py` and `flaw_scan.py` keep working from the same canonical 5-prompt smoke set.
- `eval_lib.load_slice` takes `split="dev"|"holdout"`; manifest tracks both files; loader verifies sha256 for whichever side was requested.
- Encryption deferred. v1-dev holdouts are plaintext-in-repo. Phase 7 freeze will age-encrypt holdouts behind a CI-only key.

| Slice | Dev | Holdout | Holdout % |
|---|---|---|---|
| dialogue | 133 | 17 | 11.3% |
| intent | 25 | 5 | 16.7% |
| reaction | 27 | 3 | 10.0% |
| tier2-sim | 27 | 3 | 10.0% |
| tier3-sim | 13 | 2 | 13.3% |

Effective holdout rates fall short of the 20% target because the `core` tier carve-out reduces the eligible pool. At the planned v1.0 corpus sizes (200+ per slice) the deviation will be < 2 pp.

## Phase 6 — leaderboard scaffold

- `docs/proofs/rundale-bench/leaderboard.md` — append-only ranking table. Seeded with one row from the pre-split intent smoke; future rows must run against the split slices.
- Submission rules documented inline: holdout is the leaderboard, dev rows are reproducibility-only, re-runs replace earlier tuples, cost must come from `CostTracker`, `harness_sha` is `git rev-parse HEAD` at run-time.
- Eligible-targets backlog lists every `preset_models()` cloud + local pick that should be benchmarked before any preset swap.

Multi-target sweep against the holdout split is the work that closes Phase 6 substantively — that requires API keys for the cloud providers and is out of scope for this PR. The scaffold is in place; running it is a follow-up.

## Phase 7 — freeze deferral

Originally intended to flip `MANIFEST.json::frozen=true` and tag `rundale-bench-v1.0`. Deferring because the dataset is intentionally undersized:

- 30 intent prompts (target 200) — too few for stable cross-model deltas
- 150 dialogue prompts (target 500)
- 30 reaction prompts (target 200)
- 30 + 15 sim prompts (target 200 + 100)
- 1 leaderboard row (target ≥3 independent targets, including holdout sweeps)

Tagging `v1.0` at this corpus size would lock in a benchmark that can't distinguish frontier-vs-mid-tier models with confidence. The README's status table tracks the freeze blockers; once the corpus grows, freeze is a one-line MANIFEST flip + tag.

## What worked

- Sha256-verified loaders catch silent dataset drift on every load.
- The `judge_v1` rubric pinning contract works in tests and is enforced at runtime — tamper-detection landed in `test_grade.py::test_judge_rubric_tamper_detected` and `test_dialogue_rubric_tamper_blocks_call`.
- 22/22 grader unit tests pass.
- Deterministic-by-id-hash split survived a full re-split (after restoring records that had been pre-split) and produced an identical-up-to-core-preservation outcome.
- One real smoke against `openai/gpt-oss-120b:free` produced a measured 70% label-match rate, demonstrating the orchestrator works end-to-end on a free cloud target.

## What didn't

- Corpus authoring is the bottleneck. Without 1000+ in-character prompts, the benchmark can't surface deltas <3 percentage points reliably.
- The plan envisaged authoring via parallel sub-agents per phase. In practice, sub-agents could not operate in sibling worktree paths (permission scoping), so all work landed sequentially in one branch. Documented in the session transcript; no code change required.
- Reproducibility delta on the dialogue judge couldn't be measured without `PARISH_ANTHROPIC_API_KEY` available in `.env`. Pending.
