# NPC dialogue prompt lab

The prompt lab of the [NPC dialogue prompt requirements](../../docs/design/npc-dialogue-prompt.md#6-prompt-lab)
(§6, issue 2294). An agent drives it with the
[prompt-lab skill](../../.agents/skills/prompt-lab/SKILL.md). Each round it
runs promptfoo on the training cases, reads the judges' notes, and rewrites
the prompt. There is no research script.

It calls models directly and is not the game. It is separate from the
rundale-bench leaderboard in the parent directory.

| Path                    | What it holds                                                                                                                                    |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `promptfooconfig.yaml`  | The production model (`gemini-3.5-flash-lite`), the two judges (Claude Opus 5.5 at high effort, GPT 6.1 Sol at xhigh), and the rubric they share |
| `prompts/brief.yaml`    | The prompt under research: the instructions and the request, built from the content and the case                                                 |
| `prompts/baseline.yaml` | The v1 Endpoint's instructions with the v1 input the engine builds, for reference                                                                |
| `cases/train.yaml`      | 25 moments in the tiny world the loop scores and reads, each tagged with the requirements it tests and the #2024 probes it rephrases             |
| `cases/heldout.yaml`    | 12 more, covering B1 to B20, run once at the end                                                                                                 |
| `content/`              | Fixed facts: §4.1 common knowledge and each character's shaded note, the §4.2 characters with hand-written backstory, and the §4.3 village       |
| `summary.jq`            | A round's mean score, score per requirement, judge splits, spend and worst replies                                                               |
| `log.md`                | Every round's score, change, reasoning and spend                                                                                                 |

The content was copied from `mods/rundale/` (`npcs.json`, `world.json`,
`festivals.json`) when the lab was made. The backstories, the shaded notes
and the cases were written for the lab and await the owner's review. The
common knowledge is the fact-checked list in §4.1.

To run a round by hand: `cd promptfoo/prompt-lab && ../node_modules/.bin/promptfoo eval -c promptfooconfig.yaml -o output/round.json`,
then `jq -f summary.jq output/round.json`. It needs `GEMINI_API_KEY`,
`ANTHROPIC_API_KEY` and `OPENAI_API_KEY`. Raw output goes to `output/`,
which git ignores.
