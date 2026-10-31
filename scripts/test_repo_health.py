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
        nightly = rh.WorkflowState(
            "testflight.yml#schedule", "TF", "red", run(7, "failure", 3, "schedule"), ago(3)
        )
        (action,) = self.plan([nightly], existing)
        self.assertEqual((action.op, action.number), ("tracked", 5))
        # Without the managed issue the watcher opens its own.
        (created,) = self.plan([nightly], [])
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
        self.assertEqual(finding.kind, "routine")
        chatter = {"body": "thanks!", "created_at": ago(1)}
        self.assertIsNotNone(rh.routine_finding([chatter], NOW))
        self.assertIsNotNone(rh.routine_finding([], NOW))

    def test_issue_stale_after_3_days_without_comments(self) -> None:
        quiet = issue(1, "quiet", labels=("ci-red",), hours=24 * 4)
        talked = issue(2, "talked", labels=("ci-red",), hours=24 * 4)
        own = issue(3, "own", labels=("ci-red",), hours=24 * 4)
        comments = {
            1: [],
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

    def test_any_problem_mentions_the_owner_however_young(self) -> None:
        young = rh.Finding("ci_red", "wf red", 5)
        summary = self.summary(red_workflows=[young])
        self.assertTrue(summary.needs_owner)
        self.assertIn("@dmooney this needs you", summary.body)
        for field, finding in {
            "stale": [rh.Finding("stale_issue", "issue 4 stale", 80)],
            "routine": rh.Finding("routine", "upkeep silent", 200),
        }.items():
            self.assertTrue(self.summary(**{field: finding}).needs_owner, field)
        prs = rh.PrReport(approved=[rh.Finding("pr_approved", "PR 3 approved", 3)])
        self.assertTrue(self.summary(prs=prs).needs_owner)
        prs = rh.PrReport(needs_owner=[rh.Finding("pr_red", "PR 3 x", 10)])
        self.assertTrue(self.summary(prs=prs).needs_owner)

    def test_watcher_errors_mention_the_owner(self) -> None:
        summary = self.summary(errors=["list open issues: `boom`"])
        self.assertTrue(summary.needs_owner)
        self.assertIn("@dmooney this needs you", summary.body)
        self.assertIn("watcher error: list open issues", summary.body)

    def test_tracked_and_errors_are_listed(self) -> None:
        summary = self.summary(tracked=[("testflight.yml", 5)], errors=["boom"])
        self.assertIn("tracked by issue 5", summary.body)
        self.assertIn("Watcher errors", summary.body)

    def test_a_tracked_red_workflow_alone_is_a_problem_that_mentions_the_owner(self) -> None:
        summary = self.summary(tracked=[("testflight.yml", 5)])
        self.assertTrue(summary.needs_owner)
        self.assertNotIn("All clear.", summary.body)
        self.assertIn("tracked by issue 5", summary.body.split("### ")[0])

    def test_a_tracked_workflow_is_not_also_counted_as_red(self) -> None:
        red = rh.WorkflowState("testflight.yml#schedule", "TF", "red", run(3, "failure", 5), ago(5))
        other = rh.WorkflowState("ci.yml", "CI", "red", run(4, "failure", 5), ago(5))
        found = rh.red_workflow_findings([red, other], NOW, {"testflight.yml#schedule"})
        self.assertEqual(len(found), 1)
        self.assertIn("ci.yml", found[0].text)


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
        self.branches: list[str] = []

    def _maybe_fail(self, name: str) -> None:
        if name in self.data["fail"]:
            raise RuntimeError(f"{name} unavailable")

    def workflows(self) -> Any:
        return self.data["workflows"]

    def workflow_files(self, branch: str) -> set[str]:
        return {w["path"].split("/")[-1] for w in self.data["workflows"]}

    def workflow_runs(self, workflow_id: int, branch: str) -> Any:
        self._maybe_fail("runs")
        self.branches.append(branch)
        return self.data["runs"][workflow_id]

    def run_jobs(self, run_id: int) -> Any:
        self._maybe_fail("jobs")
        return [{"name": "build", "conclusion": "failure", "html_url": "u", "steps": []}]

    def open_issues(self) -> Any:
        self._maybe_fail("issues")
        return self.data["issues"]

    def issue_comments(self, number: int, since: Any = None) -> Any:
        self._maybe_fail(f"comments{number}")
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
    def go(self, client: FakeClient, dry_run: bool = False, branch: str = "main") -> int:
        self.logs: list[str] = []
        return rh.run(client, NOW, dry_run, self.logs.append, branch)

    def test_first_run_opens_ci_red_issue_creates_pins_and_posts_summary(self) -> None:
        client = FakeClient()
        self.assertEqual(self.go(client), 0)
        kinds = [w[0] for w in client.writes]
        self.assertIn(
            ("create", "CI red on main: CI (ci.yml)", ("ci-red", "infra", "P1")), client.writes
        )
        self.assertIn(("create", "Repo health", ("repo-health", "infra", "P3")), client.writes)
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
        client.workflow_files = lambda branch: set()  # type: ignore[method-assign]
        self.go(client)
        self.assertNotIn(("close", 41), client.writes)


class ReviewTests(unittest.TestCase):
    def review(self, login: str, typename: str = "User", oid: str = "ab12cd34") -> Any:
        return {
            "author": {"__typename": typename, "login": login},
            "body": "",
            "submittedAt": ago(10),
            "state": "COMMENTED",
            "commit": {"oid": oid},
        }

    def test_bot_reviews_do_not_count_as_a_review(self) -> None:
        scanner = self.review("github-advanced-security", "Bot")
        suffixed = self.review("copilot[bot]")
        for review in (scanner, suffixed):
            report = rh.analyze_prs([pr(1, reviews={"nodes": [review]})], NOW)
            self.assertEqual(len(report.unreviewed), 1)

    def test_a_human_review_still_counts_next_to_a_bot_one(self) -> None:
        nodes = {"nodes": [self.review("github-advanced-security", "Bot"), self.review("rev")]}
        self.assertEqual(rh.analyze_prs([pr(1, reviews=nodes)], NOW).unreviewed, [])

    def test_the_query_reads_the_newest_reviews(self) -> None:
        self.assertIn("reviews(last: 50)", rh.PR_QUERY)
        self.assertNotIn("reviews(first", rh.PR_QUERY)

    def test_short_marker_shas_do_not_prefix_match(self) -> None:
        self.assertFalse(rh.sha_matches("ab1", "ab12cd34"))
        self.assertTrue(rh.sha_matches("ab12cd3", "ab12cd34"))


class CheckStateTests(unittest.TestCase):
    def state(self, *checks: Any) -> str:
        return rh.check_state(commit(list(checks))["nodes"][0]["commit"])[0]

    def test_cancelled_and_blocked_checks_are_not_green(self) -> None:
        for conclusion in ("CANCELLED", "ACTION_REQUIRED", "STALE", "FAILURE", "TIMED_OUT"):
            self.assertEqual(self.state(check_run(conclusion, 5)), "red", conclusion)

    def test_success_neutral_and_skipped_are_green(self) -> None:
        for conclusion in ("SUCCESS", "NEUTRAL", "SKIPPED"):
            self.assertEqual(self.state(check_run(conclusion, 5)), "green", conclusion)


class StaleActivityTests(unittest.TestCase):
    def test_bot_comments_do_not_reset_the_clock(self) -> None:
        nightly = issue(1, "Nightly mobile-verify failed", hours=24 * 6)
        comments = {
            1: [
                {
                    "body": "Failing again",
                    "created_at": ago(2),
                    "user": {"login": "github-actions[bot]", "type": "Bot"},
                }
            ]
        }
        self.assertEqual(len(rh.stale_issue_findings([nightly], comments, NOW)), 1)
        comments[1][0]["user"] = {"login": "dmooney", "type": "User"}
        self.assertEqual(rh.stale_issue_findings([nightly], comments, NOW), [])

    def test_an_issue_whose_comments_were_not_read_is_skipped(self) -> None:
        quiet = issue(1, "quiet", labels=("ci-red",), hours=24 * 9)
        self.assertEqual(rh.stale_issue_findings([quiet], {}, NOW), [])


class UpkeepHeadingTests(unittest.TestCase):
    def finding(self, body: str) -> Any:
        return rh.routine_finding([{"body": body, "created_at": ago(24)}], NOW)

    def test_heading_matches_after_a_marker_or_mention(self) -> None:
        self.assertIsNone(self.finding("<!-- upkeep -->\n@dmooney\n## Dependabot upkeep, 10-26"))

    def test_dry_run_reports_do_not_count(self) -> None:
        self.assertIsNotNone(self.finding("## Dependabot upkeep dry run, 2026-10-26"))


class BranchTests(unittest.TestCase):
    def test_classify_reads_only_the_requested_branch(self) -> None:
        runs = {1: [run(2, "failure", 1, branch="proof"), run(1, "success", 2)]}
        (main,) = rh.classify_workflows([wf(1, "ci.yml")], runs, {"ci.yml"})
        (proof,) = rh.classify_workflows([wf(1, "ci.yml")], runs, {"ci.yml"}, "proof")
        self.assertEqual((main.status, proof.status), ("green", "red"))

    def test_branch_issues_are_keyed_apart_from_main_issues(self) -> None:
        state = rh.WorkflowState("ci.yml", "CI", "red", run(7, "failure", 3), ago(3))
        main_issue = issue(10, "t", "<!-- ci-red: workflow=ci.yml -->", ("ci-red",))
        (action,) = rh.plan_ci_red([state], [main_issue], lambda _r: [], {"ci.yml"}, "proof")
        self.assertEqual(action.op, "create")
        self.assertEqual(action.title, "CI red on proof: CI (ci.yml)")
        self.assertIn("workflow=ci.yml@proof", action.body)
        # The branch's own issue is found and closed when the branch goes green; main's is not.
        green = rh.WorkflowState("ci.yml", "CI", "green", run(9, "success", 1), "")
        proof_issue = issue(11, "t", action.body, ("ci-red",))
        actions = rh.plan_ci_red(
            [green], [main_issue, proof_issue], lambda _r: [], {"ci.yml"}, "proof"
        )
        self.assertEqual([(a.op, a.number) for a in actions], [("close", 11)])

    def test_run_passes_the_branch_and_posts_no_summary_off_main(self) -> None:
        client = FakeClient(
            runs={1: [run(11, "failure", 3, branch="proof")], 2: [run(21, "success", 3)]}
        )
        self.assertEqual(rh.run(client, NOW, False, lambda _m: None, "proof"), 0)
        self.assertEqual(set(client.branches), {"proof"})
        titles = [w[1] for w in client.writes if w[0] == "create"]
        self.assertEqual(titles, ["CI red on proof: CI (ci.yml)"])
        self.assertFalse([w for w in client.writes if w[0] == "comment"])

    def test_cli_branch_option_defaults_to_main(self) -> None:
        seen: list[str] = []

        def fake_run(client: Any, now: Any, dry: bool, log: Any, branch: str) -> int:
            seen.append(branch)
            return 0

        original = rh.run
        rh.run = fake_run  # type: ignore[assignment]
        try:
            rh.main(["run", "--dry-run"])
            rh.main(["run", "--dry-run", "--branch", "proof"])
        finally:
            rh.run = original
        self.assertEqual(seen, ["main", "proof"])


class PartialFailureTests(unittest.TestCase):
    def test_unreadable_issue_list_creates_nothing(self) -> None:
        client = FakeClient(fail={"issues"})
        logs: list[str] = []
        self.assertEqual(rh.run(client, NOW, False, logs.append), 1)
        self.assertEqual(client.writes, [])
        self.assertTrue(any("@dmooney" in line for line in logs))

    def test_unreadable_comments_do_not_make_an_issue_stale(self) -> None:
        old = issue(41, "t", "<!-- ci-red: workflow=ci.yml -->", ("ci-red",), hours=24 * 9)
        health = issue(40, "Repo health", rh.HEALTH_MARKER, ("repo-health",))
        client = FakeClient(issues=[health, old], fail={"comments41"})
        rh.run(client, NOW, False, lambda _m: None)
        summary = [w for w in client.writes if w[0] == "comment"][-1][2]
        self.assertNotIn("Issue 41", summary)

    def test_a_job_fetch_failure_still_opens_the_issue(self) -> None:
        client = FakeClient(fail={"jobs"})
        self.assertEqual(rh.run(client, NOW, False, lambda _m: None), 1)
        creates = [w for w in client.writes if w[0] == "create"]
        self.assertIn("CI red on main: CI (ci.yml)", [c[1] for c in creates])


class EventVerdictTests(unittest.TestCase):
    def states(self, file: str, runs: Any) -> Any:
        found = rh.classify_workflows([wf(1, file)], {1: runs}, {file})
        return {s.file: s for s in found}

    def test_unified_workflow_uses_its_newest_run_whatever_the_event(self) -> None:
        # A nightly failure fixed by a later push is green, and the reverse is red.
        fixed = self.states("ci.yml", [run(2, "success", 1), run(1, "failure", 5, "schedule")])
        self.assertEqual(fixed["ci.yml"].status, "green")
        broken = self.states("ci.yml", [run(2, "failure", 1, "schedule"), run(1, "success", 5)])
        self.assertEqual(broken["ci.yml"].status, "red")

    def test_testflight_push_and_schedule_are_independent_tracks(self) -> None:
        states = self.states(
            "testflight.yml", [run(2, "success", 1, "schedule"), run(1, "failure", 5)]
        )
        self.assertEqual(states["testflight.yml"].status, "red")
        self.assertEqual(states["testflight.yml#schedule"].status, "green")
        states = self.states(
            "testflight.yml", [run(2, "success", 1), run(1, "failure", 5, "schedule")]
        )
        self.assertEqual(states["testflight.yml"].status, "green")
        self.assertEqual(states["testflight.yml#schedule"].status, "red")

    def test_a_passing_push_closes_its_issue_and_leaves_the_nightly_issue_tracked(self) -> None:
        states = self.states(
            "testflight.yml", [run(3, "success", 1), run(2, "failure", 5, "schedule")]
        )
        push_issue = issue(
            10, "CI red on main: x", "<!-- ci-red: workflow=testflight.yml -->", ("ci-red",)
        )
        nightly = issue(5, "Nightly mobile-verify failed")
        actions = rh.plan_ci_red(
            list(states.values()), [push_issue, nightly], lambda _r: [], set(states)
        )
        self.assertEqual(sorted((a.op, a.number) for a in actions), [("close", 10), ("tracked", 5)])


class BodyCapTests(unittest.TestCase):
    def test_oversized_bodies_are_truncated_keeping_the_head_marker(self) -> None:
        big = "<!-- ci-red: workflow=ci.yml -->\n" + "x" * 100_000
        capped = rh.cap_body(big)
        self.assertLessEqual(len(capped), int(65536 * 0.9))
        self.assertTrue(capped.startswith("<!-- ci-red: workflow=ci.yml -->"))
        self.assertIn("[truncated", capped)
        self.assertEqual(rh.cap_body("short"), "short")

    def test_every_write_is_capped(self) -> None:
        sent: list[Any] = []
        client = rh.GhClient("o/r")

        def record(args: Any, payload: Any = None) -> Any:
            sent.append(payload)
            return {}

        client._api = record  # type: ignore[method-assign]
        client.create_issue("t", "x" * 100_000, [])
        client.update_issue(1, "x" * 100_000)
        client.comment(1, "x" * 100_000)
        self.assertEqual(len(sent), 3)
        for payload in sent:
            self.assertLessEqual(len(payload["body"]), int(65536 * 0.9))


class GatekeeperReviewTests(unittest.TestCase):
    def test_a_blocked_marker_is_not_a_review(self) -> None:
        blocked = pr(1, comments={"nodes": [marker("blocked", 10)]})
        self.assertEqual(len(rh.analyze_prs([blocked], NOW).unreviewed), 1)
        body = marker("blocked", 10)["body"]
        review = {
            "author": {"login": "o"},
            "body": body,
            "submittedAt": ago(10),
            "state": "COMMENTED",
            "commit": {"oid": "ab12cd34"},
        }
        report = rh.analyze_prs([pr(1, reviews={"nodes": [review]})], NOW)
        self.assertEqual(len(report.unreviewed), 1)


class HealthIssueTests(unittest.TestCase):
    def test_only_the_labelled_issue_receives_the_summary(self) -> None:
        quoting = issue(50, "discussion", rh.HEALTH_MARKER)
        real = issue(40, "Repo health", rh.HEALTH_MARKER, ("repo-health",))
        client = FakeClient(issues=[quoting, real])
        rh.run(client, NOW, False, lambda _m: None)
        posted = {w[1] for w in client.writes if w[0] == "comment"}
        self.assertEqual(posted, {40})


if __name__ == "__main__":
    unittest.main()
