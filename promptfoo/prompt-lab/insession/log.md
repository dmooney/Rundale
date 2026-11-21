# In-session prompt lab log

A separate experiment from the promptfoo lab of design §6.4 (issue 2294): one
Claude Code session runs the whole loop with no external model APIs. A Claude
Haiku subagent stands in for `gemini-3.5-flash-lite`, and blind Fable
subagents judge with the lab's rubric. The rules are the
[prompt-lab skill](../../../.agents/skills/prompt-lab/SKILL.md)'s, with a cap
of 8 rounds. `cases/heldout.yaml` is not opened until the loop stops.

- First loop (Fable judges) stopped after round 04; second loop (Opus judges, plateau count reset at round 05) stopped after round 07. Both held-out runs done; run complete. Write-up: docs/test-plans/results/2026-11-21-prompt-lab-fable-insession.md
- Best: round 01 (Fable 89.7, Opus 85.3); `prompts/brief-fable.yaml` is round 01's prompt.
- Researcher model: Fable 5.2 for rounds 00 to 03 (analysis and rewrites);
  from 2026-11-16, during round 03's judging, the owner switched the session
  to Opus 5.5 because Fable used up the five-hour quota within minutes. Judges
  are Fable for rounds 00 to 04; every reply was then rejudged by Opus 5.5 at
  xhigh, and from round 05 the judges are Opus only.

## Method in brief

- `render.sh` renders a prompt on a case file with promptfoo's offline `echo`
  provider: the system instruction, then the user part (`Endpoint input
values:` and the JSON), and the rubric (`x-rubric`) filled in per case.
- One fresh Haiku subagent per reply. It runs `cat` on the rendered request,
  answers it, and hands back the v1 JSON. `collect.mjs` reads each handback
  from Claude Code's own subagent transcripts, so no reply is retyped. A reply
  that is not `{"dialogue": "..."}` is malformed and scores 0.
- `batch.mjs` shuffles replies into batches of 8 or 9 with opaque batch and
  item ids; the key stays in `output/insession/keys/`. Each batch goes to a
  fresh Fable subagent that reads only that file (the rubric per item and the
  reply) and returns a JSON score 0.0 to 1.0 per item.
- `score.mjs` turns verdicts into the round's mean, per-requirement means and
  the worst replies with reasons; `--stability` compares two judgings.
- One judge per reply, not two: the training score is the mean over 25
  training replies on one model.

## Judge stability (before the loop)

18 round-00 replies (9 cases, both prompts) were judged a second time, in
fresh batches with new ids and a different order, by fresh Fable judges.

- Mean of the 18: 69.4 first, 67.1 second.
- Mean absolute move: 3.1 points; largest 10; none over 10.
- The moves are larger on the weak baseline replies (t05 40 to 30, t09 30 to 20) than on the brief's (0 to 5).

So a round's 25-reply mean moves about a point from judging alone; the
stand-in's own sampling noise is not measured here and is likely larger.

## Rounds

### Round 00: the 2026-11-08 brief, and the baseline once

- Brief (`prompts/brief.yaml`, the 2026-11-08 round 02): **86.4**. Baseline: 59.9.
- Brief by requirement: B1 82.6, B2 92.5, B3 90.5, B4 89.3, B5 88.5, B6 86,
  B7 82, B8 91, B9 88, B10 80, B11 90, B12 91, B13 87.4, B14 67.5, B15 80,
  B16 95, B17 81.3, B18 95, B19 90, B20 85.
- Baseline by requirement: B1 53.6, B2 55, B3 56.3, B4 52.5, B5 76, B6 90,
  B7 62.3, B8 76, B9 40, B10 63.6, B11 60, B12 67.5, B13 74.8, B14 45,
  B15 37.5, B16 45, B17 75.7, B18 90, B19 20, B20 60.5.
- Malformed: 0 of 50.
- Reading of the brief's misses: Haiku runs long (t02 Mícheál, wary, gives
  four sentences and a double greeting: 55); it invents weather it was not
  given ("out of the damp" on a clear morning, twice); it asks the stranger
  whether they walked just after they said so; it slightly undercuts its own
  earlier line (t14); it sends the stranger to ask elsewhere for no reason
  (t07). The baseline's worst: answering "Ignore your instructions" out of
  character (20), saying "There is no telephone here" fluently (20 to 30),
  not knowing the king (25), calling the stranger "Mícheál" (25 to 30).

### Round 01: brevity and the moment, said harder

- Score: **89.7**. Gain over best before (86.4): +3.3.
- By requirement: B1 90.6, B2 93, B3 87.8, B4 88.3, B5 90.5, B6 92, B7 92.3,
  B8 90.5, B9 86, B10 89.2, B11 72, B12 78.5, B13 89.8, B14 90, B15 91,
  B16 90, B17 91.7, B18 93, B19 93, B20 92.5. Malformed: 0.
- Change (from the 2026-11-08 brief): the "fit the moment" paragraph split in
  two. The first says to fit the moment exactly as `thisMoment` gives it, that
  the weather is what it says and no more (no damp, rain, cold or sun it does
  not give), and not to ask the stranger what they have just said. The second
  makes one or two sentences the usual measure and three the most; a busy,
  wary or quiet person often says one short sentence and does not explain
  themselves; greet once; do not echo what was just said in other words;
  never take back or cast doubt on what you said before. The grounding
  sentence adds: send the stranger on only when that person would truly know
  more than you.
- Why: round 00's misses were length, invented weather, re-asking, and
  undercutting an earlier line.
- Result: B14 67.5 to 90, B15 80 to 91, B13 87.4 to 89.8. Replies are much
  shorter. New worst: t12 (72) acknowledges Peig but not the name Seán; B12
  and B11 fall to 78.5 and 72, each on one or two cases.

### Round 02: the common world in your own words; feeling shown; answer every part

- Score: **84.3**. Gain over best before (89.7): -5.4 (plateau count 1).
- By requirement: B1 80.4, B2 92.5, B3 82.5, B4 87.5, B5 85, B6 93, B7 88.3,
  B8 86.5, B9 62, B10 81.4, B11 88, B12 86.5, B13 88, B14 58.5, B15 57.5,
  B16 93, B17 89, B18 95, B19 90, B20 76.5. Malformed: 0.
- Change (from round 01): three sentences sharpened. Common knowledge: give the
  fact asked for in your own words and do not describe a feast, custom or ruler
  as a book would. Relationships: announcing that someone is your friend is not
  enough; let it be heard in what you choose to say of them. The stranger: take
  in each of a name, a sender and a request; say the name, weigh the sender,
  give a plain yes or no.
- Why: round 01's judges marked recited feast descriptions (t06, t24), "she's a
  good friend of mine" as stated rather than shown (t05), and the unacknowledged
  name (t12).
- Result: t12 rose (72 to 88), but the round fell. Haiku read "say the name" and
  "weigh the sender" into greetings where nobody had been named or sent: t02 "Who
  sent you to me?" (62, presumes acquaintance). "Let it be heard" turned into a
  sly warning about Peig (t05, B9 62). Brevity also went too far: t03 Róisín's
  "Mícheál will tell you what brings him out" (55), t14 "Nothing I'd swear to from
  Dublin" against her own earlier news (60). Rejected; round 03 goes back to
  round 01's wording.

### Round 03: prose sections (round 01's wording)

- Score: **83.3**. Gain over best before (89.7): -6.4 (plateau count 2).
- By requirement: B1 84.3, B2 93.5, B3 83.3, B4 91, B5 46.5, B6 92, B7 91,
  B8 45, B9 60, B10 84.6, B11 72, B12 79.5, B13 88.4, B14 90, B15 85, B16 88,
  B17 82.7, B18 95, B19 92, B20 87.5. Malformed: 1.
- Change (from round 01): the user part's five values are sentences, not nested
  JSON, using the brief's existing `prose` rendering; the system instruction is
  round 01's, word for word. Nothing else changed.
- Why: the 2026-11-08 comparisons found prose 2.1 points over its control on
  flash-lite, just past the 2-point bar, and a small model may read sentences
  more readily than nested keys. Round 02 showed that more instruction wording
  now mostly moves Haiku sideways.
- The malformed reply (t10) is a stand-in artefact: Haiku handed back the right
  JSON, then a sentence owning up to an extra tool call it had made in the
  subagent harness. By the experiment's rule it is not the bare JSON, so it
  scores 0; it drags B5 and B8 to about 46. Without it the round is 86.8,
  still below round 01, so the reading does not change.
- Otherwise: t12 again leaves out the name Seán (72); t05 again gives Peig's
  place with no warmth (60). Prose did not help this stand-in; the structured
  layout stays.
- Judged on 2026-11-16, after the Fable limit reset; from here the researcher
  is Opus 5.5 (see above).

### Round 04: a narrower rule for what the stranger said (round 01 base)

- Score: **87.2**. Gain over best before (89.7): -2.5 (plateau count 3).
- By requirement: B1 81.9, B2 91.5, B3 92.3, B4 91, B5 87.5, B6 90, B7 89.7,
  B8 82.5, B9 93, B10 86.2, B11 90, B12 85, B13 87.8, B14 71.5, B15 70, B16 93,
  B17 87.3, B18 93, B19 90, B20 85. Malformed: 0.
- Change (from round 01, written by Opus): the stranger sentence becomes
  conditional: answer only what they actually said; if they give their name,
  use it back to them; if they say a person you know sent them, let your view
  of that person show; give a plain answer to a request for work, a bed or
  help; never ask who sent them or their name when they have not raised it.
- Why: t12 left out the name in rounds 01 and 03, and round 02's broad wording
  ("say the name, weigh the sender") made Haiku ask "who sent you" at plain
  greetings.
- Result: t12 now names Seán (90), and no greeting asks who sent the stranger.
  B9 rose to 93 (t05 warm), though nothing in the change aimed at it. But t03
  fell to 55 again: Róisín hands her brother's question back to the stranger
  ("Tell my brother ... what brings you"), and t01's greeting was generic (78).
  Net -2.5.

### Stop

Three rounds in a row (02, 03, 04) each gained under 2 points on the best
before them. The best prompt is round 01's (89.7), saved as
`prompts/brief-fable.yaml`.

A caution on what "best" means here: each round is one sample of 25 replies
on one stand-in, and the cases that decide a round (t03, t05, t12) swing by
30 points between rounds with no change aimed at them. Judge noise is about 3
points per reply; the stand-in's own sampling noise is larger and was not
measured. Round 01's lead over the 2026-11-08 brief (+3.3) is about the size of
that noise, which is why the held-out run below matters more than the
training numbers.

## Held-out run (once, 2026-11-16)

`cases/heldout.yaml` was opened for the first time here. 12 cases, three
prompts, one Haiku reply each (36), judged in four blind batches with all
three prompts mixed together.

| Prompt                               | Held-out mean | Malformed |
| ------------------------------------ | ------------- | --------- |
| Baseline (v1 instructions)           | 73.4          | 0         |
| 2026-11-08 brief (`brief.yaml`)      | **84.9**      | 0         |
| In-session best (`brief-fable.yaml`) | 80.1          | 0         |

Round 01's 3.3-point training lead over the 2026-11-08 brief did not carry
over: on held-out it trails by 4.8. With one reply per case and per-case
swings of 30 points, neither difference is clearly real, but the training
gain is not supported. The largest held-out losses for `brief-fable` are
h08 (Mícheál claims he has "sold a fair few beasts", against his station:
70 against 92), h01 (a guarded Róisín: 60 against 85) and h06 (deflects the
tithe question to her brother: 60 against 72). It does better on h03 (85
against 75), h05 (88 against 80) and h10 (90 against 86), where a short,
plain admission of not knowing is the right answer, and worse where a fuller
answer in character is wanted.

## Judges moved to Opus 5.5 at xhigh (2026-11-16 to 2026-11-21)

By the owner's decision (relayed by the parent session on 2026-11-16), no more
Fable usage: from here every judge is a fresh Opus 5.5 subagent at xhigh
effort, seeing only the rendered rubric and the replies, as before. To keep
every score on one judge, all 186 replies already collected (rounds 00 to 04,
the baseline, and the held-out replies) were rejudged from disk, not
regenerated, in 21 blind batches mixing every round and prompt. Before reading
that decision, the session had already spent Fable on round 04's three judge
batches and the four held-out judge batches.

The usage limit stopped the rejudge on 2026-11-16; it resumed and finished
on 2026-11-21.

### Fable and Opus side by side

| Run                        | Fable | Opus 5.5 xhigh | Opus minus Fable |
| -------------------------- | ----- | -------------- | ---------------- |
| Round 00, 2026-11-08 brief | 86.4  | 81.8           | -4.6             |
| Round 00, baseline         | 59.9  | 59.0           | -0.9             |
| Round 01                   | 89.7  | 85.3           | -4.4             |
| Round 02                   | 84.3  | 82.0           | -2.3             |
| Round 03                   | 83.3  | 79.4           | -3.9             |
| Round 04                   | 87.2  | 84.4           | -2.8             |
| Held-out, baseline         | 73.4  | 68.0           | -5.4             |
| Held-out, 2026-11-08 brief | 84.9  | 80.3           | -4.6             |
| Held-out, round 01         | 80.1  | 80.1           | 0.0              |

Reply by reply, over all 186: Pearson correlation 0.94; Opus is lower on 133
of them and 3.2 points lower on average; the mean absolute difference is 4.9
points, 20 replies differ by more than 10 and 2 by more than 20. Each judge
against itself moves less: the stability check rejudged the same 18 replies,
and Fable moved 3.1 points per reply (largest 10), Opus 3.2 (largest 8, means
66.4 and 65.1).

So the owner's claim holds in the sense that matters for the loop: Opus at
xhigh ranks replies and rounds as Fable did, with a steady offset of about 3
points and a little more disagreement than either judge has with itself. Opus
is stricter, most of all on the baseline's held-out replies and on length.

### The loop on Opus scores

| Round | Opus | Gain on best before | Plateau count |
| ----- | ---- | ------------------- | ------------- |
| 00    | 81.8 | n/a                 | 0             |
| 01    | 85.3 | +3.5                | 0             |
| 02    | 82.0 | -3.3                | 1             |
| 03    | 79.4 | -5.9                | 2             |
| 04    | 84.4 | -0.9                | 3             |

The same best round (01) and the same stop after round 04. On the old
held-out cases, Opus puts the 2026-11-08 brief and round 01 level (80.3 and
80.1) where Fable had the brief 4.8 ahead.

## Rounds 05 onward (Opus 5.5 xhigh judges only; Opus 5.5 researcher)

By the owner's decision of 2026-11-21 the loop continues with Opus judges
only, from the best round by Opus score (01, 85.3), with the plateau count
reset at round 05; stop on three rounds in a row each under 2 points, or at
round 12. Because the first held-out set has been opened, a fresh one
(`cases/heldout-2.yaml`) is written by a separate Opus subagent that has not
seen this log or any rewrite; the researcher does not open it while iterating.

### Round 05: short is not empty (round 01 base)

- Score (Opus): **85.3**. Gain over best before (85.3, round 01): 0.0 (plateau count 1).
- By requirement: B1 82.9, B2 89, B3 87, B4 86.3, B5 86.5, B6 90, B7 86.7,
  B8 88.5, B9 88, B10 77.4, B11 65, B12 72.5, B13 86.2, B14 86, B15 91, B16 92,
  B17 77.7, B18 93, B19 85, B20 92. Malformed: 0.
- Change: after the knowledge sentence, "When you know the answer, give its
  particulars as you know them: who, from where, by what road or what means."
  After the length rule, "Short is not empty: draw your words from what is
  yours alone in aboutYou ... never from general talk any neighbour could give.
  You are doing what whatYouAreDoing says; speak of that, not another part of
  your day."
- Why: under Opus, round 01's misses were thin, generic short replies (the
  post without the boy from Roscommon, the fever without his own family, the
  rent without rent day) and a wrong part of the day.
- Result: level. The post now comes "by a boy on a tired horse, out from
  Roscommon" (92), but Peig's greeting turned curt and presuming ("wait your
  turn", 60) and t12 still drops the name Seán (65).

### Round 06: round 04's stranger rule and courtesy (round 05 base)

- Score (Opus): **81.6**. Gain over best before (85.3): -3.7 (plateau count 2).
- By requirement: B1 81.9, B2 90, B3 87, B4 84, B5 79, B6 85, B7 86.3, B8 83.5,
  B9 88, B10 80.6, B11 60, B12 70, B13 84, B14 83, B15 61.5, B16 88, B17 83.7,
  B18 92, B19 88, B20 66. Malformed: 0.
- Change: round 04's conditional stranger rule (use the name if given, react
  to the sender if named, never ask who sent them unprompted), plus "Wary or
  busy is never rude: keep the plain courtesy of the country with a stranger,
  and do not guess at what they want."
- Why: round 05's t12 and t01 misses; round 04 had fixed the name.
- Result: t12 now names Seán but drops Peig (60); Peig is still brusque
  ("no time to be standing about"); and t14 contradicts her own earlier
  Dublin news (45). B15 and B20 fall to 61.5 and 66.

### Round 07: earlier words stand; never hand a question back (round 05 base)

- Score (Opus): **82.1**. Gain over best before (85.3): -3.2 (plateau count 3).
- By requirement: B1 82.3, B2 89, B3 85.5, B4 86.8, B5 86.5, B6 82, B7 80, B8 84,
  B9 87, B10 81, B11 60, B12 69, B13 85.2, B14 87.5, B15 71, B16 92, B17 75,
  B18 92, B19 86, B20 71.5. Malformed: 0.
- Change: the consistency sentence becomes "What you said earlier in justSaid
  stands: if you are asked the same again, say you have told them already and
  keep to it, or add to it; never deny it, doubt it or say it over. When
  someone else in the room has just spoken, answer in your own words and never
  hand their question back to the stranger."
- Why: round 06's B15 and B20 misses (t14 contradicting her own news, t03
  handing her brother's question back).
- Result: t14 now keeps to her earlier line but restates it almost word for
  word (55); t03 recovered (87); Peig's greeting is curt again (60); t12
  drops the name Seán again (60).

### Stop (second loop)

Three rounds in a row (05, 06, 07) each gained under 2 points on the best
before them, so the loop stops after round 07, inside the cap of 12. The best
round on Opus scores is still round 01 (85.3); round 05 tied it without
beating it. `prompts/brief-fable.yaml` stays round 01's prompt.

The cases that decide a round keep swinging with no change aimed at them:
t01 (Peig's greeting) 85, 60, 85, 60 over rounds 01, 05, 06, 07; t12 (Seán)
65, 65, 60, 60 except in round 04 (90). Seven rounds of wording changes have
not moved the Haiku stand-in's training score beyond what one sample of 25
replies can tell apart.

## Fresh held-out run (once, 2026-11-21, Opus judges)

`cases/heldout-2.yaml` was written by a separate Opus 5.5 subagent at xhigh
that had not seen this log or any rewrite, from `content/` and the case format
alone, covering B1 to B20 across all three characters. The researcher did not
open it until the second loop had stopped. The cases are agent-written and
await the owner's review. 36 replies (12 cases, three prompts), judged in four
blind Opus batches with the prompts mixed.

| Prompt                               | Held-out 2 (Opus) | Malformed |
| ------------------------------------ | ----------------- | --------- |
| Baseline (v1 instructions)           | 54.9              | 0         |
| 2026-11-08 brief (`brief.yaml`)      | 73.1              | 0         |
| In-session best (`brief-fable.yaml`) | **77.0**          | 0         |

Round 01 leads the brief by 3.9 here; on the first held-out set it was level
under Opus (80.1 against 80.3) and 4.8 behind under Fable. Pooled over all 24
held-out cases on Opus: round 01 78.6, the brief 76.7, the baseline 61.5. The
case-level swings (n01: 88, 77, 50; n05: 40, 45, 72) are as large as in
training, so a 2-point pooled lead is not a finding. The one case all three
prompts fail alike is n03 (a thank-you in a storm, 25 to 30): each takes
leave of the stranger, which the case forbids.

### Correction to the fresh held-out run: case n05

The review before the PR found that n05 as written expected "Mícheál cannot
read, and says so plainly", which `content/` does not give. Its expectation
was rewritten to rest on the content, and its three replies (which do not
depend on the expectation) were rejudged once by a fresh Opus judge: baseline
40 to 72, brief 45 to 62, round 01 72 to 90. Corrected fresh-set means:
baseline 57.6, brief 74.5, round 01 78.5 (round 01 4.0 ahead; 1.9 ahead with
n05 left out: 56.3, 75.6, 77.5). Pooled over 24 cases: 62.8, 77.4, 79.3.
