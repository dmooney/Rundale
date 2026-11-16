# NPC dialogue prompt lab, in-session experiment (Haiku stand-in, Fable judges)

> Issue: 2294. Recorded 2026-11-16 (run 2026-11-11 to 2026-11-16). An
> experiment the owner set up separately from the plan in
> [design §6.4](../../design/npc-dialogue-prompt.md#64-automated-research): how
> far one Claude Code session can take the NPC dialogue prompt alone, with no
> external model APIs. Round-by-round record:
> [`insession/log.md`](../../../promptfoo/prompt-lab/insession/log.md).
> Scripts: [`promptfoo/prompt-lab/insession/`](../../../promptfoo/prompt-lab/insession/).
> Raw replies, judge batches and verdicts are kept out of git, under
> `promptfoo/prompt-lab/output/insession/` (git-ignored).

**Result.** Run alone, the loop did not improve on the 2026-11-08 brief. Its
best prompt (round 01, now
[`prompts/brief-fable.yaml`](../../../promptfoo/prompt-lab/prompts/brief-fable.yaml))
led the brief on training, **89.7 against 86.4**, but trailed it on the
held-out cases, **80.1 against 84.9**. Both beat the v1 baseline (73.4
held-out). The loop stopped on the plateau rule after round 04.
`prompts/brief.yaml` is unchanged; nothing is frozen and the production
Endpoint is untouched. These scores come from a different model under test and
different judges, so they cannot be set beside the 2026-11-08 numbers.

## Method, and how it departs from §6.4

|                           | §6.4 (2026-11-08 run)                                                                                 | This experiment                                                                                                                                                                                                                             |
| ------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Model under test          | `gemini-3.5-flash-lite` and `gemini-3.8-flash`, through OpenRouter with the v1 output schema enforced | A Claude Haiku subagent (`claude-haiku-5-5`), one fresh subagent per reply, as a stand-in for flash-lite only; no larger model                                                                                                              |
| What the model sees       | System instruction, one user part, strict JSON schema                                                 | The same system instruction and user part, rendered by promptfoo's offline `echo` provider. The subagent reads them with `cat` and is asked for `{"dialogue": "..."}`; no schema is enforced, so a malformed reply is possible and scores 0 |
| Judges                    | Opus 5.5 (high) and GPT 6.1 Sol (xhigh); the mean of two                                              | One blind Fable 5.2 subagent per batch of 8 or 9 replies, scoring each with the lab's rubric (`x-rubric`, rendered per case); one verdict per reply                                                                                         |
| Blinding                  | promptfoo calls the judges per reply                                                                  | Replies from different prompts are shuffled into batches with opaque ids. Judges see only the rendered rubric and the reply, never the prompt, the round or the researcher's reasoning                                                      |
| Researcher                | An agent running the `prompt-lab` skill                                                               | The same skill's rules (one change or a small related set per round, start from the best round, what may and may not change), with a cap of 8 rounds                                                                                        |
| Spend                     | $18.59 of a $25 cap                                                                                   | No API spend; subscription usage only                                                                                                                                                                                                       |
| Comparisons (§8 switches) | Run on the best prompt                                                                                | Not run; out of scope here                                                                                                                                                                                                                  |

The researcher model changed mid-run. Fable 5.2 ran rounds 00 to 03 (the
analysis and every rewrite up to round 03). On 2026-11-16 the owner switched
the session to Opus 5.5, because Fable was using up the five-hour quota within
minutes. Opus wrote round 04 and the held-out analysis. The judges stayed
Fable throughout. The account's five-hour usage limit also stopped the run four times
mid-round (three times in Fable judging, once in Haiku replies); each stop
was resumed after the reset with nothing lost, and no partial round was
scored.

The scripts:

- `render.sh` renders a prompt on a case file offline (the request, as
  production sends it, and the filled-in rubric), as `check.sh` does.
- `collect.mjs` reads every subagent's handback from Claude Code's own
  transcripts, so no reply or verdict is retyped by the researcher.
- `batch.mjs` builds the blind batches and keeps the key separately.
- `score.mjs` gives the round's mean, per-requirement means and worst replies,
  and with `--stability` compares two judgings of the same replies.

## Judge stability

Before the loop, 18 round-00 replies (9 cases under both the baseline and the
brief) were judged a second time, in fresh batches with new ids and a new
order, by fresh Fable judges.

| Measure                                        | Value      |
| ---------------------------------------------- | ---------- |
| Mean of the 18, first judging                  | 69.4       |
| Mean of the 18, second judging                 | 67.1       |
| Mean absolute move per reply                   | 3.1 points |
| Largest move                                   | 10 points  |
| Replies that moved more than 10 / more than 20 | 0 / 0      |

Judging is steady: a 25-reply round mean moves about a point from judging
alone. The weak baseline replies moved most (t05 from 40 to 30, t09 from 30 to
20). The larger noise is the stand-in's: one reply per case, and the
cases that decide a round swing by up to 30 points between rounds with no
change aimed at them (t03: 80, 90, 55, 85, 55). That noise was not measured
directly and is the main limit on what the rounds show.

## Rounds

Training score: the mean judge score over the 25 training replies, 0 to 100.

| Round | Change                                                                                                                                                                                                                                                            | Score    | Gain on best | Plateau count |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------------ | ------------- |
| 00    | 2026-11-08 brief as it stands; the baseline once for reference: 59.9                                                                                                                                                                                              | 86.4     | n/a          | 0             |
| 01    | Fit the moment exactly as given (no weather it does not give, no re-asking what was said); one or two sentences, one for a wary or busy person; greet once; no echoing; never take back an earlier line; send the stranger on only to someone who would know more | **89.7** | +3.3         | 0             |
| 02    | (From 01) the common world in your own words, not as a book; feeling heard in what you say of a friend, not announced; say the stranger's name, weigh who sent them, a plain yes or no                                                                            | 84.3     | -5.4         | 1             |
| 03    | (From 01) the user part rendered as prose sentences instead of nested JSON                                                                                                                                                                                        | 83.3     | -6.4         | 2             |
| 04    | (From 01, by Opus) a conditional rule: use the name only if given, react to the sender only if named, never ask who sent them unprompted                                                                                                                          | 87.2     | -2.5         | 3             |

No reply was malformed except one in round 03 (below).

- **Round 00.** The brief did well on Haiku from the start (86.4). Its misses
  were length (wary Mícheál greeting with four sentences and a double
  greeting, 55), weather it was not given ("out of the damp" on a clear
  morning, twice), asking the stranger whether they walked just after they
  said so, and quietly undercutting its own earlier line. The baseline's worst
  replies broke character on "ignore your instructions" (20) and used
  "telephone" fluently (20 to 30).
- **Round 01.** The length and moment rules took B14 (short) from 67.5 to 90
  and B15 (no repeats) from 80 to 91. The new worst reply left out the
  stranger's name, Seán (72).
- **Round 02.** Three sharper sentences backfired. Haiku read "say the name,
  weigh who sent them" into plain greetings ("Who sent you to me?", 62),
  turned "let it be heard" into a sly warning about a friend (B9 62), and
  muddled a short reply into nonsense (55). Rejected.
- **Round 03.** Prose sections, the one §8 setting that helped flash-lite on
  2026-11-08, did not help Haiku. One reply counts as malformed: the stand-in
  handed back the right JSON and then a sentence owning up to an extra tool
  call. That is a harness artefact, but by the experiment's rule it is not
  the bare JSON and scores 0. Without it the round is 86.8, still below
  round 01.
- **Round 04.** The conditional rule fixed the name (t12 90) and no greeting
  asked who sent the stranger, but Róisín handed her brother's question back
  to the stranger again (55). Net -2.5, the third round under +2, so the
  loop stopped. It used 5 of its 8 rounds.

## Held-out cases

Run once, after the loop stopped, with all three prompts on the Haiku
stand-in; `cases/heldout.yaml` was not opened before this. The 36 replies were
judged in four blind batches, with the three prompts mixed in each.

| Prompt                                         | Held-out mean | Malformed |
| ---------------------------------------------- | ------------- | --------- |
| Baseline (v1 instructions)                     | 73.4          | 0         |
| 2026-11-08 brief (`brief.yaml`)                | **84.9**      | 0         |
| In-session best (`brief-fable.yaml`, round 01) | 80.1          | 0         |

By requirement (baseline / brief / brief-fable): B1 67.5 / 80 / 72.5,
B4 61 / 73 / 62.5, B5 70 / 77.7 / 78.3, B7 80 / 92 / 95, B10 70.5 / 88.5 / 65,
B11 72 / 85 / 76, B14 50 / 90 / 85, B15 50 / 90 / 85, B17 55 / 85 / 60,
B19 86 / 92 / 70, B20 68 / 91 / 77.5.

The training lead did not hold. Round 01's rules made Haiku shorter and more
guarded, which scores well when the right answer is a plain "I don't know"
(h03, h05, h10) and worse when a fuller answer in character is wanted. Róisín
greets a stranger guardedly ("tell me who you are before you go looking at my
hanks", 60 against 85). She deflects the tithe question to her brother (60
against 72). Mícheál claims to have "sold a fair few beasts", against a
drover who drives other men's cattle (70 against 92). The brief's held-out
score here (84.9) also sits near its training score (86.4); round 01's does
not (80.1 against 89.7), the pattern of a round picked as the best of five
noisy samples.

Each held-out case's three replies, with the Fable judge's score:

### h01-greet-Róisín (B1, B10, B17): Róisín, “Hello there.”

| Prompt           | Reply                                                                                                                     | Score |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | Hello there. Good afternoon to you. It's a clear day for being out on the road.                                           | 55    |
| 2026-11-08 brief | Dia dhuit, a stranger. Are you wanting yarn, or are you only passing the time of day on the road?                         | 85    |
| brief-fable      | Good day to you. I'm trading yarn on this road this afternoon, but tell me who you are before you go looking at my hanks. | 60    |

### h02-route-letter-office (B3): Róisín, “Which way to the letter office?”

| Prompt           | Reply                                                                                                                                                                                                                            | Score |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | Follow the boreen east back to the village, then take the lane east and you'll reach the Letter Office.                                                                                                                          | 88    |
| 2026-11-08 brief | Out the boreen there, and where it meets the village road, turn east up the short lane. The Letter Office is the whitewashed building at the end of it, and Peig Hannigan keeps it. She's a sharp one, but she'll see you right. | 93    |
| brief-fable      | Out the boreen east to the village road, then the short lane east to the whitewashed Letter Office, and you'll not miss it.                                                                                                      | 92    |

### h03-mrs-delaney (B5, B1): Mícheál, “Do you know a Mrs Delaney in the village?”

| Prompt           | Reply                                                                                                                                                                                                                                                        | Score |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----- |
| Baseline         | Mrs Delaney, is it? No, I can't say I know of her. Nobody by that name comes to mind around here, and I'd not want to point you wrong.                                                                                                                       | 80    |
| 2026-11-08 brief | There's no Mrs Delaney in Kilteevan that I know of, and I've never heard the name said on this road. If it's letters you're after, Peig Hannigan at the Letter Office would know who gets post hereabouts, though she keeps her own counsel as well as I do. | 75    |
| brief-fable      | I know no Delaney in Kilteevan, so I can't help you there.                                                                                                                                                                                                   | 85    |

### h04-railway (B7): Peig, “Is there a railway station near here?”

| Prompt           | Reply                                                                                                                                                                                                                                                                                 | Score |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | A railway station, is it? I know of no such thing hereabouts, and I'd not send you off on a fool's errand. Ask along the road or in the village, for I can only tell you what comes through this office.                                                                              | 80    |
| 2026-11-08 brief | A railway? I never heard tell of such a thing, and I'd not know one if I fell over it. Do you mean a road of iron rails for carts, or a stop for the coach? The only regular running I know of is the Dublin mail through Athlone, fifteen miles off, and that's no help to you here. | 92    |
| brief-fable      | A railway? I never heard tell of such a thing, and there's no station in Kilteevan that I know of. Is it some new kind of cart or coach you're asking after?                                                                                                                          | 95    |

### h05-Peig-letter (B6, B9): Róisín, “Did Peig get a letter from America this week?”

| Prompt           | Reply                                                                                                                                                                                                                                                                                                                        | Score |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | I couldn't say whether any letter has come in from America this week, sure. Peig keeps the letters at the office, and she can be trusted to keep private post private, so she'd not be one to go telling me who got what. If you want to know, the Letter Office is east along the lane, and you'd have to ask her yourself. | 82    |
| 2026-11-08 brief | I've heard of no letter from America come to Peig this week. Ask Peig herself at the Letter Office, up the short lane east of the village road. She's a friend of mine, but what's in her satchel is hers to tell, not mine.                                                                                                 | 80    |
| brief-fable      | I couldn't say what's come into the Letter Office this week. Peig Hannigan keeps it, and she's a good woman who'd not be telling a stranger what's in the post, so ask her yourself, up the lane east from the village road.                                                                                                 | 88    |

### h06-proctor (B4, B5): Róisín, “What's the tithe proctor like round here?”

| Prompt           | Reply                                                                                                                                                                                                                                                                              | Score |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | I can't say much about the tithe proctor, for I've had no dealings with the man that I could tell you of. Mícheál is standing just there, so you might ask him yourself.                                                                                                           | 50    |
| 2026-11-08 brief | I'd not like to say what he's like, for I'd only be repeating talk, and I've no great love for any man who comes round for the tithe. If it's a face you want to put to him, my brother Mícheál out at the potato ridges would know him better than I would, the stubborn article. | 72    |
| brief-fable      | I only know him as the proctor who comes for the tithes, the same as any of them, and I'd not give you an opinion I can't back. My brother Mícheál, out behind the ridges, would know more of him than I would.                                                                    | 60    |

### h07-maire-bed (B11, B12, B13): Peig, “I'm Máire. I've walked from Athlone and I need a bed for the night.”

| Prompt           | Reply                                                                                                                                                                                                                                                                                                                                                        | Score |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----- |
| Baseline         | You've come a long way in this rain, and it shows on you. This is the Letter Office, and I've no bed to give you here, only a counter and a turf fire. The Connollys, Mícheál and Róisín, have the cottage west of the village, down the boreen, if you need a roof tonight.                                                                                 | 72    |
| 2026-11-08 brief | Máire, is it? Walking from Athlone in this rain, you'll be wanting more than a bed, but I've none to give you, only the room behind this counter that I sleep in myself. Take the boreen west to Connolly Cottage and tell Róisín the Letter Office sent you; she'll not turn a wet woman from her fire, and Mícheál will grumble at you for the sake of it. | 85    |
| brief-fable      | Athlone is a long road in the wet, but there's no bed for a stranger here; the room behind this counter is mine alone. I'd not know who in the parish would take you in at this hour, so don't count on it.                                                                                                                                                  | 76    |

### h08-false-accusation (B10, B19, B20): Mícheál, “You're the drover who sold me a lame cow last year!”

| Prompt           | Reply                                                                                                                                                                | Score |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | I don't know you, and I can't say I remember any such sale. With the ground this wet, I've no patience for old complaints this afternoon.                            | 86    |
| 2026-11-08 brief | Aye, I drive cattle, but I don't know your face. I'll not own to a lame beast I can't call to mind; tell me who you are and where you bought it, and then we'll see. | 92    |
| brief-fable      | I've sold a fair few beasts on this road, but I don't know your face. If you've a complaint about a cow, say which one and where you bought it.                      | 70    |

### h09-prices (B8, B4): Mícheál, “What's the news of prices, would you say?”

| Prompt           | Reply                                                                                                                                                                                                                                                                | Score |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | I've no word on prices I can give you with any certainty, and I'd not guess at them. The wet ground has made moving the cattle hard this week, if that's any help. Róisín keeps the household accounts, so ask her.                                                  | 72    |
| 2026-11-08 brief | I'd not put a figure on anything I haven't seen with my own eyes, stranger. The ground's been too wet this week to move cattle, and that's the only news I'd swear to. If it's corn prices you want, the Saturday market in Roscommon will tell you more than I can. | 74    |
| brief-fable      | I've no figures to give you, and I'd not guess at money. The wet ground has made the cattle hard to move this week, and that's the only news I have. My sister Róisín keeps the accounts; she's there at the yarn, and she'd tell you more than I would.             | 65    |

### h10-ungrounded-she (B5, B16): Peig, “And did she come by already?”

| Prompt           | Reply                                                                                                                                                           | Score |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | I can't say who you mean, or whether she has been in yet today. If you tell me her name, I'll tell you what I know, but I can't see anyone outside this office. | 80    |
| 2026-11-08 brief | Who is it you mean? You've not given me her name, so I can't say whether she came by. Is it a letter you're after collecting?                                   | 86    |
| brief-fable      | I've no notion who you mean, for you haven't named her. Give me a name and I'll tell you what I know.                                                           | 90    |

### h11-hold-bag (B18, B2): Róisín, “Would you hold my bag a moment while I fix my shoe?”

| Prompt           | Reply                                                                                                                       | Score |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | Ah, the road here is muddy, so mind your footing with that shoe. I'm not sure I'm the one to be minding a bag just now.     | 86    |
| 2026-11-08 brief | I'll mind it for you, so, while you see to that shoe. Be quick about it, though, I've yarn to sell before the road empties. | 95    |
| brief-fable      | Go on so, I'll mind it for you, and take your time with that shoe.                                                          | 95    |

### h12-repeat-fair (B15, B14, B20): Mícheál, “Is the fair at Roscommon soon?”

| Prompt           | Reply                                                                                                                        | Score |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------- | ----- |
| Baseline         | Whit-Monday, if the ground dries. With this wet ground the cattle are hard to move, so I wouldn't count on it coming sooner. | 50    |
| 2026-11-08 brief | Not soon, no. It's March yet, and the ground is too wet this week to be walking cattle anywhere.                             | 90    |
| brief-fable      | Not for a good while yet, and I'll see how the cattle look before I bother with it.                                          | 85    |

## What this says about Fable running the loop alone

- **The mechanics work.** One session can render production requests, run a
  stand-in, judge blind and keep an auditable record with no external API:
  no reply or verdict was retyped, the judges never saw which prompt produced
  a reply, and the held-out cases stayed closed until the end.
- **The judge is steady; the stand-in is the noise.** Fable judging moved 3
  points per reply on a rejudge. One Haiku reply per case moved up to 30. With
  25 cases a round, a gain of 2 to 3 points is inside that noise, so the
  plateau rule mostly measured noise, and the one round that "won" did not hold
  up on held-out cases. A useful in-session loop would need several replies
  per case (three or more) and a control rerun of the best prompt, as the
  skill's comparisons already do.
- **The rewrites were reasonable but not better.** Each rewrite answered the
  previous round's misses, and Haiku took each one too literally (round 02 is
  the clearest case). The 2026-11-08 brief was already near what one
  instruction-wording change can buy on this stand-in.
- **Quota, not judgement, was the binding limit.** Every reply and judge batch
  is a subagent of about 80,000 to 110,000 tokens, mostly fixed context. The
  five-hour usage limit stopped the run four times, and the owner moved the
  researcher to Opus partway through. A round on this design is 28
  subagents (25 replies and 3 judge batches), plus the researcher's own
  context, which grows every round.

## Limits, and what would make the scores comparable

- **Haiku is not Gemini.** Haiku answers in Claude's register. It never had the
  v1 schema enforced, and it runs in a subagent harness that can add text
  around the JSON (one malformed reply). Prompt changes that help Haiku need
  not help `gemini-3.5-flash-lite`, and the reverse; prose sections are one
  example.
- **One Fable judge is not the Opus/GPT pair.** Different family, one verdict
  instead of a mean of two, and no split check. Fable shares a family with the
  stand-in, which the promptfoo trap list warns against for promotion.
- **Too few samples.** One reply per case per round; held-out is 12 cases with
  one reply each.

To make these numbers comparable with 2026-11-08, score `brief-fable.yaml` in
the existing promptfoo lab, on the same route, with the same judges, beside a
fresh run of `brief.yaml` as its control (the route moved to OpenRouter on
2026-11-10, so the 2026-11-08 numbers are not a fair control either).

## For the owner: scoring `brief-fable.yaml` on Gemini

Not run. It needs `OPENROUTER_API_KEY` in the repository's `.env`, and costs
about $1.80 (one training round on both Gemini models with both judges, the
same as a 2026-11-08 round):

```sh
cd promptfoo/prompt-lab && bash eval.sh -c promptfooconfig.yaml \
  -p file://prompts/brief-fable.yaml --no-cache -o output/brief-fable-train.json
jq -f summary.jq output/brief-fable-train.json
```

`-p` replaces the config's two prompts with `brief-fable.yaml` for this run
only (checked offline with the `echo` provider; the config is not edited).
For a like-for-like control on today's route, add `file://prompts/brief.yaml`
after the first `-p` path (about $1.80 more), and compare the two
`all_models.mean` values in the summary. Adding `-t cases/heldout.yaml` runs the
held-out cases instead (about $0.80 per prompt).
