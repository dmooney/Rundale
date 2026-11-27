#!/usr/bin/env python3
"""Tests for the upkeep routine's security-fix step (issue #2342)."""

from __future__ import annotations

import datetime as dt
import importlib
import sys
import unittest
from collections.abc import Sequence
from pathlib import Path
from typing import TYPE_CHECKING, Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

if TYPE_CHECKING:
    import dependabot_upkeep as upkeep
    import security_upkeep as sec
    import test_dependabot_upkeep as base
else:
    upkeep = importlib.import_module("dependabot_upkeep")
    sec = importlib.import_module("security_upkeep")
    base = importlib.import_module("test_dependabot_upkeep")

Result = upkeep.Result
TODAY = dt.date(2026, 11, 26)
OSV_BODY = (
    "<!-- ci-red: workflow=osv-scanner.yml -->\n<!-- ci-red-run: 99 -->\n"
    "The OSV-Scanner workflow failed on main: rustix 1.0.0 RUSTSEC-2026-0001"
)
AUDIT_BODY = "<!-- ci-red: workflow=audit.yml -->\nThe Security audit failed on main."
NEW_PR = 900


def issue(number: int, body: str, *labels: str, title: str = "CI red on main") -> dict[str, Any]:
    return {
        "number": number,
        "title": f"{title} #{number}",
        "body": body,
        "labels": [{"name": name} for name in labels],
    }


OSV_ISSUE = issue(2403, OSV_BODY, "ci-red", "infra", "P1")


class PureTests(unittest.TestCase):
    def test_parse_findings_keeps_scanner_issues_about_main(self) -> None:
        issues = [
            OSV_ISSUE,
            issue(2404, AUDIT_BODY, "ci-red"),
            issue(2405, "<!-- ci-red: workflow=ci.yml -->", "ci-red"),  # not a security scan
            issue(2406, "<!-- ci-red: workflow=audit.yml@test -->", "ci-red"),  # another branch
            issue(2407, OSV_BODY, "bug"),  # no ci-red label
            issue(2408, "no marker", "ci-red"),
            issue(2409, OSV_BODY, "ci-red"),  # a duplicate of the OSV finding
        ]
        todo, held = sec.parse_findings(issues)
        self.assertEqual(
            [(f.number, f.workflow) for f in todo], [(2403, "osv-scanner.yml"), (2404, "audit.yml")]
        )
        self.assertEqual(held, [])

    def test_the_watchers_own_issue_body_is_recognised(self) -> None:
        # The contract with repo_health.py: its marker is how this step finds a finding.
        health = importlib.import_module("repo_health")
        for file in sec.SCANNERS:
            state = health.WorkflowState(file, "Scan", "red", {"id": 7}, "2026-11-11T00:00:00Z")
            body = health.render_ci_red_body(state, [])
            todo, _ = sec.parse_findings([issue(5, body, "ci-red")])
            self.assertEqual([f.workflow for f in todo], [file])

    def test_needs_owner_parks_a_finding(self) -> None:
        todo, held = sec.parse_findings([issue(2403, OSV_BODY, "ci-red", "needs-owner")])
        self.assertEqual(todo, [])
        self.assertEqual([f.number for f in held], [2403])

    def test_classify_lockfile_and_manifest_changes_are_mergeable(self) -> None:
        paths = [
            "limerick/Cargo.lock",
            "limerick/apps/ui/package.json",
            "limerick/apps/ui/pnpm-lock.yaml",
        ]
        self.assertEqual(sec.classify_change(paths).action, "merge")

    def test_classify_ignores_and_source_go_to_the_owner(self) -> None:
        ignore = sec.classify_change(["limerick/Cargo.lock", "limerick/osv-scanner.toml"])
        self.assertEqual(ignore.action, "owner")
        self.assertIn("ignore", ignore.reason)
        audit = sec.classify_change(["limerick/.cargo/audit.toml"])
        self.assertEqual(audit.action, "owner")
        source = sec.classify_change(["limerick/Cargo.lock", "limerick/crates/core/src/lib.rs"])
        self.assertEqual(source.action, "owner")
        self.assertIn("lib.rs", source.reason)
        workflow = sec.classify_change([".github/workflows/osv-scanner.yml"])
        self.assertEqual(workflow.action, "owner")

    def test_only_real_requirements_files_are_dependency_files(self) -> None:
        for name in ("requirements.txt", "requirements-dev.txt", "requirements.in"):
            self.assertEqual(sec.classify_change([f"bench/{name}"]).action, "merge", name)
        for name in ("requirements_check.py", "requirements.md", "requirementsfoo"):
            self.assertEqual(sec.classify_change([f"bench/{name}"]).action, "owner", name)

    def test_classify_no_change_is_none(self) -> None:
        self.assertEqual(sec.classify_change([]).action, "none")

    def test_ignore_problems_need_reason_and_near_expiry(self) -> None:
        good = (
            '+[[IgnoredVulns]]\n+id = "RUSTSEC-2026-1"\n+ignoreUntil = 2027-01-28T00:00:00Z\n'
            '+reason = "unmaintained, no fixed release; reassess by the expiry"\n'
        )
        self.assertEqual(sec.ignore_problems(good, TODAY), [])
        bad = (
            '+[[IgnoredVulns]]\n+id = "A"\n+reason = "short"\n'
            '+[[IgnoredVulns]]\n+id = "B"\n+ignoreUntil = 2031-01-01T00:00:00Z\n'
            '+reason = "a long enough reason for this advisory"\n'
            '+[[IgnoredVulns]]\n+id = "C"\n+ignoreUntil = 2026-01-01T00:00:00Z\n'
            '+reason = "a long enough reason for this advisory"\n'
        )
        problems = sec.ignore_problems(bad, TODAY)
        self.assertIn("A: no reason", problems)
        self.assertIn("A: no ignoreUntil expiry", problems)
        self.assertTrue(any(p.startswith("B: expiry 2031-01-01 is over") for p in problems))
        self.assertIn("C: expiry 2026-01-01 is already past", problems)

    def test_audit_toml_ignore_is_flagged_for_lacking_an_expiry(self) -> None:
        diff = '+ignore = ["RUSTSEC-2023-0071", "RUSTSEC-2026-0387"]\n'
        self.assertTrue(any("audit.toml" in p for p in sec.ignore_problems(diff, TODAY)))

    def test_removed_lines_and_headers_are_not_additions(self) -> None:
        diff = '--- a/x.toml\n+++ b/x.toml\n-[[IgnoredVulns]]\n-id = "A"\n'
        self.assertEqual(sec.ignore_problems(diff, TODAY), [])

    def test_prompt_carries_scanner_issue_and_the_rules(self) -> None:
        finding = sec.Finding(2403, "t", "osv-scanner.yml", OSV_BODY)
        prompt = sec.fix_prompt(finding)
        self.assertIn("osv-scanner scan source -r ./", prompt)
        self.assertIn("RUSTSEC-2026-0001", prompt)
        self.assertIn("overrides", prompt)
        self.assertIn("ignoreUntil", prompt)
        self.assertIn("Do not push", prompt)
        audit = sec.fix_prompt(sec.Finding(2404, "t", "audit.yml", AUDIT_BODY))
        self.assertIn("`cargo audit` in limerick/", audit)

    def test_branch_and_marker_round_trip(self) -> None:
        finding = sec.Finding(2403, "CI red", "osv-scanner.yml", OSV_BODY)
        self.assertEqual(sec.branch_name(finding, TODAY), "upkeep/security-osv-scanner-20261126")
        decision = sec.Decision("merge", "x")
        body = sec.pr_body(finding, decision, ["limerick/Cargo.lock"], [])
        self.assertEqual(sec.pr_workflow(body), ("osv-scanner.yml", 2403))
        self.assertIn("Refs #2403", body)
        self.assertNotIn("needs-owner", body)
        pr = {"body": body, "labels": [{"name": "security-upkeep"}]}
        self.assertIs(sec.existing_pr([{"body": "x", "labels": []}, pr], finding), pr)
        self.assertIsNone(sec.existing_pr([{"body": body, "labels": []}], finding))

    def test_owner_pr_body_says_who_decides_and_lists_warnings(self) -> None:
        finding = sec.Finding(2403, "CI red", "osv-scanner.yml", OSV_BODY)
        decision = sec.Decision("owner", "it proposes a new ignore")
        body = sec.pr_body(finding, decision, ["limerick/osv-scanner.toml"], ["A: no reason"])
        self.assertIn("needs-owner", body)
        self.assertIn("A: no reason", body)

    def test_escalation_comment_names_the_reason_and_the_label(self) -> None:
        finding = sec.Finding(2403, "CI red", "osv-scanner.yml", OSV_BODY)
        text = sec.escalation_comment(finding, "no commit", log="/l")
        self.assertIn("no commit", text)
        self.assertIn("needs-owner", text)


class SecurityWorld(base.FakeWorld):
    """The fake GitHub, git and agent, plus the security step's commands."""

    def __init__(self, work: Path, review: Path):
        super().__init__(work, review)
        self.ci_red: list[dict[str, Any]] = [OSV_ISSUE]
        self.sec_prs: list[dict[str, Any]] = []
        self.changes: list[str] = ["limerick/Cargo.lock"]
        self.diff = '+version = "1.2.3"\n'
        self.dirty = ""
        self.scanner_rc = 0
        self.fix_pr_labels: list[str] = []
        self.main_green = False
        self.claude_ran = False
        self.issue_list_fails = False
        self.closed_prs: list[dict[str, Any]] = []

    def __call__(
        self, cmd: Sequence[str], cwd: Path | None, timeout: float | None, output: Path | None
    ) -> Result:
        cmd = tuple(cmd)
        handled = self.security(cmd, cwd)
        if handled is None:
            return super().__call__(cmd, cwd, timeout, output)
        self.calls.append((cmd, cwd))
        return handled

    def security(self, cmd: tuple[str, ...], cwd: Path | None) -> Result | None:
        if cmd[:3] == ("gh", "issue", "list") and "--label" in cmd:
            if self.issue_list_fails:
                return Result(1, "", "HTTP 404: Not Found")
            return self.ok(self.ci_red)
        if cmd[:3] == ("gh", "pr", "list") and "--label" in cmd:
            return self.ok(self.closed_prs if "closed" in cmd else self.sec_prs)
        if cmd[:3] == ("gh", "pr", "list") and "--author" in cmd:  # Dependabot's PRs only
            return self.ok(
                [
                    p
                    for n, p in sorted(self.prs.items())
                    if "security-upkeep" not in {lab["name"] for lab in p["labels"]}
                ]
            )
        if cmd[:3] == ("gh", "label", "create"):
            return self.ok()
        if cmd[:3] == ("gh", "pr", "edit"):
            return self.ok()
        if cmd[:3] == ("gh", "pr", "close"):
            return self.ok()
        if cmd[:3] == ("gh", "issue", "edit"):
            return self.ok()
        if cmd[:3] == ("gh", "pr", "create"):
            self.fix_pr_labels = [cmd[i + 1] for i, a in enumerate(cmd) if a == "--label"]
            branch = cmd[cmd.index("--head") + 1]
            self.add(NEW_PR, headRefName=branch, labels=[{"name": n} for n in self.fix_pr_labels])
            self.gatekeeper.setdefault(NEW_PR, self.approve_and_merge(NEW_PR))
            self.files[NEW_PR] = list(self.changes)
            return Result(0, f"https://github.com/{base.REPO}/pull/{NEW_PR}\n")
        if cmd[:2] == ("git", "checkout") and "-B" in cmd:
            return Result(0)
        if cmd[:3] == ("git", "diff", "--name-only"):
            return Result(0, "\n".join(self.changes))
        if cmd[:2] == ("git", "diff"):
            return Result(0, self.diff)
        if cmd[:2] == ("git", "status"):
            return Result(0, self.dirty)
        if cmd[:2] == ("git", "push") and "upkeep/security-" in cmd[-1]:
            branch = cmd[-1].removeprefix("HEAD:refs/heads/")
            for number, pr in self.prs.items():
                if pr["headRefName"] == branch:
                    self.new_head(number)  # the push moves an existing PR's head
            return Result(0)
        if cmd[0] in ("osv-scanner", "cargo"):
            if self.claude_ran:
                return Result(self.scanner_rc)
            # Before the fix attempt: main is red unless a test says it is already green.
            return Result(0 if self.main_green else (127 if self.scanner_rc == 127 else 1))
        if cmd[0] == "claude" and "is red on main" in cmd[-1]:
            assert cwd == self.work
            self.claude_ran = True
            return Result(0)
        return None


class SecurityFlowTests(base.UpkeepCase):
    world: SecurityWorld

    def setUp(self) -> None:
        super().setUp()
        self.world = SecurityWorld(self.work, self.review)

    def make(self, *, dry_run: bool = False, fix: bool = True, security: bool = True) -> Any:
        def sleep(seconds: float) -> None:
            self.clock[0] += seconds

        shell = upkeep.Shell(dry_run=dry_run, log=self.log, executor=self.world)
        return upkeep.Upkeep(
            shell,
            repo=base.REPO,
            state=Path(self.tmp.name) / "state",
            review=self.review,
            work=self.work,
            fix=fix,
            security=security,
            sleep=sleep,
            clock=lambda: self.clock[0],
        )

    def run_security(self, **kwargs: Any) -> list[Any]:
        outcomes, _ = self.make(**kwargs).run(only=set(), exclude=set())
        return outcomes

    def test_red_osv_scan_gets_a_fix_pr_that_the_gatekeeper_merges(self) -> None:
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "merged")
        self.assertIn("opened for #2403", outcome.notes[0])
        self.assertEqual(self.world.fix_pr_labels, ["security-upkeep"])
        calls = [c for c, _ in self.world.calls]
        prompt = next(c[-1] for c in calls if c[0] == "claude" and "is red on main" in c[-1])
        self.assertIn("osv-scanner scan source", prompt)
        scan = calls.index(("osv-scanner", "scan", "source", "-r", "./"))
        push = next(i for i, c in enumerate(calls) if c[:2] == ("git", "push"))
        create = next(i for i, c in enumerate(calls) if c[:3] == ("gh", "pr", "create"))
        self.assertLess(scan, push)  # the scan is proven before anything is pushed
        self.assertLess(push, create)
        self.assertEqual(len(self.world.gatekept()), 1)
        checkout = next(c for c in calls if c[:2] == ("git", "checkout") and "-B" in c)
        self.assertEqual(checkout[-1], "origin/main")  # a fresh branch from main
        self.assertTrue(checkout[-2].startswith("upkeep/security-osv-scanner-"))

    def test_audit_scan_runs_in_the_limerick_directory(self) -> None:
        self.world.ci_red = [issue(2404, AUDIT_BODY, "ci-red")]
        self.run_security()
        scan = next((c, d) for c, d in self.world.calls if c == ("cargo", "audit"))
        self.assertEqual(scan[1], self.work / "limerick")

    def test_ignore_proposal_opens_a_needs_owner_pr_and_skips_the_gatekeeper(self) -> None:
        self.world.changes = ["limerick/Cargo.lock", "limerick/osv-scanner.toml"]
        self.world.diff = '+[[IgnoredVulns]]\n+id = "RUSTSEC-2026-1"\n+reason = "short"\n'
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "owner")
        self.assertEqual(self.world.fix_pr_labels, ["security-upkeep", "needs-owner"])
        self.assertEqual(self.world.gatekept(), [])
        body = next(c for c, _ in self.world.calls if c[:3] == ("gh", "pr", "create"))
        text = body[body.index("--body") + 1]
        self.assertIn("RUSTSEC-2026-1: no ignoreUntil expiry", text)

    def test_source_changes_are_for_the_owner_not_the_gatekeeper(self) -> None:
        self.world.changes = ["limerick/Cargo.lock", "limerick/crates/core/src/lib.rs"]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "owner")
        self.assertEqual(self.world.gatekept(), [])

    def test_no_commit_escalates_without_a_pr(self) -> None:
        self.world.changes = []
        self.world.dirty = " M limerick/Cargo.lock"
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "escalated")
        self.assertIn("uncommitted", outcome.detail)
        self.assertEqual(self.world.ran("gh", "pr", "create"), [])
        self.assertEqual(self.world.ran("git", "push"), [])
        (edit,) = self.world.ran("gh", "issue", "edit")
        self.assertEqual(edit[3], "2403")
        self.assertIn("needs-owner", edit)
        (comment,) = self.world.ran("gh", "issue", "comment")
        self.assertIn("could not", comment[-1])

    def test_uncommitted_edits_beside_commits_escalate_without_pushing(self) -> None:
        self.world.dirty = " M limerick/Cargo.lock"
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "escalated")
        self.assertIn("uncommitted", outcome.detail)
        self.assertEqual(self.world.ran("git", "push"), [])

    def test_a_conflicting_security_pr_is_closed_and_redone_not_sent_to_dependabot(self) -> None:
        finding = sec.Finding(2403, "t", "osv-scanner.yml", OSV_BODY)
        self.world.add(NEW_PR - 1, mergeable="CONFLICTING")
        body = sec.pr_body(finding, sec.Decision("merge", "x"), ["limerick/Cargo.lock"], [])
        self.world.sec_prs = [
            {
                **self.world.prs[NEW_PR - 1],
                "body": body,
                "labels": [{"name": "security-upkeep"}],
            }
        ]
        del self.world.prs[NEW_PR - 1]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "merged")
        (tag,) = self.world.ran("gh", "pr", "edit")
        self.assertIn("upkeep-redone", tag)
        (close,) = self.world.ran("gh", "pr", "close")
        self.assertIn("--delete-branch", close)
        self.assertEqual(self.world.ran("gh", "pr", "comment"), [])
        self.assertEqual(len(self.world.ran("gh", "pr", "create")), 1)

    def closed_pr(self, number: int, *labels: str, merged: str | None = None) -> dict[str, Any]:
        finding = sec.Finding(2403, "t", "osv-scanner.yml", OSV_BODY)
        body = sec.pr_body(finding, sec.Decision("merge", "x"), ["limerick/Cargo.lock"], [])
        return {
            "number": number,
            "url": f"https://github.com/{base.REPO}/pull/{number}",
            "body": body,
            "labels": [{"name": n} for n in ("security-upkeep", *labels)],
            "mergedAt": merged,
        }

    def test_a_rejected_fix_pr_ends_the_attempts_and_goes_to_the_owner(self) -> None:
        self.world.closed_prs = [self.closed_pr(850)]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "escalated")
        self.assertIn("pull/850", outcome.detail)
        self.assertEqual(self.world.ran("claude"), [])  # no new attempt
        (edit,) = self.world.ran("gh", "issue", "edit")
        self.assertIn("needs-owner", edit)
        (comment,) = self.world.ran("gh", "issue", "comment")
        self.assertIn("pull/850", comment[-1])
        text = upkeep.render_summary([outcome], when="now", dry_run=False)
        self.assertIn("### Needs attention", text)

    def test_merged_and_routine_redone_prs_are_not_rejections(self) -> None:
        self.world.closed_prs = [
            self.closed_pr(850, merged="2026-11-20T00:00:00Z"),
            self.closed_pr(851, "upkeep-redone"),
            {
                **self.closed_pr(852),
                "body": "<!-- upkeep-security: workflow=osv-scanner.yml issue=9 -->",
            },
        ]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "merged")

    def test_dry_run_reports_a_rejected_finding_without_changes(self) -> None:
        self.world.closed_prs = [self.closed_pr(850)]
        (outcome,) = self.run_security(dry_run=True)
        self.assertEqual(outcome.status, "planned")
        self.assertIn("owner", outcome.detail)
        for prefix in base.MUTATING:
            self.assertEqual(self.world.ran(*prefix), [], prefix)

    def sweep(self, *labels: str, behind: int = 2) -> list[tuple[str, ...]]:
        self.world.add(NEW_PR, behind=behind, labels=[{"name": n} for n in labels])
        self.world.sec_prs = [self.world.prs[NEW_PR]]
        outcome = upkeep.Outcome(NEW_PR, "fix", "u", "red", "red: CI gate")
        self.make().final_sweep([outcome], set())
        return self.world.ran("gh", "api", "-X")

    def test_final_sweep_brings_a_red_security_pr_current_with_main(self) -> None:
        self.assertEqual(len(self.sweep("security-upkeep")), 1)

    def test_final_sweep_leaves_a_needs_owner_security_pr_alone(self) -> None:
        self.assertEqual(self.sweep("security-upkeep", "needs-owner"), [])

    def test_final_sweep_ignores_security_prs_when_the_step_is_off(self) -> None:
        self.world.add(NEW_PR, behind=2, labels=[{"name": "security-upkeep"}])
        self.world.sec_prs = [self.world.prs[NEW_PR]]
        outcome = upkeep.Outcome(NEW_PR, "fix", "u", "red", "red")
        self.make(security=False).final_sweep([outcome], set())
        self.assertEqual(self.world.ran("gh", "api", "-X"), [])

    def test_a_main_that_is_already_green_is_skipped_without_cost_or_noise(self) -> None:
        self.world.main_green = True
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "skipped")
        self.assertIn("already passes on main", outcome.detail)
        self.assertEqual(self.world.ran("claude"), [])
        self.assertEqual(self.world.ran("gh", "issue", "edit"), [])
        self.assertEqual(self.world.ran("gh", "issue", "comment"), [])
        self.assertEqual(self.world.ran("gh", "pr", "create"), [])
        text = upkeep.render_summary([outcome], when="now", dry_run=False)
        self.assertNotIn("### Needs attention", text)

    def test_a_missing_scanner_is_caught_before_any_claude_run(self) -> None:
        self.world.scanner_rc = 127
        self.run_security()
        self.assertEqual(self.world.ran("claude"), [])

    def resumed_security_pr(self, **extra: Any) -> int:
        """An open security PR from an earlier run, in the fake GitHub and the PR list."""

        number = NEW_PR - 1
        finding = sec.Finding(2403, "t", "osv-scanner.yml", OSV_BODY)
        body = sec.pr_body(finding, sec.Decision("merge", "x"), ["limerick/Cargo.lock"], [])
        self.world.add(
            number,
            headRefName="upkeep/security-osv-scanner-20261125",
            labels=[{"name": "security-upkeep"}],
            **extra,
        )
        self.world.files[number] = ["limerick/Cargo.lock"]
        self.world.gatekeeper[number] = self.world.approve_and_merge(number)
        self.world.sec_prs = [{**self.world.prs[number], "body": body}]
        return number

    def no_dependabot_comment(self) -> None:
        for call in self.world.ran("gh", "pr", "comment"):
            self.assertNotIn("@dependabot", call[-1])

    def fix_prompts(self) -> list[str]:
        return [
            c[-1]
            for c, _ in self.world.calls
            if c[0] == "claude" and not c[-1].startswith("Run the gatekeeper")
        ]

    def test_red_ci_on_a_security_pr_gets_a_security_prompt_and_a_lockfile_only_fix(self) -> None:
        number = self.resumed_security_pr()
        self.world.checks[number] = [
            [{"name": "CI gate", "bucket": "fail", "link": "l"}],
            [{"name": "CI gate", "bucket": "pass", "link": "l"}],
        ]
        self.world.fix_commits[number] = 1
        self.world.changes = ["limerick/Cargo.lock"]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "merged")
        (prompt,) = self.fix_prompts()
        self.assertIn("lockfiles and package manifests only", prompt)
        self.assertNotIn("Dependabot", prompt)
        self.assertEqual(len(self.world.ran("git", "push")), 1)
        self.assertEqual(len(self.world.gatekept()), 1)
        self.no_dependabot_comment()

    def test_red_ci_fix_that_touches_source_is_not_pushed_and_skips_the_gatekeeper(self) -> None:
        number = self.resumed_security_pr()
        self.world.checks[number] = [[{"name": "CI gate", "bucket": "fail", "link": "l"}]]
        self.world.fix_commits[number] = 1
        self.world.changes = ["limerick/Cargo.lock", "limerick/crates/core/src/lib.rs"]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "owner")
        self.assertIn("lib.rs", outcome.detail)
        self.assertEqual(self.world.ran("git", "push"), [])
        self.assertEqual(self.world.gatekept(), [])
        (edit,) = self.world.ran("gh", "pr", "edit")
        self.assertEqual(edit[3:], (str(NEW_PR - 1), "-R", base.REPO, "--add-label", "needs-owner"))
        self.no_dependabot_comment()

    def test_an_unreadable_file_list_fails_closed_before_the_gatekeeper(self) -> None:
        cap = sec.FILE_LIST_CAP
        lockfiles = [f"pkg{i}/package-lock.json" for i in range(cap)]
        for files in ([], lockfiles, lockfiles[: cap - 1] + ["x/lib.rs"]):
            with self.subTest(files=len(files)):
                self.setUp()  # a fresh fake GitHub for each case
                number = self.resumed_security_pr()
                self.world.files[number] = files
                (outcome,) = self.run_security()
                self.assertEqual(outcome.status, "owner")
                self.assertEqual(self.world.gatekept(), [])
                (edit,) = self.world.ran("gh", "pr", "edit")
                self.assertIn("needs-owner", edit)

    def test_just_under_the_cap_of_lockfiles_still_reaches_the_gatekeeper(self) -> None:
        number = self.resumed_security_pr()
        self.world.files[number] = [
            f"pkg{i}/package-lock.json" for i in range(sec.FILE_LIST_CAP - 1)
        ]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "merged")

    def test_a_security_pr_that_picked_up_source_changes_never_reaches_the_gatekeeper(self) -> None:
        number = self.resumed_security_pr()  # green, but someone pushed a source change to it
        self.world.files[number] = ["limerick/Cargo.lock", "limerick/crates/core/src/lib.rs"]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "owner")
        self.assertEqual(self.world.gatekept(), [])
        self.assertEqual(self.world.ran("claude"), [])

    def test_a_resumed_pr_that_turns_out_to_conflict_is_redone_not_sent_to_dependabot(self) -> None:
        number = self.resumed_security_pr(mergeable="UNKNOWN")
        self.world.behind[number] = 1
        self.world.conflicts.add(number)
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "merged")  # the redone PR
        (tag,) = self.world.ran("gh", "pr", "edit")
        self.assertIn("upkeep-redone", tag)
        self.assertEqual(len(self.world.ran("gh", "pr", "close")), 1)
        self.assertEqual(len(self.world.ran("gh", "pr", "create")), 1)
        self.no_dependabot_comment()

    def test_the_dependabot_listing_never_picks_up_a_security_pr(self) -> None:
        self.resumed_security_pr()
        self.assertEqual(self.make(security=False).open_prs(), [])

    def test_a_scan_that_stays_red_escalates_without_pushing(self) -> None:
        self.world.scanner_rc = 1
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "escalated")
        self.assertIn("still fails", outcome.detail)
        self.assertEqual(self.world.ran("git", "push"), [])

    def test_a_missing_scanner_escalates_instead_of_passing_unproven(self) -> None:
        self.world.scanner_rc = 127
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "escalated")
        self.assertIn("not installed", outcome.detail)
        self.assertEqual(self.world.ran("gh", "pr", "create"), [])

    def test_a_parked_finding_is_reported_and_not_retried(self) -> None:
        self.world.ci_red = [issue(2403, OSV_BODY, "ci-red", "needs-owner")]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "owner")
        self.assertEqual(self.world.ran("claude"), [])

    def test_an_open_security_pr_is_continued_not_duplicated(self) -> None:
        finding = sec.Finding(2403, "t", "osv-scanner.yml", OSV_BODY)
        self.world.add(
            NEW_PR,
            headRefName="upkeep/security-osv-scanner-20261125",
            labels=[{"name": "security-upkeep"}],
        )
        self.world.gatekeeper[NEW_PR] = self.world.approve_and_merge(NEW_PR)
        body = sec.pr_body(finding, sec.Decision("merge", "x"), ["limerick/Cargo.lock"], [])
        self.world.files[NEW_PR] = ["limerick/Cargo.lock"]
        self.world.sec_prs = [
            {**self.world.prs[NEW_PR], "body": body, "labels": [{"name": "security-upkeep"}]}
        ]
        (outcome,) = self.run_security()
        self.assertEqual(outcome.status, "merged")
        self.assertIn("continuing", outcome.notes[0])
        self.assertEqual(self.world.ran("gh", "pr", "create"), [])
        self.assertEqual(
            [c for c, _ in self.world.calls if c[0] == "claude" and "is red on main" in c[-1]], []
        )

    def test_dry_run_plans_the_fix_and_changes_nothing(self) -> None:
        (outcome,) = self.run_security(dry_run=True)
        self.assertEqual(outcome.status, "planned")
        for prefix in base.MUTATING:
            self.assertEqual(self.world.ran(*prefix), [], prefix)

    def test_no_fix_flag_reports_the_finding_without_attempting_it(self) -> None:
        (outcome,) = self.run_security(fix=False)
        self.assertEqual(outcome.status, "skipped")
        self.assertEqual(self.world.ran("claude"), [])

    def test_security_step_is_off_unless_enabled_and_skipped_for_a_pr_filter(self) -> None:
        self.assertEqual(self.run_security(security=False), [])
        self.assertEqual(self.world.ran("gh", "issue", "list"), [])
        self.world.add(10)
        self.world.gatekeeper[10] = self.world.approve_and_merge(10)
        outcomes, _ = self.make().run(only={10}, exclude=set())
        self.assertEqual([o.number for o in outcomes], [10])

    def test_an_unreadable_issue_list_is_a_run_problem_not_a_crash(self) -> None:
        routine = self.make()
        self.world.issue_list_fails = True
        outcomes, summary = routine.run(only=set(), exclude=set())
        self.assertEqual(outcomes, [])
        self.assertTrue(any("security findings" in p for p in routine.problems))
        self.assertIn("### Problems", summary)

    def test_summary_lists_an_escalation_under_needs_attention(self) -> None:
        self.world.changes = []
        outcomes = self.run_security()
        text = upkeep.render_summary(outcomes, when="now", dry_run=False)
        self.assertIn("### Needs attention", text)
        self.assertIn("**escalated**", text)


class ScheduleTests(unittest.TestCase):
    def test_agent_runs_daily_not_weekly(self) -> None:
        plist = upkeep.launch_agent_plist(
            python="/p", script=Path("/s/x.py"), state=Path("/s"), path_env="/bin", exclude=[]
        )
        self.assertNotIn("Weekday", plist["StartCalendarInterval"])
        self.assertEqual(set(plist["StartCalendarInterval"]), {"Hour", "Minute"})

    def test_default_run_follows_the_repo_health_watcher(self) -> None:
        # repo-health.yml opens the ci-red issue at 13:23 UTC; the run must come after it.
        offset = dt.datetime.now().astimezone().utcoffset() or dt.timedelta()
        hour, minute = upkeep.schedule_local()
        local = dt.timedelta(hours=hour, minutes=minute)
        utc = (local - offset) % dt.timedelta(days=1)
        self.assertGreater(utc, dt.timedelta(hours=13, minutes=23))


if __name__ == "__main__":
    unittest.main()
