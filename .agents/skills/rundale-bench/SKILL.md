---
name: rundale-bench
description: Route current model evaluation requests to the public Rundale Benchmark 3.0 project. Use this repository's archived benchmark instructions only when the user explicitly requests a pre-3.0 historical run or result.
argument-hint: '[current 3.0 model evaluation | explicit pre-3.0 historical request]'
---

# Rundale benchmark routing

## Current benchmark: 3.0

For current requests such as “test model X,” continue the work in the public
[dmooney/rundale-bench repository](https://github.com/dmooney/rundale-bench).
Its README and `.agents/skills/rundale-bench/SKILL.md` define the canonical
implementation and run procedure. Do not execute this Rundale repository's
legacy benchmark workflow for a current evaluation.

The initial 3.0 suite uses native provider APIs only and covers dialogue,
multiturn conversation, and objective Gaeilge checks. Resolve the highest
supported reasoning setting by default. Keep candidate inference and judge
spend within the $5 per-model/configuration budget. Follow the new repository's
qualification gates before publishing or ranking results; an unqualified or
incomplete run must not be presented as a complete leaderboard result. The
current implementation PR is [dmooney/rundale-bench#1](https://github.com/dmooney/rundale-bench/pull/1).

## Historical benchmark: before 3.0

Use the archived legacy material in this repository only when the user
explicitly asks for a pre-3.0 run, a historical result, or reproduction of a
specific legacy evaluation. Confirm the requested benchmark version and use its
then-current artifacts as historical evidence. Label any reproduced output as
historical and keep it out of the 3.0 leaderboard.

The old all-slices workflow, OpenRouter target resolution, Sonnet judge routing,
queue-draining procedures, local serving recipes, and historical score reports
do not apply to current 3.0 requests. Do not silently fall back to them when the
new repository, credentials, or qualification gates are unavailable; report
the blocker and leave the result unranked.
