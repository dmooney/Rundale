#!/usr/bin/env python3
"""Tests for the TestFlight upload decision and the delivery record."""

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
    from . import test_verify_record as record_tests
    from . import testflight_delivery as delivery
    from . import verify_record as record_module
else:  # direct discovery from mobile/scripts
    _pkg = __package__ + "." if __package__ else ""
    delivery = importlib.import_module(f"{_pkg}testflight_delivery")
    record_module = importlib.import_module(f"{_pkg}verify_record")
    test_verify_module = importlib.import_module(f"{_pkg}test_verify")
    record_tests = importlib.import_module(f"{_pkg}test_verify_record")

commit = record_tests.commit


class DecideTests(unittest.TestCase):
    def test_a_push_of_the_delivered_tree_uploads_nothing(self):
        self.assertFalse(delivery.decide("push", "a", "a")[0])

    def test_a_push_of_a_new_tree_uploads(self):
        self.assertTrue(delivery.decide("push", "b", "a")[0])

    def test_the_first_push_with_no_record_uploads(self):
        self.assertTrue(delivery.decide("push", "a", None)[0])

    def test_a_manual_run_always_uploads(self):
        self.assertTrue(delivery.decide("workflow_dispatch", "a", "a")[0])

    def test_only_a_build_in_testing_is_recorded(self):
        self.assertEqual(delivery.build_from_receipt({"status": "testing", "uploadedBuild": 7}), 7)
        self.assertEqual(delivery.build_from_receipt({"status": "testing", "requestedBuild": 8}), 8)
        self.assertIsNone(delivery.build_from_receipt({"status": "upload_requested"}))
        self.assertIsNone(delivery.build_from_receipt({}))


class DeliveryRecordTests(unittest.TestCase):
    """The scenario in #2433, in a real repository with a bare origin."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        base = Path(self.directory.name)
        self.root = base / "work"
        self.origin = base / "origin.git"
        subprocess.run(["git", "init", "-q", "--bare", str(self.origin)], check=True)
        self.root.mkdir()
        test_verify_module.create_cached_repository(self.root)
        subprocess.run(
            ["git", "remote", "add", "origin", str(self.origin)], cwd=self.root, check=True
        )
        self.patch = patch.object(record_module, "ROOT", self.root)
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.directory.cleanup()

    def push_decision(self) -> str:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            self.assertEqual(delivery.main(["decide", "--event", "push"]), 0)
        return out.getvalue().strip()

    def deliver(self, build: int, status: str = "testing") -> None:
        receipt = self.root / "receipt.json"
        receipt.write_text(json.dumps({"status": status, "uploadedBuild": build}), encoding="utf-8")
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            delivery.main(["record", "--receipt", str(receipt)])

    def change_app(self, text: str = "name: Rundale 2\n") -> None:
        (self.root / "mobile" / "project.yml").write_text(text, encoding="utf-8")
        commit(self.root, "app change")

    def test_first_run_uploads_then_records_and_stops(self):
        self.assertEqual(self.push_decision(), "upload=true")
        self.deliver(10)
        self.assertEqual(self.push_decision(), "upload=false")
        # The tag reached origin, where the next run's checkout fetches it.
        remote = subprocess.run(
            ["git", "--git-dir", str(self.origin), "tag", "--list", "testflight/*"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.split()
        self.assertEqual(remote, ["testflight/10"])

    def test_a_no_app_push_after_a_replaced_app_push_still_uploads_the_app(self):
        self.deliver(10)
        # An app change whose run a newer pending run replaced, by a push that
        # changes no app input (its tree equals its own `before`).
        self.change_app()
        scripts = self.root / "mobile" / "scripts"
        scripts.mkdir(exist_ok=True)
        (scripts / "verify.py").write_text("# gate\n", encoding="utf-8")
        commit(self.root, "gate tooling")
        self.assertEqual(self.push_decision(), "upload=true")

    def test_a_push_that_leaves_the_delivered_tree_uploads_nothing(self):
        self.deliver(10)
        (self.root / "docs" / "notes.md").write_text("more\n", encoding="utf-8")
        commit(self.root, "docs")
        self.assertEqual(self.push_decision(), "upload=false")

    def test_the_newest_build_is_the_last_delivered(self):
        self.deliver(9)
        self.change_app()
        self.deliver(10)
        self.assertEqual(self.push_decision(), "upload=false")
        self.assertEqual([b for b, _ in delivery.delivered_builds()], [10, 9])

    def test_a_build_that_never_reached_testing_records_nothing(self):
        self.deliver(10, status="upload_requested")
        self.assertEqual(delivery.delivered_builds(), [])
        self.assertEqual(self.push_decision(), "upload=true")

    def test_recording_a_build_on_another_commit_fails_loudly(self):
        self.deliver(10)
        self.change_app()
        with self.assertRaises(SystemExit):
            self.deliver(10)


if __name__ == "__main__":
    unittest.main()
