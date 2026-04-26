---
name: drain-backlog
description: Multi-wave bug-fix backlog cleanup. Triage open bug/security/perf issues, dispatch parallel fix agents in worktrees, monitor PR CI/reviews via ScheduleWakeup, merge bug-fix PRs as they go green. Trigger when the user says "drain the backlog", "merge ready PRs", "sweep open PRs", "fix the bug backlog", or asks to clean up after a triage pass. Per Parish convention, bugs ship before enhancements.
argument-hint: 'optional scope filter (e.g. "P0/P1 only", "frontend bugs", "ignore feat: PRs")'
---

Cycle: triage → wave-dispatch fix agents → wake-loop sweep → merge as PRs go green → stop when the bug backlog is clear.

The canonical triage-vocabulary lives in [`docs/agent/triage-vocabulary.md`](../../../docs/agent/triage-vocabulary.md). The `triage-backlog` skill labels issues; this skill *closes* them.

## Steps

1. **Triage the work set.** `gh issue list --state open --limit 100 --json number,title,labels,closedByPullRequestsReferences`. Filter to issues with `bug`, `security`, or `performance` labels AND no open PR linked. Sort by priority (`P0` first). Defer `enhancement` / `scaling` unless the user overrides — bugs ship first. Bundle related issues into single PRs (e.g. all Gemini-workflow security issues in one PR; all inference-client bugs in one PR).

2. **Dispatch a wave (≤6 agents in parallel).** For each issue or bundled cluster, spawn one fix agent via the `Agent` tool with `subagent_type: general-purpose`, `model: sonnet`, `isolation: worktree`, `run_in_background: true`. Each prompt **must** start with the verbatim WORKTREE DISCIPLINE block:

   > **WORKTREE DISCIPLINE — verify pwd contains `/.claude/worktrees/agent-`. Never `cd` to `/Users/dmooney/Parish` or any other worktree. Push only with `git push origin <branch>` — never `HEAD:other-branch`. Do not push to any orchestrator-owned branch. Open new PRs via `gh pr create --base main --head <branch>` and verify a NEW PR number is returned.**

   Then the task: branch name (e.g. `fix/<issue-list>-<topic>`), `Fixes #N, fixes #M.` in PR body for auto-close, conventional commit prefix (`fix:` / `security:` / `perf:` / `chore(deps):`), `just check` must pass before push, report back pwd / NEW PR number / per-issue summary. For mode-parity bugs, the same fix must apply to Tauri, web, and CLI paths (CLAUDE.md rule #2).

3. **Schedule the sweep loop.** Once a wave is dispatched, call `ScheduleWakeup` with `delaySeconds` ≤ 240 (cache TTL is 5min — staying under keeps the prompt cache warm). The wake prompt is a self-contained sweep recipe — pass it verbatim each tick:

   ```sh
   PRS=$(gh pr list --state open --search "author:@me" --limit 30 --json number,title --jq '.[] | "\(.number)\t\(.title[:55])"')
   while IFS=$'\t' read -r pr title; do
     ci=$(gh pr view $pr --json statusCheckRollup --jq '[.statusCheckRollup[] | select(.name | test("Rust|UI|Full"))] | map(.conclusion // "PEND") | group_by(.) | map("\(.[0])=\(length)") | join(",")')
     unr=$(gh api graphql -f query="query { repository(owner: \"dmooney\", name: \"Parish\") { pullRequest(number: $pr) { reviewThreads(first: 50) { nodes { isResolved isOutdated comments(first: 1) { nodes { author { login } } } } } } } }" --jq '[.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved == false and .isOutdated == false) | select(.comments.nodes[0].author.login | endswith("[bot]"))] | length')
     echo "PR $pr | ci:$ci | unr:$unr | $title"
   done <<< "$PRS"
   gh issue list --state open --limit 100 --json number --jq '"open issues: \(length)"'
   ```

   For each PR with `unr > 0`, also read the bot REVIEW BODIES (`gh api repos/:owner/:repo/pulls/$pr/reviews`) — gemini sometimes leaves substantive feedback only in the review summary with no inline comments; the inline-only filter misses it.

4. **Merge gate.** A PR is mergeable when ALL of:
   - title prefix is `fix:` / `security:` / `perf:` / `bug:` / `chore(deps):` / `fix(scope):`
   - all `Rust*`/`UI*`/`Full*` checks are SUCCESS
   - `unr == 0` (zero unresolved-non-outdated bot threads)

   Then: `gh pr merge <n> --squash --delete-branch`. Branch-deletion errors are harmless when an agent worktree still holds the branch — the merge succeeded. Verify auto-close via `gh pr view <n> --json closingIssuesReferences`; if the PR body lacked `Fixes #N` syntax, fall back to `gh issue close N --comment "Resolved by PR #M"`.

   **Bots COMMENT but never APPROVE.** Don't wait for `state: APPROVED` — gate on thread resolution + CI green. The user has explicitly said: "use judgement that comments have been dealt with."

5. **Address review feedback.** For each PR with `unr > 0` or with substantive review-body feedback, dispatch a sonnet sub-agent (worktree, run_in_background:true) with the WORKTREE DISCIPLINE block to push a fix to the existing branch. Constrain the agent: `Push only with git push origin <existing-branch>`. Do not create a new PR. Resolve the thread via the GraphQL `resolveReviewThread` mutation if confidence is high that the fix landed.

6. **Rebase when DIRTY.** If a PR's `mergeStateStatus` is `DIRTY` or `CONFLICTING`, dispatch a rebase agent: `git merge origin/main --no-edit`, resolve conflicts (prefer the PR's intent for new code, main's version for already-merged work), `just check`, push. Common after sister-PRs merge in the same wave.

7. **Verify-close before working.** Before dispatching a fix for an issue, check whether it's already fixed in tree: `git log --all --oneline -S '<symbol>' -- <path>` plus a grep of the cited code. If yes, `gh issue close N --comment "Fixed in commit <sha> — <one-line evidence>. (Stale ready-for-test label.)"` — saves a wasted PR. Common for `ready-for-test`-labeled issues.

8. **Stop conditions.** When all bug-fix PRs are merged AND no new bug issues filed → schedule one final ~240s confirm-stable wake, then stop scheduling. If the user signals a usage cap ("5hr window approaching"), schedule a long-delay wake (e.g. 3600s) and stop initiating new work.

## Failure-mode catalog

The bench notes — patterns burned-in across two long sessions on this repo. Reference, not procedure.

- **Chimera PR.** When an agent's worktree-discipline fails, multiple agents push to the same branch and the PR accumulates unrelated commits. *Recovery:* split via cherry-pick onto fresh branches, or rewrite the PR title/body to acknowledge the bundle. *Prevention:* the WORKTREE DISCIPLINE block in step 2.

- **Codex out of usage.** If codex stops responding mid-cleanup, gemini still works. Don't block on codex re-review; use judgement.

- **CI transient failures (false positives, retry).**
  - Cache reserve race: `Failed to save: Unable to reserve cache with key v0-rust-...`
  - Playwright artifact upload 403: `Upload Playwright report ... Failed request: (403) Forbidden: job is completed`
  
  Both clear via `gh pr close <n> && gh pr reopen <n>` to retrigger.

- **Empty CI on a PR (only `semgrep` ran).** Workflow didn't trigger. Try close+reopen first; if still empty, the branch likely needs a fresh non-bot commit.

- **Dependabot CI doesn't auto-fire.** GitHub's dependabot branch security policy blocks workflows even after a non-bot commit. Workaround:
  1. Push an empty commit to the dependabot branch: `git commit --allow-empty -m "ci: retrigger" && git push origin <branch>`
  2. Manually dispatch via `gh workflow run ci.yml --ref <branch>`
  3. The dispatched run shows green but doesn't update the PR's check-rollup
  4. After verifying success: `gh pr merge <n> --squash --delete-branch --admin`

- **DIRTY merge state.** Main moved while the PR was in flight. See step 6.

- **Local branch-delete after merge fails.** "cannot delete branch ... used by worktree at ..." — harmless. The PR merged on remote; only the local branch deletion failed because an agent worktree still has it checked out.

## Notes

- Triage-vocabulary is in `docs/agent/triage-vocabulary.md`. CLAUDE.md project rules (esp. mode parity #2 and feature flag gating #6) apply to every fix.
- The orchestrator's own worktree must NOT be a worktree any sub-agent can write to. The `claude/eloquent-murdock-*` style branch this skill runs from is off-limits to sub-agents.
- Wake intervals: ≤ 240s during active work (cache-warm), 3600s when user signals usage conservation, single confirm-stable wake when work is done.
- This skill closes issues; the sister `triage-backlog` skill labels them.
