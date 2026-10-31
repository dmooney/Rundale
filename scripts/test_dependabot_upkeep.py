#!/usr/bin/env python3
"""Tests for the weekly Dependabot upkeep routine, against a fake GitHub and git."""

from __future__ import annotations

import fcntl
import importlib
import io
import json
import os
import shlex
import signal
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import TYPE_CHECKING, Any
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

if TYPE_CHECKING:
    import dependabot_upkeep as upkeep
else:
    upkeep = importlib.import_module("dependabot_upkeep")

Result = upkeep.Result
REPO = "owner/repo"
CLAUDE_FIX = "fix: adapt to the bump"

RELEASE = """
on:
  push: {tags: ["v*"]}
  workflow_dispatch: {}
jobs: {}
"""
CI = """
on:
  pull_request:
jobs:
  suite:
    uses: ./.github/workflows/full-ci.yml
"""
FULL_CI = "on:\n  workflow_call:\njobs: {}\n"
TARGET = "on:\n  pull_request_target:\njobs: {}\n"


def pr_json(number: int, head: str, **extra: Any) -> dict[str, Any]:
    return {
        "number": number,
        "title": f"chore(deps): bump thing {number}",
        "url": f"https://github.com/{REPO}/pull/{number}",
        "headRefName": f"dependabot/x/{number}",
        "headRefOid": head,
        "baseRefName": "main",
        "labels": [],
        "isDraft": False,
        "mergeable": "MERGEABLE",
        **extra,
    }


class FakeWorld:
    """Answers the gh, git, just and agent commands the routine runs."""

    def __init__(self, work: Path, review: Path):
        self.work, self.review = work, review
        self.calls: list[tuple[tuple[str, ...], Path | None]] = []
        self.prs: dict[int, dict[str, Any]] = {}
        self.behind: dict[int, int] = {}
        self.classify: dict[int, int] = {}
        self.conflicts: set[int] = set()
        self.checks: dict[int, list[Any]] = {}  # check lists, or a failure name
        self.commits: dict[int, list[dict[str, Any]]] = {}
        self.reviews: dict[int, list[dict[str, Any]]] = {}
        self.merged: set[int] = set()
        self.gatekeeper: dict[int, Callable[[str], None]] = {}
        self.fix_commits: dict[int, int] = {}
        self.files: dict[int, list[str]] = {}
        self.workflows: dict[str, str] = {}
        self.pgrep: list[int] = [1]
        self.verify_rc = 0
        self.runs: list[dict[str, Any]] = []
        self.issues: list[dict[str, Any]] = []
        self.generation = 0
        self.work_pr: int | None = None
        self.comment_rc = 0
        self.view_fail: set[int] = set()
        self.commits_fail: set[int] = set()
        self.fix_app_inputs: set[int] = set()  # PRs whose fix touches an app input
        self.main_moves_inputs: set[int] = set()  # PRs whose merge with main changes inputs
        self.ls_tree_fails = False
        self.pinned = True
        self.dispatch_fails = False
        self.summary_status = "completed"
        self.summary_conclusion = "success"
        self.comment_fail: set[int] = set()
        self.list_ok = 1_000  # pr list calls that succeed before GitHub "fails"

    def add(self, number: int, *, behind: int = 0, classify: int = 3, **extra: Any) -> None:
        self.prs[number] = pr_json(number, f"{number:04d}{'0' * 36}", **extra)
        self.behind[number] = behind
        self.classify[number] = classify
        self.checks[number] = [[{"name": "CI gate", "bucket": "pass", "link": "l"}]]
        self.commits[number] = [
            {"commit": {"message": "chore(deps): bump"}, "author": {"login": "dependabot[bot]"}}
        ]
        self.reviews[number] = []

    def new_head(self, number: int) -> None:
        self.generation += 1
        self.prs[number]["headRefOid"] = f"{number:04d}{self.generation:036d}"
        self.behind[number] = 0

    def approve_and_merge(self, number: int) -> Callable[[str], None]:
        def act(_family: str) -> None:
            self.merged.add(number)

        return act

    def verdict(self, number: int, verdict: str) -> Callable[[str], None]:
        def act(_family: str) -> None:
            sha = self.prs[number]["headRefOid"]
            self.reviews[number].append(
                {"body": f"<!-- gatekeeper: sha={sha} verdict={verdict} round=1 -->\nx"}
            )

        return act

    def ran(self, *prefix: str) -> list[tuple[str, ...]]:
        return [cmd for cmd, _ in self.calls if cmd[: len(prefix)] == prefix]

    @staticmethod
    def ok(payload: Any = None, rc: int = 0) -> Result:
        return Result(rc, json.dumps(payload) if payload is not None else "")

    def __call__(
        self, cmd: Sequence[str], cwd: Path | None, timeout: float | None, output: Path | None
    ) -> Result:
        cmd = tuple(cmd)
        self.calls.append((cmd, cwd))
        if cmd[0] == "gh":
            return self.gh(cmd[1:])
        if cmd[0] == "git":
            return self.git(cmd[1:], cwd)
        if cmd[0] == sys.executable and cmd[1:3] == ("mobile/scripts/verify_record.py", "tree"):
            app = cmd[3] == "HEAD" and self.work_pr in self.fix_app_inputs | self.main_moves_inputs
            return Result(0, "app-tree\n" if app else "base-tree\n")
        if cmd[0] == sys.executable and cmd[1] == "mobile/scripts/verify_record.py":
            return Result(self.classify[int(cmd[-1])])
        if cmd[0] == "pgrep":
            return Result(self.pgrep.pop(0) if len(self.pgrep) > 1 else self.pgrep[0])
        if cmd[:2] == ("just", "mobile-verify"):
            return Result(self.verify_rc)
        if cmd[:2] == ("just", "mobile-verify-record"):
            return Result(0)
        if cmd[0] == "claude":  # a fix attempt; the gatekeeper runs through agy
            assert cwd == self.work and self.work_pr is not None
            return Result(0)
        if cmd[0] == "env" and "agy" in cmd:
            number = int(cmd[-1].rsplit(" ", 1)[1].rstrip("."))
            self.gatekeeper[number]("agy")
            return Result(0)
        raise AssertionError(f"unexpected command {cmd}")

    def gh(self, args: tuple[str, ...]) -> Result:
        if args[:2] == ("pr", "list"):
            self.list_ok -= 1
            if self.list_ok < 0:
                return Result(1, "", "HTTP 502: Bad Gateway")
            return self.ok([self.prs[n] for n in sorted(self.prs) if n not in self.merged])
        if args[:2] == ("pr", "view"):
            number, fields = int(args[2]), args[args.index("--json") + 1]
            if number in self.view_fail:
                return Result(1, "", "HTTP 502: Bad Gateway")
            if fields == "state":
                return self.ok({"state": "MERGED" if number in self.merged else "OPEN"})
            if fields == "files":
                return self.ok({"files": [{"path": p} for p in self.files.get(number, [])]})
            return self.ok(self.prs[number])
        if args[:2] == ("pr", "checks"):
            states = self.checks[int(args[2])]
            current: Any = states.pop(0) if len(states) > 1 else states[0]
            if current == "none-yet":
                return Result(1, "", "no required checks reported on the 'x' branch")
            if current == "api-error":
                return Result(1, "", "HTTP 502: Bad Gateway")
            if current == "garbled":
                return Result(0, "<html>", "")
            failing = any(c["bucket"] == "fail" for c in current)
            return self.ok(current, 1 if failing else 0)
        if args[:2] == ("issue", "comment"):
            return Result(self.comment_rc, "", "HTTP 502" if self.comment_rc else "")
        if args[:2] == ("pr", "comment"):
            return Result(1, "", "HTTP 502") if int(args[2]) in self.comment_fail else self.ok()
        if args[:2] == ("issue", "pin"):
            return self.ok()
        if args[:2] == ("issue", "list"):
            return self.ok(self.issues)
        if args[:2] == ("issue", "view"):
            return self.ok({"isPinned": self.pinned})
        if args[:2] == ("issue", "create"):
            return Result(0, f"https://github.com/{REPO}/issues/77\n")
        if args[:2] == ("workflow", "run"):
            if self.dispatch_fails:
                return Result(1, "", "HTTP 422")
            if args[2] == "upkeep-summary.yml":  # the dispatched run appears and finishes
                self.runs.append(
                    {
                        "databaseId": 500 + len(self.runs),
                        "status": self.summary_status,
                        "conclusion": self.summary_conclusion,
                        "url": "https://github.com/run/500",
                    }
                )
            return self.ok()
        if args[:2] == ("run", "list"):
            return self.ok(self.runs)
        if args[:2] == ("run", "rerun"):
            return self.ok()
        if args[0] == "api":
            return self.api(args[1:])
        raise AssertionError(f"unexpected gh {args}")

    def api(self, args: tuple[str, ...]) -> Result:
        path = args[-1]
        if "-X" in args:
            path = args[args.index("-X") + 2]
            assert path.endswith("/update-branch"), args
            number = int(path.split("/")[-2])
            self.new_head(number)
            return self.ok({"message": "Updating pull request branch."})
        if "/compare/" in path:
            head = path.rsplit("...", 1)[1]
            number = next(n for n, p in self.prs.items() if p["headRefOid"] == head)
            return self.ok({"behind_by": self.behind[number]})
        number = int(path.split("/")[-2])
        if path.endswith("/commits") and number in self.commits_fail:
            return Result(1, "", "HTTP 502: Bad Gateway")
        if path.endswith("/commits"):
            return self.ok([self.commits[number]])
        if path.endswith("/reviews"):
            return self.ok([self.reviews[number]])
        raise AssertionError(f"unexpected api {args}")

    def git(self, args: tuple[str, ...], cwd: Path | None) -> Result:
        if args[0] == "merge-tree":
            number = int(args[-1].rsplit("/", 1)[1])
            return Result(1 if number in self.conflicts else 0)
        if args[0] == "checkout":
            sha = args[-1]
            self.work_pr = next(n for n, p in self.prs.items() if p["headRefOid"] == sha)
            return Result(0)
        if args[0] == "push":
            number = int(args[-1].rsplit("/", 1)[1])
            self.new_head(number)
            return Result(0)
        if args[0] == "rev-list":
            assert self.work_pr is not None
            count = self.fix_commits.get(self.work_pr, 0)
            if count:
                self.commits[self.work_pr].append(
                    {"commit": {"message": CLAUDE_FIX}, "author": {"login": "dmooney"}}
                )
            return Result(0, f"{count}\n")
        if args[0] == "ls-tree":
            if self.ls_tree_fails:
                return Result(128, "", "fatal: not a tree object")
            return Result(0, "\n".join(sorted(self.workflows)))
        if args[0] == "merge-base":
            return Result(0, "base\n")
        if args[0] == "show":
            return Result(0, self.workflows[args[-1].split(":", 1)[1]])
        if args[0] in ("fetch", "clean", "merge", "reset"):
            return Result(0)
        raise AssertionError(f"unexpected git {args}")


MUTATING = (
    ("gh", "api", "-X"),
    ("gh", "pr", "comment"),
    ("gh", "issue", "create"),
    ("gh", "issue", "comment"),
    ("gh", "issue", "pin"),
    ("gh", "workflow", "run"),
    ("gh", "run", "rerun"),
    ("git", "push"),
    ("git", "merge"),
    ("git", "checkout"),
    ("git", "reset"),
    ("just",),
    ("claude",),
    ("env",),
)


class UpkeepCase(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.work, self.review = base / "work", base / "review"
        self.world = FakeWorld(self.work, self.review)
        self.clock = [0.0]
        self.log = io.StringIO()

    def make(self, *, dry_run: bool = False, fix: bool = True) -> upkeep.Upkeep:
        def sleep(seconds: float) -> None:
            self.clock[0] += seconds

        shell = upkeep.Shell(dry_run=dry_run, log=self.log, executor=self.world)
        return upkeep.Upkeep(
            shell,
            repo=REPO,
            state=Path(self.tmp.name) / "state",
            review=self.review,
            work=self.work,
            fix=fix,
            sleep=sleep,
            clock=lambda: self.clock[0],
        )

    def outcome(self, number: int, **kwargs: Any) -> upkeep.Outcome:
        routine = self.make(**kwargs)
        outcomes, _ = routine.run(only={number}, exclude=set())
        return outcomes[0]


class PureLogicTests(unittest.TestCase):
    def test_latest_verdict_takes_the_newest_marker(self) -> None:
        reviews = [
            {"body": "<!-- gatekeeper: sha=aaa verdict=changes round=1 -->"},
            {"body": "a human comment"},
            {"body": "<!-- gatekeeper: sha=bbb verdict=handoff round=1 -->\n**Gatekeeper**"},
        ]
        self.assertEqual(upkeep.latest_verdict(reviews), ("bbb", "handoff"))
        self.assertIsNone(upkeep.latest_verdict([{"body": None}]))

    def test_check_summary(self) -> None:
        def check(bucket: str) -> dict[str, str]:
            return {"name": "CI gate", "bucket": bucket, "link": "u"}

        self.assertEqual(upkeep.summarize_checks([]), ("pending", []))
        self.assertEqual(upkeep.summarize_checks([check("pending")])[0], "pending")
        self.assertEqual(upkeep.summarize_checks([check("pass"), check("skipping")]), ("pass", []))
        self.assertEqual(upkeep.summarize_checks([check("fail")]), ("fail", ["CI gate (u)"]))
        self.assertEqual(upkeep.summarize_checks([check("cancel")])[0], "fail")

    def test_unparseable_workflow_text_has_no_triggers(self) -> None:
        self.assertEqual(upkeep.workflow_triggers("key: [unclosed"), set())

    def test_unexercised_workflows(self) -> None:
        workflows = {
            ".github/workflows/release.yml": RELEASE,
            ".github/workflows/ci.yml": CI,
            ".github/workflows/full-ci.yml": FULL_CI,
            ".github/workflows/pr-title.yml": TARGET,
        }
        changed = [*workflows, "README.md", ".github/dependabot.yml"]
        self.assertEqual(
            upkeep.unexercised_workflows(changed, workflows),
            [".github/workflows/pr-title.yml", ".github/workflows/release.yml"],
        )

    def test_summary_groups_merged_and_attention(self) -> None:
        outcomes = [
            upkeep.Outcome(1, "bump a", "u", "merged", "merged by the gatekeeper (claude)"),
            upkeep.Outcome(2, "bump b", "u", "red", "red: CI gate", app_input=True, notes=["n"]),
            upkeep.Outcome(3, "bump c", "u", "skipped", "excluded for this run"),
        ]
        text = upkeep.render_summary(outcomes, when="now", dry_run=False)
        self.assertIn("Merged: 1. Needs attention: 1.", text)
        merged, attention = text.index("### Merged"), text.index("### Needs attention")
        self.assertLess(merged, text.index("#1 bump a"))
        self.assertLess(attention, text.index("#2 bump b (app input): **red**"))
        self.assertLess(text.index("### Other"), text.index("#3 bump c"))
        self.assertIn("No open Dependabot", upkeep.render_summary([], when="now", dry_run=True))

    def test_dry_run_shell_never_runs_mutating_commands(self) -> None:
        seen: list[Sequence[str]] = []

        def executor(cmd: Sequence[str], *_: Any) -> Result:
            seen.append(cmd)
            return Result(0, "x")

        shell = upkeep.Shell(dry_run=True, log=io.StringIO(), executor=executor)
        self.assertTrue(shell.run(["git", "push"], mutating=True).skipped)
        self.assertEqual(shell.run(["git", "status"]).stdout, "x")
        self.assertEqual(seen, [["git", "status"]])

    def test_launch_agent_runs_weekly_after_dependabot(self) -> None:
        plist = upkeep.launch_agent_plist(
            python="/opt/homebrew/bin/python3",
            script=Path("/s/review/scripts/dependabot_upkeep.py"),
            state=Path("/s"),
            path_env="/opt/homebrew/bin:/usr/bin",
            exclude=[2238],
        )
        self.assertEqual(plist["Label"], "com.rundale.dependabot-upkeep")
        self.assertEqual(plist["StartCalendarInterval"]["Weekday"], 1)
        self.assertEqual(plist["ProcessType"], "Standard")
        args = plist["ProgramArguments"]
        self.assertEqual(args[:3], ["/usr/bin/caffeinate", "-i", "/opt/homebrew/bin/python3"])
        self.assertEqual(args[4:], ["run", "--self-update", "--exclude", "2238"])
        self.assertEqual(plist["EnvironmentVariables"]["PATH"], "/opt/homebrew/bin:/usr/bin")
        hour, minute = upkeep.schedule_local(9, 0)
        self.assertTrue(0 <= hour < 24 and 0 <= minute < 60)


class FlowTests(UpkeepCase):
    def test_green_pr_behind_main_is_updated_then_gatekept_by_agy(self) -> None:
        self.world.add(10, behind=3)
        self.world.gatekeeper[10] = self.world.approve_and_merge(10)
        outcome = self.outcome(10)
        self.assertEqual(outcome.status, "merged")
        self.assertFalse(outcome.app_input)
        self.assertEqual(len(self.world.ran("gh", "api", "-X", "PUT")), 1)
        gatekeeper = self.world.ran("env")[0]
        self.assertEqual(gatekeeper[-1], "Run the gatekeeper skill on PR 10.")
        self.assertIn(
            ("--model", upkeep.AGY_MODEL), list(zip(gatekeeper, gatekeeper[1:], strict=False))
        )
        for name in upkeep.AGY_ENV_UNSET:  # subscription sign-in, never an API key
            self.assertIn(("-u", name), list(zip(gatekeeper, gatekeeper[1:], strict=False)))
        self.assertEqual(self.world.ran("just"), [])
        self.assertEqual(self.world.ran("claude"), [])

    def test_current_green_pr_is_not_updated(self) -> None:
        self.world.add(11)
        self.world.gatekeeper[11] = self.world.verdict(11, "handoff")
        outcome = self.outcome(11)
        self.assertEqual(outcome.status, "handoff")
        self.assertEqual(self.world.ran("gh", "api", "-X", "PUT"), [])

    def test_dry_run_plans_without_changing_anything(self) -> None:
        self.world.add(12, behind=2)
        self.world.add(13, behind=4, classify=1)
        self.world.add(14)
        self.world.checks[14] = [[{"name": "CI gate", "bucket": "fail", "link": "l"}]]
        routine = self.make(dry_run=True)
        outcomes, summary = routine.run(only=set(), exclude=set())
        routine.post_summary(summary)
        mutated = [cmd for cmd, _ in self.world.calls for m in MUTATING if cmd[: len(m)] == m]
        self.assertEqual(mutated, [])
        self.assertEqual([o.status for o in outcomes], ["planned", "planned", "red"])
        self.assertIn("would update from main (2 commits behind)", outcomes[0].notes)
        self.assertIn("would run mobile-verify --phase all and record the pass", outcomes[1].notes)
        self.assertIn("Dependabot upkeep dry run", summary)
        self.assertIn("would comment the summary", self.log.getvalue())

    def test_app_input_pr_is_merged_locally_verified_pushed_then_recorded(self) -> None:
        self.world.add(20, behind=5, classify=upkeep.NEEDS_PASS)
        self.world.gatekeeper[20] = self.world.approve_and_merge(20)
        outcome = self.outcome(20)
        self.assertEqual(outcome.status, "merged")
        self.assertTrue(outcome.app_input)
        order = [
            cmd[:2]
            for cmd, _ in self.world.calls
            if cmd[:2] in {("git", "merge"), ("just", "mobile-verify"), ("git", "push"), ("just", "mobile-verify-record")}
        ]  # fmt: skip
        self.assertEqual(
            order,
            [("git", "merge"), ("just", "mobile-verify"), ("git", "push"), ("just", "mobile-verify-record")],
        )  # fmt: skip
        self.assertEqual(self.world.ran("gh", "api", "-X", "PUT"), [])  # pushed, not update-branch

    def test_recorded_app_pr_behind_main_reuses_its_pass_when_main_changed_no_input(self) -> None:
        self.world.add(24, behind=2, classify=upkeep.RECORDED)
        self.world.gatekeeper[24] = self.world.approve_and_merge(24)
        outcome = self.outcome(24)
        self.assertEqual(outcome.status, "merged")
        self.assertIn("main changed no app input; the recorded pass still applies", outcome.notes)
        self.assertEqual(self.world.ran("just"), [])
        self.assertEqual(len(self.world.ran("git", "push")), 1)

    def test_recorded_app_pr_is_reverified_when_main_changed_an_input(self) -> None:
        self.world.add(25, behind=2, classify=upkeep.RECORDED)
        self.world.main_moves_inputs.add(25)
        self.world.gatekeeper[25] = self.world.approve_and_merge(25)
        self.assertEqual(self.outcome(25).status, "merged")
        self.assertEqual(len(self.world.ran("just", "mobile-verify")), 1)
        self.assertEqual(len(self.world.ran("just", "mobile-verify-record")), 1)

    def test_recorded_current_app_pr_skips_mobile_verify(self) -> None:
        self.world.add(21, classify=upkeep.RECORDED)
        self.world.gatekeeper[21] = self.world.approve_and_merge(21)
        self.assertEqual(self.outcome(21).status, "merged")
        self.assertEqual(self.world.ran("just"), [])

    def test_unrecorded_current_app_pr_records_and_reruns_ci(self) -> None:
        self.world.add(22, classify=upkeep.NEEDS_PASS)
        self.world.runs = [{"databaseId": 5, "status": "completed", "conclusion": "failure"}]
        self.world.gatekeeper[22] = self.world.approve_and_merge(22)
        self.assertEqual(self.outcome(22).status, "merged")
        self.assertEqual(len(self.world.ran("just", "mobile-verify-record")), 1)
        self.assertEqual(self.world.ran("git", "push"), [])
        self.assertEqual(
            self.world.ran("gh", "run", "rerun"),
            [("gh", "run", "rerun", "5", "-R", REPO, "--failed")],
        )

    def test_mobile_verify_waits_for_another_run(self) -> None:
        self.world.add(23, classify=upkeep.NEEDS_PASS)
        self.world.pgrep = [0, 0, 1]  # found twice, then gone
        self.world.gatekeeper[23] = self.world.approve_and_merge(23)
        self.outcome(23)
        self.assertEqual(len(self.world.ran("pgrep")), 3)
        self.assertGreaterEqual(self.clock[0], 2 * upkeep.POLL)

    def test_a_failing_pgrep_is_an_error_not_a_free_slot(self) -> None:
        self.world.add(26, classify=upkeep.NEEDS_PASS)
        self.world.pgrep = [3]
        outcome = self.outcome(26)
        self.assertEqual(outcome.status, "error")
        self.assertEqual(self.world.ran("just"), [])

    def test_red_pr_gets_one_fix_then_the_agy_gatekeeper(self) -> None:
        self.world.add(30)
        red = [{"name": "CI gate", "bucket": "fail", "link": "l"}]
        green = [{"name": "CI gate", "bucket": "pass", "link": "l"}]
        self.world.checks[30] = [red, green]
        self.world.fix_commits[30] = 1
        self.world.gatekeeper[30] = self.world.approve_and_merge(30)
        outcome = self.outcome(30)
        self.assertEqual(outcome.status, "merged")
        self.assertIn("(agy)", outcome.detail)
        self.assertEqual(len(self.world.ran("claude", "-p")), 1)  # the fix, not a gatekeeper
        self.assertEqual(len(self.world.ran("env")), 1)  # one agy gatekeeper
        self.assertEqual(len(self.world.ran("git", "push")), 1)

    def test_a_fix_that_touches_app_inputs_is_verified_and_recorded(self) -> None:
        self.world.add(34)  # a tooling bump: no app input until the fix
        red = [{"name": "CI gate", "bucket": "fail", "link": "l"}]
        green = [{"name": "CI gate", "bucket": "pass", "link": "l"}]
        self.world.checks[34] = [red, green]
        self.world.fix_commits[34] = 1
        self.world.fix_app_inputs.add(34)
        self.world.gatekeeper[34] = self.world.approve_and_merge(34)
        outcome = self.outcome(34)
        self.assertEqual(outcome.status, "merged")
        self.assertTrue(outcome.app_input)
        self.assertIn("the fix changes app inputs", outcome.notes)
        order = [cmd[:2] for cmd, _ in self.world.calls if cmd[0] in ("just", "git") and cmd[1] in ("mobile-verify", "push", "mobile-verify-record")]  # fmt: skip
        self.assertEqual(
            order, [("just", "mobile-verify"), ("git", "push"), ("just", "mobile-verify-record")]
        )

    def test_red_pr_left_open_when_the_fix_makes_no_commit(self) -> None:
        self.world.add(31)
        self.world.checks[31] = [[{"name": "CI gate", "bucket": "fail", "link": "l"}]]
        outcome = self.outcome(31)
        self.assertEqual(outcome.status, "red")
        self.assertIn("the fix attempt made no commit", outcome.notes)
        self.assertEqual(self.world.ran("env"), [])
        self.assertEqual(self.world.ran("git", "push"), [])
        self.assertEqual(len(self.world.ran("claude")), 1)  # only the fix attempt

    def test_no_fix_flag_reports_red_without_trying(self) -> None:
        self.world.add(32)
        self.world.checks[32] = [[{"name": "CI gate", "bucket": "fail", "link": "l"}]]
        self.assertEqual(self.outcome(32, fix=False).status, "red")
        self.assertEqual(self.world.ran("claude"), [])
        self.assertEqual(self.world.ran("env"), [])  # no agy gatekeeper

    def test_checks_not_yet_registered_count_as_pending(self) -> None:
        self.world.add(35)
        green = [{"name": "CI gate", "bucket": "pass", "link": "l"}]
        self.world.checks[35] = ["none-yet", "api-error", green]  # one transient read failure
        self.world.gatekeeper[35] = self.world.approve_and_merge(35)
        self.assertEqual(self.outcome(35).status, "merged")

    def test_repeated_check_read_failures_are_an_error_not_a_timeout(self) -> None:
        for number, failure in ((36, "api-error"), (37, "garbled")):
            self.world.add(number)
            self.world.checks[number] = [failure]
            outcome = self.outcome(number)
            self.assertEqual(outcome.status, "error", failure)
            self.assertIn("gh pr checks", outcome.detail)
        self.assertEqual(self.world.ran("claude"), [])
        self.assertEqual(self.world.ran("env"), [])  # no agy gatekeeper

    def test_checks_that_never_finish_time_out(self) -> None:
        self.world.add(33)
        self.world.checks[33] = [[{"name": "CI gate", "bucket": "pending", "link": "l"}]]
        self.assertEqual(self.outcome(33).status, "timeout")
        self.assertEqual(self.world.ran("claude"), [])
        self.assertEqual(self.world.ran("env"), [])  # no agy gatekeeper

    def test_a_base_merge_keeps_the_claude_gatekeeper(self) -> None:
        self.world.add(42)
        self.world.commits[42].append(
            {"commit": {"message": "Merge"}, "author": {"login": "dmooney"}, "parents": [{}, {}]}
        )
        self.world.gatekeeper[42] = self.world.approve_and_merge(42)
        outcome = self.outcome(42)
        self.assertEqual(outcome.status, "merged")
        self.assertIn("(agy)", outcome.detail)

    def test_conflict_asks_dependabot_to_rebase_or_recreate(self) -> None:
        self.world.add(40, behind=1)
        self.world.conflicts.add(40)
        outcome = self.outcome(40)
        self.assertEqual(outcome.status, "conflict")
        self.assertEqual(self.world.ran("gh", "pr", "comment")[0][-1], "@dependabot rebase")
        self.world.add(41, mergeable="CONFLICTING")
        self.world.commits[41].append(
            {"commit": {"message": "Merge"}, "author": {"login": "dmooney"}}
        )
        self.outcome(41)
        self.assertEqual(self.world.ran("gh", "pr", "comment")[1][-1], "@dependabot recreate")

    def test_owner_label_exclusions_and_drafts_are_left_alone(self) -> None:
        self.world.add(50, labels=[{"name": "needs-owner"}])
        self.world.add(51)
        self.world.add(52, isDraft=True)
        outcomes, _ = self.make().run(only=set(), exclude={51})
        self.assertEqual([o.status for o in outcomes], ["owner", "skipped", "skipped"])
        touched = [c for c, _ in self.world.calls if c[:2] not in {("gh", "pr")}]
        self.assertEqual(touched, [])

    def test_missing_gatekeeper_verdict_is_an_error(self) -> None:
        self.world.add(60)
        self.world.gatekeeper[60] = lambda family: None
        outcome = self.outcome(60)
        self.assertEqual(outcome.status, "error")
        self.assertIn("no verdict", outcome.detail)

    def test_changed_release_workflow_is_dry_run_before_the_gatekeeper(self) -> None:
        self.world.add(70)
        self.world.files[70] = [".github/workflows/release.yml", ".github/workflows/ci.yml"]
        self.world.workflows = {
            ".github/workflows/AGENTS.md": "# Notes\n- **a:** [not yaml\n",
            ".github/workflows/release.yml": RELEASE,
            ".github/workflows/ci.yml": CI,
            ".github/workflows/full-ci.yml": FULL_CI,
        }
        head = self.world.prs[70]["headRefOid"]
        self.world.runs = [
            {
                "databaseId": 9,
                "headSha": head,
                "status": "completed",
                "conclusion": "success",
                "url": "r",
            }
        ]
        self.world.gatekeeper[70] = self.world.approve_and_merge(70)
        self.assertEqual(self.outcome(70).status, "merged")
        dispatch = self.world.ran("gh", "workflow", "run")[0]
        self.assertIn("release.yml", dispatch)
        self.assertIn("dry_run=true", dispatch)
        self.assertEqual(len(self.world.ran("gh", "pr", "comment")), 1)

    def test_a_release_dry_run_that_cannot_start_is_an_error(self) -> None:
        self.world.add(73)
        self.world.files[73] = [".github/workflows/release.yml"]
        self.world.workflows = {".github/workflows/release.yml": RELEASE}
        self.world.dispatch_fails = True
        outcome = self.outcome(73)
        self.assertEqual(outcome.status, "error")
        self.assertEqual(self.world.ran("claude"), [])
        self.assertEqual(self.world.ran("env"), [])  # no agy gatekeeper

    def test_unreadable_workflows_are_an_error_not_a_skipped_probe(self) -> None:
        self.world.add(72)
        self.world.files[72] = [".github/workflows/release.yml"]
        self.world.workflows = {".github/workflows/release.yml": RELEASE}
        self.world.ls_tree_fails = True
        outcome = self.outcome(72)
        self.assertEqual(outcome.status, "error")
        self.assertIn("ls-tree", outcome.detail)
        self.assertEqual(self.world.ran("gh", "workflow", "run"), [])
        self.assertEqual(self.world.ran("claude"), [])
        self.assertEqual(self.world.ran("env"), [])  # no agy gatekeeper

    def test_failed_release_dry_run_blocks_the_gatekeeper(self) -> None:
        self.world.add(71)
        self.world.files[71] = [".github/workflows/release.yml"]
        self.world.workflows = {".github/workflows/release.yml": RELEASE}
        head = self.world.prs[71]["headRefOid"]
        self.world.runs = [
            {
                "databaseId": 9,
                "headSha": head,
                "status": "completed",
                "conclusion": "failure",
                "url": "r",
            }
        ]
        outcome = self.outcome(71)
        self.assertEqual(outcome.status, "red")
        self.assertEqual(self.world.ran("claude"), [])
        self.assertEqual(self.world.ran("env"), [])  # no agy gatekeeper

    def test_summary_goes_to_an_existing_tracking_issue(self) -> None:
        self.world.dispatch_fails = True
        self.world.issues = [{"number": 5, "body": f"{upkeep.TRACKING_MARKER}\nreports"}]
        self.make().post_summary("hello")
        self.assertEqual(self.world.ran("gh", "issue", "create"), [])
        self.assertEqual(self.world.ran("gh", "issue", "comment")[0][3], "5")

    def test_an_unpinned_tracking_issue_is_pinned_again(self) -> None:
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        self.make().post_summary("hello")
        self.assertEqual(self.world.ran("gh", "issue", "pin"), [])
        self.world.pinned = False
        self.make().post_summary("hello")
        self.assertEqual(self.world.ran("gh", "issue", "pin")[0][3], "5")

    def test_tracking_issue_is_created_and_pinned_once(self) -> None:
        self.world.dispatch_fails = True
        self.make().post_summary("hello")
        self.assertEqual(len(self.world.ran("gh", "issue", "create")), 1)
        self.assertEqual(self.world.ran("gh", "issue", "pin")[0][3], "77")
        self.assertEqual(self.world.ran("gh", "issue", "comment")[0][3], "77")


class SweepAndSummaryTests(UpkeepCase):
    def test_final_sweep_updates_red_prs_but_never_owner_held_ones(self) -> None:
        red = [{"name": "CI gate", "bucket": "fail", "link": "l"}]
        for number in (80, 81, 82, 83):
            self.world.add(number)
        self.world.checks[80] = [red]
        self.world.checks[83] = [red]

        def merge_and_move_main(_family: str) -> None:
            self.world.merged.add(81)
            for other in (80, 82, 83):
                self.world.behind[other] = 1
            # The owner takes PR 83 while the run is going.
            self.world.prs[83]["labels"] = [{"name": "needs-owner"}]

        self.world.gatekeeper[81] = merge_and_move_main
        self.world.gatekeeper[82] = self.world.verdict(82, "handoff")
        outcomes, _ = self.make(fix=False).run(only=set(), exclude=set())
        self.assertEqual([o.status for o in outcomes], ["red", "merged", "handoff", "owner"])
        updated = [cmd[4].split("/")[-2] for cmd in self.world.ran("gh", "api", "-X", "PUT")]
        # 82 was behind when processed and 83 was labelled before its turn; after
        # the run only the red, unheld 80 is refreshed.
        self.assertEqual(updated, ["82", "80"])

    def test_a_pr_that_cannot_be_read_does_not_stop_the_others(self) -> None:
        self.world.add(90)
        self.world.add(91)
        self.world.view_fail.add(90)
        self.world.gatekeeper[91] = self.world.approve_and_merge(91)
        outcomes, summary = self.make().run(only=set(), exclude=set())
        self.assertEqual([o.status for o in outcomes], ["error", "merged"])
        self.assertIn("could not re-read it", outcomes[0].detail)
        self.assertIn("#90", summary)
        self.assertIn("#91", summary)

    def test_failed_conflict_recovery_does_not_stop_the_others(self) -> None:
        self.world.add(93, mergeable="CONFLICTING")
        self.world.add(94, mergeable="CONFLICTING")
        self.world.add(95)
        self.world.commits_fail.add(93)  # cannot read who pushed to the branch
        self.world.comment_fail.add(94)  # cannot post the rebase request
        self.world.gatekeeper[95] = self.world.approve_and_merge(95)
        routine = self.make()
        outcomes, summary = routine.run(only=set(), exclude=set())
        routine.post_summary(summary)
        self.assertEqual([o.status for o in outcomes], ["error", "error", "merged"])
        self.assertIn("could not ask Dependabot to rebuild it", outcomes[0].detail)
        self.assertIn("could not ask Dependabot to rebuild it", outcomes[1].detail)
        self.assertEqual(len(self.world.ran("gh", "workflow", "run")), 1)

    def test_github_failures_outside_a_pr_are_reported_in_the_summary(self) -> None:
        self.world.add(92)
        self.world.checks[92] = [[{"name": "CI gate", "bucket": "fail", "link": "l"}]]
        self.world.list_ok = 1  # the inventory works; the final sweep's read fails
        routine = self.make(fix=False)
        outcomes, summary = routine.run(only=set(), exclude=set())
        self.assertEqual([o.status for o in outcomes], ["red"])
        self.assertEqual(len(routine.problems), 1)
        self.assertIn("### Problems", summary)
        self.assertIn("final update could not list", summary)
        self.world.list_ok = 0  # now the inventory itself fails
        routine = self.make()
        outcomes, summary = routine.run(only=set(), exclude=set())
        self.assertEqual(outcomes, [])
        self.assertIn("could not list the open Dependabot pull requests", summary)

    def test_summary_is_dispatched_to_the_posting_workflow(self) -> None:
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        self.make().post_summary("hello")
        (cmd,) = self.world.ran("gh", "workflow", "run")
        self.assertEqual(cmd[3], "upkeep-summary.yml")
        self.assertEqual(cmd[cmd.index("--ref") + 1], "main")
        self.assertIn("issue=5", cmd)
        self.assertIn("summary=hello", cmd)
        self.assertEqual(self.world.ran("gh", "issue", "comment"), [])

    def test_dry_run_plans_the_dispatch_without_polling_or_falling_back(self) -> None:
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        routine = self.make(dry_run=True)
        routine.post_summary("hello")
        self.assertEqual(self.world.ran("gh", "run", "list"), [])
        self.assertEqual(self.world.ran("gh", "issue", "comment"), [])
        self.assertIn("would run: gh workflow run upkeep-summary.yml", self.log.getvalue())
        self.assertNotIn("dispatch failed", self.log.getvalue())

    def test_dispatched_summary_is_capped_below_the_input_limit(self) -> None:
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        self.make().post_summary("x" * 100_000)
        (cmd,) = self.world.ran("gh", "workflow", "run")
        value = next(a for a in cmd if a.startswith("summary="))[len("summary=") :]
        self.assertLessEqual(len(value), int(upkeep.SUMMARY_DISPATCH_LIMIT * 0.9))
        self.assertTrue(value.endswith("chars]"))

    def test_failed_dispatch_falls_back_to_a_direct_comment_and_logs_it(self) -> None:
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        self.world.dispatch_fails = True
        routine = self.make()
        routine.post_summary("hello")
        self.assertEqual(self.world.ran("gh", "issue", "comment")[0][3], "5")
        self.assertIn("dispatch failed", self.log.getvalue())

    def test_dispatch_accepted_but_run_failed_falls_back_and_logs_it(self) -> None:
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        self.world.summary_conclusion = "failure"
        self.make().post_summary("hello")
        self.assertEqual(self.world.ran("gh", "issue", "comment")[0][3], "5")
        self.assertIn("ended failure", self.log.getvalue())

    def test_dispatch_accepted_but_run_never_finishes_falls_back(self) -> None:
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        self.world.summary_status = "in_progress"
        self.world.summary_conclusion = None
        self.make().post_summary("hello")
        self.assertEqual(self.world.ran("gh", "issue", "comment")[0][3], "5")
        self.assertIn("did not finish in time", self.log.getvalue())

    def test_undelivered_summary_fails_the_run(self) -> None:
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        self.world.dispatch_fails = True
        self.world.comment_rc = 1
        with self.assertRaises(upkeep.CommandError):
            self.make().post_summary("hello")

    def test_summary_is_capped_below_githubs_body_limit(self) -> None:
        self.world.dispatch_fails = True
        self.world.issues = [{"number": 5, "body": upkeep.TRACKING_MARKER}]
        self.make().post_summary("x" * 100_000)
        body = self.world.ran("gh", "issue", "comment")[0][-1]
        self.assertLessEqual(len(body), int(upkeep.GITHUB_BODY_LIMIT * 0.9))
        self.assertTrue(body.endswith("chars]"))
        self.assertEqual(upkeep.cap_body("short"), "short")


class ProcessTests(unittest.TestCase):
    def test_a_timeout_stops_the_commands_descendants_too(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            pidfile = Path(tmp) / "child.pid"
            script = f"sleep 60 & echo $! > {pidfile}; wait"
            result = upkeep.subprocess_executor(["sh", "-c", script], None, 1, None)
            self.assertEqual(result.returncode, 124)
            child = int(pidfile.read_text())
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                try:
                    os.kill(child, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.1)
            else:
                os.kill(child, 9)
                self.fail("the backgrounded child outlived the timed-out command")

    def test_a_stopped_group_gets_time_to_clean_up(self) -> None:
        # The leader exits on SIGTERM at once; its child traps SIGTERM and needs
        # a second to clean up, as verify.py does while stopping its builds.
        with tempfile.TemporaryDirectory() as tmp:
            done = Path(tmp) / "cleaned"
            child = f"trap 'sleep 1; echo ok > {done}; exit 0' TERM; while :; do sleep 0.1; done"
            script = f"sh -c {shlex.quote(child)} & wait"
            result = upkeep.subprocess_executor(["sh", "-c", script], None, 1, None)
            self.assertEqual(result.returncode, 124)
            self.assertEqual(done.read_text(), "ok\n")

    def test_sigterm_to_the_routine_stops_the_running_command(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            pidfile = Path(tmp) / "child.pid"
            program = textwrap.dedent(f"""
                import sys
                sys.path.insert(0, {str(Path(upkeep.__file__).parent)!r})
                import dependabot_upkeep as upkeep
                upkeep.handle_termination()
                try:
                    upkeep.subprocess_executor(
                        ["sh", "-c", "sleep 60 & echo $! > {pidfile}; wait"], None, None, None
                    )
                except upkeep.Terminated:
                    sys.exit(143)
            """)
            routine = subprocess.Popen([sys.executable, "-c", program])
            deadline = time.monotonic() + 10
            while not pidfile.exists() or not pidfile.read_text().strip():
                self.assertLess(time.monotonic(), deadline, "the command never started")
                time.sleep(0.05)
            child = int(pidfile.read_text())
            routine.send_signal(signal.SIGTERM)
            self.assertEqual(routine.wait(timeout=30), 143)
            with self.assertRaises(ProcessLookupError):
                os.kill(child, 0)

    def test_stdin_is_closed_and_output_captured(self) -> None:
        result = upkeep.subprocess_executor(["sh", "-c", "cat; echo done"], None, 10, None)
        self.assertEqual((result.returncode, result.stdout), (0, "done\n"))


class MergeOwnershipTests(unittest.TestCase):
    def test_the_routine_never_merges_a_pull_request_itself(self) -> None:
        source = Path(upkeep.__file__).read_text(encoding="utf-8")
        for pattern in (
            r'"pr",\s*"merge"',
            r"gh pr merge",
            r"--auto\b",
            r"/merge\b",
            r"enablePullRequestAutoMerge|mergePullRequest",
        ):
            self.assertNotRegex(source, pattern)


class MainTests(unittest.TestCase):
    def test_second_run_exits_while_the_first_holds_the_lock(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp)
            (state / "review").mkdir()
            with (state / "upkeep.lock").open("w") as held:
                fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
                self.assertEqual(upkeep.main(["--state", str(state), "run"]), 3)

    def test_self_update_waits_for_the_lock_before_touching_the_checkout(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp)
            (state / "review").mkdir()
            with (
                (state / "upkeep.lock").open("w") as held,
                patch.object(
                    upkeep, "self_update", side_effect=AssertionError("updated without the lock")
                ),
            ):
                fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
                self.assertEqual(upkeep.main(["--state", str(state), "run", "--self-update"]), 3)

    def test_self_update_hands_the_held_lock_to_the_new_process(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp)
            (state / "review").mkdir()
            seen: dict[str, Any] = {}

            def execv(_exe: str, argv: list[str]) -> None:
                # Runs where the new process would start, with the lock still held.
                fd = int(argv[argv.index("--lock-fd") + 1])
                seen["argv"], seen["inheritable"] = argv, os.get_inheritable(fd)
                # Re-locking the inherited descriptor (the same open file
                # description) succeeds; any other open of the lock fails.
                inherited = upkeep.acquire_run_lock(state, os.dup(fd))
                seen["inherited"] = inherited is not None
                seen["other"] = upkeep.acquire_run_lock(state, None)
                if inherited is not None:
                    inherited.close()
                raise SystemExit(0)

            with (
                patch.object(upkeep.subprocess, "run") as git,
                patch.object(upkeep.os, "execv", side_effect=execv),
                self.assertRaises(SystemExit),
            ):
                upkeep.main(["--state", str(state), "run", "--self-update"])
            self.assertEqual(git.call_count, 3)  # fetch, reset, provision .venv-dev
            self.assertNotIn("--self-update", seen["argv"])
            self.assertTrue(seen["argv"][0].endswith("review/.venv-dev/bin/python"))
            self.assertTrue(seen["argv"][1].endswith("review/scripts/dependabot_upkeep.py"))
            self.assertTrue(seen["inheritable"])
            self.assertTrue(seen["inherited"])
            self.assertIsNone(seen["other"])

    def test_real_run_requires_install(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(upkeep.main(["--state", tmp, "run"]), 2)


if __name__ == "__main__":
    unittest.main()
