---
name: techdebt
description: Continuous technical-debt sweeper: consume TODO.md tasks (or discover debt when empty), dispatch focused fix agents, keep the list current, and repeat until no actionable debt remains.
disable-model-invocation: false
argument-hint: [path]
---

Use this skill to run a structured debt-reduction loop for a target file/folder. If no argument is provided, default to the current working directory.

## Inputs

- `$ARGUMENTS` (optional): file or directory path to scope work.
  - If omitted, use `.`.
  - If a file is provided, use its parent directory as debt-list home and keep analysis focused on that file.

Set `TARGET` from the argument (or `.`), then resolve:

- `SCOPE`: exact file/folder(s) agents should inspect/fix.
- `ROOT`: directory where `TODO.md` is read/written.

## Loop contract

Run this cycle repeatedly until exit criteria are met.

1. **Load or initialize debt list**
   - Look for `TODO.md` under `ROOT`.
   - If missing, create one with sections:
     - `## Open`
     - `## In Progress`
     - `## Done`
   - Keep entries concise and actionable, with stable IDs (`TD-001`, `TD-002`, ...), owner, and status.

2. **Choose work source**
   - If `## Open` has items, pick the highest-impact small batch (1–3 items).
   - If no open items exist (or everything is done), spawn discovery agents to scan `SCOPE` for technical debt:
     - dead/unreachable code
     - duplication and abstraction opportunities
     - weak or missing tests
     - stale docs/comments/config
     - high-complexity hotspots and brittle conditionals
   - Add each validated finding to `## Open` before fixing.

3. **Dispatch fix agents**
   - Spawn parallel agents when tasks are independent; otherwise run serially.
   - Give each agent exactly one debt item ID and acceptance criteria.
   - Require each fix agent to:
     - implement minimal, behavior-safe change
     - add/update tests when behavior or guarantees change
     - run relevant checks
     - report file list + commands run + residual risks

4. **Reconcile and update `TODO.md`**
   - Move started items to `## In Progress`, then to `## Done` only after checks pass.
   - For partially fixed work, keep item open with a narrowed remaining scope.
   - Remove duplicates; merge equivalent debt items under the earliest ID.
   - Append a short progress log entry (date + IDs completed).

5. **Gate before next loop**
   - Ensure repository is in a clean, buildable state for touched areas.
   - If new debt was discovered during fixes, record it under `## Open`.
   - Return to Step 2.

## Exit criteria

Stop only when all are true:

- `## Open` is empty.
- Discovery pass finds no credible new debt in `SCOPE`.
- No `## In Progress` items remain.

Then leave a final note in `TODO.md` summarizing what was checked and why the loop ended.

## Operating rules

- Keep tasks small and independently landable.
- Prefer deleting dead code over refactoring it.
- Do not invent speculative debt; every item needs concrete evidence (file/line/symptom).
- Preserve AGENTS.md rules (tests, feature flags, mode parity, docs updates).
- If uncertain whether something is debt vs intentional, record a question item instead of changing behavior.
