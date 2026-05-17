---
name: task-start
description: Write acceptance criteria before any implementation. First thing to do for any coding task — produce docs/proofs/<task-id>/acceptance-criteria.md and a verification fixture, then stop for human review before writing a single line of code.
argument-hint: <kebab-case-task-id>
---

Before writing a single line of code, write acceptance criteria. This is the mandatory first step for every implementation task.

## Steps

Use the kebab-case argument as `$TASK_ID` (e.g. `fix-npc-schedule`, `market-day`, `save-path-bug`).

1. **Create the proof bundle directory** — `docs/proofs/$TASK_ID/`

2. **Write `docs/proofs/$TASK_ID/acceptance-criteria.md`**:

   ```markdown
   # Acceptance Criteria: $TASK_ID

   ## Task

   <one-paragraph restatement of what the task asks for — what the player or
   system experiences differently after this change>

   ## Criteria

   - <criterion 1> — observable via: <command or game action that proves it>
   - <criterion 2> — observable via: <command or game action that proves it>
   - ...

   ## Verification script

   Run: `cargo run -- --script parish/testing/fixtures/play_$TASK_ID.txt`

   Expected signals in output:
   - <JSON field or text pattern that confirms criterion 1>
   - <JSON field or text pattern that confirms criterion 2>
   ```

   Criteria must be **observable** — describable as "the output shows X" or "the game does Y when Z". Avoid vague criteria like "the feature works".

3. **Write `parish/testing/fixtures/play_$TASK_ID.txt`** — a script that exercises the feature or fix and makes each criterion visible in the game's JSON output. Pattern after existing fixtures like `play_weather.txt` or `banshee_playtest.txt`. Include `/status`, `/time`, `/npcs`, `/wait`, `look`, and movement commands as appropriate.

4. **Stop here.** Do not write any implementation code. Show the two files to the user and ask for review. A wrong design caught at this stage costs two short markdown files; caught after coding it costs a revert.

## After approval

Once the user signs off on the acceptance criteria:

1. Implement the change one commit at a time.
2. Run the verification script: `cargo run -- --script parish/testing/fixtures/play_$TASK_ID.txt`
3. Capture the output to `docs/proofs/$TASK_ID/transcript.txt`.
4. Write `docs/proofs/$TASK_ID/evidence.md` with header `Evidence type: live gameplay transcript` and a section mapping each criterion to the specific line(s) in the transcript that prove it.
5. Write `docs/proofs/$TASK_ID/judge.md` verifying each criterion:
   ```
   Verdict: sufficient
   Technical debt: clear
   Acceptance criteria: met

   [criterion 1]: <quote or line number from transcript confirming it>
   [criterion 2]: <quote or line number from transcript confirming it>
   ```
6. Run `just agent-check` — all three lines in judge.md plus the AC file are required for the gate to pass.

## Why this exists

A shared definition of "done" written before coding constrains the implementation to what was asked for, not what was easy to build. The transcript then proves the constraint was satisfied, and the judge verifies each criterion against the actual output — not against the author's recollection of what they intended.
