---
name: prompt-lab
description: Run the NPC dialogue prompt research loop (issue 2294) — score the dialogue prompt on the training cases with promptfoo, read the judges' misses, rewrite the prompt's wording and format, and repeat until the score plateaus or $25 is spent, then run the held-out cases once for the owner. Trigger for "run the prompt lab", "next prompt-lab round", "/prompt-lab", or a looped prompt-lab job.
disable-model-invocation: false
---

# Prompt lab

You are the researcher. Each round you run the eval, read what the judges
say, and rewrite `promptfoo/prompt-lab/prompts/brief.yaml`. No script makes
these choices; you do. The rules come from
`docs/design/npc-dialogue-prompt.md` §6.

## Before the first round

- Run from the repository root, with `GEMINI_API_KEY`, `ANTHROPIC_API_KEY`
  and `OPENAI_API_KEY` set. The Gemini model under test and both judges
  (Claude Opus 5.5 at high effort, GPT 6.1 Sol at xhigh) are billed through
  those keys.
- `npm ci --prefix promptfoo` if `promptfoo/node_modules` is missing.
- Read `promptfoo/prompt-lab/log.md`. It says which round is next, the best
  score so far and its round, and the spend so far. Never start over a run
  the log shows in progress.

## One round

Set `L=promptfoo/prompt-lab` and `N` to the round number (two digits).

1. **Run.** Round 00 also scores the baseline once, for reference:

   ```sh
   cd $L && PROMPTFOO_DISABLE_TELEMETRY=1 ../node_modules/.bin/promptfoo eval \
     -c promptfooconfig.yaml --no-cache -o output/round-$N.json \
     --filter-prompts brief          # drop this flag in round 00
   cp prompts/brief.yaml output/round-$N-brief.yaml
   ```

2. **Read.** `jq -f summary.jq output/round-$N.json` gives, per prompt: the
   mean score (0-100, the mean of both judges over every training reply),
   the score per requirement, errors, cases where the judges differ by more
   than 20 points, the round's spend, and the worst replies with each
   judge's reasons. Read the worst replies yourself; do not rely on the mean.
   - An error with no grading is a reply that was not the output schema's
     JSON, or a failed request. Open it in the results file. A malformed
     reply is the prompt's miss; a failed request (HTTP error, quota) is an
     outage: rerun the round, and do not score it.
   - On the first round, check that the judges' token counts are non-zero.
     The cap depends on them.

3. **Record.** Append the round to `log.md`: the score, the gain over the
   best score before it, the per-requirement scores, the flagged splits, the
   round's spend and the running total, and what you changed and why.

4. **Stop or rewrite.**
   - Stop after three rounds in a row that each gain under 2 points on the
     best score before them, or when the running spend reaches $25. Before
     the next round, check that the last round's spend still fits under $25,
     with about $3 left for the held-out run.
   - Otherwise rewrite `prompts/brief.yaml`, starting from the best round's
     prompt (`output/round-<best>-brief.yaml`), not necessarily the last
     one. Make one change, or a small set of related changes, aimed at what
     the misses have in common.

## What you may change

You may change wording, framing, ordering and format: the instructions, the
key names, the order of the sections, the wording of the moment's cues (the
stranger, how the character came to answer, the anachronism notice), JSON
values versus prose, and whether the common knowledge is shaded by station
(`ck.shaded[speaker]`).

You may not:

- change facts or add them to the prompt: characters, backstory, village and
  common knowledge come from `content/`, and the moment from the cases;
- name a person, place or line from a case, or tailor the prompt to one case;
- change when the anachronism notice appears (it follows the case's
  `anachronism` value);
- edit `content/`, the cases, the rubric, or `prompts/baseline.yaml` to raise
  a score.

## Held-out cases

Never open `cases/heldout.yaml` while you are iterating. When the loop stops,
run the best prompt and the baseline on it once:

```sh
cd $L && cp output/round-<best>-brief.yaml prompts/brief.yaml && \
  PROMPTFOO_DISABLE_TELEMETRY=1 ../node_modules/.bin/promptfoo eval \
  -c promptfooconfig.yaml -t cases/heldout.yaml --no-cache -o output/heldout.json
```

Then write `docs/test-plans/results/<date>-prompt-lab.md`. Include every
round's prompt change, score, reasoning and spend from `log.md`, the
held-out scores for both prompts, and each held-out case's two replies side
by side for the owner to read. Leave raw output out of git. The owner
accepts or rejects the prompt; do not freeze it yourself.
