---
name: task-start
description: The mandatory first step for any coding task — write acceptance criteria and a verification fixture before any implementation, then stop for human review. For non-trivial gameplay features, also decompose depth-first into a design note and a plan. Produces .proofs/<task-id>/acceptance-criteria.md (+ a design note and plan for larger features) before a single line of code.
argument-hint: <kebab-case-task-id>
---

Before writing a single line of code, define "done". This is the mandatory first step for every
implementation task. The size of the up-front artifacts scales with the size of the task: every task gets
acceptance criteria + a verification fixture; non-trivial **features** also get a design note and a plan.

Proof bundles live in `.proofs/<task-id>/` at the repo root. `.proofs/` is gitignored — bundles are
attached to the PR via `just attach-proof`, not committed.

Use the kebab-case argument as `$TASK_ID` (e.g. `fix-npc-schedule`, `market-day`, `save-path-bug`).

## Every task: acceptance criteria + fixture

1. **Create the bundle directory** — `.proofs/$TASK_ID/`.

2. **Write `.proofs/$TASK_ID/acceptance-criteria.md`** — copy the skeleton from
   `.claude/skills/task-start/templates/acceptance-criteria.md` and fill in the task description, criteria,
   and verification signals. Criteria must be **observable** — describable as "the output shows X" or "the
   game does Y when Z". Avoid vague criteria like "the feature works".

3. **Write `parish/testing/fixtures/play_$TASK_ID.txt`** — a script that exercises the feature or fix and
   makes each criterion visible in the game's JSON output. Pattern after existing fixtures like
   `play_weather.txt` or `banshee_playtest.txt`. Include `/status`, `/time`, `/npcs`, `/wait`, `look`, and
   movement commands as appropriate. For a new feature, the fixture should currently demonstrate **the
   absence** of the feature — e.g. `/wait 480` over a festival day then `/status`, where the missing
   festival data is what changes once implemented.

## Non-trivial features: also decompose depth-first

For a feature (not a small fix), break the goal into reviewable artifacts before any code — a wrong design
caught now costs a few short markdown files; caught after coding it costs a feature.

4. **Design note** — `docs/design/$TASK_ID.md`
   - Restate the feature in one paragraph: what does the player experience?
   - List affected subsystems by crate (`parish-world`, `parish-npc`, `parish-inference`,
     `parish-persistence`, `parish-config`, etc.).
   - Sketch data-model changes (new fields on `Npc` / `World`, new event variants, new `mods/rundale/` files).
   - Specify the **observable signal** in the harness: which JSON line(s) prove the feature is live.
     Reference the relevant `ActionResult` variants in `crates/parish-cli/src/testing.rs`.
   - Name the feature flag (`config.flags.is_enabled("$TASK_ID")`) per `AGENTS.md` §6.

5. **Implementation plan** — `docs/plans/$TASK_ID.md`
   - Ordered code-level steps: which files change, in which order, why. One commit per step with a
     conventional-commit prefix.
   - Note tests to add or update. If gameplay-visible, schedule a `/parish-engine prove $TASK_ID` and
     optionally a rubric snapshot run.

## Then stop

**Do not write any implementation code.** Surface the artifacts (criteria + fixture, plus design note + plan
for features) and ask the user to review. The point is to make redirection cheap.

## After approval

Once the user signs off:

1. Implement the change one commit at a time.
2. Run the verification script:
   `cargo run --manifest-path parish/Cargo.toml -p parish-cli -- --script parish/testing/fixtures/play_$TASK_ID.txt`
3. Capture the output to `.proofs/$TASK_ID/transcript.txt`.
4. Write `.proofs/$TASK_ID/evidence.md` from `.claude/skills/task-start/templates/evidence.md` — keep the
   `Evidence type: live gameplay transcript` header and map each criterion to the specific transcript
   line(s) that prove it.
5. Write `.proofs/$TASK_ID/judge.md` from `.claude/skills/task-start/templates/judge.md` — keep the three
   header lines (`Verdict: sufficient`, `Technical debt: clear`, `Acceptance criteria: met`) and verify each
   criterion individually.
6. Run `just agent-check` — all three lines in judge.md plus the AC file are required for the gate to pass.
7. Run `just attach-proof $TASK_ID` to post the bundle to the PR as a structured comment. CI fetches that
   comment to re-validate the gate.
8. For a feature, run `/parish-engine prove $TASK_ID` to read the JSON critically, and consider adding
   `play_$TASK_ID` to `BASELINED_FIXTURES` if the output is deterministic — that locks the feature against
   regression. Run `just harness-audit` to confirm the feature no longer shows as a coverage gap.

## Why this exists

A shared definition of "done" written before coding constrains the implementation to what was asked for, not
what was easy to build. The transcript then proves the constraint was satisfied, and the judge verifies each
criterion against the actual output — not against the author's recollection of what they intended. For
larger features, the design note and plan let a human redirect the approach while it's still just prose.
