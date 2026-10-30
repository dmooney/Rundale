#!/usr/bin/env python3
"""Tests for the repo health watcher, against recorded-shape fake GitHub data."""

from __future__ import annotations

import importlib
import sys
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import TYPE_CHECKING, Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

if TYPE_CHECKING:
    import repo_health as rh
else:
    rh = importlib.import_module("repo_health")

NOW = datetime(2026, 10, 30, 12, 0, tzinfo=timezone.utc)


def ago(hours: float) -> str:
    return (NOW - timedelta(hours=hours)).strftime("%Y-%m-%dT%H:%M:%SZ")


def wf(wid: int, file: str, name: str = "", state: str = "active") -> dict[str, Any]:
    return {"id": wid, "path": f".github/workflows/{file}", "name": name or file, "state": state}


def run(rid: int, conclusion: str, hours: float, event: str = "push", branch: str = "main") -> Any:
    return {
        "id": rid,
        "conclusion": conclusion,
        "status": "completed",
        "event": event,
        "head_branch": branch,
        "created_at": ago(hours),
        "html_url": f"https://github.com/o/r/actions/runs/{rid}",
        "head_sha": "abcdef1234567890",
    }


def issue(
    number: int, title: str, body: str = "", labels: tuple[str, ...] = (), hours: float = 1
) -> Any:
    return {
        "number": number,
        "title": title,
        "body": body,
        "labels": [{"name": n} for n in labels],
        "created_at": ago(hours),
        "html_url": f"https://github.com/o/r/issues/{number}",
        "node_id": f"I_{number}",
    }


class ClassifyTests(unittest.TestCase):
    def classify(self, workflows: Any, runs: Any, present: set[str] | None = None) -> Any:
        files = present if present is not None else {w["path"].split("/")[-1] for w in workflows}
        return {s.file: s for s in rh.classify_workflows(workflows, runs, files)}

    def test_latest_verdict_wins_and_ignores_cancelled_pr_and_branch_runs(self) -> None:
        runs = {
            1: [
                run(5, "cancelled", 1),
                run(4, "failure", 2, event="pull_request"),
                run(3, "failure", 3, branch="feature"),
                run(2, "success", 4),
                run(1, "failure", 9),
            ]
        }
        state = self.classify([wf(1, "ci.yml")], runs)["ci.yml"]
        self.assertEqual((state.status, state.run["id"]), ("green", 2))

    def test_red_streak_start_is_the_oldest_consecutive_failure(self) -> None:
        runs = {
            1: [
                run(4, "failure", 1),
                run(3, "timed_out", 5),
                run(2, "success", 9),
                run(1, "failure", 20),
            ]
        }
        state = self.classify([wf(1, "ci.yml")], runs)["ci.yml"]
        self.assertEqual(state.status, "red")
        self.assertEqual(state.since, ago(5))

    def test_deleted_disabled_and_dynamic_workflows_are_skipped(self) -> None:
        workflows = [
            wf(1, "gone.yml"),
            wf(2, "off.yml", state="disabled_manually"),
            {"id": 3, "path": "dynamic/pages", "name": "pages", "state": "active"},
            wf(4, "here.yml"),
        ]
        states = self.classify(workflows, {}, present={"off.yml", "here.yml"})
        self.assertEqual(set(states), {"here.yml"})
        self.assertEqual(states["here.yml"].status, "unknown")


class CiRedPlanTests(unittest.TestCase):
    def plan(self, states: Any, issues: Any, active: set[str] | None = None) -> Any:
        jobs: list[dict[str, Any]] = [
            {
                "name": "Rust `tests` @dmooney",
                "conclusion": "failure",
                "html_url": "https://x/job/1",
                "steps": [{"name": "cargo test", "conclusion": "failure"}],
            },
            {"name": "lint", "conclusion": "success", "steps": []},
        ]
        return rh.plan_ci_red(states, issues, lambda _rid: jobs, active or {s.file for s in states})

    def red(self, file: str = "ci.yml", rid: int = 7) -> Any:
        return rh.WorkflowState(file, "CI", "red", run(rid, "failure", 3), ago(3))

    def test_red_workflow_without_issue_creates_one_with_marker_and_job_details(self) -> None:
        (action,) = self.plan([self.red()], [])
        self.assertEqual(action.op, "create")
        self.assertEqual(action.title, "CI red on main: CI (ci.yml)")
        self.assertIn("<!-- ci-red: workflow=ci.yml -->", action.body)
        self.assertIn("https://github.com/o/r/actions/runs/7", action.body)
        self.assertIn("cargo test", action.body)
        self.assertNotIn("lint", action.body)

    def test_untrusted_job_names_are_code_spans_that_cannot_ping(self) -> None:
        (action,) = self.plan([self.red()], [])
        self.assertIn("[`Rust 'tests' @dmooney`]", action.body)

    def test_same_run_leaves_the_issue_alone_new_run_updates_it(self) -> None:
        body = "<!-- ci-red: workflow=ci.yml -->\n<!-- ci-red-run: 7 -->"
        existing = [issue(10, "t", body, ("ci-red",))]
        self.assertEqual(self.plan([self.red(rid=7)], existing), [])
        (action,) = self.plan([self.red(rid=8)], existing)
        self.assertEqual((action.op, action.number), ("update", 10))

    def test_workflow_with_a_managed_issue_is_tracked_not_duplicated(self) -> None:
        existing = [issue(5, "Nightly mobile-verify failed")]
        (action,) = self.plan([self.red("testflight.yml")], existing)
        self.assertEqual((action.op, action.number), ("tracked", 5))
        # Without the managed issue the watcher opens its own.
        (created,) = self.plan([self.red("testflight.yml")], [])
        self.assertEqual(created.op, "create")

    def test_passing_workflow_closes_its_issue_with_a_comment(self) -> None:
        body = "<!-- ci-red: workflow=ci.yml -->"
        state = rh.WorkflowState("ci.yml", "CI", "green", run(9, "success", 1))
        (action,) = self.plan([state], [issue(10, "t", body, ("ci-red",))])
        self.assertEqual((action.op, action.number), ("close", 10))
        self.assertIn("runs/9", action.comment)

    def test_unknown_verdict_keeps_the_issue_and_a_removed_workflow_closes_it(self) -> None:
        body = "<!-- ci-red: workflow=ci.yml -->"
        issues = [issue(10, "t", body, ("ci-red",))]
        unknown = rh.WorkflowState("ci.yml", "CI", "unknown", None)
        self.assertEqual(self.plan([unknown], issues), [])
        (action,) = self.plan([], issues, active=set())
        self.assertEqual((action.op, action.number), ("close", 10))

    def test_marker_without_the_label_is_not_ours(self) -> None:
        body = "<!-- ci-red: workflow=ci.yml -->"
        (action,) = self.plan([self.red()], [issue(10, "t", body)])
        self.assertEqual(action.op, "create")


def commit(checks: list[dict[str, Any]] | None, committed: float = 40) -> Any:
    rollup = None if checks is None else {"state": "x", "contexts": {"nodes": checks}}
    return {
        "nodes": [
            {
                "commit": {
                    "oid": "ab12cd34",
                    "committedDate": ago(committed),
                    "statusCheckRollup": rollup,
                }
            }
        ]
    }


def check_run(conclusion: str, hours: float, status: str = "COMPLETED") -> Any:
    return {
        "__typename": "CheckRun",
        "name": "c",
        "status": status,
        "conclusion": conclusion,
        "completedAt": ago(hours),
    }


def pr(number: int = 1, **over: Any) -> Any:
    base = {
        "number": number,
        "title": "feat: thing",
        "url": f"https://github.com/o/r/pull/{number}",
        "isDraft": False,
        "createdAt": ago(100),
        "headRefOid": "ab12cd34",
        "author": {"login": "dev"},
        "labels": {"nodes": []},
        "reviews": {"nodes": []},
        "comments": {"nodes": []},
        "commits": commit([check_run("SUCCESS", 30)]),
    }
    base.update(over)
    return base


def marker(verdict: str, hours: float, sha: str = "ab12cd34") -> Any:
    body = f"<!-- gatekeeper: sha={sha} verdict={verdict} round=1 -->\n**Gatekeeper: {verdict}**"
    return {"body": body, "createdAt": ago(hours), "author": {"login": "owner"}}


class PrTests(unittest.TestCase):
    def analyze(self, *prs: Any) -> Any:
        return rh.analyze_prs(list(prs), NOW)

    def test_red_over_24h_is_flagged_and_recent_red_is_not(self) -> None:
        old = pr(1, commits=commit([check_run("FAILURE", 30), check_run("SUCCESS", 2)]))
        new = pr(2, commits=commit([check_run("FAILURE", 3)]))
        report = self.analyze(old, new)
        self.assertEqual(len(report.red), 1)
        self.assertIn("PR 1", report.red[0].text)

    def test_pending_checks_are_neither_red_nor_green(self) -> None:
        pending = pr(
            1, commits=commit([check_run("", 0, status="IN_PROGRESS"), check_run("SUCCESS", 40)])
        )
        report = self.analyze(pending)
        self.assertEqual((report.red, report.unreviewed), ([], []))

    def test_status_context_failure_counts_as_red(self) -> None:
        ctx = {
            "__typename": "StatusContext",
            "context": "x",
            "state": "FAILURE",
            "createdAt": ago(50),
        }
        self.assertEqual(len(self.analyze(pr(1, commits=commit([ctx]))).red), 1)

    def test_green_and_unreviewed_over_24h(self) -> None:
        self.assertEqual(len(self.analyze(pr(1)).unreviewed), 1)
        fresh = pr(2, commits=commit([check_run("SUCCESS", 5)], committed=6))
        self.assertEqual(self.analyze(fresh).unreviewed, [])

    def test_reviews_count_only_at_the_current_head_and_not_from_the_author(self) -> None:
        def review(login: str, oid: str) -> Any:
            return {
                "author": {"login": login},
                "body": "",
                "submittedAt": ago(10),
                "state": "COMMENTED",
                "commit": {"oid": oid},
            }

        self.assertEqual(
            self.analyze(pr(1, reviews={"nodes": [review("dev", "ab12cd34")]})).unreviewed[0].kind,
            "pr_unreviewed",
        )
        self.assertEqual(
            len(self.analyze(pr(1, reviews={"nodes": [review("rev", "old")]})).unreviewed), 1
        )
        self.assertEqual(
            self.analyze(pr(1, reviews={"nodes": [review("rev", "ab12cd34")]})).unreviewed, []
        )

    def test_gatekeeper_marker_at_head_counts_as_review(self) -> None:
        reviewed = pr(1, comments={"nodes": [marker("changes", 10)]})
        self.assertEqual(self.analyze(reviewed).unreviewed, [])

    def test_approved_marker_unmerged_after_2h(self) -> None:
        late = pr(1, comments={"nodes": [marker("approve", 3)]})
        early = pr(2, comments={"nodes": [marker("approve", 1)]})
        report = self.analyze(late, early)
        self.assertEqual([f.text.split()[1] for f in report.approved], ["1"])

    def test_marker_for_an_older_head_or_a_later_changes_verdict_does_not_count(self) -> None:
        stale = pr(1, comments={"nodes": [marker("approve", 5, sha="1234ef56")]})
        reversed_ = pr(2, comments={"nodes": [marker("approve", 9), marker("changes", 4)]})
        self.assertEqual(self.analyze(stale, reversed_).approved, [])

    def test_marker_in_a_review_body_is_read(self) -> None:
        body = marker("approve", 5)["body"]
        review = {
            "author": {"login": "o"},
            "body": body,
            "submittedAt": ago(5),
            "state": "COMMENTED",
            "commit": {"oid": "ab12cd34"},
        }
        self.assertEqual(len(self.analyze(pr(1, reviews={"nodes": [review]})).approved), 1)

    def test_drafts_are_parked_and_needs_owner_is_listed_apart(self) -> None:
        red = commit([check_run("FAILURE", 90)])
        draft = pr(1, isDraft=True, commits=red)
        owner = pr(2, labels={"nodes": [{"name": "needs-owner"}]}, commits=red)
        report = self.analyze(draft, owner)
        self.assertEqual((report.red, report.unreviewed, report.approved), ([], [], []))
        self.assertEqual(len(report.needs_owner), 1)
        self.assertIn("PR 2", report.needs_owner[0].text)

    def test_pr_without_checks_is_not_green(self) -> None:
        self.assertEqual(self.analyze(pr(1, commits=commit(None))).unreviewed, [])


class RoutineAndStaleTests(unittest.TestCase):
    def test_recent_report_is_fine_old_or_missing_report_is_a_finding(self) -> None:
        report = {"body": "## Dependabot upkeep, 2026-10-26\nMerged: 6.", "created_at": ago(24 * 4)}
        self.assertIsNone(rh.routine_finding([report], NOW))
        old = {"body": "## Dependabot upkeep", "created_at": ago(24 * 9)}
        finding = rh.routine_finding([old], NOW)
        self.assertIsNotNone(finding)
        assert finding is not None
        self.assertTrue(finding.escalates)
        chatter = {"body": "thanks!", "created_at": ago(1)}
        self.assertIsNotNone(rh.routine_finding([chatter], NOW))
        self.assertIsNotNone(rh.routine_finding([], NOW))

    def test_issue_stale_after_3_days_without_comments(self) -> None:
        quiet = issue(1, "quiet", labels=("ci-red",), hours=24 * 4)
        talked = issue(2, "talked", labels=("ci-red",), hours=24 * 4)
        own = issue(3, "own", labels=("ci-red",), hours=24 * 4)
        comments = {
            2: [{"body": "looking", "created_at": ago(5)}],
            3: [{"body": f"{rh.WATCHER_COMMENT_MARKER} hi", "created_at": ago(5)}],
        }
        found = rh.stale_issue_findings([quiet, talked, own], comments, NOW)
        self.assertEqual(sorted(f.text.split()[1] for f in found), ["1", "3"])

    def test_tracking_issues_are_the_label_and_the_managed_titles(self) -> None:
        issues = [
            issue(1, "x", labels=("ci-red",)),
            issue(2, "Nightly mobile-verify failed"),
            issue(3, "other"),
        ]
        self.assertEqual([i["number"] for i in rh.tracking_issues(issues)], [1, 2])


class SummaryTests(unittest.TestCase):
    def summary(self, **over: Any) -> Any:
        args: dict[str, Any] = {
            "red_workflows": [],
            "tracked": [],
            "prs": rh.PrReport(),
            "routine": None,
            "stale": [],
            "errors": [],
        }
        args.update(over)
        return rh.render_summary(NOW, **args)

    def test_quiet_day_does_not_mention_the_owner(self) -> None:
        summary = self.summary()
        self.assertFalse(summary.needs_owner)
        self.assertNotIn("@dmooney", summary.body)
        self.assertIn("All clear.", summary.body)

    def test_young_problems_do_not_notify_old_ones_and_needs_owner_do(self) -> None:
        young = rh.Finding("ci_red", "wf red", 5)
        self.assertFalse(self.summary(red_workflows=[young]).needs_owner)
        old = rh.Finding("ci_red", "wf red", 80)
        self.assertIn("@dmooney", self.summary(red_workflows=[old]).body)
        prs = rh.PrReport(needs_owner=[rh.Finding("pr_red", "PR 3 x", 10)])
        self.assertTrue(self.summary(prs=prs).needs_owner)

    def test_tracked_and_errors_are_listed(self) -> None:
        summary = self.summary(tracked=[("testflight.yml", 5)], errors=["boom"])
        self.assertIn("tracked by issue 5", summary.body)
        self.assertIn("Watcher errors", summary.body)


class FakeClient:
    """In-memory GitHub; records every write."""

    def __init__(self, **data: Any) -> None:
        self.data: dict[str, Any] = {
            "workflows": [wf(1, "ci.yml", "CI"), wf(2, "ok.yml", "OK")],
            "runs": {1: [run(11, "failure", 3)], 2: [run(21, "success", 3)]},
            "issues": [],
            "prs": [],
            "comments": {
                rh.DEPENDABOT_ISSUE: [{"body": "## Dependabot upkeep", "created_at": ago(24)}]
            },
            "fail": set(),
        }
        self.data.update(data)
        self.writes: list[tuple[Any, ...]] = []

    def _maybe_fail(self, name: str) -> None:
        if name in self.data["fail"]:
            raise RuntimeError(f"{name} unavailable")

    def workflows(self) -> Any:
        return self.data["workflows"]

    def workflow_files(self) -> set[str]:
        return {w["path"].split("/")[-1] for w in self.data["workflows"]}

    def workflow_runs(self, workflow_id: int) -> Any:
        self._maybe_fail("runs")
        return self.data["runs"][workflow_id]

    def run_jobs(self, run_id: int) -> Any:
        return [{"name": "build", "conclusion": "failure", "html_url": "u", "steps": []}]

    def open_issues(self) -> Any:
        return self.data["issues"]

    def issue_comments(self, number: int, since: Any = None) -> Any:
        return self.data["comments"].get(number, [])

    def open_prs(self) -> Any:
        self._maybe_fail("prs")
        return self.data["prs"]

    def ensure_label(self, name: str, color: str, description: str) -> None:
        self.writes.append(("label", name))

    def create_issue(self, title: str, body: str, labels: list[str]) -> Any:
        self.writes.append(("create", title, tuple(labels)))
        return {"number": 99, "node_id": "I_99"}

    def update_issue(self, number: int, body: str) -> None:
        self.writes.append(("update", number))

    def comment(self, number: int, body: str) -> None:
        self.writes.append(("comment", number, body))

    def close_issue(self, number: int) -> None:
        self.writes.append(("close", number))

    def pin_issue(self, node_id: str) -> None:
        self.writes.append(("pin", node_id))


class RunTests(unittest.TestCase):
    def go(self, client: FakeClient, dry_run: bool = False) -> int:
        self.logs: list[str] = []
        return rh.run(client, NOW, dry_run, self.logs.append)

    def test_first_run_opens_ci_red_issue_creates_pins_and_posts_summary(self) -> None:
        client = FakeClient()
        self.assertEqual(self.go(client), 0)
        kinds = [w[0] for w in client.writes]
        self.assertIn(("create", "CI red on main: CI (ci.yml)", ("ci-red",)), client.writes)
        self.assertIn(("create", "Repo health", ("repo-health",)), client.writes)
        self.assertIn(("pin", "I_99"), client.writes)
        self.assertEqual(kinds.count("comment"), 1)

    def test_later_run_reuses_the_health_issue_and_closes_fixed_workflow(self) -> None:
        health = issue(40, "Repo health", rh.HEALTH_MARKER, ("repo-health",))
        fixed = issue(41, "t", "<!-- ci-red: workflow=ok.yml -->", ("ci-red",))
        client = FakeClient(issues=[health, fixed])
        self.go(client)
        self.assertIn(("close", 41), client.writes)
        comments = [w for w in client.writes if w[0] == "comment"]
        self.assertEqual({c[1] for c in comments}, {41, 40})
        self.assertFalse([w for w in client.writes if w[0] == "pin"])

    def test_dry_run_writes_nothing(self) -> None:
        client = FakeClient()
        self.assertEqual(self.go(client, dry_run=True), 0)
        self.assertEqual(client.writes, [])
        self.assertTrue(any("Repo health" in line for line in self.logs))

    def test_a_failing_check_is_reported_and_the_others_still_run(self) -> None:
        client = FakeClient(fail={"prs"})
        self.assertEqual(self.go(client), 1)
        summary = [w for w in client.writes if w[0] == "comment"][-1][2]
        self.assertIn("Watcher errors", summary)
        self.assertIn("CI red on main", " ".join(str(w) for w in client.writes))

    def test_missing_workflow_files_do_not_close_every_issue(self) -> None:
        client = FakeClient(
            issues=[issue(41, "t", "<!-- ci-red: workflow=ok.yml -->", ("ci-red",))]
        )
        client.workflow_files = lambda: set()  # type: ignore[method-assign]
        self.go(client)
        self.assertNotIn(("close", 41), client.writes)


if __name__ == "__main__":
    unittest.main()
