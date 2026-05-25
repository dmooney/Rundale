---
name: rundale-bench
description: Drive Rundale model-quality evals. Three modes — (1) `bench <target-spec>` (default): the user says "benchmark X on Y" and you run every slice + perf for that target, with Sonnet subagents as judge, end-to-end. (2) `drain-queue`: dispatch Sonnet subagents to score pending bundles already in the on-disk queue. (3) `eval-dialogue <target-spec> ...`: an in-chat blind A/B/N where Opus scores dialogue across MLX/cloud candidates on a 5-axis rubric with per-candidate USD cost. Use when comparing dialogue quality across models/providers from inside Claude Code.
argument-hint: '[bench <target-spec> [--model-id ID --provider-id ID] | drain-queue | eval-dialogue <target-spec> <target-spec> ...]'
---

# rundale-bench

Model-quality eval with three modes:

- **`bench <target-spec>` (default)** — the user-facing entry point. Triggered when the user says "benchmark
  X on Y" or "/rundale-bench bench …". Runs every slice + perf for one target, using Sonnet subagents as
  judge, and ingests the scores in one workflow. Jump to the **bench mode** section below.
- **`drain-queue`** — drain an already-queued judging backlog. Useful after a prior run was aborted, or after
  someone (or a script) generated bundles outside this skill.
- **`eval-dialogue`** — a lightweight in-chat blind A/B/N. Opus (this conversation) is the judge; no separate
  judge process, no queue. Best for a quick cross-family dialogue comparison with USD-cost accounting. Jump
  to the **eval-dialogue mode** section below.

---

## bench mode

The default. Triggered by "benchmark X on Y", "/rundale-bench bench …", or any equivalent phrasing where the
user asks for a single model+provider to be evaluated. The workflow runs all slices, runs the perf probe,
and uses Sonnet subagents to judge — no other prompts needed from the user.

### Steps

1. **Resolve the target spec.** Either the user supplied a full `model@base_url[#env:VAR]` string, or they
   gave a logical id ("kimi-k2.5 on opencode-go"). In the logical-id case, look it up in
   `rundale-bench/v1/models.toml` and assemble the spec from the matching `[[model.providers]]` row, so
   `--model-id` and `--provider-id` can be passed through correctly. Abort if the env var named in the spec
   is unset — they won't get past the first HTTP call.
2. **Generate samples + queue judge bundles** (every slice end-to-end):
   ```sh
   just -f rundale-bench/justfile bench-it '<target-spec>'
   ```
   This is the unified recipe: it invokes `rundale_bench.py --slice all --judge judge_sonnet_v1` for every
   slice (dialogue / intent / reaction / tier2-sim / tier3-sim / gaeilge) and then runs the perf probe in
   one go. The judge slices write bundles to `rundale-bench/.bench-queue/pending/` instead of scoring inline.
3. **Drain the judging queue.** Same as the **drain-queue mode** below: list `.bench-queue/pending/*.json`,
   skip any that already have `.bench-queue/done/<stem>.json`, and dispatch up to **8 Agent subagents per
   message** running the `/rundale-bench-judge` skill on each bundle. Each subagent returns a JSON object;
   write it verbatim to `done/<stem>.json`. Loop until `pending/` is fully matched.
4. **Ingest + finalise:**
   ```sh
   python3 rundale-bench/rundale_bench.py ingest --finalize
   ```
   Validates results, writes content-addressed judgments under `docs/proofs/rundale-bench/judgments/`, and
   folds scores back into the run JSONs. `--finalize` errors if anything is unscored — go back to step 3 and
   re-dispatch the missing bundles.
5. **Report** in chat: target id; per-slice aggregate (one row each for dialogue, intent, reaction,
   tier2-sim, tier3-sim, gaeilge); perf p50 / p95 / tokens-per-second; total USD spend. Flag any slice where
   `> 10%` of records errored with HTTP 4xx — those signal a provider quirk worth investigating.

### Notes

- **Subagent batching.** Send 8 Agent tool uses in a single message to keep judge throughput high without
  tripping per-minute rate limits.
- **Cost expectation.** The Sonnet judge is the dominant line item. Plan for ~$0.01–$0.05 per slice per
  target depending on `--limit`. The candidate side is usually <$0.01.
- **Skips you should respect.** If `slice` returns HTTP 400 for every record (e.g. opencode-go doesn't honour
  `response_format=json_schema` → intent slice always 400s), skip that slice in the report rather than
  publishing a floored 0.0 score. Strip the bad records or move the artifact aside.

## drain-queue mode

For draining an existing judging backlog (resuming after an aborted run, or scoring bundles produced
outside of the `bench` workflow). The orchestrator and subagents communicate through an on-disk queue
under `rundale-bench/.bench-queue/`, so a run can be aborted and resumed at any point.

For every bundle in `rundale-bench/.bench-queue/pending/*.json` that does **not** already have a matching
`rundale-bench/.bench-queue/done/<stem>.json`:

1. Read the bundle JSON.
2. Dispatch an `Agent` subagent (batch up to **8 in parallel per message** to stay within rate limits)
   running the `/rundale-bench-judge` skill, passing the bundle file path. The subagent uses the system
   prompt named in the bundle's `system_prompt_file` (`judge_sonnet_v1.system.md`) and returns a single
   JSON object — nothing else.
3. Write the subagent's JSON reply verbatim to `rundale-bench/.bench-queue/done/<bundle_id>.json` (same
   stem as the pending file).

**Resume semantics:** a bundle is "done" iff `done/<stem>.json` exists. Re-running `drain-queue` only
dispatches bundles still missing a `done/` file, so an interrupted run is safe to restart. Never edit a
`pending/` bundle by hand.

After draining, the caller (or `bench` mode step 4) finalises via
`python3 rundale-bench/rundale_bench.py ingest --finalize` — validates each `done/` result against its
bundle (rubric_sha256 must match; axes must be ints 1-5; every prompt scored), writes content-addressed
judgments to `docs/proofs/rundale-bench/judgments/<cache_key>.json`, and folds scores back into the run
JSON aggregate. Malformed or dropped items are reported as judge failures and excluded from the aggregate
(re-dispatch to retry).

### Inspect helpers

```sh
python3 rundale-bench/rundale_bench.py catalog --show          # models + resolved providers
python3 rundale-bench/rundale_bench.py judge --verify sonnet   # rubric_sha256 integrity
```

---

## eval-dialogue mode

A blind A/B/N where **Opus (this conversation) is the judge** — no orchestrator, no queue, no judge
subagent. Use it to decide which model + provider combo to wire into
`parish-config::presets::preset_models()` for an `InferenceCategory`. Invoke with target specs:
`/rundale-bench eval-dialogue <target-spec> <target-spec> [...]`.

### Target spec

Every target is a `model@base_url[#env:VAR]` string accepted by `eval_lib.parse_target`:

```text
# Local MLX (no auth)
mlx-community/Qwen2.5-7B-Instruct-4bit@http://localhost:8000/v1

# Cloud (API key in environment)
claude-sonnet-4-6@https://api.anthropic.com/v1#env:ANTHROPIC_API_KEY
llama-3.3-70b-versatile@https://api.groq.com/openai/v1#env:PARISH_GROQ_API_KEY
gpt-5.5@https://api.openai.com/v1#env:PARISH_OPENAI_API_KEY
```

`mlx-community/*` targets pointing at `http://localhost:*` are treated as local — the skill spawns a
vllm-mlx server for each on consecutive ports. Anything else is cloud — no spawn; calls hit the URL directly
using the API key from `$VAR`. Pass at least two specs; three is the sweet spot for an A/B/C.

### Steps

1. **Classify each target.** `base_url` starting with `http://localhost` is *local*, else *cloud*. Cloud
   targets need their `$VAR` set; verify with `printenv $VAR` and abort if any required key is missing.
2. **Pre-flight local targets only.** If any target is local, check `which vllm-mlx`. If missing, tell the
   user to run `uv tool install vllm-mlx`. Skip for pure-cloud runs.
3. **The judge is YOU (Claude Opus, in this conversation).** Do not spawn a separate judge process. Opus is
   cross-family and stronger than any local MLX tier, eliminating same-family bias; for cloud-vs-cloud it is
   still the most independent in-chat judge. State this in the report header with the judge model id and date.
4. **Spawn local vllm-mlx processes** on consecutive ports from `:8000` for each *local* target. Use
   `--enable-prefix-cache --continuous-batching`. Save pids for cleanup. Wait for `/v1/models` to respond
   (Monitor with an `until curl ... ; do sleep 2; done` loop). Skip entirely for all-cloud runs.
5. **Generate dialogue samples.** First make a per-run temp dir so concurrent invocations don't collide on
   `/tmp/cand_*.txt`:
   ```sh
   RUN_DIR=$(mktemp -d -t eval-dialogue-XXXXXX)
   ```
   Then for each candidate:
   ```sh
   python3 parish/scripts/local-eval/gen_dlg.py '<target-spec>' "$RUN_DIR/cand_<letter>.txt"
   ```
   `gen_dlg.py` uses the canonical 5 prompts and writes a transcript plus a `=== Cost: ... ===` footer.
   Record the cost line per candidate.
6. **Score the replies yourself (Opus).** Build a single bundle in chat — one section per prompt, each
   candidate labelled `Model X`, `Model Y`, … (no model-name leakage). Apply the 5-axis rubric below to
   every candidate reply, producing JSON `{character,authenticity,language,responsiveness,craft,overall}`
   per candidate per prompt. Output 1-5 integers (one decimal for `overall`).
7. **Aggregate.** Mean each axis across the 5 prompts per candidate. Build a markdown table sorted by
   **Overall** descending, with a `Cost (USD)` column from the `gen_dlg.py` footer.
8. **Write the report** to `docs/proofs/local-perf/quality_eval_<UTC-stamp>.md`: judge model + URL; candidate
   mapping (Model X → real target spec); aggregate table (Overall + per-axis + cost); per-prompt detail
   (each candidate's reply + parsed scores); any prompts where score-parsing failed (kept raw).
9. **Clean up.** Kill every spawned vllm-mlx pid. `pkill -f Qwen2.5` is a safety net, but kill specific pids
   first so an unrelated server stays up. Cloud targets need no cleanup.
10. **Print the summary table** to chat — one line per candidate with the real target id, Overall, and USD cost.

### Judge rubric (paste verbatim into the judge system prompt)

```
You are an impartial judge scoring fictional dialogue from rural
Ireland in 1820. Each model plays Brigid O'Brien, a 42-year-old
midwife. Score each candidate reply on a 1-5 scale (5 = best) on:
  1. CHARACTER — does the reply read as an 1820 rural Irish midwife?
  2. AUTHENTICITY — period-appropriate vocabulary, no modern terms?
  3. LANGUAGE — only English (en-IE) plus optional Irish (ga-IE); no
      Cyrillic, Han, Hangul, or other scripts; well-formed prose.
  4. RESPONSIVENESS — does the reply actually address the prompt?
  5. CRAFT — concise, evocative, in 1-3 sentences as instructed?

Output ONLY a single line of compact JSON with one key per candidate
label, mapped to an object {character,authenticity,language,
responsiveness,craft,overall}. The 'overall' field is your weighted
mean of the five sub-scores (1-5, one decimal). No prose, no markdown.
```

### Notes

- The judge is itself an LLM with its own biases — interpret deltas <0.3 as noise.
- For cross-family sweeps (MLX vs Claude vs GPT vs Llama), prefer three or four candidates so Opus has more
  relative signal than absolute.
- Costs come from each provider's `usage` block (where reported). Local targets show $0.00. Static
  $/M-token rates live in `parish/scripts/local-eval/eval_lib.py::COSTS` — verify before treating totals as
  gospel; providers change pricing without warning.
- Memory (local only): each candidate vllm-mlx process keeps the model resident. On a 32 GB Mac, judge + 2
  candidates is comfortable; 3 pushes memory. Cloud candidates use no local RAM.
- Companion scripts live at `parish/scripts/local-eval/` — `gen_samples.py` (per-category sweep),
  `flaw_scan.py` (100-prompt non-Latin script audit), `gen_dlg.py` (5-prompt dialogue), all taking the same
  `model@base_url[#env:VAR]` spec.

### Output contract

Summarize in chat: (1) judge model + why; (2) aggregate scoring table (Overall + cost); (3) path to the
saved report; (4) recommendation — which candidate is best and whether the delta justifies a preset swap
given cost/latency.
