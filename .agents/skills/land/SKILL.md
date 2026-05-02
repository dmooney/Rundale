---
name: land
description: Land a single PR end-to-end — resolve conflicts, address unaddressed review-bot comments, fix CI, and squash-merge once green. Trigger when the user says "land #N", "land this PR", "land the current branch", "ship #N", or "merge #N once it's clean".
argument-hint: 'PR number (e.g. "842"). If omitted, use the PR for the current branch.'
---

Drive one PR to merge. Sequential, not parallel — this skill is for finishing a specific PR cleanly. Use `drain-backlog` for bulk sweeps.

Repo: `dmooney/Rundale`. Default merge: `--squash --delete-branch`. Project gates: `just check` (fmt + clippy + tests + witness-scan + check-doc-paths). See [`docs/agent/git-workflow.md`](../../../docs/agent/git-workflow.md) and CLAUDE.md non-negotiables (mode parity, feature-flag gating, README freshness).

## Steps

1. **Resolve target PR.**
   - If `$ARGUMENTS` set → `PR=$ARGUMENTS`.
   - Else → `PR=$(gh pr view --json number --jq .number)` from current branch. If none, stop and ask.
   - Fetch state in one call:
     ```sh
     gh pr view $PR --json number,title,headRefName,baseRefName,mergeable,mergeStateStatus,state,isDraft,statusCheckRollup,reviewDecision,body,url
     ```
     Save to a file; reread fields as needed.
   - Capture portability vars used downstream:
     ```sh
     BASE=$(gh pr view $PR --json baseRefName --jq .baseRefName)
     HEAD=$(gh pr view $PR --json headRefName --jq .headRefName)
     OWNER=$(gh repo view --json owner --jq .owner.login)
     REPO=$(gh repo view --json name --jq .name)
     ```

2. **Pre-flight gates.**
   - `state == OPEN` and `isDraft == false`. If draft, ask the user before marking ready.
   - `baseRefName == main` (or whatever the user expects).
   - Title prefix is conventional (`feat:`/`fix:`/`refactor:`/`docs:`/`test:`/`chore:`/`security:`/`perf:`). If not, fix the title via `gh pr edit $PR --title "<new>"` — required by branch protection convention.
   - PR body has `Fixes #N` / `Closes #N` for any issue it claims to resolve, so squash-merge auto-closes them.

3. **Rebase if DIRTY/BEHIND.** When `mergeStateStatus` is `DIRTY` or `BEHIND`:
   - Check out the PR branch in a clean worktree:
     ```sh
     gh pr checkout $PR
     git fetch origin $BASE
     git merge origin/$BASE --no-edit
     ```
   - Resolve conflicts file-by-file. For each `git diff --name-only --diff-filter=U`:
     - Read both sides. Read `git log --oneline origin/$BASE..HEAD` (PR intent) and `git log --oneline HEAD..origin/$BASE` (upstream intent).
     - Prefer the PR's intent for new logic; prefer the base's version for code already-merged upstream.
     - Verify no `<<<<<<<` / `=======` / `>>>>>>>` markers remain. `git add <file>`.
   - Conclude: `git commit --no-edit` (merge) — do NOT use `git rebase`, since force-pushing rewrites history that bots already commented on, breaking thread anchors.
   - `just check` must pass before push.
   - `git push` (no force; merges advance the branch fast-forward; relies on `gh pr checkout`'s tracking config so fork PRs work too).

4. **Address unaddressed bot review threads.**
   - Inline threads:
     ```sh
     gh api graphql -f query='
       query($owner:String!,$repo:String!,$pr:Int!) {
         repository(owner:$owner, name:$repo) {
           pullRequest(number:$pr) {
             reviewThreads(first:100) {
               nodes { id isResolved isOutdated comments(first:5) { nodes { author { login } body path line } } }
             }
           }
         }
       }' -f owner=$OWNER -f repo=$REPO -F pr=$PR \
       --jq '.data.repository.pullRequest.reviewThreads.nodes[]
              | select(.isResolved == false and .isOutdated == false)
              | select(.comments.nodes[0].author.login | endswith("[bot]"))'
     ```
   - Review summary bodies (gemini sometimes leaves substantive feedback only here):
     ```sh
     gh api repos/:owner/:repo/pulls/$PR/reviews --jq '.[] | select(.user.login | endswith("[bot]")) | {id, state, body: (.body[:500])}'
     ```
   - For each unaddressed thread/review:
     - Read the cited path/line. Decide: **act** (legitimate bug/nit) or **dismiss** (false positive, out-of-scope, intentional).
     - Acting: edit code. Re-run `just check`. Commit (`fix: address <bot> review on <path>`). Push.
     - Resolve the thread once the fix lands:
       ```sh
       gh api graphql -f query='mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{isResolved}}}' -f id=<threadId>
       ```
     - Dismissing: post a brief reply explaining why, then resolve. Don't leave threads open as "ignored".
   - **Bots COMMENT but never APPROVE.** Don't gate on `reviewDecision == APPROVED`; gate on threads-resolved + CI green.

5. **Fix CI.** Refetch `statusCheckRollup`. For every check that isn't `SUCCESS` or `NEUTRAL`:
   - Pull logs:
     ```sh
     gh run view --log-failed --job=<jobId>
     ```
     `jobId` from `statusCheckRollup[].id` for failed entries.
   - Classify:
     - **Real failure:** code/test bug. Fix locally, `just check`, commit, push.
     - **Transient:** known patterns from the failure-mode catalog (cache reserve race, Playwright 403). Retry via `gh pr close $PR && gh pr reopen $PR` to retrigger the workflow.
     - **Empty CI / only `semgrep` ran:** workflow didn't trigger. Try close+reopen first; if still empty, push an empty commit (`git commit --allow-empty -m "ci: retrigger"`).
   - Loop until all `Rust*` / `UI*` / `Full*` checks are SUCCESS.

6. **Final merge gate.** All of:
   - `state == OPEN`, not draft.
   - `mergeStateStatus == CLEAN` (or `HAS_HOOKS` — both mergeable).
   - All required checks SUCCESS.
   - Zero unresolved-non-outdated bot threads (re-run the GraphQL query from step 4).
   - Title is conventional; body has `Fixes #N` for claimed issues.

7. **Merge.**
   ```sh
   gh pr merge $PR --squash --delete-branch
   ```
   - Branch-deletion errors ("cannot delete branch ... used by worktree at ...") are harmless — the merge succeeded on remote.
   - Verify auto-close: `gh pr view $PR --json closingIssuesReferences`. If body lacked `Fixes #N`, fall back to `gh issue close N --comment "Resolved by PR #$PR"`.

8. **Report.** One block:
   - PR title + URL.
   - Conflicts resolved (count + files).
   - Bot threads acted-on / dismissed.
   - CI fixes (real vs transient).
   - Linked issues closed.
   - Merge SHA from `gh pr view $PR --json mergeCommit --jq .mergeCommit.oid`.

## Notes

- **Why merge-not-rebase in step 3.** Bot review threads anchor to commit SHAs. `git rebase` + force-push detaches them as `outdated`, which both hides feedback and forces the bots to re-review from scratch. `git merge origin/main` preserves history and thread anchors. The user's drain-backlog skill follows the same convention.
- **Mode parity (CLAUDE.md rule #2).** If the PR touches IPC handlers or shared logic, verify the change is wired through Tauri, web, and CLI entry points before merging. Architecture-fitness tests catch some of this; wiring parity is still convention.
- **Feature flag gate (CLAUDE.md rule #6).** New engine/gameplay features must be wrapped in `config.flags.is_enabled("feature-name")` and noted in the PR body. Reject the merge if missing — push back to the PR author or fix it inline.
- **README freshness (CLAUDE.md rule #7).** If the PR adds/removes a feature visible in the feature list or changes deps, ensure `README.md` and `just notices` were run. If not, do it before merging.
- **Out of scope.** This skill lands ONE PR. For multi-PR sweeps, use `drain-backlog`. For rebasing the current branch onto main without merging, use `rebase`.
