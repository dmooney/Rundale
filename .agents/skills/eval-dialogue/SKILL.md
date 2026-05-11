---
name: eval-dialogue
description: Blind A/B/N quality evaluation across dialogue-sample transcripts from candidate local-inference models. Generates fresh samples for any requested model list, masks model identities, asks a subagent judge to score each reply on a 5-axis rubric (character, authenticity, language, responsiveness, craft), prints an aggregate scoring table, and archives the report under docs/proofs/local-perf/. Use when comparing dialogue quality between MLX models (Qwen2.5-1.5B / 7B / 14B, future Qwen3.x, gemma swaps, etc.).
disable-model-invocation: false
argument-hint: <model-id> <model-id> [<model-id> ...]
---

Blind-judge dialogue quality across two or more local-inference models on the same canonical prompt set. Used to decide whether a tier swap (e.g. 7B → 14B) is worth the extra latency.

## Inputs

- `$ARGUMENTS` — space-separated Hugging Face model ids in `mlx-community/<name>` form, e.g.
  ```
  /eval-dialogue mlx-community/Qwen2.5-7B-Instruct-4bit mlx-community/Qwen2.5-14B-Instruct-4bit
  ```
- At least two ids. Three is the sweet spot for an A/B/C with one model abstaining as judge.

## Steps

1. **Pre-flight.** Check vllm-mlx is installed (`which vllm-mlx`). If not, tell the user to run `uv tool install vllm-mlx`.

2. **The judge is YOU (Claude Opus, in this conversation).** Do NOT spawn a separate judge vllm-mlx process. Run candidates through their own spawned servers, collect replies, then score in the same chat using the 5-axis rubric below. Opus is cross-family and stronger than any local MLX tier, eliminating same-family bias. State this in the report header.

3. **Spawn one vllm-mlx process per candidate** on consecutive ports starting at `:8000`. Use `--enable-prefix-cache --continuous-batching`. Save the pid for each so you can clean up on exit. Wait for `/v1/models` to respond on each port before continuing (Monitor with an `until curl ... ; do sleep 2; done` loop).

4. **Generate dialogue samples** for each candidate using the 5 canonical prompts from `parish/scripts/local-eval/gen_dlg.py`:
   - "I have been having trouble sleeping. The dreams keep coming back."
   - "What do you know about the old Cailleach who lives near the fairy fort?"
   - "My mother is taken with a bad cough. Is there anything you can give her?"
   - "They say a stranger arrived in the village. Have you heard?"
   - "I lost a sheep last night. Could it be more than a wolf?"

   Use the production system prompt from `parish-npc/src/lib.rs::language_directive` (en-IE + ga-IE + non-Latin guard) wrapped around the Brigid persona — see `parish/scripts/local-eval/flaw_scan.py` for the canonical shape.

5. **Score the replies yourself (Opus).** After collecting all candidate replies, build a single transcript bundle in chat — one section per prompt, each candidate labelled `Model X`, `Model Y`, `Model Z`... (no model-name leakage). Apply the 5-axis rubric below to every candidate reply, producing JSON `{character,authenticity,language,responsiveness,craft,overall}` per candidate per prompt. Output 1-5 integers (one decimal for `overall`).

6. **Aggregate.** Compute the mean of each axis across all 5 prompts for each candidate. Build a markdown table sorted by **Overall** descending.

7. **Write the report** to `docs/proofs/local-perf/quality_eval_<UTC-stamp>.md` containing:
   - judge model + URL
   - candidate mapping (Model X → real id)
   - aggregate table
   - per-prompt detail: each candidate's reply + parsed JSON scores
   - any prompts where the judge reply failed to parse (kept as a raw block so future tuning can see why)

8. **Clean up.** Kill every spawned vllm-mlx pid before reporting. `pkill -f Qwen2.5` is a reasonable safety net but kill specific pids first so an unrelated server stays up.

9. **Print the summary table** to the chat — one line per candidate with the real model id and the Overall score.

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
- Memory cost: each candidate vllm-mlx process keeps the model resident. On a 32 GB Mac, judge + 2 candidates is comfortable; 3 candidates pushes memory.
- This skill prefers local judging (no API key required) so the loop runs offline. Switch to a cloud judge by editing `JUDGE_URL` / `JUDGE_MODEL` env vars inside the spawned `urllib` call — leave the env var contract documented when you do.
- Mirrors the May 2026 manual `dlg-qwen{7,15}.txt` blind compare. That run scored 4.6/5 (7B) vs 2.4/5 (1.5B) and informed the two-slot Apple Silicon loadout; this skill reproduces that workflow automatically.
- Companion scripts live at `parish/scripts/local-eval/` for direct CLI use without invoking the skill.

## Output contract

After running, summarize in chat:
1. Judge model + reason for picking it.
2. Aggregate scoring table.
3. Path to the saved markdown report.
4. Recommendation: which candidate is best, and whether the delta is large enough to justify a tier swap given the memory / latency cost difference.
