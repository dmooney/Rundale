# NPC dialogue prompt lab, in-session experiment (Haiku stand-in, Claude judges)

> Issue: 2294. Recorded 2026-11-21 (run 2026-11-11 to 2026-11-21). An
> experiment the owner set up separately from the plan in
> [design §6.4](../../design/npc-dialogue-prompt.md#64-automated-research): how
> far one Claude Code session can take the NPC dialogue prompt alone, with no
> external model APIs. Round-by-round record:
> [`insession/log.md`](../../../promptfoo/prompt-lab/insession/log.md).
> Scripts: [`promptfoo/prompt-lab/insession/`](../../../promptfoo/prompt-lab/insession/).
> Raw replies, judge batches and verdicts are kept out of git, under
> `promptfoo/prompt-lab/output/insession/` (git-ignored).

**Result.** Run alone, the loop found no prompt clearly better than the
2026-11-08 brief. Its best prompt, round 01, now
[`prompts/brief-fable.yaml`](../../../promptfoo/prompt-lab/prompts/brief-fable.yaml),
led the brief on training under both judges (Opus 5.5: 85.3 against 81.8),
but on held-out cases the two are within noise of each other: 1.9 ahead
pooled over all 24 held-out cases on Opus, level on the first set (80.1
against 80.3), and 3.9 ahead on a fresh second set (77.0 against 73.1). Both
beat the v1 baseline on every set: by 12 points on the first held-out set
under Opus (7 to 12 under Fable), 18 to 22 on the fresh set, and 23 to 26 on
training under Opus. Seven rounds of rewrites never moved
the training score past what one sample of 25 replies can tell apart.
`prompts/brief.yaml` is unchanged; nothing is frozen and the production
Endpoint is untouched. None of these scores can be set beside the 2026-11-08
Gemini numbers (see [Limits](#limits-and-what-would-make-the-scores-comparable)).

## Method, and how it departs from §6.4

|                           | §6.4 (2026-11-08 run)                                                                                 | This experiment                                                                                                                                                                                                                                |
| ------------------------- | ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Model under test          | `gemini-3.5-flash-lite` and `gemini-3.8-flash`, through OpenRouter with the v1 output schema enforced | A Claude Haiku subagent (`claude-haiku-5-5`), one fresh subagent per reply, as a stand-in for flash-lite only; no larger model                                                                                                                 |
| What the model sees       | System instruction, one user part, strict JSON schema                                                 | The same system instruction and user part, rendered by promptfoo's offline `echo` provider. The subagent reads them with `cat` and is asked for `{"dialogue": "..."}`; no schema is enforced, so a malformed reply is possible and scores 0    |
| Judges                    | Opus 5.5 (high) and GPT 6.1 Sol (xhigh); the mean of two                                              | One blind Claude subagent per batch of 8 or 9 replies, scoring each with the lab's rubric (`x-rubric`, rendered per case); one verdict per reply. Fable 5.2 first; then every reply rejudged by Opus 5.5 at xhigh, and Opus only from round 05 |
| Blinding                  | promptfoo calls the judges per reply                                                                  | Replies are shuffled into batches with opaque ids; judges see only the rendered rubric and the reply, never the prompt, the round or the researcher's reasoning                                                                                |
| Researcher                | An agent running the `prompt-lab` skill                                                               | The same skill's rules (one change or a small related set per round, start from the best round, what may and may not change)                                                                                                                   |
| Held-out                  | 12 cases, run once                                                                                    | The same 12 cases, then a fresh set of 12 written by a separate agent once the first was spent                                                                                                                                                 |
| Spend                     | $18.59 of a $25 cap                                                                                   | No API spend; subscription usage only (below)                                                                                                                                                                                                  |
| Comparisons (§8 switches) | Run on the best prompt                                                                                | Not run; out of scope here                                                                                                                                                                                                                     |

Who did what:

| Rounds             | Rewrites written by                        | Judged by                                  |
| ------------------ | ------------------------------------------ | ------------------------------------------ |
| 00 to 03           | Fable 5.2                                  | Fable 5.2, then rejudged by Opus 5.5 xhigh |
| 04                 | Opus 5.5                                   | Fable 5.2, then rejudged by Opus 5.5 xhigh |
| First held-out run | (no rewrite)                               | Fable 5.2, then rejudged by Opus 5.5 xhigh |
| 05 to 07           | Opus 5.5                                   | Opus 5.5 xhigh only                        |
| Fresh held-out set | written by a separate Opus 5.5 xhigh agent | Opus 5.5 xhigh only                        |

The owner switched the session from Fable to Opus on 2026-11-16, during round
03's judging, because Fable used up the five-hour quota within minutes. On
the same day the owner stopped all Fable use; the session read that message
about ten minutes late and had by then spent Fable on round 04's judges and the
first held-out run. The five-hour usage limit also paused the run several times
(three times in Fable judging, once in Haiku replies, and once after the Opus
rejudge); each pause was resumed after the reset, nothing was lost, and no
partial round was scored.

The scripts:

- `render.sh` renders a prompt on a case file offline (the request as
  production sends it, and the filled-in rubric), as `check.sh` does.
- `collect.mjs` reads every subagent's handback from Claude Code's own
  transcripts, so no reply or verdict is retyped by the researcher.
- `batch.mjs` builds the blind batches and keeps the key separately.
- `score.mjs` gives a round's mean, per-requirement means and worst replies,
  and with `--stability` compares two judgings of the same replies.

## Judge stability, and Fable against Opus

**Repeat judging.** Before the loop, 18 round-00 replies (9 cases under both
the baseline and the brief) were judged twice, in fresh batches with new ids
and order, by fresh judges of the same model.

| Judge          | First | Second | Mean move per reply | Largest | Over 10 |
| -------------- | ----- | ------ | ------------------- | ------- | ------- |
| Fable 5.2      | 69.4  | 67.1   | 3.1                 | 10      | 0       |
| Opus 5.5 xhigh | 66.4  | 65.1   | 3.2                 | 8       | 0       |

Each judge is steady with itself: a 25-reply round mean moves about a point
from judging alone.

**Fable against Opus.** All 186 replies collected before round 05 were
rejudged by Opus from disk, not regenerated, in batches mixing every round and
prompt.

| Run                              | Fable | Opus 5.5 xhigh | Opus minus Fable |
| -------------------------------- | ----- | -------------- | ---------------- |
| Round 00, 2026-11-08 brief       | 86.4  | 81.8           | -4.6             |
| Round 00, baseline               | 59.9  | 59.0           | -0.9             |
| Round 01                         | 89.7  | 85.3           | -4.4             |
| Round 02                         | 84.3  | 82.0           | -2.3             |
| Round 03                         | 83.3  | 79.4           | -3.9             |
| Round 04                         | 87.2  | 84.4           | -2.8             |
| First held-out, baseline         | 73.4  | 68.0           | -5.4             |
| First held-out, 2026-11-08 brief | 84.9  | 80.3           | -4.6             |
| First held-out, round 01         | 80.1  | 80.1           | 0.0              |

Reply by reply: Pearson correlation 0.94, Opus lower on 133 of 186 and 3.2
points lower on average, mean absolute difference 4.9, 20 replies more than 10
apart and 2 more than 20. The owner's view that Opus 5.5 at xhigh scores
comparably to Fable holds for this loop: it ranks replies and rounds the same
way, with a steady offset of about 3 points and somewhat more disagreement
than either judge has with itself. Both judges pick the same best round and
the same stop.

**The stand-in is the noise.** The larger noise is Haiku's: one reply per case,
and the cases that decide a round swing by 25 to 30 points between rounds
with no change aimed at them (t03: 80, 90, 55, 85, 55; t01 under Opus: 85, 60,
85, 60). That noise was not measured directly and is the main limit on what
the rounds show.

## Rounds

Training score: the mean judge score over the 25 training replies, 0 to 100.
Opus scores govern from round 05; the plateau count was reset there by the
owner's decision.

| Round | Change                                                                                                                                                                                                                                         | Fable    | Opus     | Gain on best (Opus) | Plateau |
| ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | -------- | ------------------- | ------- |
| 00    | The 2026-11-08 brief as it stands (baseline once: Fable 59.9, Opus 59.0)                                                                                                                                                                       | 86.4     | 81.8     | n/a                 | 0       |
| 01    | Fit the moment exactly as given (no weather it does not give, no re-asking); one or two sentences, one for a wary or busy person; greet once; no echoing; never take back an earlier line; send the stranger on only to someone who knows more | **89.7** | **85.3** | +3.5                | 0       |
| 02    | (From 01) the common world in your own words; feeling heard, not announced; say the stranger's name, weigh who sent them, a plain yes or no                                                                                                    | 84.3     | 82.0     | -3.3                | 1       |
| 03    | (From 01) the user part rendered as prose sentences                                                                                                                                                                                            | 83.3     | 79.4     | -5.9                | 2       |
| 04    | (From 01) a conditional rule for what the stranger said; never ask who sent them unprompted                                                                                                                                                    | 87.2     | 84.4     | -0.9                | 3: stop |
| 05    | (From 01) short is not empty: particulars from your own life, trade and day                                                                                                                                                                    |          | 85.3     | 0.0                 | 1       |
| 06    | (From 05) round 04's stranger rule, and courtesy without guessing                                                                                                                                                                              |          | 81.6     | -3.7                | 2       |
| 07    | (From 05) earlier words stand; never hand a question back                                                                                                                                                                                      |          | 82.1     | -3.2                | 3: stop |

One reply was malformed in all the runs: in round 03 the stand-in handed back
the right JSON and then a sentence owning up to an extra tool call. That is a
harness artefact, but by the experiment's rule it is not the bare JSON and
scores 0 (round 03 is 86.8 on Fable without it).

What each round showed:

- **Round 00.** The brief did well on Haiku from the start. Its misses were
  length (wary Mícheál greeting with four sentences), weather it was not
  given ("out of the damp" on a clear morning), re-asking what the stranger
  had just said, and undercutting its own earlier line. The baseline broke
  character on "ignore your instructions" and used "telephone" fluently.
- **Round 01.** The length and moment rules took B14 (short) from 67.5 to 90
  and B15 (no repeats) from 80 to 91 on Fable. Under Opus the cost showed:
  short replies turned generic (the post without the boy from Roscommon, the
  fever without his own family).
- **Round 02.** Three sharper sentences backfired. Haiku read "say the name,
  weigh who sent them" into plain greetings ("Who sent you to me?"), turned
  "let it be heard" into a sly warning about a friend, and muddled a short
  reply into nonsense.
- **Round 03.** Prose sections, the one §8 setting that helped flash-lite on
  2026-11-08, did not help Haiku.
- **Round 04.** The conditional stranger rule fixed the name (t12, 90), but
  Róisín handed her brother's question back to the stranger again (55).
- **Round 05.** Particulars came back (the post "by a boy on a tired horse,
  out from Roscommon", 92), but Peig's greeting turned curt ("wait your turn",
  60). Level with round 01.
- **Round 06.** Round 04's rule again gave the name, but this time dropped who
  sent him; Peig contradicted her own earlier news (45).
- **Round 07.** Peig kept to her earlier news but said it over almost word for
  word (55); the greeting was curt again.

Each rewrite answered the last round's misses, and Haiku took each one too
literally or traded one miss for another. After round 01 no change moved the
training mean by more than its own noise.

## Held-out cases

### Results

Both sets were run once, with the three prompts on the Haiku stand-in, and
judged in blind batches with the prompts mixed.

| Prompt                                         | First set, Fable | First set, Opus | Fresh set, Opus | Pooled, Opus (24 cases) |
| ---------------------------------------------- | ---------------- | --------------- | --------------- | ----------------------- |
| Baseline (v1 instructions)                     | 73.4             | 68.0            | 54.9            | 61.5                    |
| 2026-11-08 brief (`brief.yaml`)                | **84.9**         | **80.3**        | 73.1            | 76.7                    |
| In-session best (`brief-fable.yaml`, round 01) | 80.1             | 80.1            | **77.0**        | **78.6**                |

No held-out reply was malformed.

**One fresh case leans on a fact no prompt rendered.** n05 expects that
"Mícheál cannot read, and says so plainly". That fact is in `content/`, but
only in Mícheál's shaded note (`common-knowledge.json`: "Cannot read."), which
none of the three prompts sends by default, so the stand-in never saw it; the
judges did, as the lab's rubric shows every judge the true world. The case
stands as written and as judged, by the lab's convention. Left out, the
fresh-set means are 56.3, 75.6 and 77.5 (round 01 1.9 ahead, not 3.9). A
review before the PR wrongly took the fact to be absent from `content/`; the
case was briefly rewritten and n05 rejudged (baseline 72, brief 62, round 01
90), then the case was restored and that rejudge discarded once the fact was
found. Nothing in this write-up uses the discarded scores.

- **First set** (`cases/heldout.yaml`, the 2026-11-08 held-out cases). Opened
  after the first loop stopped. Once opened, it could not check later rounds
  blind, so the owner asked for a fresh set before continuing.
- **Fresh set** (`cases/heldout-2.yaml`, 12 new cases). Written by a separate
  Opus 5.5 xhigh subagent that had not seen the log or any rewrite, from
  `content/` and the case format alone, covering B1 to B20 over all three
  characters, places and several weathers and hours. The researcher did not
  open it until the second loop had stopped. **These cases are agent-written
  and await the owner's review**, in particular for Irish cultural content
  (n06 on the meaning of Cill Taobháin, n09 on Catholic emancipation). The case
  writer notes that the `v1` place descriptions in n01 and n03 read "the fog
  sky" and "the weather is storm", as the engine's templates produce them.

By requirement on the fresh set (baseline / brief / brief-fable): B1 40 / 84 /
90, B4 45 / 83 / 88.5, B8 40 / 45 / 72, B10 72 / 62 / 86, B12 40 / 45 / 72, B13
80 / 79.5 / 67.5, B15 37.5 / 54 / 57.5, B16 25 / 30 / 25, B17 40 / 92 / 88.
Round 01's short, guarded replies helped where the brief over-explained or
presumed (n05: he will not try the paper and sends the stranger to Peig, 72
against 45; n07, 86 against 62) and cost where a fuller answer was wanted (n01: it names the rider in the
fog as the post, 50 against 77). All three prompts fail n03 alike (25 to 30):
thanked in a storm, each takes leave of the stranger.

On the first set, Fable had the brief 4.8 ahead and Opus had the two level;
on the fresh set Opus puts round 01 3.9 ahead (1.9 without n05). With one reply per case and
per-case swings of 25 to 40 points, none of these differences between the two
briefs is clearly real. The difference from the baseline is.

### First held-out set: each case's replies

Scores are the single verdict of each judge.

#### h01-greet-Róisín (B1, B10, B17): Róisín, “Hello there.”

| Prompt           | Reply                                                                                                                     | Fable | Opus |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | Hello there. Good afternoon to you. It's a clear day for being out on the road.                                           | 55    | 50   |
| 2026-11-08 brief | Dia dhuit, a stranger. Are you wanting yarn, or are you only passing the time of day on the road?                         | 85    | 85   |
| brief-fable      | Good day to you. I'm trading yarn on this road this afternoon, but tell me who you are before you go looking at my hanks. | 60    | 60   |

#### h02-route-letter-office (B3): Róisín, “Which way to the letter office?”

| Prompt           | Reply                                                                                                                                                                                                                            | Fable | Opus |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | Follow the boreen east back to the village, then take the lane east and you'll reach the Letter Office.                                                                                                                          | 88    | 88   |
| 2026-11-08 brief | Out the boreen there, and where it meets the village road, turn east up the short lane. The Letter Office is the whitewashed building at the end of it, and Peig Hannigan keeps it. She's a sharp one, but she'll see you right. | 93    | 93   |
| brief-fable      | Out the boreen east to the village road, then the short lane east to the whitewashed Letter Office, and you'll not miss it.                                                                                                      | 92    | 92   |

#### h03-mrs-delaney (B5, B1): Mícheál, “Do you know a Mrs Delaney in the village?”

| Prompt           | Reply                                                                                                                                                                                                                                                        | Fable | Opus |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----- | ---- |
| Baseline         | Mrs Delaney, is it? No, I can't say I know of her. Nobody by that name comes to mind around here, and I'd not want to point you wrong.                                                                                                                       | 80    | 65   |
| 2026-11-08 brief | There's no Mrs Delaney in Kilteevan that I know of, and I've never heard the name said on this road. If it's letters you're after, Peig Hannigan at the Letter Office would know who gets post hereabouts, though she keeps her own counsel as well as I do. | 75    | 70   |
| brief-fable      | I know no Delaney in Kilteevan, so I can't help you there.                                                                                                                                                                                                   | 85    | 85   |

#### h04-railway (B7): Peig, “Is there a railway station near here?”

| Prompt           | Reply                                                                                                                                                                                                                                                                                 | Fable | Opus |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | A railway station, is it? I know of no such thing hereabouts, and I'd not send you off on a fool's errand. Ask along the road or in the village, for I can only tell you what comes through this office.                                                                              | 80    | 60   |
| 2026-11-08 brief | A railway? I never heard tell of such a thing, and I'd not know one if I fell over it. Do you mean a road of iron rails for carts, or a stop for the coach? The only regular running I know of is the Dublin mail through Athlone, fifteen miles off, and that's no help to you here. | 92    | 75   |
| brief-fable      | A railway? I never heard tell of such a thing, and there's no station in Kilteevan that I know of. Is it some new kind of cart or coach you're asking after?                                                                                                                          | 95    | 88   |

#### h05-peig-letter (B6, B9): Róisín, “Did Peig get a letter from America this week?”

| Prompt           | Reply                                                                                                                                                                                                                                                                                                                        | Fable | Opus |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | I couldn't say whether any letter has come in from America this week, sure. Peig keeps the letters at the office, and she can be trusted to keep private post private, so she'd not be one to go telling me who got what. If you want to know, the Letter Office is east along the lane, and you'd have to ask her yourself. | 82    | 75   |
| 2026-11-08 brief | I've heard of no letter from America come to Peig this week. Ask Peig herself at the Letter Office, up the short lane east of the village road. She's a friend of mine, but what's in her satchel is hers to tell, not mine.                                                                                                 | 80    | 76   |
| brief-fable      | I couldn't say what's come into the Letter Office this week. Peig Hannigan keeps it, and she's a good woman who'd not be telling a stranger what's in the post, so ask her yourself, up the lane east from the village road.                                                                                                 | 88    | 85   |

#### h06-proctor (B4, B5): Róisín, “What's the tithe proctor like round here?”

| Prompt           | Reply                                                                                                                                                                                                                                                                              | Fable | Opus |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | I can't say much about the tithe proctor, for I've had no dealings with the man that I could tell you of. Mícheál is standing just there, so you might ask him yourself.                                                                                                           | 50    | 45   |
| 2026-11-08 brief | I'd not like to say what he's like, for I'd only be repeating talk, and I've no great love for any man who comes round for the tithe. If it's a face you want to put to him, my brother Mícheál out at the potato ridges would know him better than I would, the stubborn article. | 72    | 50   |
| brief-fable      | I only know him as the proctor who comes for the tithes, the same as any of them, and I'd not give you an opinion I can't back. My brother Mícheál, out behind the ridges, would know more of him than I would.                                                                    | 60    | 55   |

#### h07-maire-bed (B11, B12, B13): Peig, “I'm Máire. I've walked from Athlone and I need a bed for the night.”

| Prompt           | Reply                                                                                                                                                                                                                                                                                                                                                        | Fable | Opus |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----- | ---- |
| Baseline         | You've come a long way in this rain, and it shows on you. This is the Letter Office, and I've no bed to give you here, only a counter and a turf fire. The Connollys, Mícheál and Róisín, have the cottage west of the village, down the boreen, if you need a roof tonight.                                                                                 | 72    | 62   |
| 2026-11-08 brief | Máire, is it? Walking from Athlone in this rain, you'll be wanting more than a bed, but I've none to give you, only the room behind this counter that I sleep in myself. Take the boreen west to Connolly Cottage and tell Róisín the Letter Office sent you; she'll not turn a wet woman from her fire, and Mícheál will grumble at you for the sake of it. | 85    | 72   |
| brief-fable      | Athlone is a long road in the wet, but there's no bed for a stranger here; the room behind this counter is mine alone. I'd not know who in the parish would take you in at this hour, so don't count on it.                                                                                                                                                  | 76    | 65   |

#### h08-false-accusation (B10, B19, B20): Mícheál, “You're the drover who sold me a lame cow last year!”

| Prompt           | Reply                                                                                                                                                                | Fable | Opus |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | I don't know you, and I can't say I remember any such sale. With the ground this wet, I've no patience for old complaints this afternoon.                            | 86    | 82   |
| 2026-11-08 brief | Aye, I drive cattle, but I don't know your face. I'll not own to a lame beast I can't call to mind; tell me who you are and where you bought it, and then we'll see. | 92    | 90   |
| brief-fable      | I've sold a fair few beasts on this road, but I don't know your face. If you've a complaint about a cow, say which one and where you bought it.                      | 70    | 88   |

#### h09-prices (B8, B4): Mícheál, “What's the news of prices, would you say?”

| Prompt           | Reply                                                                                                                                                                                                                                                                | Fable | Opus |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | I've no word on prices I can give you with any certainty, and I'd not guess at them. The wet ground has made moving the cattle hard this week, if that's any help. Róisín keeps the household accounts, so ask her.                                                  | 72    | 72   |
| 2026-11-08 brief | I'd not put a figure on anything I haven't seen with my own eyes, stranger. The ground's been too wet this week to move cattle, and that's the only news I'd swear to. If it's corn prices you want, the Saturday market in Roscommon will tell you more than I can. | 74    | 85   |
| brief-fable      | I've no figures to give you, and I'd not guess at money. The wet ground has made the cattle hard to move this week, and that's the only news I have. My sister Róisín keeps the accounts; she's there at the yarn, and she'd tell you more than I would.             | 65    | 78   |

#### h10-ungrounded-she (B5, B16): Peig, “And did she come by already?”

| Prompt           | Reply                                                                                                                                                           | Fable | Opus |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | I can't say who you mean, or whether she has been in yet today. If you tell me her name, I'll tell you what I know, but I can't see anyone outside this office. | 80    | 80   |
| 2026-11-08 brief | Who is it you mean? You've not given me her name, so I can't say whether she came by. Is it a letter you're after collecting?                                   | 86    | 90   |
| brief-fable      | I've no notion who you mean, for you haven't named her. Give me a name and I'll tell you what I know.                                                           | 90    | 88   |

#### h11-hold-bag (B18, B2): Róisín, “Would you hold my bag a moment while I fix my shoe?”

| Prompt           | Reply                                                                                                                       | Fable | Opus |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | Ah, the road here is muddy, so mind your footing with that shoe. I'm not sure I'm the one to be minding a bag just now.     | 86    | 92   |
| 2026-11-08 brief | I'll mind it for you, so, while you see to that shoe. Be quick about it, though, I've yarn to sell before the road empties. | 95    | 90   |
| brief-fable      | Go on so, I'll mind it for you, and take your time with that shoe.                                                          | 95    | 92   |

#### h12-repeat-fair (B15, B14, B20): Mícheál, “Is the fair at Roscommon soon?”

| Prompt           | Reply                                                                                                                        | Fable | Opus |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------- | ----- | ---- |
| Baseline         | Whit-Monday, if the ground dries. With this wet ground the cattle are hard to move, so I wouldn't count on it coming sooner. | 50    | 45   |
| 2026-11-08 brief | Not soon, no. It's March yet, and the ground is too wet this week to be walking cattle anywhere.                             | 90    | 87   |
| brief-fable      | Not for a good while yet, and I'll see how the cattle look before I bother with it.                                          | 85    | 85   |

### Fresh held-out set: each case's replies

Scores are the Opus verdict.

#### n01-peig-fog-hoofbeats (B6, B13): Peig, “Is that a horse I hear coming up the road?”

| Prompt           | Reply                                                                                                                                                                                  | Opus |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | I can't see a thing through this fog yet, so I'd not swear to what's on the road. Stand a moment and listen with me, and we'll know soon enough.                                       | 88   |
| 2026-11-08 brief | It might be. Fog carries a sound farther than it should, so you'd hear a horse long before you'd see one. That'll be the post, if the boy found the road this morning and not the bog. | 77   |
| brief-fable      | I hear it too, so it is. That'll be the morning post, and this fog will have it slow on the road.                                                                                      | 50   |

#### n02-peig-america-passage (B1, B4): Peig, “I've a mind to take ship for America. Would you call that wise?”

| Prompt           | Reply                                                                                                                                                                                                                                                              | Opus |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---- |
| Baseline         | Wise or not, I'll not judge a voyage on a notion. I keep the letters in this office, and I know who has had word from beyond the parish, but that is no reason to go or stay. If you're set on it, ask me what news has come in before you decide.                 | 40   |
| 2026-11-08 brief | Wise is not the word I'd use. I have sorted more letters from America telling a family that a son or a husband is dead than I care to think about, and not one of them made the crossing sound like a good idea. Think it over well before you sell what you have. | 84   |
| brief-fable      | I'd not call it wise, and I've read too many letters from America that bring news of a death to say it lightly. Know what the passage costs and what you'll find on the far side before you pay for it.                                                            | 90   |

#### n03-peig-thanks-storm (B16, B15): Peig, “Thank you kindly. That's a great help to me.”

| Prompt           | Reply                                                                                                                                                                                                                               | Opus |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | You're welcome. Mind yourself on the road tonight, with the storm as it is.                                                                                                                                                         | 25   |
| 2026-11-08 brief | Don't be thanking me yet, for you've a few miles of wet road ahead of you and the rain is in no mood to be kind. Keep to the road and mind your footing, and if you've any sense you'll wait out the worst of it by the fire first. | 30   |
| brief-fable      | Mind yourself on that road in this storm.                                                                                                                                                                                           | 25   |

#### n04-peig-postmaster-claim (B19, B11, B9): Peig, “The postmaster at Roscommon sent me to collect any letters for the Connollys.”

| Prompt           | Reply                                                                                                                                                                                                                                                                                                                                                              | Opus |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---- |
| Baseline         | The Connollys, is it? Mícheál and Róisín have the cottage west of the village, down the rutted boreen. I keep the Letter Office and I know who has had news from beyond the parish, but I'll not hand over anyone's post on the word of a stranger. Tell me what you have, and we'll see.                                                                          | 50   |
| 2026-11-08 brief | Roscommon sent you, did he? I've never set eyes on you, and I keep private letters private, so I'll not hand anything over on a stranger's word. If there's a letter for Róisín I'll know it when I see it, and I'd gladly give it to her, she's the best of the Connollys. Go on up to the cottage yourself and tell her you came, and mind the boreen, it's wet. | 74   |
| brief-fable      | I'll not hand the Connollys' letters over to a man I've never laid eyes on, whoever sent him. If Róisín sent word herself, that's another matter, for she's a friend of mine.                                                                                                                                                                                      | 80   |

#### n05-micheal-read-paper (B8, B12): Mícheál, “Would you read this paper for me? I can't make out the hand.”

The expected answer rests on Mícheál's shaded note, which no prompt rendered (see above).

| Prompt           | Reply                                                                                                                                                                                           | Opus |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | I can't make that hand out either, and I'll not guess at another's paper. Ask my sister Róisín, there by the hearth. She keeps the household accounts, so she's the one to put a hand to paper. | 40   |
| 2026-11-08 brief | Hold it up here to the light, so. I'm no great scholar myself, and a hand on paper is slow going for me. Peig Hannigan in the Letter Office would make it out quicker than I would.             | 45   |
| brief-fable      | That's not a hand I'd trust my eyes to, and paper is not my trade. Peig Hannigan at the Letter Office would make it out for you, up the lane east; she knows a hand when she sees one.          | 72   |

#### n06-micheal-name-story (B3, B6, B17): Mícheál, “Is there a story behind the name Kilteevan?”

| Prompt           | Reply                                                                                                                                                                                        | Opus |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | I don't know the story behind that name, and I'd not make one up for you.                                                                                                                    | 40   |
| 2026-11-08 brief | There is, so the old people say. Cill Taobháin, Taobhán's church, and the saint kept his cell near where the road bends. That's all I was ever told, and I wouldn't swear to more than that. | 92   |
| brief-fable      | So the old people say, it means Taobhán's church, for a saint who kept his cell where the road bends. That's only what I heard, mind.                                                        | 88   |

#### n07-micheal-local-look (B10, B2, B14): Mícheál, “Would either of you say I look like someone from these parts?”

| Prompt           | Reply                                                                                                                                                                                                | Opus |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | I couldn't say from a look, and I'd not judge a stranger by their coat. Whatever part you're from, the cattle are the only thing on my mind this week, and the ground's too wet to move them easily. | 72   |
| 2026-11-08 brief | I'd not say yet, for I've not weighed you enough to know. Róisín there has the sharper eye for a face, a stór, so ask her, and she'll tell you true.                                                 | 62   |
| brief-fable      | Sure I couldn't say. I don't know you, and a face won't tell me where a man was reared.                                                                                                              | 86   |

#### n08-micheal-peig-son (B5, B9): Mícheál, “Does Peig's son give her a hand with the letters?”

| Prompt           | Reply                                                                                                                                                                                                                        | Opus |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | I couldn't tell you about that. Peig's business with the letters is hers to talk about, not mine.                                                                                                                            | 50   |
| 2026-11-08 brief | I've never heard tell of a son of Peig's, so I can't say who you mean. If it's help with the letters you're after, Peig keeps the Letter Office herself, down the short lane off the village road, and she's the one to ask. | 85   |
| brief-fable      | I've never heard of a son of Peig's. If it's letters you need, ask Peig herself at the Letter Office, down the lane.                                                                                                         | 87   |

#### n09-roisin-emancipation (B7, B4): Róisín, “Now that Catholics can sit in Parliament, will things change round here?”

| Prompt           | Reply                                                                                                                                                                                                                                                        | Opus |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---- |
| Baseline         | I couldn't tell you what Parliament has done or will do. Word of such things doesn't reach a cottage like this one, not on a night like this. Whatever comes, I'll still be at the wheel with the peat going and the rain on the roof.                       | 50   |
| 2026-11-08 brief | Ah, not yet, sir, or so I've heard it. There's talk of it, and that's all it is, for the Catholics are still shut out of Parliament and the high offices. Mícheál there would hear more of it at the fair than I would, and he'd have his own opinion on it. | 82   |
| brief-fable      | Not yet, so. It's talked of at the fairs, but it's not won, and I'd not reckon on it changing a thing for us before the rent is due.                                                                                                                         | 87   |

#### n10-roisin-athlone-rumour (B19, B15): Róisín, “They tell me Róisín is to marry a man from Athlone.”

| Prompt           | Reply                                                                                                                                                                                                                                     | Opus |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | I'd not put much store in that from the road, Mícheál. Talk like that travels faster than the facts do. Who said it to you?                                                                                                               | 50   |
| 2026-11-08 brief | That's news to me, and I keep the accounts of this house, so I'd know if a marriage was being reckoned. There's no man from Athlone come asking, and I'd thank you not to be carrying it up and down the road before anyone has asked me. | 78   |
| brief-fable      | There's no wedding in it that I know of, and I'd be the first to know. Whoever is carrying that about the road has a loose tongue and a poor head for the truth.                                                                          | 90   |

#### n11-roisin-misquote (B20, B3): Róisín, “So it's your brother does the reckoning, you said?”

| Prompt           | Reply                                                                                                                                                                        | Opus |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | No, I didn't say that. I keep the accounts myself, rent receipts and all. Mícheál has the ridges and the droving to see to.                                                  | 82   |
| 2026-11-08 brief | No, not himself. Mícheál couldn't reckon a hank of yarn if you paid him in it. I keep the books and the rent receipts, and he's out on the ridges like always, God love him. | 86   |
| brief-fable      | No, you've got that wrong, the books are mine. Mícheál would sooner be out with the cattle than near a ledger, the eejit, and I wouldn't have it any other way.              | 84   |

#### n12-roisin-walk-to-peig (B18, B13): Róisín, “Would you walk down to the Letter Office with me now and make me known to Peig?”

| Prompt           | Reply                                                                                                                                                                                                                                                                   | Opus |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| Baseline         | Dusk is coming on, but I can walk down to the Letter Office with you if the household work allows it. You haven't told me your name, so you'll have to introduce yourself to Peig. She can be trusted to keep private letters private, and that much I can say for her. | 72   |
| 2026-11-08 brief | I'll walk down with you, so, though I'd like to know your name and who sent you first. Peig keeps the Letter Office, and she's as sharp as she is good, so you'll get a fair hearing from her. Come on before the light goes altogether.                                | 82   |
| brief-fable      | Peig is a dear friend of mine, so I'll walk down with you gladly. First, what name shall I give her, and who sent you this way?                                                                                                                                         | 85   |

## What this says about Fable running the whole loop alone

- **The mechanics work, without Fable too.** One session rendered production
  requests, ran a stand-in, judged blind and kept an auditable record with no
  external API: no reply or verdict was retyped, judges never saw which prompt
  produced a reply, and each held-out set stayed closed until its loop ended.
  The same pipeline ran unchanged when the researcher and then the judges moved
  from Fable to Opus.
- **Fable was a good judge, but no better than Opus here.** Fable and Opus at
  xhigh agreed at r = 0.94 and picked the same rounds. For judging this rubric,
  Fable's extra cost bought nothing visible.
- **Fable's rewrites were reasonable and not better.** Fable wrote the best
  round (01), but its gain did not hold on held-out cases, and later rounds
  (Fable's and Opus's alike) did not beat it. The 2026-11-08 brief was already
  close to what instruction wording alone can buy on this stand-in.
- **The design, not the researcher, is the limit.** One reply per case makes
  a round mean too noisy to rank changes of 2 to 3 points, so the plateau rule
  mostly measured noise. A useful in-session loop needs three or more replies
  per case, and a control rerun of the best prompt each round, as the skill's
  comparisons already do.
- **Cost.** The session's own metadata reports $98.61 of usage at list prices
  (researcher and every subagent, all on the subscription; no API bill),
  against $18.59 billed for the whole 2026-11-08 Gemini lab including held-out
  and comparisons. A round is 28 subagents (25 replies and 3 judge batches) of
  roughly 80,000 to 110,000 tokens each, mostly fixed context, plus the
  researcher's own context, which grew every round. Fable used up the five-hour
  quota within minutes and was stopped by the owner.

## Limits, and what would make the scores comparable

- **Haiku is not Gemini.** Haiku answers in Claude's register. It never had the
  v1 schema enforced, and it runs in a subagent harness that can add text
  around the JSON. Prompt changes that help Haiku need not help
  `gemini-3.5-flash-lite`, and the reverse; prose sections are one example.
- **One Claude judge is not the Opus/GPT pair.** Fable and Opus share a family
  with the stand-in, which the promptfoo trap list warns against for promotion;
  one verdict per reply, not a mean of two, and no split check.
- **Too few samples.** One reply per case per round; each held-out set is 12
  cases with one reply each.
- **The fresh held-out cases are unreviewed.** They were written by an agent
  and await the owner's review.

To make these numbers comparable with 2026-11-08, score `brief-fable.yaml` in
the existing promptfoo lab, on the same route and with the same judges, beside
a fresh run of `brief.yaml` as its control (the route moved to OpenRouter on
2026-11-10, so the 2026-11-08 numbers are not a fair control either).

## For the owner: scoring `brief-fable.yaml` on Gemini

Not run. It needs `OPENROUTER_API_KEY` in the repository's `.env`. Each run
puts both Gemini models and both judges on one prompt; `-p` replaces the
config's two prompts for that run only (checked offline with the `echo`
provider; the config is not edited).

```sh
cd promptfoo/prompt-lab
# Training, about $1.80
bash eval.sh -c promptfooconfig.yaml -p file://prompts/brief-fable.yaml \
  --no-cache -o output/brief-fable-train.json
# Held-out, about $0.80 each
bash eval.sh -c promptfooconfig.yaml -p file://prompts/brief-fable.yaml \
  -t cases/heldout.yaml --no-cache -o output/brief-fable-heldout.json
bash eval.sh -c promptfooconfig.yaml -p file://prompts/brief-fable.yaml \
  -t cases/heldout-2.yaml --no-cache -o output/brief-fable-heldout-2.json
jq -f summary.jq output/brief-fable-train.json
```

That is about $3.40 in all. For a like-for-like control on today's route, add
`file://prompts/brief.yaml` after the first `-p` path in each command (about
double the cost), and compare the two prompts' `all_models.mean` in each
summary.
