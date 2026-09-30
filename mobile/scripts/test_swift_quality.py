#!/usr/bin/env python3
"""Unit tests for mobile/scripts/swift_quality.py — no Swift toolchain required."""

from __future__ import annotations

import importlib
import json
import tempfile
import unittest
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from . import swift_quality as sq
else:
    sq = (
        importlib.import_module(".swift_quality", __package__)
        if __package__
        else importlib.import_module("swift_quality")
    )

CommandResult = sq.CommandResult
SwiftQualityRun = sq.SwiftQualityRun


class FakeRunner:
    def __init__(self) -> None:
        self.calls: list[tuple[str, ...]] = []
        self.paths = {
            "swiftlint": "/usr/local/bin/swiftlint",
            "swiftformat": "/usr/local/bin/swiftformat",
            "swift": "/usr/bin/swift",
        }
        self.versions = {
            "swiftlint": "0.65.1",
            "swiftformat": "0.63.0",
        }
        self.lint_code = 0
        self.format_code = 0
        self.package_codes: dict[str, int] = {
            "RundaleKit": 0,
            "LimerickEndpointKit": 0,
            "RundaleBridge": 0,
        }
        self.lint_stderr = ""
        self.format_stderr = ""
        self.package_stderr = "Executed 1 test, with 0 failures\n"

    def run(self, argv, *, cwd, env=None, timeout_seconds=None):
        command = tuple(str(part) for part in argv)
        self.calls.append(command)
        cwd_path = Path(cwd)

        if command[:2] == ("bash", "-lc") and command[2].startswith("command -v "):
            name = command[2].split("command -v ", 1)[1].strip().strip("'\"")
            path = self.paths.get(name)
            if path is None:
                return CommandResult(1, stderr=f"{name} not found")
            return CommandResult(0, stdout=f"{path}\n")

        binary = Path(command[0]).name if command else ""
        if binary == "swiftlint" and "version" in command:
            return CommandResult(0, stdout=f"{self.versions['swiftlint']}\n")
        if binary == "swiftformat" and "--version" in command:
            return CommandResult(0, stdout=f"{self.versions['swiftformat']}\n")
        if binary == "swiftlint" and "lint" in command:
            return CommandResult(
                self.lint_code,
                stderr=self.lint_stderr or ("lint ok\n" if self.lint_code == 0 else "violation\n"),
            )
        if binary == "swiftformat" and "--lint" in command:
            return CommandResult(
                self.format_code,
                stderr=self.format_stderr
                or ("format ok\n" if self.format_code == 0 else "would reformat file\n"),
            )
        if binary == "swift" and "test" in command:
            package = Path(command[command.index("--package-path") + 1]).name
            code = self.package_codes.get(package, 1)
            return CommandResult(
                code,
                stdout=self.package_stderr if code == 0 else f"{package} boom\n",
                stderr="" if code == 0 else f"{package} failed\n",
            )
        return CommandResult(0, stdout=f"unhandled at {cwd_path}\n")


def _touch_packages(mobile: Path) -> None:
    for name in ("RundaleKit", "LimerickEndpointKit", "RundaleBridge"):
        package = mobile / name
        package.mkdir(parents=True)
        (package / "Package.swift").write_text("// swift-tools-version: 6.0\n", encoding="utf-8")
    (mobile / "tool-versions.toml").write_text(
        '[tools]\nswiftlint = "0.65.1"\nswiftformat = "0.63.0"\n',
        encoding="utf-8",
    )
    (mobile / ".swiftlint.yml").write_text("included: []\n", encoding="utf-8")
    (mobile / ".swiftformat").write_text("--swiftversion 6.0\n", encoding="utf-8")


class SwiftQualityTests(unittest.TestCase):
    def test_all_green_fast_lane_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            mobile = root / "mobile"
            mobile.mkdir()
            _touch_packages(mobile)
            runner = FakeRunner()
            run = SwiftQualityRun(root=root, runner=runner, report_dir=mobile / "report")
            code = run.run(lint=True, format_check=True, packages=True, xcode=False)
            self.assertEqual(code, 0)
            report = json.loads((mobile / "report" / "swift-quality.json").read_text())
            self.assertFalse(report["blocking"])
            self.assertEqual(report["counts"]["passed"], 5)
            self.assertEqual(report["counts"]["failed"], 0)
            self.assertEqual(report["counts"]["unavailable"], 0)

    def test_lint_violation_fails_required_gate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            mobile = root / "mobile"
            mobile.mkdir()
            _touch_packages(mobile)
            runner = FakeRunner()
            runner.lint_code = 2
            runner.lint_stderr = "error: Force Cast Violation\n"
            run = SwiftQualityRun(root=root, runner=runner, report_dir=mobile / "report")
            code = run.run(lint=True, format_check=False, packages=False)
            self.assertEqual(code, 1)
            gate = next(g for g in run.gates if g.id == "swiftlint")
            self.assertEqual(gate.status, "failed")
            self.assertTrue(gate.required)
            self.assertIn("Force Cast", gate.reason)

    def test_failing_package_test_fails_required_gate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            mobile = root / "mobile"
            mobile.mkdir()
            _touch_packages(mobile)
            runner = FakeRunner()
            runner.package_codes["RundaleKit"] = 1
            run = SwiftQualityRun(root=root, runner=runner, report_dir=mobile / "report")
            code = run.run(lint=False, format_check=False, packages=True)
            self.assertEqual(code, 1)
            gate = next(g for g in run.gates if g.id == "package-tests:RundaleKit")
            self.assertEqual(gate.status, "failed")
            self.assertTrue(gate.required)

    def test_missing_toolchain_is_unavailable_not_passed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            mobile = root / "mobile"
            mobile.mkdir()
            _touch_packages(mobile)
            runner = FakeRunner()
            runner.paths = {}
            run = SwiftQualityRun(root=root, runner=runner, report_dir=mobile / "report")
            code = run.run(lint=True, format_check=True, packages=True)
            self.assertEqual(code, 1)
            statuses = {gate.id: gate.status for gate in run.gates}
            self.assertEqual(statuses["swiftlint"], "unavailable")
            self.assertEqual(statuses["swiftformat"], "unavailable")
            self.assertEqual(statuses["package-tests:RundaleKit"], "unavailable")
            report = json.loads((mobile / "report" / "swift-quality.json").read_text())
            self.assertGreater(report["counts"]["unavailable"], 0)
            self.assertEqual(report["counts"]["passed"], 0)

    def test_opt_in_gates_are_skipped_not_passed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            mobile = root / "mobile"
            mobile.mkdir()
            _touch_packages(mobile)
            runner = FakeRunner()
            run = SwiftQualityRun(root=root, runner=runner, report_dir=mobile / "report")
            code = run.run(
                lint=False,
                format_check=False,
                packages=False,
                opt_in=["live-endpoint", "soak", "physical-device"],
            )
            self.assertEqual(code, 0)
            by_id = {gate.id: gate for gate in run.gates}
            self.assertEqual(by_id["opt-in:live-endpoint"].status, "skipped")
            self.assertEqual(by_id["opt-in:soak"].status, "skipped")
            self.assertEqual(by_id["opt-in:physical-device"].status, "skipped")
            self.assertFalse(by_id["opt-in:live-endpoint"].required)


if __name__ == "__main__":
    unittest.main()
