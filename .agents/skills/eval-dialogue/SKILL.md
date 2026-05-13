---
name: eval-dialogue
description: Blind A/B/N quality evaluation across dialogue-sample transcripts from candidate models (local MLX or cloud). Generates fresh samples for any requested target list, masks identities, asks a subagent judge to score each reply on a 5-axis rubric (character, authenticity, language, responsiveness, craft), prints an aggregate scoring table with per-candidate USD cost, and archives the report under docs/proofs/local-perf/. Use when comparing dialogue quality across MLX models (Qwen2.5-1.5B / 7B / 14B, future Qwen3.x) and / or cloud providers (Anthropic, OpenAI, Groq, OpenRouter, Together, xAI, Mistral, DeepSeek, NVIDIA NIM, Google's OpenAI-compat endpoint).
disable-model-invocation: false
argument-hint: <target-spec> <target-spec> [<target-spec> ...]
---

Blind-judge dialogue quality across two or more targets on the same canonical prompt set. Used to decide which model + provider combo to wire into `parish-config::presets::preset_models()` for each `InferenceCategory`.

## Target spec

Every target is a `model@base_url[#env:VAR]` string accepted by `eval_lib.parse_target`:

```text
# Local MLX (no auth)
mlx-community/Qwen2.5-7B-Instruct-4bit@http://localhost:8000/v1

# Cloud (API key in environment)
claude-sonnet-4-6@https://api.anthropic.com/v1#env:PARISH_ANTHROPIC_API_KEY
llama-3.3-70b-versatile@https://api.groq.com/openai/v1#env:PARISH_GROQ_API_KEY
gpt-5.5@https://api.openai.com/v1#env:PARISH_OPENAI_API_KEY
```

`mlx-community/*` targets pointing at `http://localhost:*` are treated as local — the skill spawns a vllm-mlx server for each on consecutive ports. Anything else is treated as cloud — no spawn; calls hit the URL directly using the API key from `$VAR`.

## Inputs

- `$ARGUMENTS` — space-separated target specs.
- At least two specs. Three is the sweet spot for an A/B/C.

## Steps

1. **Classify each target.** Targets whose `base_url` starts with `http://localhost` are *local*. Others are *cloud*. Cloud targets need their `$VAR` set; verify with `printenv $VAR` before continuing and abort if any required key is missing.

2. **Pre-flight local targets only.** If any target is local, check `which vllm-mlx`. If missing, tell the user to run `uv tool install vllm-mlx`. Skip this for pure-cloud runs.

3. **The judge is YOU (Claude Opus, in this conversation).** Do not spawn a separate judge process. Opus is cross-family and stronger than any local MLX tier, eliminating same-family bias; for cloud-vs-cloud sweeps it is still the most independent judge available in-chat. State this in the report header along with the judge model id and the date.

4. **Spawn local vllm-mlx processes** on consecutive ports starting at `:8000` for each *local* target. Use `--enable-prefix-cache --continuous-batching`. Save the pid for cleanup. Wait for `/v1/models` to respond before continuing (Monitor with an `until curl ... ; do sleep 2; done` loop). Skip this step entirely if every target is cloud.

5. **Generate dialogue samples** for each target. First make a per-run temp dir so concurrent `/eval-dialogue` invocations don't collide on `/tmp/cand_*.txt`:
   ```sh
   RUN_DIR=$(mktemp -d -t eval-dialogue-XXXXXX)
   ```
   Then for each candidate:
   ```sh
   python3 parish/scripts/local-eval/gen_dlg.py '<target-spec>' "$RUN_DIR/cand_<letter>.txt"
   ```
   `gen_dlg.py` uses the canonical 5 prompts and writes a transcript plus a `=== Cost: ... ===` footer. Record the cost line per candidate.

6. **Score the replies yourself (Opus).** After collecting all transcripts, build a single bundle in chat — one section per prompt, each candidate labelled `Model X`, `Model Y`, `Model Z`... (no model-name leakage). Apply the 5-axis rubric below to every candidate reply, producing JSON `{character,authenticity,language,responsiveness,craft,overall}` per candidate per prompt. Output 1-5 integers (one decimal for `overall`).

7. **Aggregate.** Compute the mean of each axis across all 5 prompts for each candidate. Build a markdown table sorted by **Overall** descending, including a `Cost (USD)` column from the `gen_dlg.py` cost footer.

8. **Write the report** to `docs/proofs/local-perf/quality_eval_<UTC-stamp>.md` containing:
   - judge model + URL
   - candidate mapping (Model X → real target spec)
   - aggregate table (Overall + per-axis + cost)
   - per-prompt detail: each candidate's reply + parsed JSON scores
   - any prompts where score-parsing failed (kept raw)

9. **Clean up.** Kill every spawned vllm-mlx pid. `pkill -f Qwen2.5` is a reasonable safety net but kill specific pids first so an unrelated server stays up. Cloud targets have nothing to clean up.

10. **Print the summary table** to chat — one line per candidate with the real target id, the Overall score, and the USD cost.

## Judge rubric (paste verbatim into the judge system prompt)

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

## Notes

- The judge is itself an LLM and has its own biases — interpret deltas <0.3 as noise.
- For cross-family sweeps (MLX vs Claude vs GPT vs Llama), prefer three or four candidates so Opus has more relative signal than absolute.
- Costs in the aggregate table come from each provider's `usage` block (where reported). Local targets always show $0.00. Static $/M-token rates live in `parish/scripts/local-eval/eval_lib.py::COSTS` — verify those before treating totals as gospel; providers change pricing without warning.
- Memory cost (local only): each candidate vllm-mlx process keeps the model resident. On a 32 GB Mac, judge + 2 candidates is comfortable; 3 candidates pushes memory. Cloud candidates use no local RAM.
- Mirrors the May 2026 manual `dlg-qwen{7,15}.txt` blind compare. That run scored 4.6/5 (7B) vs 2.4/5 (1.5B) and informed the two-slot Apple Silicon loadout; this skill reproduces that workflow automatically and now extends it to cloud providers so `preset_models()` can be picked from data instead of guesses.
- Companion scripts live at `parish/scripts/local-eval/` — `gen_samples.py` (per-category sweep), `flaw_scan.py` (100-prompt non-Latin script audit), `gen_dlg.py` (5-prompt dialogue) all take the same `model@base_url[#env:VAR]` spec.

## Output contract

After running, summarize in chat:
1. Judge model + reason for picking it.
2. Aggregate scoring table (Overall + cost).
3. Path to the saved markdown report.
4. Recommendation: which candidate is best, and whether the delta is large enough to justify a preset swap given the cost / latency difference.
