# NPC dialogue prompt lab, first run

> Issue: 2294. Recorded 2026-11-08 (rounds run 2026-11-01). Method: the
> [prompt lab](../../../promptfoo/prompt-lab/README.md) of
> [design §6](../../design/npc-dialogue-prompt.md#6-prompt-lab), driven by the
> `prompt-lab` skill. The round-by-round record is
> [`log.md`](../../../promptfoo/prompt-lab/log.md). The raw promptfoo output
> (every reply, both judges' scores and reasons, per-run summaries) is kept
> out of git, in the owner's iCloud Drive under
> `Rundale/test-data/2026-11-08-prompt-lab/`.

**Result.** The brief beats the baseline on the held-out cases: **83.9 against
66.8** on both models, and **77.6 against 66.3** on the production model
(`gemini-3.5-flash-lite`). The loop stopped on the plateau rule after round 03. The best prompt is round 02's, now in
[`prompts/brief.yaml`](../../../promptfoo/prompt-lab/prompts/brief.yaml). It
scored 82.1 on training against the starting brief's 80.8. On the production
model, leaving out the backstory, rendering prose sections and dropping the
anachronism flag each scored higher than the control by more than 2 points.
Shaded common knowledge did not clear the bar. Total spend: **$18.59 of $25**.

The owner accepts or rejects the prompt. Nothing here freezes it.

## Method

- **Models under test:** `gemini-3.5-flash-lite` (production) and
  `gemini-3.8-flash` (larger), both on Google's API with the v1 output schema,
  called as the Endpoint calls them.
- **Judges:** Claude Opus 5.5 at high effort and GPT 6.1 Sol at xhigh. Each
  scores every reply 0 to 100 against the requirements its case is tagged
  with, and the reply's score is their mean. Rounds 00 to 03 called them on
  their own APIs. The OpenAI balance ran out during the first comparison
  attempt, so from the control rerun on, both went through OpenRouter's
  Anthropic Messages and OpenAI Responses routes, with the same request bodies.
- **Cases:** 25 training cases the loop saw, and 12 held-out cases run once,
  at the end.
- **Training score:** the brief's mean over both models, both judges and every
  training reply.

## Rounds

| Round | Change                                                                                                                                     | Score    | Gain on best | flash | flash-lite | Spend |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------ | -------- | ------------ | ----- | ---------- | ----- |
| 00    | Starting brief (§6.1.2); the baseline scored once for reference: 65.3                                                                      | 80.8     | n/a          | 87.7  | 74.0       | $3.53 |
| 01    | A closed-world rule moved up front: everything named must be in the request, and a plausible detail not given is still invention           | 81.8     | +1.0         | 86.6  | 77.1       | $1.76 |
| 02    | Feeling for anyone in the character's relationships shows in the same line, and a line any neighbour could say gets said as only you would | **82.1** | +0.3         | 86.4  | 77.7       | $1.80 |
| 03    | Answer every part of the stranger's request, promise only what is yours, show courtesy when refusing, and never guess about unknowns       | 81.8     | -0.3         | 88.0  | 75.6       | $1.78 |

No round had a malformed reply or an outage. Three rounds in a row gained
under 2 points, so the loop stopped.

What each round showed:

- **Round 00.** The six worst brief replies were all flash-lite, and four of
  them invented specifics to be helpful: two priests for an unknown name, a
  house beyond a bridge to send the stranger to, post on the mail-car. The
  rule against invention was in the brief, but mid-paragraph.
- **Round 01.** The inventions went: B12 rose from 67.6 to 76.0 and B20 from
  80.4 to 91.3. The most common miss became B9, correct but neutral
  directions to a friend's home.
- **Round 02.** B9 rose from 58.8 to 89.3. The worst replies then mishandled
  the stranger's request: guessing where an unknown person might be,
  promising shelter at other houses, a cold rebuff to a tired traveller,
  ignoring a given name.
- **Round 03.** B11 rose from 63.8 to 77.5, but flash-lite answered a request
  for a bed by inventing a named villager, and B9 fell back to 74.5. Net -0.3.

## Held-out cases

| Prompt   | Both models | flash | flash-lite |
| -------- | ----------- | ----- | ---------- |
| Baseline | 66.8        | 67.2  | 66.3       |
| Brief    | **83.9**    | 90.1  | 77.6       |

No outages, and no case where the judges differed by more than 20 points.
The brief's held-out flash-lite score (77.6) matches its training score
(77.7), so round 02 does not look fitted to the training cases.

By requirement, the brief's largest gains over the baseline are B7 (89.5
against 28.8), B4 (83.6 against 56.9), B8 (95.0 against 68.8) and B3 (85.8
against 65.0). It is level or slightly behind on B6 (66.8 against 68.0) and
B9 (66.8 against 68.0), both scored on one case, h05.

Each held-out case's replies, with the judges' mean score:

### h01-greet-roisin (B1, B10, B17): Róisín, “Hello there.”

| Model      | Baseline                                                                          | Brief                                                                                                                                               |
| ---------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | Good day to you. The village is quiet this afternoon, isn't it? **(55)**          | Good day to you, stranger. **(55)**                                                                                                                 |
| flash      | Good afternoon to you. It is a fine, clear day to be out in the village. **(55)** | Good day to you. 'Tis a grand clear afternoon for the road—are you seeking someone in Kilteevan, or would you have need of good spun yarn? **(86)** |

### h02-route-letter-office (B3): Róisín, “Which way to the letter office?”

| Model      | Baseline                                                                                                                     | Brief                                                                                                                                                                                                                                                      |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | The letter office is east in the village, down the lane from the road. **(65)**                                              | Go back up the boreen and turn down the road east, and you will find Peig Hannigan at the counter. **(77.5)**                                                                                                                                              |
| flash      | Follow the boreen down into the village, then take the lane leading east. That will bring you to the Letter Office. **(65)** | Take the boreen climbing east from our door up to the village road, and follow the short lane east again to the whitewashed house. You'll find my friend Peig Hannigan inside at her counter, and there's no better woman to trust with a letter. **(94)** |

### h03-mrs-delaney (B5, B1): Mícheál, “Do you know a Mrs Delaney in the village?”

| Model      | Baseline                                                 | Brief                                                                                                                               |
| ---------- | -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | I know no one by that name here. **(77.5)**              | I know no Mrs Delaney here. **(90)**                                                                                                |
| flash      | I don't know anyone by that name around here. **(63.5)** | There is no Mrs Delaney in Kilteevan, nor did I ever hear tell of one in this place. You have the wrong townland, I'd say. **(90)** |

### h04-railway (B7): Peig, “Is there a railway station near here?”

| Model      | Baseline                                                                                     | Brief                                                                                                                                                                                                                                                                          |
| ---------- | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| flash-lite | There is no railway station here, only the letters that come through this office. **(27.5)** | A rail-what now? There's no such thing here, nor likely to be; if it's travel you want, you'll be walking or taking the trap to the mail car in Athlone. **(90.5)**                                                                                                            |
| flash      | I know of no railway station in these parts. **(30)**                                        | A rail-way, did you say? I never heard tell of such a thing in all the parish, nor any station neither. If you are looking to travel, there is only the road to Roscommon or south toward Athlone, and you'll go by horse or your own two feet like any other body. **(88.5)** |

### h05-peig-letter (B6, B9): Róisín, “Did Peig get a letter from America this week?”

| Model      | Baseline                                                                                                                           | Brief                                                                                                                                                                                                                                   |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | I could not say whether Peig received any letters from America, as my work keeps me busy with our household and spinning. **(55)** | Peig Hannigan minds the Letter Office yonder, and she would tell you herself if any such post came, though I am none the wiser to what crosses the water. **(55)**                                                                      |
| flash      | I wouldn't know anything about that. Peig keeps private letters private. **(81)**                                                  | If there was any word come over the water, you would have to ask Peig herself down the short lane east at the Letter Office. She keeps the post and her own counsel, and I wouldn't be knowing what sits in her pigeonholes. **(78.5)** |

### h06-proctor (B4, B5): Róisín, “What's the tithe proctor like round here?”

| Model      | Baseline                                                                                                                       | Brief                                                                                                                                                                                                                                                    |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | I do not concern myself with such matters, trading spun yarn keeps me busy enough. **(42.5)**                                  | We have enough to mind with our own drop of water without drinking the proctor's cup, and that's all I'll say on him. **(57.5)**                                                                                                                         |
| flash      | I couldn't tell you anything about that. My time is taken up with the spinning wheel and our own household affairs. **(47.5)** | He is like any proctor, I suppose, come to collect what is claimed for the Church of Ireland rector whether a house has it easily or not. He wants his count reckoned down to the last penny, so we pay what must be paid and leave it at that. **(87)** |

### h07-maire-bed (B11, B12, B13): Peig, “I'm Máire. I've walked from Athlone and I need a bed for the night.”

| Model      | Baseline                                                                                                                                             | Brief                                                                                                                                                                                                                                                                                      |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| flash-lite | This is the Letter Office, not an inn, and it is night with rain falling. You should look for shelter elsewhere in the village. **(52.5)**           | I keep no lodging-house, woman, and you'll find no bed here in the Letter Office. **(57.5)**                                                                                                                                                                                               |
| flash      | There are no beds to be had here. This is the Letter Office, and I have only the counter, the post, and a bit of fire to keep the rain off. **(60)** | You’ve had a hard, wet trudge from Athlone, Máire, but you’ll find no inn nor hiring bed in Kilteevan. This is only the Letter Office, and I’ve no room to take in strangers. Step up to the turf a minute to shake the damp off your coat, at all events, and catch your breath. **(94)** |

### h08-false-accusation (B10, B19, B20): Mícheál, “You're the drover who sold me a lame cow last year!”

| Model      | Baseline                                                                                                                        | Brief                                                                                                                                                                                                                                        |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | I have no memory of selling you any beast, and the wet ground has made moving cattle difficult this week for anyone. **(78.5)** | I drive cattle for others and I sold no beast to a stranger last year or any other. **(88.5)**                                                                                                                                               |
| flash      | I don't know who you take me for, stranger, but I have sold you no such beast. You have the wrong man. **(92)**                 | You have mistaken your man, stranger. I only walk other men's cattle to the fairs for a few shillings, and I never had a beast of my own to sell to you nor anyone else. Mind your tongue before you go casting blame on this road. **(95)** |

### h09-prices (B8, B4): Mícheál, “What's the news of prices, would you say?”

| Model      | Baseline                                                                                                                                                         | Brief                                                                                                                                                                                                                                                                                    |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | The wet ground has made moving cattle difficult this week, and prices are hard to reckon with the roads in such a state. **(80)**                                | The cattle are hard to shift in this wet, and what the dealers give at Roscommon won't pay the rent. **(96)**                                                                                                                                                                            |
| flash      | I can't tell you about prices. The wet ground has made moving cattle difficult enough this week, and my sister Róisín manages the household accounts. **(57.5)** | That depends on what you have to sell or buy. If it's beasts you mean, the wet ground has made shifting them a hard trial this week, and we'll know nothing sure until the fair in Roscommon. A man doesn't go naming figures on the road without knowing who he's speaking to. **(94)** |

### h10-ungrounded-she (B5, B16): Peig, “And did she come by already?”

| Model      | Baseline                                                                   | Brief                                                                                                                                           |
| ---------- | -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | Who would you be asking after? **(95)**                                    | Who is it you're speaking of, and you a stranger on the road? **(92.5)**                                                                        |
| flash      | Who is it you are asking after? You have not said who you mean. **(87.5)** | Who is 'she' when she's at home? You'll have to put a name on the woman before I can tell you whether she crossed my threshold or not. **(95)** |

### h11-hold-bag (B18, B2): Róisín, “Would you hold my bag a moment while I fix my shoe?”

| Model      | Baseline                                                                                                                                 | Brief                                                                                                                                                                               |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | I am busy with my own household work just now, but you are welcome to set it down on the stone wall while you see to your shoe. **(89)** | I would not know you from Adam, stranger, so keep a hold of your own property. **(89)**                                                                                             |
| flash      | I will not take charge of your bag, but you may take your time to see to your shoe. **(80)**                                             | Set it down upon the stone wall there beside you, a chara, for my hands are full with the yarn. You will fix your shoe handier that way, and no harm done to either of us. **(96)** |

### h12-repeat-fair (B15, B14, B20): Mícheál, “Is the fair at Roscommon soon?”

| Model      | Baseline                                                                                                                          | Brief                                                                                                                                                                              |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| flash-lite | The wet ground has made moving cattle difficult this week, so we can only wait and see. **(78.5)**                                | I told you that already. **(82.5)**                                                                                                                                                |
| flash      | I told you already, and I know nothing more of that. The wet ground has made moving cattle difficult enough this week. **(87.5)** | I told you already, not till Whit-Monday. If it is corn you are after, Roscommon has a market of a Saturday, but you’ll see no drove of cattle on the road before then. **(83.5)** |

## Design §8 questions

Each question is answered by running the best prompt with one switch, on
flash-lite only, against a **control**: the same prompt, unswitched, run in
the same batch on the same judge route.

| Run                            | Cases      | Mean | Difference from control               |
| ------------------------------ | ---------- | ---- | ------------------------------------- |
| Control                        | 25         | 77.5 | n/a                                   |
| No backstory (`nobackstory`)   | 25         | 80.2 | +2.7                                  |
| Shaded knowledge (`shaded`)    | 25         | 79.3 | +1.8                                  |
| Prose sections (`prose`)       | 25         | 79.6 | +2.1                                  |
| No anachronism flag (`noflag`) | 3 (the B7) | 69.7 | +6.9 on the control's B7 cases (62.8) |

**Noise.** The control scored 77.5 against round 02's flash-lite 77.7, a
reading of **0.2 points**. Under the skill's rule, a switch answers its
question when it differs from the control by more than that reading and by
more than 2 points. One caution: the first control, run on 2026-11-01 on the
judges' own APIs, scored 78.9. That gap of 1.4 mixes rerun noise with the
change of judge route, so it is not the formal reading. But it suggests that
a difference near 2 points is close to noise.

1. **Structured JSON values or prose sections?** Prose scored **+2.1**, which
   clears the bar narrowly. Prose raised B15 (repetition, 78.3 to 93.5) and
   B14 (distinct voices, 77.0 to 89.0), but lowered B5 (admitting what you
   don't know, 76.3 to 62.5). Answer at this sample size: prose, by a margin
   close to the 1.4-point second reading.
2. **Shaded or shared common knowledge?** Shaded scored **+1.8**, under the
   2-point bar. **Unanswered** at this sample size (+1.8 against a 0.2 noise
   reading and the 2-point floor).
3. **Does backstory earn a place as mod content?** Leaving the backstory out
   scored **+2.7**. Answer at this sample size: **no**. The prompt did better
   without it. B7 (62.8 to 87.5), B8 (78.0 to 91.3) and B14 (77.0 to 83.3)
   rose. B5 (76.3 to 67.5) and B4 (90.9 to 80.4) fell, so the backstory may
   still help with grounding and period knowledge.
4. **Is standing guidance enough without the anachronism flag (§6.1.3)?**
   Without the flag, the three B7 cases scored **+6.9**. The difference is
   large, but on three replies: t09 (telephone) scored 52 without the flag,
   t17 and t22 78.5 each. Answer at this sample size: standing guidance alone
   did at least as well as the flag. A larger B7 set should confirm it before
   the flag is dropped.

The switches were checked before the runs: rendered offline, each changed
only the part it names.

## Spend

| Item                                                      | Spend      |
| --------------------------------------------------------- | ---------- |
| Rounds 00 to 03                                           | $8.87      |
| First comparison attempt: control (valid), then void runs | $4.48      |
| OpenRouter one-case check                                 | $0.03      |
| Control, four comparisons, held-out (OpenRouter judges)   | $5.21      |
| **Total**                                                 | **$18.59** |

Opus was about three quarters of all spend; both Gemini models together came
to under $1. The void runs ($3.61) came from the OpenAI balance running out
mid-batch. `eval.sh` now exits non-zero on any outage, so a chained batch
stops at the first void run.

## For the owner

- Accept or reject round 02's prompt.
- Decide the §8 settings: prose answered narrowly, backstory answered
  against, and shaded knowledge unanswered.
- Decide whether the anachronism flag stays: standing guidance did at least
  as well on three cases.
- Re-evaluate the $25 cap and the stopping rule, as §6.4 planned after the
  first run.
