---
name: backlog
description: Work the GitHub issue backlog in three modes — triage (label un-triaged issues by theme + P0–P3), fix-one (take a single issue end-to-end), and drain (multi-wave parallel fix-agent sweep that merges bug-fix PRs as they go green). Trigger for "triage the backlog", "fix issue #N", "drain the backlog", "merge ready PRs", "sweep open PRs", or cleanup after a triage pass. Per Parish convention, bugs ship before enhancements.
argument-hint: 'triage | fix-one <issue#> | drain [scope filter]'
---

One skill for the whole issue lifecycle. Pick the mode:

| Mode | What it does |
|---|---|
| **triage** | Classify un-triaged open issues by theme + priority and apply labels. *Labels* issues. |
| **fix-one** | Take a single issue end-to-end: diagnose → implement → test → commit. |
| **drain** | Multi-wave parallel fix-agent sweep that merges bug-fix PRs as they go green. *Closes* issues. |

The canonical label vocabulary and rubric live in
[`docs/agent/triage-vocabulary.md`](../../../docs/agent/triage-vocabulary.md) — read it before triage or drain.

---

## Mode: triage

Run a triage pass over open issues that lack a `P*` priority or any theme label.

1. **Fetch state in parallel.** Call `mcp__github__list_issues` (state `OPEN`) and
   `mcp__github__list_pull_requests` (state `open`) for the current repository. Both responses are large —
   save to disk and use `jq` rather than reading raw output. (The casing difference is intentional — the MCP
   schemas differ.)

2. **Find the un-PR'd set.** Extract every `#NNN` reference from PR titles + bodies, intersect with open
   issue numbers. Issues NOT referenced by any open PR are candidates.

3. **Filter to un-triaged.** Keep an issue if it lacks a `P*` priority label **or** lacks any theme label
   from `triage-vocabulary.md`. Both are reported by the `triage-audit` workflow. Don't relabel issues that
   already have both unless asked for a re-triage.

4. **Classify.** For each remaining issue, read title + body and assign:
   - **Exactly one priority** (`P0`/`P1`/`P2`/`P3`) using the rubric in `triage-vocabulary.md`.
   - **At least one theme** label. Multiple is fine when an issue genuinely spans themes (e.g. `security` +
     `infra` for a workflow vuln).
   - When uncertain between two priorities, pick the lower-urgency one and let a human escalate.

5. **Apply.** Compute the new label set as **(existing labels with any `P*` priority stripped) + (chosen
   theme labels) + (chosen priority)** — stripping the old priority is critical so a re-triage doesn't leave
   both `P1` *and* `P2`. Pre-existing non-priority labels (`bug`, `security`, `ready-for-test`,
   `in-progress`, etc.) are preserved. Pass the set to `mcp__github__issue_write` with `method: "update"`.
   Dispatch in parallel batches of 5–10 to stay clear of secondary rate limits.

6. **Verify.** For each priority, make a separate `mcp__github__list_issues` call with `labels: ["P0"]`,
   then `["P1"]`, `["P2"]`, `["P3"]` (four calls — combining priorities in one filter ANDs them and returns
   zero). Confirm each count matches what you applied. Random-sample a few issues with
   `mcp__github__issue_read` (`get_labels`) to confirm theme labels stuck.

7. **Report.** Summarize counts by priority and theme. Link to GitHub filter URLs, e.g.
   `https://github.com/OWNER/REPO/issues?q=is%3Aopen+label%3AP0`. Flag any issue carrying `ready-for-test`
   without an open PR — those usually need closing, not implementation.

**Triage notes:**
- New labels added to `triage-vocabulary.md` are auto-created on first use by `issue_write` but ship without
  colors/descriptions. Set the color in the GitHub UI afterward.
- If a new theme is needed that isn't in the vocabulary, **stop and ask the user** before inventing a label.
  Update `triage-vocabulary.md` first.
- The `triage-audit` workflow runs weekly and posts a summary of un-triaged issues — the usual trigger.

---

## Mode: fix-one

Work a single GitHub issue end-to-end. Pass the issue number.

1. If no issue number is given, find one or more GitHub issues that don't already have a pull request.
2. **Fetch the issue**: read its title, body, and labels (`mcp__github__issue_read`).
3. **Understand the problem.** Research the relevant code; identify the root cause or feature gap.
4. **Plan the fix.** Outline which files change and what tests to add. Keep it minimal — only what's needed.
5. **Implement.** Follow project style (cargo fmt, clippy clean, doc comments on public items).
6. **Add tests** covering the new or changed behaviour.
7. **Run checks**: `cd parish && cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`. All pass.
8. **Game harness**: `cargo run -- --script testing/fixtures/test_walkthrough.txt`; verify the JSON output.
9. **Update docs** affected by the change (README, docs/, doc comments).
10. **Commit**: a conventional commit (e.g. `fix: resolve #N — <description>`).
11. **Report** what changed and confirm all checks passed.

---

## Mode: drain

Multi-wave bug-fix cleanup. Cycle: triage → wave-dispatch fix agents → wake-loop sweep → merge as PRs go
green → stop when the bug backlog is clear. This mode *closes* issues; the **triage** mode above *labels*
them. Optional argument scopes the work (e.g. `"P0/P1 only"`, `"frontend bugs"`, `"ignore feat: PRs"`).

1. **Triage the work set.** `gh issue list --state open --limit 100 --json
   number,title,labels,closedByPullRequestsReferences`. Filter to issues with `bug`, `security`, or
   `performance` labels AND no open PR linked. Sort by priority (`P0` first). Defer `enhancement` /
   `scaling` unless the user overrides — bugs ship first. Bundle related issues into single PRs (e.g. all
   Gemini-workflow security issues in one PR; all inference-client bugs in one PR).

2. **Dispatch a wave (≤6 agents in parallel).** For each issue or bundled cluster, spawn one fix agent via
   the `Agent` tool with `subagent_type: general-purpose`, `model: sonnet`, `isolation: worktree`,
   `run_in_background: true`. Each prompt **must** start with the verbatim WORKTREE DISCIPLINE block:

   > **WORKTREE DISCIPLINE — verify pwd contains `/.claude/worktrees/agent-`. Never `cd` to `/Users/dmooney/Parish` or any other worktree. Push only with `git push origin <branch>` — never `HEAD:other-branch`. Do not push to any orchestrator-owned branch. Open new PRs via `gh pr create --base main --head <branch>` and verify a NEW PR number is returned.**

   Then the task: branch name (e.g. `fix/<issue-list>-<topic>`), `Fixes #N, fixes #M.` in PR body for
   auto-close, conventional commit prefix (`fix:` / `security:` / `perf:` / `chore(deps):`), `just check`
   must pass before push, report back pwd / NEW PR number / per-issue summary. For mode-parity bugs, the
   same fix must apply to Tauri, web, and CLI paths (CLAUDE.md rule #2).

3. **Schedule the sweep loop.** Once a wave is dispatched, call `ScheduleWakeup` with `delaySeconds` ≤ 240
   (cache TTL is 5min — staying under keeps the prompt cache warm). The wake prompt is a self-contained
   sweep recipe — pass it verbatim each tick:

   ```sh
   PRS=$(gh pr list --state open --search "author:@me" --limit 30 --json number,title --jq '.[] | "\(.number)\t\(.title[:55])"')
   while IFS=$'\t' read -r pr title; do
     ci=$(gh pr view $pr --json statusCheckRollup --jq '[.statusCheckRollup[] | select(.name | test("Rust|UI|Full"))] | map(.conclusion // "PEND") | group_by(.) | map("\(.[0])=\(length)") | join(",")')
     unr=$(gh api graphql -F owner="$(gh repo view --json owner --jq .owner.login)" -F name="$(gh repo view --json name --jq .name)" -F pr=$pr -f query='query($owner:String!,$name:String!,$pr:Int!){ repository(owner:$owner,name:$name){ pullRequest(number:$pr){ reviewThreads(first:50){ nodes{ isResolved isOutdated comments(first:1){ nodes{ author{ login } } } } } } } }' --jq '[.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved == false and .isOutdated == false) | select(.comments.nodes[0].author.login | endswith("[bot]"))] | length')
     echo "PR $pr | ci:$ci | unr:$unr | $title"
   done <<< "$PRS"
   gh issue list --state open --limit 100 --json number --jq '"open issues: \(length)"'
   ```

   For each PR with `unr > 0`, also read the bot REVIEW BODIES (`gh api repos/:owner/:repo/pulls/$pr/reviews`)
   — gemini sometimes leaves substantive feedback only in the review summary with no inline comments; the
   inline-only filter misses it.

4. **Merge gate.** A PR is mergeable when ALL of:
   - title prefix is `fix:` / `security:` / `perf:` / `bug:` / `chore(deps):` / `fix(scope):`
   - all `Rust*`/`UI*`/`Full*` checks are SUCCESS
   - `unr == 0` (zero unresolved-non-outdated bot threads)

   Then: `gh pr merge <n> --squash --delete-branch`. Branch-deletion errors are harmless when an agent
   worktree still holds the branch — the merge succeeded. Verify auto-close via `gh pr view <n> --json
   closingIssuesReferences`; if the PR body lacked `Fixes #N` syntax, fall back to `gh issue close N
   --comment "Resolved by PR #M"`.

   **Bots COMMENT but never APPROVE.** Don't wait for `state: APPROVED` — gate on thread resolution + CI
   green. The user has explicitly said: "use judgement that comments have been dealt with."

5. **Address review feedback.** For each PR with `unr > 0` or substantive review-body feedback, dispatch a
   sonnet sub-agent (worktree, run_in_background:true) with the WORKTREE DISCIPLINE block to push a fix to
   the existing branch. Constrain it: `Push only with git push origin <existing-branch>`. Do not create a
   new PR. Resolve the thread via the GraphQL `resolveReviewThread` mutation if confidence is high.

6. **Rebase when DIRTY.** If a PR's `mergeStateStatus` is `DIRTY` or `CONFLICTING`, dispatch a rebase agent:
   `git merge origin/main --no-edit`, resolve conflicts (prefer the PR's intent for new code, main's version
   for already-merged work), `just check`, push. Common after sister-PRs merge in the same wave.

7. **Verify-close before working.** Before dispatching a fix, check whether the issue is already fixed in
   tree: `git log --all --oneline -S '<symbol>' -- <path>` plus a grep of the cited code. If yes, `gh issue
   close N --comment "Fixed in commit <sha> — <one-line evidence>. (Stale ready-for-test label.)"` — saves a
   wasted PR. Common for `ready-for-test`-labeled issues.

8. **Stop conditions.** When all bug-fix PRs are merged AND no new bug issues filed → schedule one final
   ~240s confirm-stable wake, then stop scheduling. If the user signals a usage cap ("5hr window
   approaching"), schedule a long-delay wake (e.g. 3600s) and stop initiating new work.

### Drain failure-mode catalog

Patterns burned-in across two long sessions on this repo. Reference, not procedure.

- **Chimera PR.** When an agent's worktree-discipline fails, multiple agents push to the same branch and the
  PR accumulates unrelated commits. *Recovery:* split via cherry-pick onto fresh branches, or rewrite the PR
  title/body to acknowledge the bundle. *Prevention:* the WORKTREE DISCIPLINE block in step 2.

- **Codex out of usage.** If codex stops responding mid-cleanup, gemini still works. Don't block on codex
  re-review; use judgement.

- **CI transient failures (false positives, retry).**
  - Cache reserve race: `Failed to save: Unable to reserve cache with key v0-rust-...`
  - Playwright artifact upload 403: `Upload Playwright report ... Failed request: (403) Forbidden: job is completed`

  Both clear via `gh pr close <n> && gh pr reopen <n>` to retrigger.

- **Empty CI on a PR (only `semgrep` ran).** Workflow didn't trigger. Try close+reopen first; if still
  empty, the branch likely needs a fresh non-bot commit.

- **Dependabot CI doesn't auto-fire.** GitHub's dependabot branch security policy blocks workflows even
  after a non-bot commit. Workaround:
  1. Push an empty commit to the dependabot branch: `git commit --allow-empty -m "ci: retrigger" && git push origin <branch>`
  2. Manually dispatch via `gh workflow run ci.yml --ref <branch>`
  3. The dispatched run shows green but doesn't update the PR's check-rollup
  4. After verifying success: `gh pr merge <n> --squash --delete-branch --admin`

- **DIRTY merge state.** Main moved while the PR was in flight. See step 6.

- **Local branch-delete after merge fails.** "cannot delete branch ... used by worktree at ..." — harmless.
  The PR merged on remote; only the local branch deletion failed because an agent worktree still has it
  checked out.

### Drain notes

- Triage-vocabulary is in `docs/agent/triage-vocabulary.md`. CLAUDE.md project rules (esp. mode parity #2
  and feature-flag gating #6) apply to every fix.
- The orchestrator's own worktree must NOT be a worktree any sub-agent can write to. The
  `claude/eloquent-murdock-*` style branch this skill runs from is off-limits to sub-agents.
- Wake intervals: ≤ 240s during active work (cache-warm), 3600s when the user signals usage conservation, a
  single confirm-stable wake when work is done.
