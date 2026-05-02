---
name: five-whys
description: Root cause analysis via iterative "why?" questioning. Use when investigating a bug, regression, incident, test failure, or any unexpected behavior — drill past the symptom to the underlying cause before proposing a fix.
disable-model-invocation: false
---

# Five Whys

A root cause analysis technique developed at Toyota. Ask "why?" five times (or until you hit a real cause), each answer becoming the subject of the next question. Stops you from patching symptoms.

## How to apply

1. **State the problem precisely.** One sentence. Observable behavior, not interpretation. ("Tauri build crashes on startup", not "build is broken".)
2. **Ask "why did this happen?"** Answer with a verifiable fact, not a guess. If you don't know, investigate (read code, run command, check logs) before answering.
3. **Treat the answer as the new problem.** Ask "why?" again.
4. **Repeat ~5 times.** Stop when the next "why?" leaves the system under your control (process, design, missing test) or reaches a deliberate trade-off.
5. **Fix the root, not the chain above it.** Patching an intermediate "why" leaves the root cause intact and the bug will recur in another form.

## Example

Problem: Headless CLI hangs on `parish run` after upgrading tokio.

1. Why? — Worker task never completes.
2. Why? — `recv()` on the input channel blocks forever.
3. Why? — No sender drops the channel; producer holds a clone past shutdown.
4. Why? — Shutdown signal handler doesn't drop its sender clone.
5. Why? — New tokio version made `JoinHandle::abort()` not drop captured state synchronously; old code relied on that.

Root cause: implicit reliance on undocumented drop-on-abort behavior. Fix: explicit `drop(tx)` in shutdown path. (Patching at level 2 with a timeout would mask the leak.)

## Anti-patterns

- **Answering with speculation.** "Probably because…" → go verify.
- **Stopping at the first plausible answer.** First "why" usually names a symptom, not a cause.
- **Branching into many whys without finishing one chain.** Pick the most load-bearing branch; document the others for follow-up.
- **Blaming a person.** Whys point at process and code, not individuals.

## Always conclude with the prevention question

After identifying the root cause, ask:

> **Is there anything missing from `AGENTS.md` that would have avoided this issue in the first place?**

If yes — a missing rule, an unenforced convention, a gap in mode-parity coverage, a silently-tolerated anti-pattern — propose the rule text and **include the `AGENTS.md` change in the same PR as the fix**. The five-whys output is incomplete without this step. A root cause that only fixes one occurrence and leaves the door open for the next is half a fix.

Guidance on the change:
- Prefer enforcement (a fitness test, lint, or CI check) over convention. If enforcement is too costly, file a follow-up issue tracking the test, and add the rule as convention with a `TODO: enforce via <test>`.
- Keep the rule one short paragraph in the **Non-negotiable engineering rules** list. Lead with the imperative; one-line rationale.
- If no gap exists (the rule was there and was ignored, or the cause is genuinely one-off), say so explicitly — don't pad.

## When to invoke

- Bug reports, regressions, flaky tests, CI failures, performance cliffs, surprising user-visible behavior.
- Before writing a fix for anything you don't fully understand the cause of.
- During post-incident review.
