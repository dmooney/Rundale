# Rebench findings — issue #994

Evidence type: gameplay transcript

Apples-to-apples comparison: grok-4.3 cached under the OLD bench prompt
(269 chars, 2026-05-14) vs the NEW bench prompt (3894 chars, post-#994).
Same judge re-runs both caches.

## Judge selection

Three judges tried:

1. **`judge_v1` (qwen3-235b, 1-5 scale)** — saturated. Returns
   4.96/5.00 for OLD, 5.00/5.00 for NEW. Cannot discriminate at
   grok-4.3's skill level. Explains the issue's "3.40 vs 3.50"
   baseline reading like noise.

2. **`mistral-large-2512` (0-10 multi-axis)** — discriminates, but
   the user judges it too generous. Pulled from the comparison set.

3. **`x-ai/grok-4.3` (0-10 multi-axis)** — stricter than mistral.
   Some family bias (grok judging grok-style replies), tracked in
   the leaderboard caveats. Selected for this rebench at user
   request. Used for the headline numbers below.

## Bench-scoring artifact: template scaffolding penalty

The NEW prompt asks the model to emit dialogue, then `---`, then a JSON
metadata block (mood / action / language_hints). The runtime parses this
block and shows the player only the dialogue line. The bench, however,
ships the entire raw reply to the judge — including the `---` and JSON.

Grok-4.3 (the judge) sees the JSON as a 21st-century artifact and
penalizes immersion accordingly. Sample finding on `dialogue-0011` (a
toothache question):

| Version       | Reply contains | total | reason |
|---------------|----------------|-------|--------|
| OLD prompt    | plain dialogue | 8.4   | "Fitting herbal advice with natural Hiberno-English cadence" |
| NEW prompt    | dialogue + `---` + JSON | **3.4** | "Modern JSON metadata ruins period immersion" |

Same content, same advice, +5-point swing because of the scaffolding the
runtime strips before showing to the player. To get a meaningful
comparison the bench must score the dialogue line only — what the player
actually sees.

For this rebench we strip everything from `\n---` onward before scoring.
The stripped cache is at `dialogue_samples_NEW_stripped.json`.
The raw-NEW cache is kept at
`docs/proofs/rundale-bench/dialogue_samples_20260517T214107Z.json`.

A follow-up should either bake the metadata-strip into the bench
pipeline, or change the runtime template to JSON-first (the
`parish_npc::build_tier1_system_prompt` Rust builder already uses
JSON-first; the mod-shipped template diverged to a `---` delimiter
format). Reconciling that divergence is out of scope for #994.

## Multi-axis 0-10 (grok-4.3 judge)

| Axis           | OLD raw (n=15) | NEW raw (n=15) | NEW stripped (n=15) | Δ (stripped − OLD) |
|----------------|----------------|----------------|---------------------|--------------------|
| character      | 8.07           | 7.33           | 7.67                | -0.40              |
| authenticity   | 8.87           | 7.87           | 8.53                | -0.34              |
| language       | 7.73           | 7.93           | **8.60**            | **+0.87**          |
| responsiveness | 7.73           | 7.53           | **8.33**            | **+0.60**          |
| craft          | 8.07           | 6.60           | 7.93                | -0.14              |
| **total**      | **8.09**       | **7.45**       | **8.21**            | **+0.12**          |

## Reading the delta (grok judge, stripped)

- **language +0.87** — biggest lift, driven by the GA_IE phrase whitelist
  and Latin-script guard. OLD prompt said nothing about Irish and got
  away with confabulated phrases; the NEW prompt grounds Brigid's
  code-switching in vetted vocab.

- **responsiveness +0.60** — second-biggest lift. The NEW prompt's
  STAY-IN-CHARACTER persona binding + "react to what's happening around
  you" clause appears to keep the reply pointed at the player's
  question rather than drifting into Brigid's habitual advice.

- **character -0.40** and **authenticity -0.34** — modest dips. The
  NEW prompt's heavier behaviour scaffolding seems to make replies
  slightly less spontaneous-sounding under grok-as-judge. Could be
  real (more rules → more stilted) or judge-family bias.

- **craft -0.14** — within judge noise.

- **total +0.12** — net positive but inside the N=15 noise floor.

## Cross-judge comparison

For reference (mistral-large run, see commit `15da4fbc`):

| Judge                  | OLD total | NEW total | Δ |
|------------------------|-----------|-----------|---|
| qwen3-235b (judge_v1)  | 4.96/5.0  | 5.00/5.0  | saturated |
| mistral-large-2512     | 8.83      | 9.04      | +0.21 |
| **grok-4.3 (stripped)**| **8.09**  | **8.21**  | **+0.12** |

Both discriminative judges agree on sign (NEW > OLD) and rough
magnitude (within noise). Grok is harder on character/authenticity;
mistral is more uniformly positive.

## Verdict

The bench prompt now tracks the runtime tier-1 grounding (the
criterion of #994). Under the stricter grok-4.3 judge, the
prompt-driven lift is +0.12 total with concentrated wins on language
(+0.87) and responsiveness (+0.60) — exactly the axes the runtime
upgrades were aimed at. Dips on character / authenticity are real
but small (~-0.4 each) and worth investigating in a follow-up.

Two artifacts surface as follow-ups:

1. **Bench should strip the metadata block before judging.** The
   raw-NEW score (7.45) understates the model's quality by ~0.76
   points because the judge penalizes scaffolding that the runtime
   strips before showing to the player. Without the strip, all
   future scores under the NEW prompt will be unfairly depressed
   compared to OLD-style replies.

2. **Reconcile the mod template vs Rust builder.** The runtime
   `parish_npc::build_tier1_system_prompt` uses a JSON-first format;
   the mod-shipped template uses a `dialogue + --- + JSON` format.
   The bench reads the latter. Aligning them removes the strip step.

Both are noted in the PR description, neither is in scope for #994.

## Cost

- OLD grok-judge: 90 calls, 62k in + 76k out tokens, $0.0001
- NEW grok-judge raw: 15 calls, 11k in + 13k out tokens, $0.0000
- NEW grok-judge stripped: 15 calls, 10k in + 13k out tokens, $0.0000

Total wall: ~3 minutes. Total spend: ~$0.0001.
