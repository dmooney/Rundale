#!/usr/bin/env python3
"""Daily repo health watcher: find failures and stalls that have no path to a person.

Run by `.github/workflows/repo-health.yml` (daily and on demand) with the
workflow's `GITHUB_TOKEN` (`issues: write`, `actions: read`,
`pull-requests: read`, `contents: read`); nothing here depends on the owner's
Mac. It checks:

1. Workflows red on `main`. Each active workflow's latest completed run on
   `main` (push or schedule; cancelled and skipped runs carry no verdict) that
   failed gets one open issue labelled `ci-red`, carrying
   `<!-- ci-red: workflow=<file> -->` so the self-repairing upkeep routine
   (issue #2342) and this watcher find it by the workflow file name. The issue
   is closed with a comment when the workflow passes again. A workflow whose own
   failure issue already exists under a managed title (the TestFlight workflow's
   "Nightly mobile-verify failed") gets no second issue.
2. Open non-draft pull requests that are red for over 24 hours, green and
   unreviewed for over 24 hours, or carry a gatekeeper `approve` marker for
   their current head and are still unmerged after 2 hours. Drafts are parked on
   purpose and skipped. `needs-owner` pull requests wait on the owner and are
   listed separately.
3. The weekly Dependabot upkeep routine (a report comment on issue #2252) not
   reporting for 8 days.
4. Open failure-tracking issues with no activity for 3 days.

It then posts one comment per run to the pinned "Repo health" issue (created
and pinned the first time, found by a marker afterwards) and mentions the
owner only when something needs them: a `needs-owner` pull request, a stopped
routine, or a problem older than its escalation threshold. A quiet day posts
without a notification.

    repo_health.py run [--dry-run]

`--dry-run` only reads: it prints the issue changes and the summary it would
post. Reads go through `gh api`; text from titles and comments is only ever put
in JSON request bodies and Markdown code spans, never into a shell command.
The logic below `class GhClient` is pure and covered by
`scripts/test_repo_health.py`.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any, Protocol

CI_RED_LABEL = "ci-red"
HEALTH_LABEL = "repo-health"
NEEDS_OWNER_LABEL = "needs-owner"
HEALTH_TITLE = "Repo health"
HEALTH_MARKER = "<!-- repo-health-summary -->"
CI_RED_MARKER = re.compile(r"<!--\s*ci-red:\s*workflow=([^\s>]+)\s*-->")
CI_RED_RUN_MARKER = re.compile(r"<!--\s*ci-red-run:\s*(\d+)\s*-->")
GATEKEEPER_MARKER = re.compile(
    r"<!--\s*gatekeeper:\s*sha=([0-9a-fA-F]+)\s+verdict=(\w+)(?:\s+round=(\d+))?"
)
DEPENDABOT_ISSUE = 2252
OWNER = "dmooney"

# Failure issues another workflow opens and closes itself, by workflow file.
MANAGED_TITLES = {"testflight.yml": "Nightly mobile-verify failed"}

RED_CONCLUSIONS = {"failure", "timed_out", "startup_failure"}
VERDICT_EVENTS = {"push", "schedule"}
FAILED_CHECKS = {"FAILURE", "TIMED_OUT", "STARTUP_FAILURE", "ERROR"}

PR_RED_HOURS = 24
PR_UNREVIEWED_HOURS = 24
PR_APPROVED_HOURS = 2
ROUTINE_SILENT_DAYS = 8
ISSUE_STALE_DAYS = 3
# A finding older than this many hours mentions the owner.
ESCALATE_HOURS = {
    "ci_red": 72,
    "pr_red": 72,
    "pr_unreviewed": 72,
    "pr_approved": 24,
    "routine": 0,
    "stale_issue": 168,
}
WATCHER_COMMENT_MARKER = "<!-- repo-health-watcher -->"


def parse_time(text: str) -> datetime:
    return datetime.fromisoformat(text.replace("Z", "+00:00"))


def hours_between(then: datetime, now: datetime) -> float:
    return (now - then).total_seconds() / 3600


def span(text: Any) -> str:
    """Render untrusted text as a Markdown code span that cannot ping or break out."""
    flat = " ".join(str(text).replace("`", "'").split())
    return f"`{flat[:120]}`"


def age_text(hours: float) -> str:
    if hours < 48:
        return f"{int(hours)}h"
    return f"{int(hours // 24)}d"


@dataclass(frozen=True)
class Finding:
    kind: str
    text: str
    age_hours: float

    @property
    def escalates(self) -> bool:
        return self.age_hours >= ESCALATE_HOURS[self.kind]


# --- workflows red on main ---------------------------------------------------


@dataclass(frozen=True)
class WorkflowState:
    file: str
    name: str
    status: str  # "red", "green", or "unknown" (no verdict run yet)
    run: dict[str, Any] | None
    since: str = ""  # created_at of the oldest run in the current red streak


def workflow_file(path: str) -> str | None:
    prefix = ".github/workflows/"
    return path[len(prefix) :] if path.startswith(prefix) else None


def classify_workflows(
    workflows: list[dict[str, Any]],
    runs_by_id: dict[int, list[dict[str, Any]]],
    present: set[str],
) -> list[WorkflowState]:
    """Each active workflow's verdict: its newest completed push/schedule run on main.

    GitHub keeps listing a deleted workflow file as active, so only files that
    exist on main (`present`) count.
    """
    states = []
    for wf in workflows:
        file = workflow_file(wf.get("path", ""))
        if file is None or file not in present or wf.get("state") != "active":
            continue
        verdicts = [
            run
            for run in runs_by_id.get(wf["id"], [])
            if run.get("head_branch") == "main"
            and run.get("event") in VERDICT_EVENTS
            and run.get("status") == "completed"
            and (run.get("conclusion") == "success" or run.get("conclusion") in RED_CONCLUSIONS)
        ]
        verdicts.sort(key=lambda run: run["created_at"], reverse=True)
        if not verdicts:
            states.append(WorkflowState(file, wf["name"], "unknown", None))
        else:
            run = verdicts[0]
            status = "green" if run["conclusion"] == "success" else "red"
            streak = []
            for older in verdicts:
                if older["conclusion"] == "success":
                    break
                streak.append(older)
            states.append(
                WorkflowState(
                    file, wf["name"], status, run, streak[-1]["created_at"] if streak else ""
                )
            )
    return states


def failing_jobs(jobs: list[dict[str, Any]]) -> list[str]:
    lines = []
    for job in jobs:
        if job.get("conclusion") not in RED_CONCLUSIONS:
            continue
        steps = [
            step["name"]
            for step in job.get("steps", [])
            if step.get("conclusion") in RED_CONCLUSIONS
        ]
        detail = f" (failed step: {', '.join(span(s) for s in steps)})" if steps else ""
        lines.append(f"- [{span(job['name'])}]({job.get('html_url', '')}){detail}")
    return lines[:20]


def render_ci_red_body(state: WorkflowState, jobs: list[dict[str, Any]]) -> str:
    run = state.run or {}
    lines = [
        f"<!-- ci-red: workflow={state.file} -->",
        f"<!-- ci-red-run: {run.get('id', 0)} -->",
        f"The {span(state.name)} workflow ({span(state.file)}) failed on its latest completed "
        "run on `main`.",
        "",
        f"- Run: {run.get('html_url', '')} ({run.get('event', '?')}, "
        f"commit {span(str(run.get('head_sha', ''))[:9])}, {run.get('created_at', '?')})",
        "- Failing jobs:",
        *(f"  {line}" for line in (failing_jobs(jobs) or ["- (job details unavailable)"])),
        "",
        "Opened by the repo health watcher ([`scripts/repo_health.py`]"
        "(../blob/main/scripts/repo_health.py)). It updates this description as the failing "
        "run changes and closes the issue when the workflow passes on `main`. Fix the cause "
        "and close the process gap that let it sit, per the AGENTS.md rule.",
    ]
    return "\n".join(lines)


@dataclass(frozen=True)
class IssueAction:
    op: str  # "create", "update", "close", "tracked"
    workflow: str
    number: int | None = None
    title: str = ""
    body: str = ""
    comment: str = ""


def plan_ci_red(
    states: list[WorkflowState],
    open_issues: list[dict[str, Any]],
    jobs_for_run: Callable[[int], list[dict[str, Any]]],
    active_files: set[str],
) -> list[IssueAction]:
    existing: dict[str, dict[str, Any]] = {}
    for issue in open_issues:
        if CI_RED_LABEL not in label_names(issue):
            continue
        match = CI_RED_MARKER.search(issue.get("body") or "")
        if match:
            existing.setdefault(match.group(1), issue)
    titles = {issue["title"]: issue for issue in open_issues}
    actions: list[IssueAction] = []
    seen = set()
    for state in states:
        seen.add(state.file)
        current = existing.get(state.file)
        if state.status == "red":
            managed = titles.get(MANAGED_TITLES.get(state.file, ""))
            if current is None and managed is not None:
                actions.append(IssueAction("tracked", state.file, managed["number"]))
                continue
            run = state.run or {}
            body = render_ci_red_body(
                state, jobs_for_run(run["id"]) if issue_needs(current, run) else []
            )
            if current is None:
                title = f"CI red on main: {state.name} ({state.file})"
                actions.append(IssueAction("create", state.file, title=title, body=body))
            elif issue_needs(current, run):
                actions.append(IssueAction("update", state.file, current["number"], body=body))
        elif state.status == "green" and current is not None:
            run = state.run or {}
            comment = f"{span(state.name)} passed on `main` again: {run.get('html_url', '')}"
            actions.append(IssueAction("close", state.file, current["number"], comment=comment))
    for file, issue in existing.items():
        if file not in seen and file not in active_files:
            comment = f"The {span(file)} workflow is no longer active, so nothing can fix this."
            actions.append(IssueAction("close", file, issue["number"], comment=comment))
    return actions


def issue_needs(issue: dict[str, Any] | None, run: dict[str, Any]) -> bool:
    """True when the issue is missing or describes a different run than the latest."""
    if issue is None:
        return True
    match = CI_RED_RUN_MARKER.search(issue.get("body") or "")
    return match is None or int(match.group(1)) != run.get("id")


def label_names(item: dict[str, Any]) -> set[str]:
    return {lab["name"] if isinstance(lab, dict) else lab for lab in item.get("labels", [])}


def red_workflow_findings(states: list[WorkflowState], now: datetime) -> list[Finding]:
    findings = []
    for state in states:
        if state.status != "red" or state.run is None:
            continue
        age = hours_between(parse_time(state.since or state.run["created_at"]), now)
        findings.append(
            Finding(
                "ci_red",
                f"{span(state.name)} ({span(state.file)}) red since "
                f"{age_text(age)} ago: {state.run.get('html_url', '')}",
                age,
            )
        )
    return findings


# --- pull requests -----------------------------------------------------------


@dataclass
class PrReport:
    red: list[Finding] = field(default_factory=list)
    unreviewed: list[Finding] = field(default_factory=list)
    approved: list[Finding] = field(default_factory=list)
    needs_owner: list[Finding] = field(default_factory=list)


def check_state(commit: dict[str, Any]) -> tuple[str, datetime]:
    """The head commit's checks as (red|pending|green|none, when that state began)."""
    committed = parse_time(commit["committedDate"])
    rollup = commit.get("statusCheckRollup")
    if not rollup:
        return "none", committed
    failed: list[datetime] = []
    done: list[datetime] = []
    pending = False
    for ctx in (rollup.get("contexts") or {}).get("nodes") or []:
        if ctx.get("__typename") == "CheckRun":
            if ctx.get("status") != "COMPLETED":
                pending = True
                continue
            when = parse_time(ctx["completedAt"]) if ctx.get("completedAt") else committed
            (failed if ctx.get("conclusion") in FAILED_CHECKS else done).append(when)
        else:
            state = ctx.get("state")
            when = parse_time(ctx["createdAt"]) if ctx.get("createdAt") else committed
            if state in FAILED_CHECKS:
                failed.append(when)
            elif state in {"PENDING", "EXPECTED"}:
                pending = True
            else:
                done.append(when)
    if failed:
        return "red", min(failed)
    if pending:
        return "pending", committed
    return "green", max([committed, *done])


def gatekeeper_markers(pr: dict[str, Any]) -> list[tuple[datetime, str, str, str]]:
    """(time, sha, verdict, author) for each gatekeeper marker in reviews and comments."""
    found = []
    items = [(r.get("body"), r.get("submittedAt"), r.get("author")) for r in nodes(pr, "reviews")]
    items += [(c.get("body"), c.get("createdAt"), c.get("author")) for c in nodes(pr, "comments")]
    for body, when, author in items:
        match = GATEKEEPER_MARKER.search(body or "")
        if match and when:
            found.append(
                (
                    parse_time(when),
                    match.group(1).lower(),
                    match.group(2),
                    (author or {}).get("login", ""),
                )
            )
    found.sort(key=lambda m: m[0])
    return found


def nodes(pr: dict[str, Any], key: str) -> list[dict[str, Any]]:
    return (pr.get(key) or {}).get("nodes") or []


def sha_matches(marker_sha: str, head: str) -> bool:
    head = head.lower()
    return bool(marker_sha) and (head.startswith(marker_sha) or marker_sha.startswith(head))


def reviewed_at_head(pr: dict[str, Any], head: str) -> bool:
    author = (pr.get("author") or {}).get("login")
    for review in nodes(pr, "reviews"):
        reviewer = (review.get("author") or {}).get("login")
        commit = (review.get("commit") or {}).get("oid", "")
        if reviewer != author and review.get("state") != "PENDING" and commit == head:
            return True
    return any(sha_matches(sha, head) for _, sha, _, _ in gatekeeper_markers(pr))


def analyze_prs(prs: list[dict[str, Any]], now: datetime) -> PrReport:
    report = PrReport()
    for pr in prs:
        if pr.get("isDraft"):
            continue
        number = pr["number"]
        ref = f"PR {number} {span(pr['title'])} {pr['url']}"
        age = hours_between(parse_time(pr["createdAt"]), now)
        if NEEDS_OWNER_LABEL in {lab["name"] for lab in nodes(pr, "labels")}:
            report.needs_owner.append(Finding("pr_red", f"{ref}, open {age_text(age)}", age))
            continue
        commits = nodes(pr, "commits")
        if not commits:
            continue
        head = pr.get("headRefOid") or commits[-1]["commit"]["oid"]
        state, since = check_state(commits[-1]["commit"])
        state_age = hours_between(since, now)
        if state == "red" and state_age > PR_RED_HOURS:
            report.red.append(Finding("pr_red", f"{ref}, red for {age_text(state_age)}", state_age))
        reviewed = reviewed_at_head(pr, head)
        if state == "green" and not reviewed and state_age > PR_UNREVIEWED_HOURS:
            report.unreviewed.append(
                Finding(
                    "pr_unreviewed",
                    f"{ref}, green and unreviewed for {age_text(state_age)}",
                    state_age,
                )
            )
        markers = gatekeeper_markers(pr)
        if markers:
            when, sha, verdict, _ = markers[-1]
            approved_age = hours_between(when, now)
            if verdict == "approve" and sha_matches(sha, head) and approved_age > PR_APPROVED_HOURS:
                report.approved.append(
                    Finding(
                        "pr_approved",
                        f"{ref}, gatekeeper approved its head {age_text(approved_age)} ago, still unmerged",
                        approved_age,
                    )
                )
    return report


# --- routines and stale issues -----------------------------------------------


def first_line(text: str | None) -> str:
    lines = (text or "").strip().splitlines()
    return lines[0] if lines else ""


def routine_finding(comments: list[dict[str, Any]], now: datetime) -> Finding | None:
    """A finding when no Dependabot upkeep report was posted within ROUTINE_SILENT_DAYS."""
    reports = [
        parse_time(c["created_at"])
        for c in comments
        if "dependabot upkeep" in first_line(c.get("body")).lower()
    ]
    if reports:
        age = hours_between(max(reports), now)
        if age <= ROUTINE_SILENT_DAYS * 24:
            return None
        text = f"last upkeep report on issue {DEPENDABOT_ISSUE} was {age_text(age)} ago"
        return Finding(
            "routine", f"The Dependabot upkeep routine has stopped reporting: {text}", age
        )
    return Finding(
        "routine",
        f"The Dependabot upkeep routine has stopped reporting: no report in issue {DEPENDABOT_ISSUE}",
        ROUTINE_SILENT_DAYS * 24.0,
    )


def tracking_issues(open_issues: list[dict[str, Any]]) -> list[dict[str, Any]]:
    managed = set(MANAGED_TITLES.values())
    return [i for i in open_issues if CI_RED_LABEL in label_names(i) or i["title"] in managed]


def stale_issue_findings(
    issues: list[dict[str, Any]], comments_by_number: dict[int, list[dict[str, Any]]], now: datetime
) -> list[Finding]:
    findings = []
    for issue in issues:
        times = [parse_time(issue["created_at"])]
        times += [
            parse_time(c["created_at"])
            for c in comments_by_number.get(issue["number"], [])
            if WATCHER_COMMENT_MARKER not in (c.get("body") or "")
        ]
        age = hours_between(max(times), now)
        if age >= ISSUE_STALE_DAYS * 24:
            findings.append(
                Finding(
                    "stale_issue",
                    f"Issue {issue['number']} {span(issue['title'])} {issue['html_url']} "
                    f"has had no activity for {age_text(age)}",
                    age,
                )
            )
    return findings


# --- summary -----------------------------------------------------------------


@dataclass
class Summary:
    body: str
    needs_owner: bool


def section(title: str, items: list[Finding] | list[str], empty: str = "None.") -> list[str]:
    lines = [f"### {title}", ""]
    if not items:
        return [*lines, empty, ""]
    for item in items:
        lines.append(f"- {item.text if isinstance(item, Finding) else item}")
    return [*lines, ""]


def render_summary(
    now: datetime,
    red_workflows: list[Finding],
    tracked: list[tuple[str, int]],
    prs: PrReport,
    routine: Finding | None,
    stale: list[Finding],
    errors: list[str],
    owner: str = OWNER,
) -> Summary:
    everything = [
        *red_workflows,
        *prs.red,
        *prs.unreviewed,
        *prs.approved,
        *([routine] if routine else []),
        *stale,
    ]
    reasons = []
    if prs.needs_owner:
        reasons.append(f"{len(prs.needs_owner)} pull request(s) labelled `needs-owner`")
    reasons += [f.text for f in everything if f.escalates]
    problems = len(everything) + len(prs.needs_owner) + len(errors)
    head = f"## Repo health {now:%Y-%m-%d %H:%M} UTC"
    lines = [WATCHER_COMMENT_MARKER, head, ""]
    if reasons:
        lines += [f"@{owner} this needs you:", *(f"- {r}" for r in reasons), ""]
    lines.append("All clear." if problems == 0 else f"{problems} problem(s) found.")
    lines.append("")
    lines += section("Workflows red on main", red_workflows, "None.")
    if tracked:
        lines += section(
            "Red workflows tracked by their own issue",
            [f"{span(wf)} is tracked by issue {number}" for wf, number in tracked],
        )
    lines += section("Waiting on the owner (`needs-owner`)", prs.needs_owner, "None.")
    lines += section("Pull requests red over 24 hours", prs.red, "None.")
    lines += section("Pull requests green and unreviewed over 24 hours", prs.unreviewed, "None.")
    lines += section(
        "Gatekeeper-approved pull requests unmerged over 2 hours", prs.approved, "None."
    )
    lines += section(
        "Scheduled routines", [routine] if routine else [], "Dependabot upkeep is reporting."
    )
    lines += section("Failure issues with no activity for 3 days", stale, "None.")
    if errors:
        lines += section("Watcher errors", errors)
    lines.append(
        "Posted by the repo health watcher ([`scripts/repo_health.py`]"
        "(../blob/main/scripts/repo_health.py)). The owner is mentioned only when something "
        "needs them."
    )
    return Summary("\n".join(lines), bool(reasons))


# --- GitHub I/O --------------------------------------------------------------


class Client(Protocol):
    def workflows(self) -> list[dict[str, Any]]: ...
    def workflow_files(self) -> set[str]: ...
    def workflow_runs(self, workflow_id: int) -> list[dict[str, Any]]: ...
    def run_jobs(self, run_id: int) -> list[dict[str, Any]]: ...
    def open_issues(self) -> list[dict[str, Any]]: ...
    def issue_comments(
        self, number: int, since: datetime | None = None
    ) -> list[dict[str, Any]]: ...
    def open_prs(self) -> list[dict[str, Any]]: ...
    def ensure_label(self, name: str, color: str, description: str) -> None: ...
    def create_issue(self, title: str, body: str, labels: list[str]) -> dict[str, Any]: ...
    def update_issue(self, number: int, body: str) -> None: ...
    def comment(self, number: int, body: str) -> None: ...
    def close_issue(self, number: int) -> None: ...
    def pin_issue(self, node_id: str) -> None: ...


PR_QUERY = """
query($owner: String!, $name: String!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 50, after: $after) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title url isDraft createdAt headRefOid
        author { login }
        labels(first: 20) { nodes { name } }
        reviews(first: 50) { nodes { author { login } body submittedAt state commit { oid } } }
        comments(last: 50) { nodes { author { login } body createdAt } }
        commits(last: 1) { nodes { commit {
          oid committedDate
          statusCheckRollup { state contexts(first: 100) { nodes {
            __typename
            ... on CheckRun { name status conclusion completedAt }
            ... on StatusContext { context state createdAt }
          } } }
        } } }
      }
    }
  }
}
"""


class GhClient:
    """The `gh api` implementation of `Client`; arguments are passed as a list, never a shell."""

    def __init__(self, repo: str) -> None:
        self.repo = repo
        self.owner, self.name = repo.split("/", 1)

    def _api(self, args: list[str], payload: dict[str, Any] | None = None) -> Any:
        cmd = ["gh", "api", *args]
        if payload is not None:
            cmd += ["--input", "-"]
        proc = subprocess.run(
            cmd,
            input=json.dumps(payload) if payload is not None else "",
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(f"{' '.join(cmd[:4])} failed: {proc.stderr.strip()[:300]}")
        return json.loads(proc.stdout) if proc.stdout.strip() else None

    def _get_pages(self, path: str, key: str | None = None, **params: str) -> list[dict[str, Any]]:
        items: list[dict[str, Any]] = []
        for page in range(1, 51):
            args = ["-X", "GET", path, "-f", "per_page=100", "-f", f"page={page}"]
            for name, value in params.items():
                args += ["-f", f"{name}={value}"]
            data = self._api(args)
            batch = data[key] if key else data
            items += batch
            if len(batch) < 100:
                break
        return items

    def workflows(self) -> list[dict[str, Any]]:
        return self._get_pages(f"repos/{self.repo}/actions/workflows", "workflows")

    def workflow_files(self) -> set[str]:
        data = self._api(
            ["-X", "GET", f"repos/{self.repo}/contents/.github/workflows", "-f", "ref=main"]
        )
        return {item["name"] for item in data if item["type"] == "file"}

    def workflow_runs(self, workflow_id: int) -> list[dict[str, Any]]:
        data = self._api(
            ["-X", "GET", f"repos/{self.repo}/actions/workflows/{workflow_id}/runs",
             "-f", "branch=main", "-f", "status=completed", "-f", "per_page=30"]
        )  # fmt: skip
        return data["workflow_runs"]

    def run_jobs(self, run_id: int) -> list[dict[str, Any]]:
        data = self._api(
            ["-X", "GET", f"repos/{self.repo}/actions/runs/{run_id}/jobs",
             "-f", "filter=latest", "-f", "per_page=100"]
        )  # fmt: skip
        return data["jobs"]

    def open_issues(self) -> list[dict[str, Any]]:
        issues = self._get_pages(f"repos/{self.repo}/issues", state="open")
        return [i for i in issues if "pull_request" not in i]

    def issue_comments(self, number: int, since: datetime | None = None) -> list[dict[str, Any]]:
        params = {"since": since.strftime("%Y-%m-%dT%H:%M:%SZ")} if since else {}
        return self._get_pages(f"repos/{self.repo}/issues/{number}/comments", **params)

    def open_prs(self) -> list[dict[str, Any]]:
        prs: list[dict[str, Any]] = []
        after = None
        while True:
            args = ["graphql", "-f", f"query={PR_QUERY}", "-f", f"owner={self.owner}",
                    "-f", f"name={self.name}"]  # fmt: skip
            if after:
                args += ["-f", f"after={after}"]
            data = self._api(args)["data"]["repository"]["pullRequests"]
            prs += data["nodes"]
            if not data["pageInfo"]["hasNextPage"]:
                return prs
            after = data["pageInfo"]["endCursor"]

    def ensure_label(self, name: str, color: str, description: str) -> None:
        try:
            self._api(
                ["-X", "POST", f"repos/{self.repo}/labels"],
                {"name": name, "color": color, "description": description},
            )
        except RuntimeError as err:
            if "already_exists" not in str(err) and "422" not in str(err):
                raise

    def create_issue(self, title: str, body: str, labels: list[str]) -> dict[str, Any]:
        return self._api(
            ["-X", "POST", f"repos/{self.repo}/issues"],
            {"title": title, "body": body, "labels": labels},
        )

    def update_issue(self, number: int, body: str) -> None:
        self._api(["-X", "PATCH", f"repos/{self.repo}/issues/{number}"], {"body": body})

    def comment(self, number: int, body: str) -> None:
        self._api(["-X", "POST", f"repos/{self.repo}/issues/{number}/comments"], {"body": body})

    def close_issue(self, number: int) -> None:
        self._api(
            ["-X", "PATCH", f"repos/{self.repo}/issues/{number}"],
            {"state": "closed", "state_reason": "completed"},
        )

    def pin_issue(self, node_id: str) -> None:
        mutation = "mutation($id: ID!) { pinIssue(input: {issueId: $id}) { issue { id } } }"
        self._api(["graphql", "-f", f"query={mutation}", "-f", f"id={node_id}"])


# --- orchestration -----------------------------------------------------------


def apply_actions(
    client: Client, actions: list[IssueAction], dry_run: bool, log: Callable[[str], None]
) -> list[tuple[str, int]]:
    """Carry out the issue plan; returns (workflow file, issue number) pairs tracked elsewhere."""
    tracked = []
    if any(a.op == "create" for a in actions) and not dry_run:
        client.ensure_label(
            CI_RED_LABEL, "B60205", "A workflow is red on main; managed by the repo health watcher"
        )
    for action in actions:
        if action.op == "tracked":
            assert action.number is not None
            tracked.append((action.workflow, action.number))
            log(f"{action.workflow}: red, already tracked by issue {action.number}")
        elif action.op == "create":
            log(f"{action.workflow}: open issue {action.title!r}")
            if not dry_run:
                client.create_issue(action.title, action.body, [CI_RED_LABEL])
        elif action.op == "update":
            assert action.number is not None
            log(f"{action.workflow}: update issue {action.number}")
            if not dry_run:
                client.update_issue(action.number, action.body)
        elif action.op == "close":
            assert action.number is not None
            log(f"{action.workflow}: close issue {action.number}")
            if not dry_run:
                client.comment(action.number, f"{WATCHER_COMMENT_MARKER}\n{action.comment}")
                client.close_issue(action.number)
    return tracked


def post_summary(
    client: Client, summary: Summary, open_issues: list[dict[str, Any]], dry_run: bool,
    log: Callable[[str], None],
) -> None:  # fmt: skip
    if dry_run:
        log("would post to the Repo health issue:")
        log(summary.body)
        return
    issue = next((i for i in open_issues if HEALTH_MARKER in (i.get("body") or "")), None)
    if issue is None:
        client.ensure_label(HEALTH_LABEL, "1D76DB", "Daily repo health summaries")
        body = (
            f"{HEALTH_MARKER}\nOne comment per run from the repo health watcher "
            "([`scripts/repo_health.py`](../blob/main/scripts/repo_health.py)): workflows red on "
            "`main`, stuck pull requests, silent scheduled routines, and stale failure issues. "
            "The owner is mentioned only when something needs them."
        )
        issue = client.create_issue(HEALTH_TITLE, body, [HEALTH_LABEL])
        try:
            client.pin_issue(issue["node_id"])
        except RuntimeError as err:
            log(f"could not pin the Repo health issue: {err}")
    client.comment(issue["number"], summary.body)
    log(f"posted the summary to issue {issue['number']}")


def run(client: Client, now: datetime, dry_run: bool, log: Callable[[str], None] = print) -> int:
    errors: list[str] = []

    def guarded(name: str, fn: Callable[..., Any], default: Any, *args: Any) -> Any:
        try:
            return fn(*args)
        except Exception as err:  # keep the other checks reporting
            errors.append(f"{name}: {span(err)}")
            return default

    open_issues = guarded("list open issues", client.open_issues, [])
    workflows = guarded("list workflows", client.workflows, None)
    red: list[Finding] = []
    tracked: list[tuple[str, int]] = []
    present = guarded("list workflow files", client.workflow_files, set())
    if workflows is not None and present:
        runs = {}
        for wf in workflows:
            if workflow_file(wf.get("path", "")) and wf.get("state") == "active":
                runs[wf["id"]] = guarded(
                    f"runs of {wf['name']}", client.workflow_runs, [], wf["id"]
                )
        states = classify_workflows(workflows, runs, present)
        active = {s.file for s in states}
        actions = guarded(
            "plan ci-red issues",
            lambda: plan_ci_red(states, open_issues, client.run_jobs, active),
            [],
        )
        tracked = guarded(
            "apply ci-red issues", lambda: apply_actions(client, actions, dry_run, log), []
        )
        red = red_workflow_findings(states, now)

    report = guarded("pull requests", lambda: analyze_prs(client.open_prs(), now), PrReport())
    comments = guarded(
        "Dependabot upkeep reports",
        lambda: client.issue_comments(DEPENDABOT_ISSUE, now - timedelta(days=60)),
        None,
    )
    routine = routine_finding(comments, now) if comments is not None else None
    tracking = tracking_issues(open_issues)
    by_number = {
        i["number"]: guarded(
            f"comments of issue {i['number']}", client.issue_comments, [], i["number"]
        )
        for i in tracking
    }
    stale = stale_issue_findings(tracking, by_number, now)

    summary = render_summary(now, red, tracked, report, routine, stale, errors)
    post_summary(client, summary, open_issues, dry_run, log)
    return 1 if errors else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    run_parser = sub.add_parser("run", help="check the repo and post the summary")
    run_parser.add_argument("--dry-run", action="store_true", help="read only; print the plan")
    args = parser.parse_args(argv)
    repo = os.environ.get("GITHUB_REPOSITORY", "dmooney/Rundale")
    dry = args.dry_run or os.environ.get("DRY_RUN", "").lower() == "true"
    return run(GhClient(repo), datetime.now(timezone.utc), dry)


if __name__ == "__main__":
    sys.exit(main())
