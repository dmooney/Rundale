#!/usr/bin/env python3
"""Focused lifecycle and exit-status tests for record-ui-test.py."""

from __future__ import annotations

import importlib.util
import json
import signal
import subprocess
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("record-ui-test.py")
SPEC = importlib.util.spec_from_file_location("record_ui_test", MODULE_PATH)
assert SPEC and SPEC.loader
record_ui_test = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(record_ui_test)


DEVICES = json.dumps(
    {
        "devices": {
            "iOS": [
                {
                    "name": "QA iPhone",
                    "udid": "qa-udid",
                    "state": "Booted",
                    "isAvailable": True,
                    "deviceTypeIdentifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro",
                },
            ]
        }
    }
)


class FakeVideo:
    def __init__(self):
        self.lines = iter(["Recording started\n", "Recording finished\n"])
        self.signals = []
        self.returncode = None
        self.stdout = self

    def readline(self):
        return next(self.lines, "")

    def poll(self):
        return self.returncode

    def send_signal(self, value):
        self.signals.append(value)
        if hasattr(self, "output"):
            self.output.write_bytes(b"fake h264 video")
        self.returncode = 0

    def wait(self, timeout=None):
        self.returncode = 0
        return self.returncode

    def read(self):
        return next(self.lines, "")


class RecorderTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.calls = []
        self.video = None
        source = self.root / "RundaleUITests"
        source.mkdir()
        (self.root / "Rundale.xcodeproj").mkdir()
        (source / "Tests.swift").write_text(
            "final class RundalePhase1AuditUITests: XCTestCase {\nfunc testAcceptedCommandRowIsExactAndImmediatelyVisible() {}\n}\n",
            encoding="utf-8",
        )

    def tearDown(self):
        self.temp.cleanup()

    def command(self, argv, **kwargs):
        self.calls.append(tuple(argv))
        if argv[:5] == ["xcrun", "simctl", "list", "devices", "available"]:
            return subprocess.CompletedProcess(argv, 0, stdout=DEVICES, stderr="")
        if argv[:4] == ["xcrun", "simctl", "bootstatus", "qa-udid"]:
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")
        if "build-for-testing" in argv:
            return subprocess.CompletedProcess(argv, 0, stdout="built\n", stderr="")
        if "test-without-building" in argv:
            return subprocess.CompletedProcess(argv, self.test_status, stdout="tested\n", stderr="")
        raise AssertionError(argv)

    def make_video(self, argv, **kwargs):
        self.video = FakeVideo()
        self.video.output = Path(argv[-1])
        return self.video

    def args(self, output, **overrides):
        values = dict(
            target="RundaleUITests",
            class_name="RundalePhase1AuditUITests",
            method="testAcceptedCommandRowIsExactAndImmediatelyVisible",
            simulator="QA iPhone",
            output=str(output),
            project=str(self.root / "Rundale.xcodeproj"),
            scheme="Rundale",
            configuration="Debug",
            derived_data=str(self.root / "DerivedData"),
            reuse_build=False,
        )
        values.update(overrides)
        return type("Args", (), values)()

    def test_stops_recording_with_sigint_and_returns_test_status(self):
        self.test_status = 9
        output = self.root / "capture.mov"
        status = record_ui_test.run(
            self.args(output), command_runner=self.command, popen=self.make_video
        )
        self.assertEqual(status, 9)
        self.assertEqual(self.video.signals, [signal.SIGINT])
        self.assertIn(
            "-only-testing:RundaleUITests/RundalePhase1AuditUITests/testAcceptedCommandRowIsExactAndImmediatelyVisible",
            self.calls[-1],
        )
        self.assertTrue((self.root / "capture.log").is_file())
        self.assertTrue((self.root / "capture.xcresult").as_posix() in self.calls[-1])

    def test_reuse_build_runs_only_test_without_building(self):
        self.test_status = 0
        output = self.root / "reuse.mov"
        status = record_ui_test.run(
            self.args(output, reuse_build=True), command_runner=self.command, popen=self.make_video
        )
        self.assertEqual(status, 0)
        self.assertFalse(any("build-for-testing" in call for call in self.calls))
        self.assertTrue(any("test-without-building" in call for call in self.calls))

    def test_ambiguous_name_and_empty_output_are_rejected(self):
        duplicate = DEVICES.replace(
            "}]}}",
            '}, {"name": "QA iPhone", "udid": "qa-2", "state": "Shutdown", "isAvailable": true, "deviceTypeIdentifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro"}]}}',
        )

        def duplicate_command(argv, **kwargs):
            return subprocess.CompletedProcess(argv, 0, stdout=duplicate, stderr="")

        with self.assertRaises(record_ui_test.RecorderError):
            record_ui_test.resolve_simulator("QA iPhone", command_runner=duplicate_command)
        with self.assertRaisesRegex(record_ui_test.RecorderError, "non-empty"):
            record_ui_test.run(self.args(""), command_runner=self.command, popen=self.make_video)

    def test_unknown_method_and_existing_siblings_are_rejected(self):
        with self.assertRaisesRegex(record_ui_test.RecorderError, "was not found"):
            record_ui_test.run(
                self.args(self.root / "unknown.mov", method="testDoesNotExist"),
                command_runner=self.command,
                popen=self.make_video,
            )
        output = self.root / "capture.mov"
        output.with_suffix(".log").write_text("old", encoding="utf-8")
        with self.assertRaisesRegex(record_ui_test.RecorderError, "new files"):
            record_ui_test.run(
                self.args(output), command_runner=self.command, popen=self.make_video
            )

    def test_method_in_other_class_and_empty_video_rejected(self):
        source = self.root / "RundaleUITests" / "Tests.swift"
        source.write_text(
            "class RundalePhase1AuditUITests {}\nclass Other { func testAcceptedCommandRowIsExactAndImmediatelyVisible() {} }\n"
        )
        with self.assertRaisesRegex(record_ui_test.RecorderError, "unambiguously"):
            record_ui_test.run(self.args(self.root / "wrong.mov"), command_runner=self.command)
        source.write_text(
            "class RundalePhase1AuditUITests { func testAcceptedCommandRowIsExactAndImmediatelyVisible() {} }\n"
        )
        self.test_status = 0

        def empty_video(argv, **kwargs):
            self.video = FakeVideo()
            return self.video

        with self.assertRaisesRegex(record_ui_test.RecorderError, "non-empty video"):
            record_ui_test.run(
                self.args(self.root / "empty.mov"), command_runner=self.command, popen=empty_video
            )

    def test_failed_test_status_survives_finalize_error(self):
        from unittest.mock import patch

        self.test_status = 65

        def stop(recorder):
            recorder.process.send_signal(signal.SIGINT)
            raise record_ui_test.RecorderError("finalization failed")

        with patch.object(record_ui_test.VideoRecorder, "stop", stop):
            result = record_ui_test.run(
                self.args(self.root / "failure.mov"),
                command_runner=self.command,
                popen=self.make_video,
            )
        self.assertEqual(result, 65)
        self.assertIn("tested", (self.root / "failure.log").read_text())

    def test_stop_timeout_sends_signal_then_kills_process(self):
        class Stuck(FakeVideo):
            def kill(self):
                self.returncode = -9

            def wait(self, timeout=None):
                if timeout == 30:
                    raise subprocess.TimeoutExpired("recordVideo", timeout)
                return super().wait(timeout)

        recorder = record_ui_test.VideoRecorder(
            self.root / "stuck.mov", popen=lambda *a, **k: Stuck()
        )
        recorder.udid = "qa-udid"
        recorder.start()
        with self.assertRaisesRegex(record_ui_test.RecorderError, "finalize"):
            recorder.stop()
        self.assertEqual(recorder.process.signals, [signal.SIGINT])


if __name__ == "__main__":
    unittest.main()
