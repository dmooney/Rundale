# Acceptance Criteria: rundale-bench-phase1

## Task

Phase 1 of the eval-system redesign (see `/root/.claude/plans/i-feel-like-the-eager-cat.md`).
Stand up the foundations of the unified `rundale-bench` so the **dialogue slice**
can be judged by **Claude Sonnet 4.6 subagents** launched from inside Claude Code,
instead of the current inline `qwen3-235b` HTTP judge. This phase delivers the
candidate catalog, the Sonnet judge config + system prompt, the bundle/queue
plumbing, a content-addressed judgment cache, and the two driver skills — wired
end-to-end for the dialogue slice only. Tier funnel, perf matrix, remaining
slices, and the website come in later phases.

This is **eval tooling** (Python orchestrator + skills + config), not a gameplay
feature, so verification is via orchestrator commands rather than a `parish-cli`
play script. None of the files touched are on the runtime-shipping path
(`rundale-bench/**`, `.agents/skills/**`), so the live-gameplay-transcript proof
tier does not apply; the proof is the orchestrator transcript below.

## Scope (files)

New:
- `rundale-bench/v1/models.toml` — candidate catalog.
- `rundale-bench/v1/judge_sonnet_v1.json` — Sonnet 4.6 judge config (mirrors `judge_v1.json` shape; `model: "claude-sonnet-4-6"`, recomputed `rubric_sha256`).
- `rundale-bench/v1/judge_sonnet_v1.system.md` — judge subagent system prompt (preamble + dialogue rubric body).
- `rundale-bench/cache.py` — `cache_key()` + content-addressed judgment read/write under `docs/proofs/rundale-bench/judgments/`.
- `rundale-bench/judge_bundle.py` — assemble bundles into `.bench-queue/pending/`, drain `.bench-queue/done/`, validate against the Judgment schema.
- `.agents/skills/rundale-bench/SKILL.md` — outer driver (scans `pending/`, fans out `Agent` calls in batches, writes `done/`).
- `.agents/skills/rundale-bench-judge/SKILL.md` — inner per-bundle judge skill (reads one bundle, returns JSON only).

Changed:
- `rundale-bench/rundale_bench.py` — dialogue slice writes judging bundles + adds `ingest` subcommand instead of calling the qwen judge inline; existing `--target` / `qwen` path preserved behind a `--judge` selector for back-compat this phase.

## Criteria

1. **Catalog parses and resolves cheapest provider.** `models.toml` loads via a
   `load_catalog()` helper; for a model with multiple providers (llama-3.3-70b
   on groq/together/openrouter) `cheapest_provider()` returns the
   `min(price_in + 3·price_out)` entry. — observable via: `python rundale-bench/rundale_bench.py catalog --show` printing each model with its resolved quality provider.

2. **Sonnet judge config is pinned and consistent.** `judge_sonnet_v1.json` has
   `model == "claude-sonnet-4-6"` and a `rubric_sha256` equal to the SHA-256 of
   the rubric text it ships; loading it through `verify_judge_rubric()` (existing
   in `grade.py`) succeeds, and a deliberately corrupted SHA aborts. — observable via: `python rundale-bench/rundale_bench.py judge --verify judge_sonnet_v1` printing `rubric_sha256 OK` (and the negative test in pytest).

3. **`cache_key` matches the locked formula.** `cache_key(prompt_id, response, rubric_sha256, judge_model)` == `sha256(prompt_id ‖ response_sha256 ‖ rubric_sha256 ‖ judge_model)` where `response_sha256 = sha256(response)`. Identical inputs yield identical keys; changing any of the four inputs changes the key. — observable via: pytest `test_cache_key_*` assertions.

4. **Dialogue run writes one bundle per candidate to the queue.** Running the
   dialogue slice for N candidates over the screen prompt set produces N JSON
   bundles under `rundale-bench/.bench-queue/pending/`, each containing the
   rubric text, `rubric_sha256`, axes, and the `(prompt_id, prompt, response)`
   items — and **no** judge HTTP call is made. — observable via: `ls .bench-queue/pending/` listing one bundle per candidate after a run with `--judge sonnet`.

5. **Cache hits skip re-queueing.** A second identical run finds matching
   `docs/proofs/rundale-bench/judgments/<cache_key>.json` files and queues zero
   new bundles. — observable via: second run reporting `bundles queued: 0 (N cache hits)`.

6. **`ingest` validates and stores judgments.** Given hand-authored result files
   in `.bench-queue/done/` (simulating subagent replies), `ingest --finalize`
   validates each against the Judgment schema, writes
   `docs/proofs/rundale-bench/judgments/<cache_key>.json`, updates the run JSON
   aggregate (per-axis mean + overall mean), and errors if any `pending/` bundle
   lacks a `done/` counterpart. — observable via: run JSON showing populated `aggregate` and the judgments directory containing the hashed files.

7. **Malformed judge output is handled, not crashed.** A `done/` file with
   invalid JSON or out-of-range axis values is rejected; the affected items are
   marked `flags.judge_retry=true` / `axes=null`, excluded from the aggregate,
   and surfaced in a `judge_failures` count. — observable via: pytest feeding a bad result file and asserting the failure path.

8. **Both skills exist and document the loop.** `.agents/skills/rundale-bench/SKILL.md`
   describes the drain-queue fan-out (batch size, `Agent` dispatch, writing
   `done/`, resume semantics) and `.agents/skills/rundale-bench-judge/SKILL.md`
   describes the JSON-only contract referencing `judge_sonnet_v1.system.md`. — observable via: file presence + a docstring lint in pytest checking required headings.

9. **Existing qwen path still works.** Running the dialogue slice with the
   legacy `--judge qwen` selector behaves as before (inline HTTP judge), proving
   the refactor is additive. — observable via: existing `rundale-bench/test_grade.py` passing unchanged.

## Verification

This task has no game loop, so the verification is the orchestrator itself,
run offline (no network) with a stub judge to keep CI hermetic.

Run:
```
# Unit + integration tests (hermetic, no network)
python -m pytest rundale-bench/test_grade.py rundale-bench/test_phase1.py -q

# End-to-end queue round-trip with a local stub candidate + hand-authored done/ files
bash rundale-bench/tests/verify_phase1.sh
```

`rundale-bench/tests/verify_phase1.sh` (new) will:
1. `python rundale_bench.py catalog --show` → prints catalog + resolved providers (criterion 1).
2. `python rundale_bench.py judge --verify judge_sonnet_v1` → `rubric_sha256 OK` (criterion 2).
3. Run the dialogue slice against a stub target (`simulator` provider, canned replies) with `--judge sonnet --tier-prompts screen` → bundles appear in `pending/` (criterion 4).
4. Re-run → `bundles queued: 0` (criterion 5).
5. Copy canned subagent replies into `done/`, run `ingest --finalize` → run JSON aggregate populated, judgments written (criterion 6).

Expected signals in output:
- `catalog --show` lists `llama-3.3-70b` with `quality_provider=openrouter`.
- `judge --verify judge_sonnet_v1` prints `rubric_sha256 OK`.
- After the first dialogue run: `.bench-queue/pending/` contains one `*.json` per candidate; stderr/stdout shows `judge HTTP calls: 0`.
- After the second run: `bundles queued: 0 (… cache hits)`.
- After `ingest --finalize`: run JSON contains `aggregate.overall_mean` and `docs/proofs/rundale-bench/judgments/` contains `<cache_key>.json` files; `judge_failures: 0`.

## Out of scope (later phases)

Tier funnel + promotion logic (Phase 2), perf-by-provider (Phase 3), website
(Phase 4), reaction/sim/gaeilge slices on Sonnet (Phase 5), release snapshots +
diff (Phase 6), retiring `/eval-dialogue` (Phase 7).
