# NPC dialogue prompt lab

The prompt lab of the [NPC dialogue prompt requirements](../../docs/design/npc-dialogue-prompt.md#6-prompt-lab)
(§6, issue 2294). An agent drives it with the
[prompt-lab skill](../../.agents/skills/prompt-lab/SKILL.md). Each round it
runs promptfoo on the training cases, reads the judges' notes, and rewrites
the prompt. There is no research script.

It calls models directly and is not the game. It is separate from the
rundale-bench leaderboard in the parent directory.

| Path                       | What it holds                                                                                                                                                                                                                      |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `promptfooconfig.yaml`     | The production model (`gemini-3.5-flash-lite`), a larger Gemini (`gemini-3.8-flash`), the two judges (Claude Opus 5.5 at high effort, GPT 6.1 Sol at medium effort), and the playability rubric they share (design §3.1)           |
| `prompts/brief.yaml`       | The prompt under research: the instructions and the request, built from the content and the case                                                                                                                                   |
| `prompts/frozen.yaml`      | The prompt the owner froze on 2026-11-23 (in-session round 09, #2422), which production builds from game data (#2299)                                                                                                              |
| `prompts/baseline.yaml`    | The v1 Endpoint's instructions with a reconstruction of the v1 input the engine builds, for reference                                                                                                                              |
| `cases/train.yaml`         | 25 moments in the tiny world the loop scores and reads, each tagged with the requirements it tests and the #2024 probes it rephrases                                                                                               |
| `cases/engage-train.yaml`  | 22 playability moments (design §3.1, F1-F9, owner-requested 2026-11-24): conversations to carry on, curious questions, unknown-thing probes, grumps and moments where short is right; a training round runs them with `train.yaml` |
| `cases/heldout.yaml`       | 12 more, covering B1 to B20, run once at the end                                                                                                                                                                                   |
| `cases/heldout-2.yaml`     | 12 more, written by a separate agent for the in-session experiment once the first held-out set was spent; agent-written, awaiting the owner's review                                                                               |
| `content/`                 | Fixed facts: §4.1 common knowledge and each character's shaded note, the §4.2 characters with hand-written backstory, and the §4.3 village                                                                                         |
| `summary.jq`               | A round's mean score, score per requirement, judge splits, spend and worst replies                                                                                                                                                 |
| `check.sh`, `test/`        | The offline check (`npm --prefix promptfoo test` runs it): every case renders under both prompts, the anachronism switch, and `summary.jq` against a fixture                                                                       |
| `log.md`                   | Every round's score, change, reasoning and spend                                                                                                                                                                                   |
| `prompts/brief-fable.yaml` | The in-session experiment's best prompt (2026-11-16), for the owner to score on Gemini; not frozen                                                                                                                                 |
| `insession/`               | The in-session experiment: offline rendering, a Haiku stand-in and blind Claude judges (Fable 5.2, then Opus 5.5) run as Claude Code subagents, with its own log                                                                   |

The content was copied from `mods/rundale/` (`npcs.json`, `world.json`,
`festivals.json`) when the lab was made. The backstories, the shaded notes
and the cases were written for the lab and await the owner's review. The
common knowledge is the fact-checked list in §4.1.

Keys are sent in the order the prompt writes them. By the owner's decision
(design doc §6.4, 2026-11-01) production matches the frozen order: since #2299
the phone sends the engine's input bytes unchanged instead of sorting them.

To run a round, follow the skill's commands; they pass `--no-cache` (so a
changed prompt is never answered from cache) and `--filter-prompts '^brief$'`
(so the baseline is billed only in round 00). They run through `eval.sh`, which loads the
repository's `.env`. Each needs only `OPENROUTER_API_KEY`: the Gemini models
under test and both judges bill one OpenRouter balance. Raw output goes to
`output/`, which git ignores.

Since 2026-11-25, `eval.sh` keeps rounds cheap by default. Set these in the
environment:

| Variable             | Default                 | Effect                                                                                                                                                            |
| -------------------- | ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LAB_MODELS`         | `flash-lite`            | `flash-lite` runs only the production model; `all` adds `gemini-3.8-flash`, for the final check                                                                   |
| `LAB_JUDGES`         | `opus`                  | `opus` scores with the Opus judge alone, for rounds; `both` adds GPT, for the control's comparison and the held-out check. Compare only runs with the same judges |
| `LAB_MIN_CREDIT_USD` | `3` (`7` with all/both) | `eval.sh` exits 4 before any call when the OpenRouter balance is below this                                                                                       |
| `LAB_DRY_RUN`        | unset                   | `1` prints the promptfoo arguments and exits without calling anything                                                                                             |
| `REQUEST_TIMEOUT_MS` | `900000`                | promptfoo's per-request timeout                                                                                                                                   |

Estimated cost of a 47-case training round, from the per-reply costs measured
in run 2 (an estimate, not a quote): about $1.95 for the default (flash-lite,
Opus alone), $2.50 for flash-lite with both judges, and $5.20 for both models
with both judges.
