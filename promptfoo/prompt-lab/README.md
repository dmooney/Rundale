# NPC dialogue prompt lab

The prompt lab of the [NPC dialogue prompt requirements](../../docs/design/npc-dialogue-prompt.md#6-prompt-lab)
(§6, issue 2294). An agent drives it with the
[prompt-lab skill](../../.agents/skills/prompt-lab/SKILL.md). Each round it
runs promptfoo on the training cases, reads the judges' notes, and rewrites
the prompt. There is no research script.

It calls models directly and is not the game. It is separate from the
rundale-bench leaderboard in the parent directory.

| Path                    | What it holds                                                                                                                                                                          |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `promptfooconfig.yaml`  | The production model (`gemini-3.5-flash-lite`), a larger Gemini (`gemini-3.8-flash`), the two judges (Claude Opus 5.5 at high effort, GPT 6.1 Sol at xhigh), and the rubric they share |
| `prompts/brief.yaml`    | The prompt under research: the instructions and the request, built from the content and the case                                                                                       |
| `prompts/baseline.yaml` | The v1 Endpoint's instructions with a reconstruction of the v1 input the engine builds, for reference                                                                                  |
| `cases/train.yaml`      | 25 moments in the tiny world the loop scores and reads, each tagged with the requirements it tests and the #2024 probes it rephrases                                                   |
| `cases/heldout.yaml`    | 12 more, covering B1 to B20, run once at the end                                                                                                                                       |
| `content/`              | Fixed facts: §4.1 common knowledge and each character's shaded note, the §4.2 characters with hand-written backstory, and the §4.3 village                                             |
| `summary.jq`            | A round's mean score, score per requirement, judge splits, spend and worst replies                                                                                                     |
| `check.sh`, `test/`     | The offline check (`npm --prefix promptfoo test` runs it): every case renders under both prompts, the anachronism switch, and `summary.jq` against a fixture                           |
| `log.md`                | Every round's score, change, reasoning and spend                                                                                                                                       |

The content was copied from `mods/rundale/` (`npcs.json`, `world.json`,
`festivals.json`) when the lab was made. The backstories, the shaded notes
and the cases were written for the lab and await the owner's review. The
common knowledge is the fact-checked list in §4.1.

Keys are sent in the order the prompt writes them. The phone's bridge sorts
them today; by the owner's decision (design doc §6.4, 2026-11-01) production
matches the order the lab freezes, and #2299 changes the bridge where needed.

To run a round, follow the skill's commands; they pass `--no-cache` (so a
changed prompt is never answered from cache) and `--filter-prompts '^brief$'`
(so the baseline is billed only in round 00). Each needs `GEMINI_API_KEY`,
`ANTHROPIC_API_KEY` and `OPENAI_API_KEY`. Raw output goes to `output/`, which
git ignores.
