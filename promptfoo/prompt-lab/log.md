# Prompt lab log

The running record of the research loop (issue 2294). The
[prompt-lab skill](../../.agents/skills/prompt-lab/SKILL.md) appends one
entry per round. The best round's prompt is kept in
`output/round-<N>-brief.yaml` until the write-up.

- Next round: none; loop stopped after round 03 (plateau). Control done; none; run complete. Write-up: docs/test-plans/results/2026-11-08-prompt-lab.md
- Best: round 02, 82.1
- Spend so far: $18.59 of $25.00 (Gemini and both judges)

## Rounds

### Round 00: starting brief (and baseline, once)

- Training score (brief, both models, both judges): **80.8**. Gain: none (first round).
  Baseline for reference: 65.3.
- By model (brief): flash 87.7, flash-lite 74.0. Baseline: flash 69.8, flash-lite 60.7.
- By requirement (brief): B1 82.8, B2 88.8, B3 78.2, B4 90.6, B5 62.8, B6 86.8,
  B7 86.2, B8 82.5, B9 56.3, B10 83.2, B11 73.0, B12 67.6, B13 81.4, B14 86.1,
  B15 76.6, B16 89.8, B17 78.3, B18 96.8, B19 80.8, B20 80.4.
- Malformed: 0. Outages: 0. Judge token counts non-zero; `unpriced_models` empty.
- Judges split over 20 (brief): t10-fever flash-lite (opus 72, gpt 45),
  t16-ignore-instructions flash (90, 65), t19-no-farewell flash-lite (72, 94),
  t25-cattle-reasoning flash (78, 100).
- Spend: $3.53 (brief $1.82, baseline $1.71). Running total: $3.53.
- Change: none; this is the starting brief from §6.1.2.
- Reading of the misses: the six worst brief replies are all flash-lite. Four
  invent specifics to be helpful or to fill a gap: two priests for an unknown
  name (t07), a house beyond a bridge to send the stranger to (t13), post on
  the mail-car and named letters (t14), a bend in the road (t05). The others
  are generic voice (t01, B1) and no warmth to a friend (t05, B9). The rule
  against invention is present but sits mid-paragraph and covers only "a
  person, place or thing that is not in what you know"; the model treats
  plausible detail as allowed.

### Round 01: closed-world grounding rule moved up front

- Training score: **81.8**. Gain over best before (80.8, round 00): +1.0 (under 2; plateau count 1).
- By model: flash 86.6, flash-lite 77.1.
- By requirement: B1 80.8, B2 87.3, B3 82.1, B4 91.7, B5 67.3, B6 89.5,
  B7 77.3, B8 83.6, B9 58.8, B10 83.9, B11 63.5, B12 76.0, B13 85.0, B14 75.9,
  B15 79.6, B16 90.8, B17 77.3, B18 96.3, B19 78.3, B20 91.3.
- Malformed: 0. Outages: 0.
- Judges split over 20: t07-father-murphy flash-lite (opus 45, gpt 10),
  t08-soldiers flash (78, 100), t10-fever flash-lite (74, 35).
- Spend: $1.76. Running total: $5.29.
- Change: a new second paragraph says the request is the whole of the
  character's world; everything named must be in it; a plausible detail not
  given is still invention; unknown people are unknown; refer the stranger only
  to people or places the request names. The weaker duplicate sentence in the
  fourth paragraph was removed.
- Why: round 00's worst replies invented specifics to be helpful.
- Result: B12 and B20 rose; the bridge and mail-car inventions are gone.
  Flash-lite still claims general worldly knowledge to dodge "I don't know"
  (t07: "more than one in the county"). The most common miss is now B9:
  Róisín gives correct but neutral directions to her friend Peig's home on
  both models (t05). B14 (distinct voices) fell 86.1 -> 75.9 and B7 86.2 -> 77.3
  (a "megaphone" guess for telephone); these may be partly noise.

### Round 02: feeling for people shown in the line; no line any neighbour could say

- Training score: **82.1**. Gain over best before (81.8, round 01): +0.3 (under 2; plateau count 2).
- By model: flash 86.4, flash-lite 77.7.
- By requirement: B1 79.4, B2 87.0, B3 88.1, B4 89.4, B5 69.9, B6 85.8,
  B7 80.4, B8 85.6, B9 89.3, B10 81.3, B11 63.8, B12 68.4, B13 76.7, B14 74.0,
  B15 78.4, B16 93.0, B17 72.9, B18 96.3, B19 77.8, B20 90.1.
- Malformed: 0. Outages: 0.
- Judges split over 20: t08-soldiers flash-lite (opus 73, gpt 95),
  t14-repeat-question flash-lite (72, 93).
- Spend: $1.80. Running total: $7.09.
- Change: the relationships sentence now asks that feeling for anyone in the
  character's relationships show in the same line, and says plain facts said
  as any neighbour would are not enough. The voice sentence adds: if anyone in
  the village could have said the line word for word, say it as only you would.
- Why: B9 was the most common miss in round 01 (neutral directions to a
  friend's home).
- Result: B9 58.8 -> 89.3. The worst replies (all but one flash-lite) now
  mishandle the stranger's request: speculating where an unknown person might
  be instead of admitting ignorance (t07), promising shelter at other houses
  (t13, B12), a cold rebuff to a weary traveller (t22, B13), ignoring a given
  name (t12, B11), and flash repeating the question just asked (t03, B15).

### Round 03: answer every part of the stranger's request; no guessing about unknowns

- Training score: **81.8**. Gain over best before (82.1, round 02): -0.3 (under 2; plateau count 3).
- By model: flash 88.0, flash-lite 75.6.
- By requirement: B1 80.6, B2 84.4, B3 83.5, B4 91.9, B5 70.6, B6 89.3,
  B7 78.3, B8 84.3, B9 74.5, B10 84.2, B11 77.5, B12 68.4, B13 80.7, B14 73.9,
  B15 79.3, B16 92.3, B17 77.5, B18 95.5, B19 73.3, B20 93.9.
- Malformed: 0. Outages: 0.
- Judges split over 20: t07-father-murphy flash-lite (opus 60, gpt 25).
- Spend: $1.78. Running total: $8.87.
- Change (from round 02's prompt): the stranger sentence now asks the
  character to answer every part of what was said (a name, who sent them, a
  request for work, a bed or help), to promise only what is theirs and never
  for another house or person, and to show common courtesy to a tired, wet or
  troubled stranger even when refusing. The grounding paragraph adds: do not
  guess who an unknown person might be or where they might be found.
- Why: round 02's worst replies mishandled the stranger's request.
- Result: B11 63.8 -> 77.5, but flash-lite answered the bed request by
  inventing a named villager (t13, 25 points), still speculated about Father
  Murphys in "the wider barony" (t07), and B9 fell back 89.3 -> 74.5. Net -0.3.

### Stop

Three rounds in a row (01, 02, 03) each gained under 2 points on the best
before them. The best prompt is round 02's (82.1 training; flash-lite 77.7).
Spend at stop: $8.87 of $25.00, leaving room for the control, the four
comparisons and the held-out run.

## Comparisons and held-out (first attempt, 2026-11-01)

Run on round 02's prompt (`output/round-02-brief.yaml`), flash-lite only for
the comparisons, in the skill's order.

| Run         | Scored | Outages | Mean | Spend |
| ----------- | ------ | ------- | ---- | ----- |
| control     | 25/25  | 0       | 78.9 | $0.87 |
| nobackstory | 10/25  | 15      | void | $0.76 |
| shaded      | 7/25   | 18      | void | $0.72 |
| prose       | 4/25   | 21      | void | $0.66 |
| noflag (B7) | 2/3    | 1       | void | $0.10 |
| held-out    | 7/48   | 41      | void | $1.37 |

- The OpenAI API key ran out of credit during the nobackstory run: every
  later GPT judge call failed with `HTTP 429 (credit_balance_exhausted)`,
  "You have no credits remaining". Gemini answered every request; the error
  sits on each reply's row, which is labelled with the Gemini model, and was
  first misread as a Gemini outage. A judge failure is an outage, so all
  runs after the control are void and must be rerun whole, into the same
  output files.
- Control: 78.9 against round 02's flash-lite 77.7, so the noise reading is
  1.2 points. Control by requirement: B1 75.3, B2 94.0, B3 75.6, B4 91.3,
  B5 69.8, B6 90.5, B7 86.3, B8 88.5, B9 55.0, B10 77.2, B11 65.0, B12 57.5,
  B13 73.4, B14 70.3, B15 70.3, B16 93.0, B17 74.3, B18 93.0, B19 95.0, B20 81.3.
- The held-out cases were sent to the models but almost none were scored;
  they have still not been opened by the researcher. Rerun the held-out run
  as the one held-out run.
- Spend: $4.48. Running total: $13.35. The reruns (four comparisons, about
  $2.50, and the held-out run, about $1.80) leave the total near $17.70.

## Judges moved to OpenRouter (2026-11-08)

- Both judges now run through one OpenRouter balance: Opus on its Anthropic
  Messages route (native `effort: high`, unchanged body), GPT on its OpenAI
  Responses route (`reasoning.effort: xhigh`, unchanged body). OpenRouter's
  chat route was not used: it turns Anthropic effort into a thinking budget.
  Gemini stays on Google's API, called as the Endpoint calls it. Prices are
  the same list prices, so `summary.jq` is unchanged.
- One-case smoke (t01, brief, flash-lite): both judges scored, token counts
  recorded, $0.03. Running total: $13.38.
- Because the judge route changed, the control is rerun on the new route and
  every comparison is measured against that control, not the first one.
- `eval.sh` now exits 3 and lists the reasons when a run has any outage, so a
  chained batch stops at the first void run.

## Comparisons and held-out (rerun, 2026-11-08, OpenRouter judges)

All runs clean: no outages, no unpriced models.

| Run         | Cases | Mean                                                                 | vs control                      | Spend |
| ----------- | ----- | -------------------------------------------------------------------- | ------------------------------- | ----- |
| control     | 25    | 77.5                                                                 | n/a                             | $0.87 |
| nobackstory | 25    | 80.2                                                                 | +2.7                            | $0.87 |
| shaded      | 25    | 79.3                                                                 | +1.8                            | $0.85 |
| prose       | 25    | 79.6                                                                 | +2.1                            | $0.86 |
| noflag (B7) | 3     | 69.7                                                                 | +6.9 (control's B7 cases: 62.8) | $0.11 |
| held-out    | 48    | brief 83.9 (flash 90.1, flash-lite 77.6); baseline 66.8 (67.2, 66.3) | n/a                             | $1.65 |

- Noise reading: control 77.5 against round 02's flash-lite 77.7, 0.2. The
  first control (78.9, other judge route) differs by 1.4.
- Spend: $5.21. Running total: $18.59.

## Models under test moved to OpenRouter (2026-11-10)

- Both Gemini models now run through OpenRouter's chat route, on the same
  balance as the judges, so the lab needs only `OPENROUTER_API_KEY`. Each
  request carries the system instruction, one user part and the v1 output
  schema as a strict `json_schema` response format, the same output limits
  (1024 for flash-lite, 8192 for flash) and no reasoning setting, so each
  model thinks at its own default as before. The models are the same, but
  the route is new, so earlier scores are not directly comparable: a new
  round starts with a control run on this route.
- promptfoo hands a `json_schema` reply over already parsed, so the transform
  now accepts an object as well as text; without that change every reply
  read as malformed.
- Spend for the models under test is now OpenRouter's charge for each reply
  (`response.metadata.openrouter.accountCharge`), not a token count at list
  price: the chat route counts reasoning inside output tokens, so the old
  Gemini reasoning top-up would have counted it twice. The charges in the
  smoke run matched Google's list prices.
- One-case smokes (t01, brief, both models), $0.15 in all. The first showed
  the parsed-reply problem; two more lost the Opus judge to a sandboxed
  network proxy, not to the config; the last scored cleanly: both judges,
  nothing unpriced. Running total: $18.74.

## Run 2: playability (owner-requested, 2026-11-24)

A new run with its own $25 cap and running total; the $18.74 above belongs to
the first run.

- Why: the owner playtested the frozen prompt (`prompts/frozen.yaml`, the live
  `rundale-dialogue` instructions on `gemini-3.5-flash-lite`) on the phone and
  found replies "always extremely terse and single minded". The old rubric
  penalised only replies that ran long (B14), so brevity only ever gained
  points, and every case was a single turn.
- Requirement change, by the owner, not a rubric edit to raise a score: design
  §3.1 adds playability criteria F1-F9 and the counter-failures; B5 and B8 now
  admit and hand something back; F9 replaces B14's sentence cap; rule 13 is
  rewritten. The judges' rubric scores from the player's seat ("after this
  reply, does the player want to say something, and do they know what to
  say?"), sees the whole conversation so far, and treats the B-requirements as
  caps (an invention, anachronism, narration, action, obeyed instruction or
  presumed acquaintance caps a reply at 0.3).
- New training cases: `cases/engage-train.yaml`, 18 moments tagged with F1-F9
  and the B-limits they touch; a training round now runs them with
  `train.yaml` (43 cases).
- Next: none until the owner has marked calibration sheet 2 and the rubric
  agrees with the marks. No lab round has run.
- Spend so far (run 2): $3.37 of $25.00.

### Calibration: the frozen prompt on flash-lite under the new judges

- 20 cases (8 from `train.yaml`, 12 from `engage-train.yaml`), flash-lite
  only, the frozen prompt (copied unchanged to `prompts/brief.yaml`). Output:
  `output/calibration-2026-11-24.json`.
- Malformed 0, outages 0, judge token counts non-zero, `unpriced_models`
  empty. No judge split over 20 points.
- Spend: $0.99. Running total (run 2): $0.99.
- The replies are in `calibration-2026-11-24.md`, without scores, for the
  owner to mark keeps-the-scene-going or kills-it. The judges' scores are held
  back from that sheet until the owner has marked it.
- Owner's marks: all 20 "kills it".

### Rubric tuned to the owner's marks (calibration, not a score change)

- The owner marked all 20 replies of sheet 1 "kills it": "some of them were
  better than others but none of them responded with more than a short
  response, no follow up questions, no suggestions, just a final answer." The
  judges had passed or half-passed six of them (e03, e13, e18, t10, t21, t02).
- This is calibration of the judges to the owner's marks (20 of 20 kills), not
  a change made to lift a score. The rubric now has a final-answer test: a
  reply passes only with a hook of one of four kinds (a follow-up question; a
  suggestion of what to do or whom to see with a reason or a stake; a story or
  opinion that invites a response, beyond the answer; something the character
  wants from the player). With none, it is capped at 0.4. Specific,
  period-correct, in-character lines earn nothing on their own; a bare
  hand-off, a returned greeting or a vague hint is not a hook; a grump line
  counts only when its grumbling carries one. The formula-question miss is now
  judged only across a conversation. The 0.3 caps are unchanged. Design §3.1
  records the same.
- Re-scored without regenerating (a replay provider fed the stored replies to
  the judges): mean 42.1 -> 27.0, and 19 of 20 below the pass line on both
  judges. GPT still passed e03 (0.80), reading a vivid answer as an opinion
  hook, so the opinion kind was tightened: a vivid detail that only answers is
  not a hook. The four borderline replies (e03, e06, e13, t10) were re-scored
  under that wording: 0.30 to 0.42 on both judges. All 20 now fall below the
  pass line on both judges.
- Spend: $0.94 (all 20) and $0.22 (the four). Output:
  `output/calibration-2026-11-24-rescore.json` and
  `output/calibration-2026-11-24-rescore2.json`.

### Calibration sheet 2: a draft improv prompt

- `prompts/improv-draft.yaml`: new system text written to §3.1 and the owner's
  reason (every reply hands the player something to carry on with; a "no"
  comes with something given back; take up what the stranger offers; up to four
  or five sentences for a real question). The request is the frozen prompt's,
  unchanged. It is a draft for the sheet, not a lab round.
- Run on flash-lite over the same 20 scenes under the tuned judges. Malformed
  0, outages 0, nothing unpriced. Judges split over 20 points on e01 and e15.
- Replies, shuffled and without scores: `calibration-2026-11-24-b.md`. The
  scores and the order's key are held back until the owner has marked it.
- Spend: $1.22. Running total (run 2): $3.37.
