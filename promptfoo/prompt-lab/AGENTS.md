# prompt-lab — agent scope

The NPC dialogue prompt lab (issue 2294; requirements in
`docs/design/npc-dialogue-prompt.md` §6). [README.md](README.md) covers the
loop and every command. It is not rundale-bench: nothing here touches the
leaderboard, the manifest or the bench site.

## Commands

```sh
just -f promptfoo/prompt-lab/justfile test       # offline; also part of `npm --prefix promptfoo test`
just -f promptfoo/prompt-lab/justfile smoke      # stand-in plumbing check (OpenAI key, claude CLI)
just -f promptfoo/prompt-lab/justfile loop       # the research loop (Gemini key, both CLIs)
```

## Traps

- **Facts live in `content/`, presentation in `prompts/`.** The loop may only
  rewrite presentation. Do not add facts to a variant's instructions; the fact
  guard refuses proper nouns and numbers it does not already contain. `test_lab.py`
  checks the copied character sheets (fields, days, homes, relationships) and
  the village's place names, ways, stories and feast names against
  `mods/rundale/`, and CI runs it
  when those mod files or the requirements doc change, so a mod change fails
  until the content is updated.
- **Production sorts the input keys.** Key names, not dict order, decide the
  order the model reads a brief's sections; `variant.validate` requires the
  moment and player line to sort last.
- **Never load held-out cases in the loop or the rewriter prompt.** Use
  `content.cases("train")`; held-out is for `run_heldout` alone.
- **Cases must put everyone where their day does.** The test checks each
  speaker and present character against `characters.json` days at the case's
  clock, and part of day mirrors `time_of_day_from_hour` (19:00 is Night, not
  Dusk).
- **Stand-in runs are not results.** `--stand-in` proves the plumbing only;
  never report its scores as evidence about the prompt or Gemini.
