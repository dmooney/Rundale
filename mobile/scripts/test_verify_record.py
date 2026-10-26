#!/usr/bin/env python3
"""Tests for recording and checking full mobile-verify passes."""

from __future__ import annotations

import fnmatch
import importlib
import io
import json
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from typing import TYPE_CHECKING
from unittest.mock import patch

if TYPE_CHECKING:
    from . import test_verify as test_verify_module
    from . import verify_record as record_module
else:  # direct discovery from mobile/scripts
    record_module = (
        importlib.import_module(".verify_record", __package__)
        if __package__
        else importlib.import_module("verify_record")
    )
    test_verify_module = (
        importlib.import_module(".test_verify", __package__)
        if __package__
        else importlib.import_module("test_verify")
    )

GIT = ["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid"]


def commit(root: Path, message: str) -> str:
    subprocess.run(["git", "add", "-A"], cwd=root, check=True)
    subprocess.run([*GIT, "commit", "-q", "-m", message], cwd=root, check=True)
    return subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, check=True, capture_output=True, text=True
    ).stdout.strip()


class FakeGitHub:
    """Answers the `gh api` calls verify_record makes, from in-memory statuses."""

    def __init__(
        self, pulls: dict[int, list[str]], statuses: dict[str, list[dict]], base: str = ""
    ):
        self.pulls, self.statuses, self.base = pulls, statuses, base
        self.posted: list[tuple[str, dict[str, str]]] = []

    def __call__(self, *args: str):
        if args[:1] == ("repo",):
            return {"nameWithOwner": "owner/repo"}
        if "--method" in args:
            fields = dict(a.split("=", 1) for a in args if "=" in a)
            sha = args[args.index("--method") + 2].rsplit("/", 1)[1]
            self.posted.append((sha, fields))
            self.statuses.setdefault(sha, []).append(
                {"context": fields["context"], "state": fields["state"], **fields}
            )
            return None
        path = args[-1]
        parts = path.split("/")
        if parts[3] == "pulls" and len(parts) == 5:
            return {"head": {"sha": self.pulls[int(parts[4])][-1]}, "base": {"sha": self.base}}
        if parts[3] == "pulls":
            return [[{"sha": sha} for sha in self.pulls[int(parts[4])]]]
        if parts[-1] == "statuses":
            return [self.statuses.get(parts[4], [])]
        if parts[-1] == "pulls":
            return [{"number": n} for n, shas in self.pulls.items() if parts[4] in shas]
        raise AssertionError(f"unexpected gh call: {args}")


class VerifyRecordTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        test_verify_module.create_cached_repository(self.root)
        # main before the PR, then the PR's commit changing an app input.
        self.base = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=self.root, check=True, capture_output=True, text=True
        ).stdout.strip()
        (self.root / "mobile" / "project.yml").write_text("name: Rundale 2\n", encoding="utf-8")
        self.head = commit(self.root, "app change")
        self.patches = [
            patch.object(record_module, "ROOT", self.root),
            patch.object(
                record_module, "PASSES", self.root / "mobile" / ".verification" / "passes"
            ),
        ]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in self.patches:
            p.stop()
        self.directory.cleanup()

    def test_documentation_commits_keep_the_input_tree(self):
        before = record_module.input_tree()
        (self.root / "docs" / "notes.md").write_text("second\n", encoding="utf-8")
        commit(self.root, "docs")
        self.assertEqual(record_module.input_tree(), before)
        (self.root / "mobile" / "project.yml").write_text("name: Changed\n", encoding="utf-8")
        commit(self.root, "app")
        self.assertNotEqual(record_module.input_tree(), before)

    def test_gate_tooling_does_not_change_the_input_tree_but_the_rust_build_does(self):
        before = record_module.input_tree()
        scripts = self.root / "mobile" / "scripts"
        (scripts / "fixtures").mkdir(parents=True)
        (scripts / "test_example.py").write_text("# test\n", encoding="utf-8")
        (scripts / "fixtures" / "tree.json").write_text("{}\n", encoding="utf-8")
        commit(self.root, "tests")
        self.assertEqual(record_module.input_tree(), before)
        (scripts / "verify.py").write_text("# gate\n", encoding="utf-8")
        commit(self.root, "gate tooling")
        self.assertEqual(record_module.input_tree(), before)
        (scripts / "release.py").write_text("FEEDBACK = 'beta'\n", encoding="utf-8")
        commit(self.root, "release settings")
        # release.py writes Info.plist settings into the app.
        self.assertNotEqual(record_module.input_tree(), before)
        before = record_module.input_tree()
        (scripts / "build-rust-mobile.sh").write_text("# build\n", encoding="utf-8")
        commit(self.root, "rust build")
        self.assertNotEqual(record_module.input_tree(), before)

    def test_changes_outside_the_app_inputs_keep_the_input_tree(self):
        before = record_module.input_tree()
        for path in (".github/workflows/ci.yml", "endpoints/server.ts", "limerick/apps/ui/a.ts"):
            (self.root / path).parent.mkdir(parents=True, exist_ok=True)
            (self.root / path).write_text("changed\n", encoding="utf-8")
        commit(self.root, "elsewhere")
        self.assertEqual(record_module.input_tree(), before)
        for path in (
            "limerick/crates/core/lib.rs",
            "mods/rundale/world.json",
            ".cargo/config.toml",
            "limerick/config/gemini-3.7-flash-capabilities.json",
            "bug-report/test/fixtures/phone-report.json",
        ):
            (self.root / path).parent.mkdir(parents=True, exist_ok=True)
            (self.root / path).write_text("changed\n", encoding="utf-8")
            commit(self.root, path)
            self.assertNotEqual(record_module.input_tree(), before, path)
            before = record_module.input_tree()

    def test_nested_agent_instructions_are_excluded_but_other_app_inputs_are_not(self):
        before = record_module.input_tree()
        instructions = self.root / "limerick" / "testing" / "AGENTS.md"
        instructions.parent.mkdir(parents=True)
        instructions.write_text("local guidance\n", encoding="utf-8")
        commit(self.root, "nested instructions")
        self.assertEqual(record_module.input_tree(), before)

        fixture = self.root / "limerick" / "testing" / "fixtures" / "scenario.json"
        fixture.parent.mkdir(parents=True, exist_ok=True)
        fixture.write_text("{}\n", encoding="utf-8")
        commit(self.root, "runtime fixture")
        self.assertNotEqual(record_module.input_tree(), before)

        before = record_module.input_tree()
        prompt = self.root / "mods" / "rundale" / "prompts" / "dialogue.md"
        prompt.parent.mkdir(parents=True, exist_ok=True)
        prompt.write_text("Speak in character.\n", encoding="utf-8")
        commit(self.root, "runtime prompt")
        self.assertNotEqual(record_module.input_tree(), before)

    def test_mobile_workflow_path_filters_exclude_agent_instruction_files(self):
        repository = Path(__file__).resolve().parents[2]
        ci = (repository / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        testflight = (repository / ".github" / "workflows" / "testflight.yml").read_text(
            encoding="utf-8"
        )
        mobile_filter = ci.split("            mobile_app:\n", 1)[1].split(
            "\n            bug_report:", 1
        )[0]
        testflight_paths = testflight.split("    paths:\n", 1)[1].split(
            "\n  workflow_dispatch:", 1
        )[0]
        self.assertIn("predicate-quantifier: some-with-excludes", ci)
        self.assertIn("- '!**/AGENTS.md'", mobile_filter)
        self.assertIn('- "!**/AGENTS.md"', testflight_paths)

        patterns = [
            line.strip()[2:].strip("'\"")
            for line in mobile_filter.splitlines()
            if line.strip().startswith("- ")
        ]

        def matches_mobile_app(path: str) -> bool:
            included = any(
                fnmatch.fnmatchcase(path, pattern)
                for pattern in patterns
                if not pattern.startswith("!")
            )
            excluded = any(
                fnmatch.fnmatchcase(path, pattern[1:])
                for pattern in patterns
                if pattern.startswith("!")
            )
            return included and not excluded

        self.assertTrue(matches_mobile_app("mobile/Rundale/file.swift"))
        self.assertTrue(matches_mobile_app("mods/rundale/world.json"))
        self.assertFalse(matches_mobile_app("limerick/testing/AGENTS.md"))
        self.assertFalse(matches_mobile_app("mobile/AGENTS.md"))
        self.assertFalse(matches_mobile_app("limerick/apps/ui/src/main.ts"))
        self.assertFalse(matches_mobile_app("docs/notes.md"))
        self.assertTrue(
            any(
                matches_mobile_app(path)
                for path in ("limerick/testing/AGENTS.md", "mobile/Rundale/file.swift")
            )
        )

    def test_coverage_baseline_changes_the_input_tree(self):
        before = record_module.input_tree()
        baseline = self.root / "mobile" / "coverage-baseline.json"
        payload = json.loads(baseline.read_text())
        payload["tolerance_percentage_points"] = 0.5
        baseline.write_text(json.dumps(payload))
        commit(self.root, "baseline")
        self.assertNotEqual(record_module.input_tree(), before)

    def test_input_tree_of_a_commit_whose_docs_differ_from_the_checkout(self):
        # CI checks out a merge commit; the PR head's excluded docs can match
        # neither that checkout's HEAD nor its work tree.
        pr_head = self.head
        expected = record_module.input_tree(pr_head)
        (self.root / "docs" / "notes.md").write_text("main moved\n", encoding="utf-8")
        commit(self.root, "docs on main")
        (self.root / "docs" / "notes.md").write_text("uncommitted\n", encoding="utf-8")
        self.assertEqual(record_module.input_tree(pr_head), expected)

    def test_a_full_pass_is_stored_under_the_tree_record_reads(self):
        report = test_verify_module.VerificationRun(
            self.root, command_runner=test_verify_module.GitBackedRunner()
        ).run()
        self.assertEqual(report["exit_code"], 0)
        stored = self.root / "mobile" / ".verification" / "passes"
        self.assertEqual(
            [p.stem for p in stored.glob("*.json")], [record_module.input_tree(self.head)]
        )

    def test_record_refuses_without_a_full_pass_for_head(self):
        fake = FakeGitHub({}, {})
        with patch.object(record_module, "_gh", fake), redirect_stderr(io.StringIO()) as err:
            self.assertEqual(record_module.record(), 1)
        self.assertEqual(fake.posted, [])
        self.assertIn("No full mobile-verify pass", err.getvalue())

    def test_record_posts_the_input_tree_and_check_finds_it_after_a_docs_commit(self):
        tree = record_module.input_tree()
        passes = self.root / "mobile" / ".verification" / "passes"
        passes.mkdir(parents=True)
        (passes / f"{tree}.json").write_text(json.dumps({"input_tree": tree}))
        fake = FakeGitHub({7: [self.head]}, {}, base=self.base)
        with patch.object(record_module, "_gh", fake), redirect_stdout(io.StringIO()):
            self.assertEqual(record_module.record(), 0)
            self.assertEqual(fake.posted[0][0], self.head)
            self.assertEqual(fake.posted[0][1]["description"], f"input tree {tree}")
            (self.root / "docs" / "notes.md").write_text("later\n", encoding="utf-8")
            later = commit(self.root, "docs")
            fake.pulls[7].append(later)
            self.assertEqual(record_module.check(pr=7, commit=None, wait=0), 0)
            # A merge commit whose parent lacks the change finds the PR's pass.
            self.assertEqual(record_module.check(pr=None, commit=self.head, wait=0), 0)
            # A later docs-only merge changes no app input: nothing to test.
            self.assertEqual(
                record_module.check(pr=None, commit=later, wait=0), record_module.UNCHANGED
            )

    def test_a_pr_that_changes_no_app_input_needs_no_pass(self):
        tooling = self.root / "mobile" / "scripts" / "verify.py"
        tooling.parent.mkdir(parents=True, exist_ok=True)
        tooling.write_text("# gate tooling\n", encoding="utf-8")
        (self.root / ".github").mkdir(exist_ok=True)
        (self.root / ".github" / "ci.yml").write_text("on: push\n", encoding="utf-8")
        tooling_head = commit(self.root, "tooling")
        fake = FakeGitHub({8: [tooling_head]}, {}, base=self.head)
        with patch.object(record_module, "_gh", fake), redirect_stdout(io.StringIO()):
            self.assertEqual(
                record_module.check(pr=8, commit=None, wait=0), record_module.UNCHANGED
            )

    def test_check_fails_when_the_recorded_tree_is_stale(self):
        fake = FakeGitHub(
            base=self.base,
            pulls={7: [self.head]},
            statuses={
                self.head: [
                    {
                        "context": "mobile-verify",
                        "state": "success",
                        "description": "input tree " + "0" * 40,
                    }
                ]
            },
        )
        with patch.object(record_module, "_gh", fake), redirect_stderr(io.StringIO()):
            self.assertEqual(record_module.check(pr=7, commit=None, wait=0), 1)

    def test_latest_finds_a_pass_recorded_on_the_pr_or_by_an_earlier_night(self):
        tree = record_module.input_tree(self.head)
        status = {
            "context": "mobile-verify",
            "state": "success",
            "description": f"input tree {tree}",
        }
        # The PR's own commit carries the author's pass; main then gained a
        # docs commit, which keeps the input tree.
        pr_commit = "a" * 40
        (self.root / "docs" / "notes.md").write_text("later\n", encoding="utf-8")
        later = commit(self.root, "docs")
        fake = FakeGitHub({7: [pr_commit, self.head]}, {pr_commit: [status]}, base=self.base)
        with patch.object(record_module, "_gh", fake), redirect_stdout(io.StringIO()):
            self.assertEqual(record_module.latest(later), 0)
        # A nightly pass is recorded on main's head itself.
        fake = FakeGitHub({}, {later: [status]})
        with patch.object(record_module, "_gh", fake), redirect_stdout(io.StringIO()):
            self.assertEqual(record_module.latest(later), 0)

    def test_latest_needs_a_pass_for_the_current_tree(self):
        old_tree = record_module.input_tree(self.base)
        stale = {
            "context": "mobile-verify",
            "state": "success",
            "description": f"input tree {old_tree}",
        }
        # A pass for the tree before the app change does not cover it, and an
        # unchanged commit still needs one (unlike `check --commit`'s exit 3).
        fake = FakeGitHub({}, {self.base: [stale]})
        with patch.object(record_module, "_gh", fake), redirect_stderr(io.StringIO()):
            self.assertEqual(record_module.latest(self.head), 1)
        (self.root / "docs" / "notes.md").write_text("later\n", encoding="utf-8")
        later = commit(self.root, "docs")
        with patch.object(record_module, "_gh", fake), redirect_stderr(io.StringIO()):
            self.assertEqual(record_module.latest(later), 1)


if __name__ == "__main__":
    unittest.main()
