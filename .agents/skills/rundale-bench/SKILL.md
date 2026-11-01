---
name: rundale-bench
description: Route current model evaluation requests to the public Rundale Benchmark 3.0 project. Use this repository's archived benchmark instructions only when the user explicitly requests a pre-3.0 historical run or result.
---

# Rundale benchmark routing

## Current benchmark: 3.0

For current requests such as “test model X,” continue the work in the public
[dmooney/rundale-bench repository](https://github.com/dmooney/rundale-bench).
Read that repository's current instructions, specification, pinned suite, and
qualification receipt before executing or publishing a run. Its readiness checks
are authoritative; a successful site deployment alone does not establish readiness.
If those checks block a current evaluation, report the blocker and do not run or rank it.
Do not execute this Rundale repository's legacy workflow for a current evaluation.

The initial 3.0 suite uses native provider APIs only and covers dialogue,
multiturn conversation, and objective Gaeilge checks. Resolve the highest
supported reasoning setting by default. Keep candidate inference and judge
spend within the $5 per-model/configuration budget. Follow the new repository's
qualification gates before publishing or ranking results; an unqualified or
incomplete run must not be presented as a complete leaderboard result. Implementation history:
[foundation](https://github.com/dmooney/rundale-bench/pull/1) and
[provisional judging](https://github.com/dmooney/rundale-bench/pull/2).

## Historical benchmark: before 3.0

Use the archived legacy material in this repository only when the user
explicitly asks for a pre-3.0 run, a historical result, or reproduction of a
specific legacy evaluation. Confirm the requested benchmark version and use its
then-current artifacts as historical evidence. Label any reproduced output as
historical and keep it out of the 3.0 leaderboard.

The original pre-3.0 operating instructions are preserved at [their historical
revision](https://github.com/dmooney/Rundale/blob/3bf68689ea7f6e6717c8aec2f3a6869231742cf5/.agents/skills/rundale-bench/SKILL.md).

The old all-slices workflow, OpenRouter target resolution, Sonnet judge routing,
queue-draining procedures, local serving recipes, and historical score reports
do not apply to current 3.0 requests. Do not silently fall back to them when the
new repository, credentials, or qualification gates are unavailable; report
the blocker and leave the result unranked.
