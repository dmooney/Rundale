#!/usr/bin/env python3
"""Focused stdlib tests for release orchestration; no Xcode invocation."""

from __future__ import annotations

import importlib
import plistlib
import tempfile
import unittest
from pathlib import Path
from typing import TYPE_CHECKING
from unittest.mock import patch

if TYPE_CHECKING:
    from . import release
else:  # support both package-based pytest and direct unittest discovery
    release = (
        importlib.import_module(".release", __package__)
        if __package__
        else importlib.import_module("release")
    )


class ReleaseTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "mobile" / "Rundale" / "Resources").mkdir(parents=True)
        (self.root / "mobile" / "scripts").mkdir(parents=True, exist_ok=True)
        (self.root / "mobile" / "project.yml").write_text(
            'MARKETING_VERSION: "0.1.0"\nCURRENT_PROJECT_VERSION: "8"\n', encoding="utf-8"
        )
        (self.root / "mobile" / "Rundale" / "Resources" / "GoogleService-Info.plist").write_bytes(
            b"private"
        )
        self.paths = release.Paths(self.root)
        self.commands: list[list[str]] = []

    def tearDown(self) -> None:
        self.temp.cleanup()

    def runner(self, argv, *, cwd, log):
        self.commands.append(argv)

    def test_release_identity_matches_verified_limerick_service(self):
        self.assertEqual(
            release.ENDPOINT_SETTINGS,
            {
                "RUNDALE_ENDPOINT_BASE_URL": "https://limerick-server-24861210203.us-east1.run.app",
                "RUNDALE_ENDPOINT_ORGANIZATION": "limerick-demo",
                "RUNDALE_ENDPOINT_SLUG": "rundale-dialogue",
                "RUNDALE_ENDPOINT_VERSION": "1",
                "RUNDALE_INTENT_ENDPOINT_SLUG": "rundale-intent",
                "RUNDALE_INTENT_ENDPOINT_VERSION": "1",
            },
        )

    def test_build_number_mutation_is_numeric_and_dry_run_is_side_effect_free(self):
        old, new = release.increment_build_number(self.paths.project_spec, dry_run=True)
        self.assertEqual((old, new), (8, 9))
        self.assertIn('"8"', self.paths.project_spec.read_text())
        old, new = release.increment_build_number(self.paths.project_spec)
        self.assertEqual((old, new), (8, 9))
        self.assertIn('"9"', self.paths.project_spec.read_text())

    def test_build_orders_rust_xcodegen_and_unsigned_build(self):
        runner = release.Release(self.paths, runner=self.runner)
        runner.validate_app = lambda *args, **kwargs: None
        runner.build()
        self.assertIn("build-rust-mobile.sh", self.commands[0][1])
        self.assertEqual(self.commands[1][:3], ["xcodegen", "generate", "--spec"])
        self.assertIn("CODE_SIGNING_ALLOWED=NO", self.commands[2])

    def test_testflight_stops_before_mutation_when_verify_fails(self):
        def fail(argv, *, cwd, log):
            raise __import__("subprocess").CalledProcessError(1, argv)

        runner = release.Release(self.paths, runner=fail)
        with self.assertRaises(__import__("subprocess").CalledProcessError):
            runner.testflight()
        self.assertIn('"8"', self.paths.project_spec.read_text())

    def test_export_options_and_archive_validation(self):
        app = self.paths.app
        app.mkdir(parents=True)
        info = {
            "CFBundleIdentifier": release.BUNDLE_ID,
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "8",
            "CFBundleExecutable": "Rundale",
            "ITSAppUsesNonExemptEncryption": False,
            **release.ENDPOINT_SETTINGS,
        }
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        firebase_key = "AIza" + "F" * 35
        firebase = {**release.EXPECTED_FIREBASE, "API_KEY": firebase_key}
        (app / "GoogleService-Info.plist").write_bytes(plistlib.dumps(firebase))
        # The public Firebase key may also appear in the compiled binary.
        (app / "Rundale").write_bytes(b"binary " + firebase_key.encode())
        runner = release.Release(self.paths, runner=self.runner)
        runner.validate_archive(expected_build=8)
        options_path = runner.write_export_options()
        options = plistlib.loads(options_path.read_bytes())
        self.assertEqual(options["method"], "app-store-connect")
        self.assertTrue(options["testFlightInternalTestingOnly"])
        self.assertTrue(options["manageAppVersionAndBuildNumber"])
        self.assertEqual(self.commands[-1][0], "codesign")

        for value in (None, True, "NO", 0):
            with self.subTest(encryption_declaration=value):
                invalid_info = dict(info)
                if value is None:
                    del invalid_info["ITSAppUsesNonExemptEncryption"]
                else:
                    invalid_info["ITSAppUsesNonExemptEncryption"] = value
                (app / "Info.plist").write_bytes(plistlib.dumps(invalid_info))
                with self.assertRaisesRegex(RuntimeError, "encryption exemption"):
                    runner.validate_archive(expected_build=8)

        del info["CFBundleExecutable"]
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        with self.assertRaisesRegex(RuntimeError, "CFBundleExecutable"):
            runner.validate_archive(expected_build=8)

    def test_bundle_rejects_shipped_credentials_and_foreign_firebase(self):
        app = self.paths.app
        app.mkdir(parents=True)
        info = {
            "CFBundleIdentifier": release.BUNDLE_ID,
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "8",
            "CFBundleExecutable": "Rundale",
            "ITSAppUsesNonExemptEncryption": False,
            **release.ENDPOINT_SETTINGS,
        }
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        firebase = {**release.EXPECTED_FIREBASE, "API_KEY": "AIza" + "F" * 35}
        (app / "GoogleService-Info.plist").write_bytes(plistlib.dumps(firebase))
        runner = release.Release(self.paths, runner=self.runner)
        leaks = {
            "Limerick Endpoints consumer key": b"sfk_live_0123456789ab_" + b"x" * 43,
            "Anthropic API key": b"sk-ant-" + b"a" * 40,
            "OpenAI-style API key": b"sk-proj-" + b"b" * 40,
            "Google API key": b"AIza" + b"G" * 35,
            "provider secret variable": b"OPENAI_API_KEY=abc",
        }
        for label, secret in leaks.items():
            with self.subTest(label=label):
                (app / "Rundale").write_bytes(b"binary " + secret)
                with self.assertRaises(RuntimeError) as raised:
                    runner.validate_archive(expected_build=8)
                self.assertIn(label, str(raised.exception))
                self.assertNotIn(
                    secret.decode(), str(raised.exception), "findings must be redacted"
                )

        (app / "Rundale").write_bytes(b"binary")
        foreign = {**firebase, "PROJECT_ID": "someone-else"}
        (app / "GoogleService-Info.plist").write_bytes(plistlib.dumps(foreign))
        with self.assertRaisesRegex(RuntimeError, "Firebase configuration mismatch for PROJECT_ID"):
            runner.validate_archive(expected_build=8)

    def test_archive_signs_and_validates_without_upload(self):
        import contextlib
        import io

        output = io.StringIO()
        runner = release.Release(self.paths, runner=self.runner, dry_run=True)
        with contextlib.redirect_stdout(output):
            runner.archive()
        commands = [line for line in output.getvalue().splitlines() if line.startswith("$ ")]
        self.assertTrue(any(" -allowProvisioningUpdates archive " in line for line in commands))
        self.assertFalse(any("-exportArchive" in line for line in commands))
        self.assertTrue(any("validate app:" in line for line in output.getvalue().splitlines()))

    def test_full_dry_run_needs_no_firebase_and_changes_no_files(self):
        self.paths.firebase.unlink()
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        runner = release.Release(self.paths, dry_run=True, runner=self.runner)
        runner.build()
        runner.testflight()
        after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        self.assertEqual(before, after)
        self.assertFalse(self.paths.output.exists())
        self.assertEqual(self.commands, [])

    def test_testflight_orders_checks_archive_validation_then_upload(self):
        runner = release.Release(self.paths, runner=self.runner)
        with patch.object(runner, "validate_archive") as validate:
            runner.testflight()
        validate.assert_called_once_with(expected_build=9)
        self.assertEqual(self.commands[0][1:], ["--phase", "all"])
        self.assertEqual(self.commands[1][0], "xcodegen")
        self.assertIn("archive", self.commands[2])
        self.assertIn("-exportArchive", self.commands[3])
        self.assertFalse(any("build-rust-mobile.sh" in " ".join(c) for c in self.commands))

    def test_invalid_archive_prevents_upload_and_preserves_verification_log(self):
        def recording_runner(argv, *, cwd, log):
            self.commands.append(argv)
            with log.open("a") as output:
                output.write("verification evidence\n")

        runner = release.Release(self.paths, runner=recording_runner)
        with self.assertRaisesRegex(RuntimeError, "missing Info.plist"):
            runner.testflight()
        self.assertFalse(any("-exportArchive" in c for c in self.commands))
        self.assertIn("verification evidence", self.paths.log.read_text())
        self.assertFalse(self.paths.receipt.exists())


if __name__ == "__main__":
    unittest.main()
