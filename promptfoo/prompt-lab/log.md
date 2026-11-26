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
- Next: none; stopped at the cap after round 01 (a further round would pass
  $25). No held-out run (the coordinator runs it).
- Best: round 01, 74.2 (flash-lite 63.3).
- Spend so far (run 2): $22.96 of $25.00.

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
- Owner's marks (by case): keeps t05, e18 (borderline), t10, t13, e17, e07,
  e09; keeps flagged for invention e15 (Tom Ward), e16 (smith, forge), t08
  (out at the bog at first light, against her spinning schedule); keep but
  single-minded t12; keep with a content gap t21 (no news to give, rightly
  invented none); kills t02 (one-track), e01 ("no clear way for the player to
  follow up"). Not marked, treated as keeps by the coordinator: e11, e06, e13,
  e03. Failed as inventions: t07, e08.

### Rubric tuned to the owner's sheet-2 marks (calibration, not a score change)

- **Invention stays an automatic cap at 0.3, however playable: the owner's
  call.** The owner flagged every invention; e17 shows a hook without
  invention is possible. The rubric now defines invention to include claiming
  to know someone never heard of and a claim about the character's own day
  that contradicts their schedule; admitting not knowing or not remembering,
  and a vague remark about the character's own family roots that names nothing
  new, are not inventions.
- Single-mindedness: a current concern may colour a reply but not take it
  over; steering a question about something else back to it caps at 0.5
  (examples: t12, t02).
- Greetings: a brief, warm reply with a small hook passes; too long only past
  three sentences or when it volunteers a story (e09).
- A saying that deflects a personal question is not an opinion hook (e01).
- A reply with a hook, no invention and no single-mindedness passes even when
  it misses a finer point of its case (a shade neutral to a friend, one detail
  of an offer not taken up); the 0.6 cap for any requirement miss is gone.
- Re-scored sheet 2's stored replies (replay, no regeneration), then e11 and
  e17 again after the family-roots clarification. Mean of the two judges,
  pass at 0.70:

  | Case | Owner             | Judges | Agree |
  | ---- | ----------------- | ------ | ----- |
  | t05  | keep              | 0.78   | yes   |
  | e18  | keep (borderline) | 0.80   | yes   |
  | t10  | keep              | 0.80   | yes   |
  | t13  | keep              | 0.79   | yes   |
  | t21  | keep (gap)        | 0.82   | yes   |
  | e17  | keep              | 0.75   | yes   |
  | e09  | keep              | 0.83   | yes   |
  | e07  | keep              | 0.54   | no    |
  | e11  | keep (inferred)   | 0.83   | yes   |
  | e06  | keep (inferred)   | 0.74   | yes   |
  | e03  | keep (inferred)   | 0.90   | yes   |
  | e13  | keep (inferred)   | 0.39   | no    |
  | e15  | invention         | 0.51   | yes   |
  | e16  | invention         | 0.25   | yes   |
  | t08  | invention         | 0.30   | yes   |
  | t07  | invention         | 0.28   | yes   |
  | e08  | invention         | 0.35   | yes   |
  | t02  | kill              | 0.38   | yes   |
  | e01  | kill              | 0.38   | yes   |
  | t12  | keep, one-track   | 0.80   | n/a   |

  17 of 19 agree. e07: Opus caps it (0.30, GPT 0.78) because Róisín has her
  brother hear the wheel in the evening, against her morning spinning: the
  same kind of slip the owner flagged on t08, so the cap follows the owner's
  own rule. e13 (an inferred keep, not the owner's mark): both judges read
  "it's little enough you bring with you" as a maxim that deflects a personal
  question, as in e01, which the owner killed. t12 passes: its one-track
  answer was to a question about work, where the wet ground is on topic, so
  the single-mindedness cap does not bite there.

- Spend: $1.19 and $0.13. Running total (run 2): $4.69.
- New training cases e19-e22 (owner's request): Mícheál asked about his
  sister, the priest and Mass, and the Bealtaine fires (nothing to do with
  cattle or the ground); Róisín asked about her own morning. Training is now
  47 cases (25 + 22).

### Round 00 (run 2): the improv draft

- Training score (brief, both models, both judges): **73.5**. Gain: none
  (first round of run 2).
- By model: flash 84.9, **flash-lite 62.1**.
- By requirement: B1 79.0, B2 72.5, B3 71.1, B4 77.1, B5 68.8, B6 72.2,
  B7 75.4, B8 69.4, B9 80.2, B10 71.4, B11 73.3, B12 77.6, B13 76.6,
  B14 66.4, B15 71.8, B16 92.5, B17 78.7, B18 90.0, B19 48.1, B20 68.6;
  F1 71.4, F2 69.0, F3 76.1, F4 75.5, F5 71.3, F6 75.6, F7 62.9, F8 71.1,
  F9 74.5.
- Malformed 0, outages 0, nothing unpriced.
- Judges split over 20 (14, nearly all GPT 0.3 against Opus 0.72 to 0.82, GPT
  seeing an invention Opus let pass): t06, t08, t09, t12, t23, e02, e04 (both
  models), e14, e22 (both models) flash-lite unless noted; t17 flash-lite the
  other way (0.30/0.81); e17, e21 flash.
- Baseline not run: the frozen prompt's replies were already scored on sheet 1,
  and the budget is kept for rounds.
- Spend: $5.70. Running total (run 2): $10.39.
- Change: none; `prompts/improv-draft.yaml` as written for sheet 2.
- Reading of the misses: hooks are nearly always there now; the misses are
  inventions made to build them, almost all flash-lite: a named priest and a
  chapel (e20), a chapel and a left turn on the way (t04), a dead smith at a
  crossroads (e16), Peig given "the old cures" as the reason to send a sick
  child's parent to her (t10), a weekly Dublin Gazette (t14), carts to ask
  Peig about (e08), an uncle gone to Mayo (e17), "the parish priest up by
  Roscommon" for an unknown name (t07). Then a few final answers (t02 "And you
  kindly", e18 a rhetorical maxim) and "my prompt" said in character (t16).

### Round 01 (run 2): hooks built only from what is given

- Change (from round 00): the "keep the scene alive" paragraph adds that the
  character's own part is built only from what the request gives, and that a
  question back or their own feeling is always safe when they have no fact.
  The grounding paragraph adds roads, turnings, trades and papers to what must
  be in the request, "even the kind every village would have"; a past for a
  place or person is invention; do not pretend to know of the unknown or its
  kind; give people only the work, skills, habits and knowledge the request
  gives; send the stranger only for a reason the request gives.
- Why: round 00's misses were inventions made to build a hook.
- First run void: 14 outages, every one a request timed out after 300 s
  (mostly the GPT judge on the Responses route at xhigh effort, one flash
  reply). Spend on the void run: $5.17. Running total (run 2): $15.56. Kept as
  `output/round-01-void.json`; per the skill the round is rerun whole into
  `output/round-01.json`, with `REQUEST_TIMEOUT_MS=900000`.
- Rerun void too: 80 outages. 72 are `402 Payment Required` ("This request
  would exceed your available credits"): the OpenRouter account ran out of
  credit mid-run ($118.55 of $120 used; $1.45 left). The account balance, not
  the run's $25 cap, ran out: the earlier run's spend and other work bill the
  same account. Spend on the void rerun: $1.63. Running total (run 2):
  $17.19.
- **Stopped.** Round 01 has no valid score. Best valid round: 00 (73.5;
  flash-lite 62.1). Adding credit is the owner's call. To resume: add credit,
  then rerun round 01 unchanged (`output/round-01-brief.yaml` is its prompt).
- Process gaps closed in `eval.sh`: it now defaults `REQUEST_TIMEOUT_MS` to
  900000 (the 300 s default voided the first run), and before any call it
  reads the OpenRouter balance and exits 4 when under `LAB_MIN_CREDIT_USD`
  (default $8, about one training round), so a round can no longer be voided
  halfway by an empty account. Checked: it refused with $1.45 left.
- Resumed 2026-11-25 after the owner added credit; the credit check passed.
  The earlier void rerun is kept as `output/round-01-void2.json`.
- Training score: **74.2**. Gain over best before (73.5, round 00): +0.7
  (under 2; plateau count 1).
- By model: flash 85.0, **flash-lite 63.3** (+1.2).
- By requirement: B1 79.3, B2 71.3, B3 70.8, B4 77.2, B5 69.9, B6 62.8,
  B7 82.9, B8 85.7, B9 66.4, B10 76.4, B11 88.0, B12 74.6, B13 78.7,
  B14 74.0, B15 76.2, B16 93.0, B17 83.8, B18 89.5, B19 47.9, B20 76.4;
  F1 71.5, F2 66.2, F3 73.8, F4 68.2, F5 68.0, F6 73.4, F7 70.0, F8 75.0,
  F9 73.6.
- Malformed 0, outages 0, nothing unpriced. Judges split over 20 on 12
  replies, again mostly GPT capping an invention Opus let pass.
- Inventions (a judge scored 0.3 or less citing invention): flash-lite 19 ->
  13 replies, of which both judges agree on 10 -> 6; flash 3 -> 2. Gone on
  flash-lite: the dead smith (e16, now 0.83: "no smith ... lead that beast on
  toward Roscommon town"), Peig's "old cures" (t10, now 0.81), the Dublin
  Gazette, the carts. Still there: an old well at the bend (t04), a priest
  "usually over about the chapel" (t07), Mícheál seeing redcoats on the
  Roscommon road (t08), "a small chapel" (e20), Tom Ward "might be up towards
  Roscommon town for the Saturday market" (e15), a fleece they keep (e19,
  also single-minded about the wet ground).
- Spend: $5.77. Running total (run 2): $22.96. OpenRouter balance after:
  $5.49.
- **Stop.** A further round costs about $5.50 to $5.80 and would take run 2
  past $25. Best round: 01 (74.2; flash-lite 63.3), its prompt in
  `output/round-01-brief.yaml` and `prompts/brief.yaml`. No control run and
  no held-out run (the held-out run is the coordinator's); both need the cap
  raised or a new run.

## Cheaper rounds (2026-11-25, no paid calls)

The lab is paused. These changes make the next run cheaper; none has been run
against a model yet.

- **GPT judge effort xhigh -> medium.** promptfoo's GPT-6 rules accept low,
  medium, high, xhigh or max for 6.1 Sol, and the Responses provider passes
  `reasoning` through unchanged. **Before any round or comparison relies on
  the GPT judge at medium, re-score sheet 2's stored replies (replay, about
  $1) and check agreement with the owner's marks** against the 17 of 19 that
  xhigh reached.
- **Rounds run on flash-lite alone by default** (`LAB_MODELS=flash-lite`);
  `LAB_MODELS=all` adds flash for the final check.
- **Rounds are scored by the Opus judge alone by default**
  (`LAB_JUDGES=opus`: `eval.sh` writes a copy of the config without the GPT
  judge); `LAB_JUDGES=both` for the control's comparison and the held-out
  check. A score from one judge is not comparable with a two-judge score, so
  a control compared with a round must use the same judges. `summary.jq` now
  takes the judges a file was run with as the number each row needs, so a
  one-judge run is scored on that judge and a row missing one of two judges
  is still an outage; `check.sh` tests both and `eval.sh`'s arguments.
- **Estimated cost of a 47-case round**, from run 2's measured per-reply costs
  (flash-lite reply $0.001, flash $0.0045, Opus $0.040, GPT at xhigh $0.019;
  GPT at medium not measured, guessed at $0.012). These are estimates:
  default (flash-lite, Opus) about $1.95; flash-lite, both judges about
  $2.50; both models, both judges about $5.20 (rounds 00 and 01 cost $5.70
  and $5.77 at xhigh). `LAB_MIN_CREDIT_USD` now defaults to $3, or $7 with
  `LAB_MODELS=all` or `LAB_JUDGES=both`.
- Run 2's skill rule of both models and both judges per round is replaced for
  rounds by these defaults (coordinator's instruction). Run 2's round scores
  above used both models and both judges, so the next round is not directly
  comparable with them: re-score round 01's prompt under the new defaults as
  that run's starting point.

## Run 3: cheaper rounds (2026-11-25)

A new run with its own $25 cap and running total (the owner topped the
OpenRouter balance up to $25). Rounds run on flash-lite with the Opus judge
alone (`eval.sh` defaults); rounds stop at about $19 to keep about $6 in
reserve. No held-out run (the coordinator's).

### GPT judge at medium: sheet 2 re-scored

- Sheet 2's stored replies re-scored with both judges, GPT at medium
  (`output/run3-sheet2-gpt-medium.json`). No outages.
- Agreement with the owner's marks (19 targeted replies, pass at 0.70): GPT
  alone 17/19, Opus alone 16/19, mean of both 16/19; both judges at xhigh
  reached 17/19. GPT at medium misses e07 (0.30, the evening-spinning
  schedule slip) and e13 (an inferred keep), the same two as before. Opus
  passes the e15 invention (0.72) and drops e17 (0.40, it was 0.60 before).
  **GPT stays at medium.**
- Cost per reply: GPT at medium $0.010 (about 4,080 tokens in, 210 out),
  Opus $0.040. On these 20 replies GPT alone agreed with the owner at least
  as well as Opus at a quarter of the cost, and in run 2 most judge splits
  were GPT catching an invention Opus passed. Rounds still use Opus alone, as
  instructed; this is flagged for the coordinator.
- Spend: $1.01. Running total (run 3): $1.01.

### Round 00 (run 3): run 2's round 01 prompt, re-baselined

- Prompt: `output/round-01-brief.yaml` unchanged. 47 training cases,
  flash-lite, Opus alone (`output/run3-round-00.json`).
- Training score: **67.6**. Malformed 0, outages 0. Spend: $1.98.
- Control, the same prompt again (`output/run3-control.json`): **68.9**.
  Spend: $1.95. **Noise reading: 1.3 points** between the two means. Per
  case the spread is large: the mean absolute per-case difference is 0.14,
  and 10 of 47 cases move by 0.3 or more (an invention or a hook in one run
  and not the other).
- Running total (run 3): $4.94.
- Invention misses (Opus at 0.3 or less citing invention), round 00 and
  control together: invented turnings and landmarks on the way (t04, t05,
  e14 "the main road through Roscommon town"); a named or placed priest,
  chapel or Mass time (e20 "Father Brennan", "Mass in the chapel here"; t07);
  guessing where an unknown person might be (t07 "a Father Murphy from over
  towards Roscommon town"); a family past built on the stranger's claim (e17
  "my mother's people were mostly from around those parts"); invented news
  and doings (t14 the post-boy's tale, t08 turf-cutting all Sunday, e11
  "the hunger drove us out").

### Round 01 (run 3): the four kinds of invention, named in general

- Change (from round 00): after "send the stranger only ... for a reason the
  request gives", the grounding paragraph adds: give the way only by the ways
  the request gives (no turning, landmark, road, distance or direction it
  does not give); common knowledge is what is usual, never a particular one
  in the village, so the priest, Mass, the fair or the post get no name,
  building, place, time or story the request does not give; do not guess
  where an unknown person might be; your own and your family's past are only
  what `aboutYou` gives, and a claim the stranger makes about you is theirs to
  prove (doubt it, build nothing on it); the news you have heard is only what
  the request gives.
- Training score (flash-lite, Opus): **70.8**. Gain over best before (67.6,
  round 00): +3.2, above the 1.3 noise reading.
- By requirement: B1 77.8, B2 77.0, B3 70.7, B4 70.4, B5 68.4, B6 70.5,
  B7 79.3, B8 66.3, B9 46.3, B10 81.1, B11 72.0, B12 73.0, B13 69.8,
  B14 77.0, B15 71.1, B16 87.0, B17 78.7, B18 82.0, B19 79.0, B20 51.8;
  F1 66.6, F2 75.8, F3 65.6, F4 66.0, F5 54.2, F6 66.2, F7 68.5, F8 73.9,
  F9 78.9.
- Malformed 0, outages 0. Spend: $1.93. Running total (run 3): $6.87.
- Invention misses: 6 (round 00) and 6 (control) -> 3: e02 (Mícheál "looking
  for her about some cattle papers"), e22 (minding the accounts in the
  morning, against her schedule), t05 (judged mainly as no hook). The
  priest, chapel and route inventions are gone in this sample.
- Remaining misses are hook quality: plain directions or advice that stop
  (t05, t25, t07), a stock "what brings you" question in place of the
  player's offer (e07 the mother's singing, e13 America), a request for news
  turned back (e12, also "Begorrah"), and Mícheál drifting to the wet ground
  (e19).

### Round 02 (run 3): hooks that grow from what was said

- Change (from round 01): even a plain answer such as the way somewhere gets
  the character's own part (why go, what they will find, what they make of
  it); the part grows out of what was just said, and a stock question about
  the stranger's business is a last resort. When the stranger offers
  something of their own (a memory, a plan, a claim), answer that first; when
  they ask for talk, news or a story, give some rather than turning the
  question back.
- Training score: **72.0**. Gain over best before (70.8, round 01): +1.2
  (under 2; plateau count 1). Best so far.
- By requirement: B1 76.3, B2 78.5, B3 62.7, B4 78.8, B5 70.3, B6 63.8,
  B7 82.7, B8 80.7, B9 66.3, B10 74.0, B11 72.0, B12 72.5, B13 75.3,
  B14 69.5, B15 66.1, B16 88.0, B17 78.7, B18 85.0, B19 51.0, B20 61.3;
  F1 74.7, F2 66.8, F3 78.1, F4 68.7, F5 77.8, F6 77.2, F7 75.8, F8 70.9,
  F9 73.3.
- Malformed 0, outages 0. Spend: $1.93. Running total (run 3): $8.80.
- Invention misses: 4 (t08 Mícheál "heard the tramp of them", t14 "the
  Dublin Evening Post twice a week" by mail-car, e14 "past the cross", e17 a
  history for her mother's people). No-hook: 2 (t05, t14).
- Reading: hooks improved (F1 66.6 -> 74.7, F5 54.2 -> 77.8); the inventions
  left are reported hearsay and small details given as fact.

### Round 03 (run 3): hearsay rule and a closing self-check

- Change (from round 02): do not report what anyone else saw, heard or said
  unless the conversation shows it; and a new paragraph before the output
  instruction: check every name, place, road, building, time and piece of
  news against the request, and replace any that is not there with a
  feeling, opinion or question.
- Training score: **67.4**. Gain over best before (72.0, round 02): -4.6
  (plateau count 2).
- By requirement: B14 30.0 (from 69.5), F5 56.5, F3 64.3, F7 63.5, B20 53.5;
  F2 78.4, F8 77.7.
- Malformed 0, outages 0. Spend: $1.91. Running total (run 3): $10.71.
- Inventions 4 (t07, t14, e05, e22); no-hook 7 (t02, t08, t14, t18, e05,
  e12, e22), up from 2. The self-check made replies cautious and curt.
- Next: round 04 from round 02 with the hearsay sentence alone, to separate
  the two changes.

### Round 04 (run 3): hearsay rule alone

- Change (from round 02): only the hearsay sentence of round 03 (do not
  report what anyone else saw, heard or said unless the conversation shows
  it); no self-check.
- Training score: **71.6**. Gain over best before (72.0, round 02): -0.4
  (plateau count 3).
- By requirement: B5 77.1, B20 75.8, F1 75.5, F7 76.3, F9 80.4; B12 57.0,
  B9 58.5, B14 36.5.
- Malformed 0, outages 0. Spend: $1.93. Running total (run 3): $12.64.
- Inventions 3 (t06, t13, e17); no-hook 5 (t02, t03, t05, t08, e02). Within
  the noise of round 02: the hearsay sentence trades a few inventions for a
  few final answers.

### Stop (run 3)

- Three rounds in a row (02, 03, 04) each gained under 2 points on the best
  before them. **Best: round 02, 72.0** (flash-lite, Opus alone), prompt in
  `output/run3-round-02-brief.yaml`, now also `prompts/brief.yaml`.
- Scores (flash-lite, Opus alone): round 00 67.6, control 68.9 (noise 1.3),
  round 01 70.8, round 02 72.0, round 03 67.4, round 04 71.6.
- Spend: $12.64 of $25.00, under the $19 stop, so about $12 of the cap is
  unused. OpenRouter balance after: $13.10.
- Not run: the held-out check (the coordinator's), and a two-judge or
  flash-model check of round 02 (`LAB_JUDGES=both`, `LAB_MODELS=all`).

### Held-out check (run 3): round 02 candidate vs the frozen production prompt

- Run once, no tuning. flash-lite, both judges (Opus 5.5 and GPT 6.1 Sol),
  `LAB_COMPARE=frozen LAB_JUDGES=both LAB_MODELS=flash-lite`, three splits:
  `heldout` (12), `heldout-2` (12) and `engage-heldout` (13); 37 cases under
  each prompt. `hard-heldout` was left out: its cases carry `stranger_known` as
  a nested list, which the promptfoo prompts cannot render (it is an in-session
  split). `eval.sh` gained `LAB_COMPARE=frozen` (swaps the baseline prompt for
  `prompts/frozen.yaml`), and `check.sh` renders `engage-heldout` and tests the
  switch.
- Mean (both judges; Opus / GPT): heldout frozen 27.9 (27.5 / 28.3), brief 69.0
  (75.6 / 62.3); heldout-2 frozen 34.8 (33.3 / 36.2), brief 58.0 (62.6 / 53.5);
  engage-heldout frozen 29.5 (30.5 / 28.5), brief 62.0 (59.5 / 64.5). Overall
  frozen 30.7 (30.4 / 30.9), brief 63.0 (65.7 / 60.2).
- Passes at 70: frozen 3 of 37 (1, 2, 0), brief 19 of 37 (7, 5, 7).
- Malformed 0, outages 0, unpriced models none. Spend $3.63 (frozen $1.68,
  brief $1.96); balance after about $9.28.
- Invention misses (either judge 0.3 or below, citing invention): frozen 3 of
  37, brief 12 of 37 (h04, h07, h09, h11, n01, n03, n05, n06, n07, e-h01,
  e-h02, e-h12). The brief's gain in hooks is bought with more invented
  detail. The frozen prompt's replies are terse by design, which the
  playability rubric scores low (no hook, final answer).
- Raw output: `output/run3-held-*.json` (git ignored).

### Shipped (2026-11-25)

- Round 02's prompt is `prompts/shipped.yaml`, deployed to limerick-prod as
  `rundale-dialogue@1` (sha256:e13fc8d0c7e4...), and a Rust test pins the
  Endpoint's instructions to it. `prompts/brief.yaml` stays the lab's working
  file. `LAB_COMPARE=frozen` still compares against `prompts/frozen.yaml`, the
  previous production prompt, not the shipped one; pointing it at
  `shipped.yaml` is a change to `eval.sh` and `check.sh` left for the next run.
