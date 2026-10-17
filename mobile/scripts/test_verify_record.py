#!/usr/bin/env python3
"""Tests for recording and checking full mobile-verify passes."""

from __future__ import annotations

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

    def __init__(self, pulls: dict[int, list[str]], statuses: dict[str, list[dict]]):
        self.pulls, self.statuses = pulls, statuses
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
            return {"head": {"sha": self.pulls[int(parts[4])][-1]}}
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
        self.head = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=self.root, check=True, capture_output=True, text=True
        ).stdout.strip()
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

    def test_gate_script_tests_do_not_change_the_input_tree(self):
        before = record_module.input_tree()
        scripts = self.root / "mobile" / "scripts"
        (scripts / "fixtures").mkdir(parents=True)
        (scripts / "test_example.py").write_text("# test\n", encoding="utf-8")
        (scripts / "fixtures" / "tree.json").write_text("{}\n", encoding="utf-8")
        commit(self.root, "tests")
        self.assertEqual(record_module.input_tree(), before)
        (scripts / "verify.py").write_text("# gate\n", encoding="utf-8")
        commit(self.root, "gate")
        self.assertNotEqual(record_module.input_tree(), before)

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
        fake = FakeGitHub({7: [self.head]}, {})
        with patch.object(record_module, "_gh", fake), redirect_stdout(io.StringIO()):
            self.assertEqual(record_module.record(), 0)
            self.assertEqual(fake.posted[0][0], self.head)
            self.assertEqual(fake.posted[0][1]["description"], f"input tree {tree}")
            (self.root / "docs" / "notes.md").write_text("later\n", encoding="utf-8")
            later = commit(self.root, "docs")
            fake.pulls[7].append(later)
            self.assertEqual(record_module.check(pr=7, commit=None, wait=0), 0)
            # The squash commit on main has the same tree, and finds the PR's pass.
            self.assertEqual(record_module.check(pr=None, commit=later, wait=0), 0)

    def test_check_fails_when_the_recorded_tree_is_stale(self):
        fake = FakeGitHub(
            {7: [self.head]},
            {
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


if __name__ == "__main__":
    unittest.main()
