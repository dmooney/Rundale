---
name: rebase
description: Rebase the current branch onto origin/main, resolving all merge conflicts intelligently.
---

Rebase the current branch onto `origin/main`, resolving every conflict along the way.

## Steps

1. **Pre-flight checks**:
   - Run `git status` to confirm the working tree is clean. If there are uncommitted changes, warn the user and stop — do NOT stash or discard anything without explicit permission.
   - Run `git branch --show-current` to identify the current branch. If already on `main`, warn the user and stop.

2. **Fetch latest origin**:
   - Run `git fetch origin main`.

3. **Start the rebase**:
   - Run `git rebase origin/main`.
   - If it completes with no conflicts, skip to step 6.

4. **Resolve conflicts** (repeat until the rebase finishes):
   - Run `git diff --name-only --diff-filter=U` to list conflicted files.
   - For **each** conflicted file:
     a. Read the file to understand both sides of the conflict (look for `<<<<<<<`, `=======`, `>>>>>>>`).
     b. Read the `git log --oneline origin/main..HEAD` and recent `git log --oneline origin/main -10` to understand the intent of both sides.
     c. Resolve the conflict by keeping the intent of **our branch's changes** rebased cleanly onto the upstream code. Prefer integrating both sides when they touch different things; prefer our branch's version when they genuinely conflict on the same logic.
     d. After editing, verify the file has no remaining conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`).
     e. Run `git add <file>` to mark it resolved.
   - After all files in the current step are resolved, run `git rebase --continue`.
   - If new conflicts appear, repeat this step.

5. **Validate after rebase**:
   - Run `cargo build` to confirm the code compiles.
   - Run `cargo test` to confirm tests pass.
   - If either fails, diagnose and fix the issue, then amend the appropriate rebase commit if still rebasing, or create a fixup commit if the rebase is complete.

6. **Push**:
   - Run `git push --force-with-lease` to update the remote PR branch.

7. **Report**:
   - Show `git log --oneline origin/main..HEAD` so the user can review the rebased commits.
   - Summarize: how many commits were rebased, how many conflicts were resolved, and whether the build/tests pass.
