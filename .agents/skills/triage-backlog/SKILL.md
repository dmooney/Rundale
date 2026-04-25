---
name: triage-backlog
description: Walk the open-issue backlog, classify each un-triaged issue by theme + priority (P0–P3), and apply labels via the GitHub MCP server. Use when the audit workflow flags un-triaged issues, after a batch of new issues lands, or anytime the user asks for triage.
---

Run a triage pass over open issues that lack a `P*` priority or any theme label. The canonical label vocabulary and rubric live in [`docs/agent/triage-vocabulary.md`](../../../docs/agent/triage-vocabulary.md) — read it before starting.

## Steps

1. **Fetch state in parallel.** Call `mcp__github__list_issues` (state `OPEN`) and `mcp__github__list_pull_requests` (state `open`) for the current repository. Both responses are large — save to disk and use `jq` rather than reading raw output. (The casing difference between the two is intentional — the MCP schemas differ.)

2. **Find the un-PR'd set.** Extract every `#NNN` reference from PR titles + bodies, intersect with open issue numbers. Issues NOT referenced by any open PR are candidates.

3. **Filter to un-triaged.** Keep an issue if it lacks a `P*` priority label **or** lacks any theme label from `triage-vocabulary.md`. Both kinds are reported by the `triage-audit` workflow, so both must be addressable here. Don't relabel issues that already have both unless the user asks for a re-triage.

4. **Classify.** For each remaining issue, read title + body and assign:
   - **Exactly one priority** (`P0`/`P1`/`P2`/`P3`) using the rubric in `triage-vocabulary.md`.
   - **At least one theme** label. Multiple is fine when an issue genuinely spans themes (e.g. `security` + `infra` for a workflow vuln).
   - When uncertain between two priorities, pick the lower-urgency one and let a human escalate.

5. **Apply.** Compute the new label set as **(existing labels with any `P*` priority stripped) + (chosen theme labels) + (chosen priority)** — stripping the old priority is critical so a re-triage doesn't leave both `P1` *and* `P2` on the issue. Pre-existing non-priority labels (`bug`, `security`, `ready-for-test`, `in-progress`, etc.) are preserved. Pass the resulting set to `mcp__github__issue_write` with `method: "update"`. Dispatch in parallel batches of 5–10 to stay clear of GitHub's secondary rate limits.

6. **Verify.** For each priority, make a separate `mcp__github__list_issues` call with `labels: ["P0"]`, then `["P1"]`, then `["P2"]`, then `["P3"]` (four calls — combining priorities in one filter would AND them and return zero). Confirm each count matches what you applied. Random-sample a few issues with `mcp__github__issue_read` (`get_labels`) to confirm theme labels stuck.

7. **Report.** Summarize counts by priority and theme. Link to GitHub filter URLs for the current repository, e.g. `https://github.com/OWNER/REPO/issues?q=is%3Aopen+label%3AP0`. Flag any issue carrying `ready-for-test` without an open PR — those usually need closing, not implementation.

## Notes

- New labels added to `triage-vocabulary.md` are auto-created on first use by `issue_write`, but ship without colors/descriptions. After this skill creates one, set its color in the GitHub UI.
- If a new theme is needed that isn't in the vocabulary, **stop and ask the user** before inventing a label. Update `triage-vocabulary.md` first.
- The `triage-audit` workflow runs weekly and posts a summary listing un-triaged issues — that's the usual trigger for invoking this skill.
