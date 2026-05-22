---
name: task-start
description: Write acceptance criteria before any implementation. First thing to do for any coding task — produce .proofs/<task-id>/acceptance-criteria.md and a verification fixture, then stop for human review before writing a single line of code.
argument-hint: <kebab-case-task-id>
---

Before writing a single line of code, write acceptance criteria. This is the mandatory first step for every implementation task.

Proof bundles live in `.proofs/<task-id>/` at the repo root. `.proofs/` is gitignored — bundles are attached to the PR via `just attach-proof`, not committed.

## Steps

Use the kebab-case argument as `$TASK_ID` (e.g. `fix-npc-schedule`, `market-day`, `save-path-bug`).

1. **Create the bundle directory** — `.proofs/$TASK_ID/`

2. **Write `.proofs/$TASK_ID/acceptance-criteria.md`** — copy the skeleton from `.claude/skills/task-start/templates/acceptance-criteria.md` and fill in the task description, criteria, and verification signals. Criteria must be **observable** — describable as "the output shows X" or "the game does Y when Z". Avoid vague criteria like "the feature works".

3. **Write `parish/testing/fixtures/play_$TASK_ID.txt`** — a script that exercises the feature or fix and makes each criterion visible in the game's JSON output. Pattern after existing fixtures like `play_weather.txt` or `banshee_playtest.txt`. Include `/status`, `/time`, `/npcs`, `/wait`, `look`, and movement commands as appropriate.

4. **Stop here.** Do not write any implementation code. Show the two files to the user and ask for review. A wrong design caught at this stage costs two short markdown files; caught after coding it costs a revert.

## After approval

Once the user signs off on the acceptance criteria:

1. Implement the change one commit at a time.
2. Run the verification script: `cargo run --manifest-path parish/Cargo.toml -p parish-cli -- --script parish/testing/fixtures/play_$TASK_ID.txt`
3. Capture the output to `.proofs/$TASK_ID/transcript.txt`.
4. Write `.proofs/$TASK_ID/evidence.md` from `.claude/skills/task-start/templates/evidence.md` — keep the `Evidence type: live gameplay transcript` header and map each criterion to the specific line(s) in the transcript that prove it.
5. Write `.proofs/$TASK_ID/judge.md` from `.claude/skills/task-start/templates/judge.md` — keep the three header lines (`Verdict: sufficient`, `Technical debt: clear`, `Acceptance criteria: met`) and verify each criterion individually.
6. Run `just agent-check` — all three lines in judge.md plus the AC file are required for the gate to pass.
7. Run `just attach-proof $TASK_ID` to post the bundle to the PR as a structured comment. CI fetches that comment to re-validate the gate.

## Why this exists

A shared definition of "done" written before coding constrains the implementation to what was asked for, not what was easy to build. The transcript then proves the constraint was satisfied, and the judge verifies each criterion against the actual output — not against the author's recollection of what they intended.
