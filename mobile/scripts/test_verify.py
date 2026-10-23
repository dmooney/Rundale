#!/usr/bin/env python3
"""Regression tests for the mobile verification orchestrator."""

from __future__ import annotations

import contextlib
import importlib
import io
import json
import os
import plistlib
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import TYPE_CHECKING
from unittest.mock import patch

if TYPE_CHECKING:
    from . import verify as verify_module
else:  # direct discovery from mobile/scripts
    verify_module = (
        importlib.import_module(".verify", __package__)
        if __package__
        else importlib.import_module("verify")
    )

CommandResult = verify_module.CommandResult
VerificationRun = verify_module.VerificationRun


# Runs in these tests must never wait on, or block, a real run's host lock.
os.environ["RUNDALE_MOBILE_VERIFY_LOCK"] = str(
    Path(tempfile.mkdtemp(prefix="rundale-verify-test-")) / "mobile-verify.lock"
)

SIMCTL_JSON = json.dumps(
    {
        "devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-26-5": [
                {
                    "name": "iPhone 17 Pro",
                    "udid": "11111111-1111-1111-1111-111111111111",
                    "state": "Booted",
                    "isAvailable": True,
                },
                {
                    "name": "Apple TV",
                    "udid": "22222222-2222-2222-2222-222222222222",
                    "state": "Shutdown",
                    "isAvailable": True,
                },
            ]
        }
    }
)
XCRESULT_JSON = json.dumps(
    {
        "result": "Passed",
        "totalTestCount": 8,
        "passedTests": 8,
        "failedTests": 0,
        "skippedTests": 0,
    }
)


class FakeRunner:
    def __init__(self, results=None):
        self.calls = []
        self.results = results or {}
        self.fail_swift = False
        self.run_swift_failure = CommandResult(7, stderr="swift tests failed")
        # None leaves no coverage report, like a toolchain that wrote none.
        self.coverage_percent: float | None = 80.0
        self.swift_output = "Executed 8 tests, with 0 failures (0 unexpected) in 0.01 seconds\n"
        self.rust_output = (
            "test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        )
        self.cargo_tree_output = "limerick-mobile-ffi v0.1.0\n"
        self.simctl_output = SIMCTL_JSON
        self.xcresult_output = XCRESULT_JSON
        # Per `-only-testing:` selector case results in a combined run's tree.
        self.case_results: dict[str, str] = {}
        # Per selector failure message, as xcresulttool nests it under the case.
        self.case_failures: dict[str, str] = {}
        # Called with each command first; a CommandResult it returns wins.
        self.intercept = None
        self.device_output = json.dumps(
            {
                "devices": [
                    {
                        "identifier": "00008140-001E30603C10801C",
                        "name": "iPhone 16 Pro Max",
                        "deviceProperties": {
                            "platformVersion": "26.6.1",
                            "osVersionNumber": "26.6.1",
                            "hardwareModel": "iPhone17,2",
                            "developerModeStatus": "enabled",
                        },
                        "hardwareProperties": {
                            "udid": "00008140-001E30603C10801C",
                            "marketingName": "iPhone 16 Pro Max",
                            "productType": "iPhone17,2",
                        },
                        "connectionProperties": {"pairingState": "paired"},
                    }
                ]
            }
        )

    def run(self, argv, *, cwd, env=None, timeout_seconds=None):
        command = tuple(str(part) for part in argv)
        self.calls.append({"argv": command, "cwd": Path(cwd), "env": dict(env or {})})
        if self.intercept is not None:
            intercepted = self.intercept(command)
            if intercepted is not None:
                return intercepted
        result = self.results.get(command)
        if callable(result):
            return result(command)
        if result is not None:
            return result
        if command[:5] == ("xcrun", "simctl", "list", "devices", "available"):
            return CommandResult(0, stdout=self.simctl_output)
        if command[:5] == ("xcrun", "xcresulttool", "get", "test-results", "summary"):
            return CommandResult(0, stdout=self.xcresult_output)
        if command[:5] == ("xcrun", "xcresulttool", "get", "test-results", "tests"):
            return CommandResult(0, stdout=json.dumps(self.xcresult_tree()))
        if command[:5] == ("xcrun", "devicectl", "list", "devices", "--json-output"):
            return CommandResult(0, stdout=self.device_output)
        if command[:5] == ("rustup", "run", verify_module.RUST_TOOLCHAIN, "cargo", "test"):
            return CommandResult(0, stdout=self.rust_output)
        if command[:5] == ("rustup", "run", verify_module.RUST_TOOLCHAIN, "cargo", "tree"):
            return CommandResult(0, stdout=self.cargo_tree_output)
        if self.fail_swift and command[:2] == ("swift", "test"):
            return self.run_swift_failure
        if command[:2] == ("swift", "test"):
            if self.coverage_percent is not None:
                write_codecov(Path(cwd), command, self.coverage_percent)
            return CommandResult(0, stdout=self.swift_output)
        return CommandResult(0, stdout="ok\n")

    def xcresult_tree(self):
        """One case per `-only-testing` selector of the last xcodebuild test run."""

        test_run = next(
            call["argv"]
            for call in reversed(self.calls)
            if call["argv"][:1] == ("xcodebuild",) and call["argv"][-1] == "test"
        )
        bundles: dict[str, dict[str, list[dict]]] = {}
        for arg in test_run:
            if not arg.startswith("-only-testing:"):
                continue
            bundle, _, rest = arg.removeprefix("-only-testing:").partition("/")
            suite, _, test = rest.partition("/")
            bundles.setdefault(bundle, {}).setdefault(suite or "FixtureTests", []).append(
                {
                    "nodeType": "Test Case",
                    "name": test or "testFixture()",
                    "result": self.case_results.get(arg, "Passed"),
                    "children": [
                        {"nodeType": "Failure Message", "name": message, "result": "Failed"}
                        for message in filter(None, [self.case_failures.get(arg)])
                    ],
                }
            )
        return {
            "testNodes": [
                {
                    "nodeType": "Test Plan",
                    "name": "Rundale",
                    "children": [
                        {
                            "nodeType": "UI test bundle"
                            if bundle.endswith("UITests")
                            else "Unit test bundle",
                            "name": bundle,
                            "children": [
                                {"nodeType": "Test Suite", "name": suite, "children": cases}
                                for suite, cases in suites.items()
                            ],
                        }
                        for bundle, suites in bundles.items()
                    ],
                }
            ]
        }


def write_codecov(root: Path, command: tuple[str, ...], percent: float) -> None:
    """Write the llvm-cov export `swift test --enable-code-coverage` leaves behind."""

    package = root / command[command.index("--package-path") + 1]
    scratch = root / command[command.index("--scratch-path") + 1]
    report = scratch / "arm64-apple-macosx" / "debug" / "codecov" / f"{package.name}.json"
    report.parent.mkdir(parents=True, exist_ok=True)
    files = [
        {
            "filename": str(package / "Sources" / package.name / "Code.swift"),
            "summary": {"lines": {"count": 200, "covered": int(percent * 2)}},
        },
        {
            # Test and dependency files are outside the package's Sources.
            "filename": str(package / "Tests" / "FixtureTests.swift"),
            "summary": {"lines": {"count": 100, "covered": 0}},
        },
    ]
    report.write_text(json.dumps({"data": [{"files": files}]}), encoding="utf-8")


def create_phase1_fixture(root: Path) -> None:
    package = root / "mobile" / "RundaleKit"
    (package / "Tests" / "RundaleKitTests").mkdir(parents=True)
    (package / "Package.swift").write_text("// fixture\n", encoding="utf-8")
    (package / "Tests" / "RundaleKitTests" / "FixtureTests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    (root / "mobile" / "RundaleUITests").mkdir(parents=True)
    (root / "mobile" / "RundaleUITests" / "RundaleUITests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    (root / "mobile" / "project.yml").write_text("name: Rundale\n", encoding="utf-8")
    (root / "mobile" / "coverage-baseline.json").write_text(
        json.dumps(
            {
                "tolerance_percentage_points": 1.0,
                "packages": {
                    name: {"line_percent": 80.0}
                    for name in ("RundaleKit", "RundaleBridge", "LimerickEndpointKit")
                },
            }
        ),
        encoding="utf-8",
    )
    (root / "mobile" / "Rundale.xcodeproj").mkdir(parents=True)


def create_phase2_fixture(root: Path) -> None:
    create_phase1_fixture(root)
    bridge = root / "mobile" / "RundaleBridge"
    (bridge / "Tests" / "RundaleBridgeTests").mkdir(parents=True)
    (bridge / "Package.swift").write_text("// fixture\n", encoding="utf-8")
    (bridge / "Tests" / "RundaleBridgeTests" / "FixtureTests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    (root / "mobile" / "RundaleUITests" / "RundalePhase2UITests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    (root / "mobile" / "RundaleUITests" / "RundalePhase3UITests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    (root / "mobile" / "RundaleUITests" / "RundalePhase4UITests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    (root / "mobile" / "RundaleTests").mkdir()
    (root / "mobile" / "RundaleTests" / "RundalePhase4Tests.swift").write_text("// fixture\n")
    endpoint_kit = root / "mobile" / "LimerickEndpointKit"
    (endpoint_kit / "Tests" / "LimerickEndpointKitTests").mkdir(parents=True)
    (endpoint_kit / "Package.swift").write_text("// fixture\n", encoding="utf-8")
    (endpoint_kit / "Tests" / "LimerickEndpointKitTests" / "FixtureTests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    packaging = root / "mobile" / "scripts" / "build-rust-mobile.sh"
    packaging.parent.mkdir(parents=True, exist_ok=True)
    packaging.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")


class VerificationRunnerTests(unittest.TestCase):
    def test_build_identity_uses_app_not_runner_or_dependency_plist(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            derived = root / "DerivedData"
            for app, version in (("Dependency.app", "999"), ("Rundale.app", "7")):
                plist = derived / "Build" / "Products" / "Release-iphoneos" / app / "Info.plist"
                plist.parent.mkdir(parents=True)
                plist.write_bytes(
                    plistlib.dumps(
                        {"CFBundleVersion": version, "CFBundleShortVersionString": "0.1.0"}
                    )
                )
            identity = VerificationRun(root, command_runner=FakeRunner())._built_app_identity(
                derived
            )
            self.assertEqual(identity["app_identity"]["CFBundleVersion"], "7")

    def test_release_can_pin_simulator_without_changing_other_booted_devices(self):
        with patch.dict(os.environ, {"RUNDALE_IOS_SIMULATOR": "small-phone"}):
            parser = verify_module.build_parser()
            self.assertEqual(parser.parse_args([]).simulator, "small-phone")
            self.assertEqual(
                parser.parse_args(["--simulator", "primary-phone"]).simulator, "primary-phone"
            )

    def test_simulator_type_identifier_accepts_custom_iphone_name_only(self):
        payload = {
            "devices": {
                "com.apple.CoreSimulator.SimRuntime.iOS-26-5": [
                    {
                        "name": "Rundale Phase 1 Small iPhone",
                        "udid": "small-iphone",
                        "state": "Booted",
                        "isAvailable": True,
                        "deviceTypeIdentifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation",
                    },
                    {
                        "name": "iPhone-shaped Apple TV",
                        "udid": "named-nonphone",
                        "state": "Booted",
                        "isAvailable": True,
                        "deviceTypeIdentifier": "com.apple.CoreSimulator.SimDeviceType.Apple-TV-4K-3rd-generation",
                    },
                ]
            }
        }

        candidates = verify_module._simulator_candidates(payload)

        self.assertEqual([candidate["udid"] for candidate in candidates], ["small-iphone"])

    def test_missing_required_inputs_are_unavailable_and_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            fake = FakeRunner()
            report = VerificationRun(root, command_runner=fake).run(1)

            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(report["status"], "failed")
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(by_id["swift-package-tests"]["status"], "unavailable")
            self.assertEqual(by_id["xcodegen"]["status"], "unavailable")
            self.assertTrue(by_id["swift-package-tests"]["blocking"])
            self.assertEqual(by_id["physical-iphone-interaction"]["status"], "not_automatable")
            self.assertFalse(by_id["physical-iphone-interaction"]["blocking"])
            self.assertTrue((root / "mobile" / ".verification" / "verify.json").is_file())
            self.assertTrue((root / "mobile" / ".verification" / "verify.junit.xml").is_file())

            junit = ET.parse(root / "mobile" / ".verification" / "verify.junit.xml").getroot()
            self.assertGreater(int(junit.attrib["errors"]), 0)
            self.assertTrue(any(node.attrib.get("status") == "not_automatable" for node in junit))

    def test_successful_automated_run_reports_commands_and_physical_pending(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            report = VerificationRun(root, command_runner=fake).run(1)

            self.assertEqual(report["exit_code"], 0)
            self.assertEqual(report["status"], "passed")
            self.assertEqual(report["summary"]["failed"], 0)
            self.assertEqual(report["summary"]["unavailable"], 0)
            self.assertGreater(report["summary"]["not_automatable"], 0)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(by_id["swift-package-tests"]["status"], "passed")
            self.assertEqual(by_id["xcodegen"]["status"], "passed")
            self.assertEqual(by_id["ios-device-build"]["status"], "passed")
            self.assertEqual(by_id["ios-simulator-tests"]["status"], "passed")
            simulator_call = next(
                call
                for call in fake.calls
                if call["argv"][0:2] == ("xcodebuild", "-project") and "test" in call["argv"]
            )
            self.assertIn(
                "platform=iOS Simulator,id=11111111-1111-1111-1111-111111111111",
                simulator_call["argv"],
            )
            self.assertEqual(
                {arg for arg in simulator_call["argv"] if arg.startswith("-only-testing:")},
                {
                    "-only-testing:RundaleUITests/RundaleUITests",
                    "-only-testing:RundaleUITests/RundalePhase1AuditUITests",
                    "-only-testing:RundaleUITests/RundalePhase1TimerAuditUITests",
                    "-only-testing:RundaleTests/Phase1AuditVolumeTests",
                    "-only-testing:RundaleTests/LaunchConfigurationTests",
                    "-only-testing:RundaleTests/TranscriptScrollingTests",
                    "-only-testing:RundaleTests/HeaderWeatherTests",
                },
            )
            self.assertTrue(
                (root / "mobile" / ".verification" / "logs" / "ios-simulator-tests.log").is_file()
            )

    def test_explicit_device_runs_signed_phase1_suite_and_records_xcresult(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            report = VerificationRun(
                root,
                command_runner=fake,
                device="00008140-001E30603C10801C",
                development_team="TEAM123",
            ).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(by_id["physical-iphone-phase1-tests"]["status"], "passed")
            device_call = next(
                call
                for call in fake.calls
                if call["argv"][0:2] == ("xcodebuild", "-project")
                and "platform=iOS,id=00008140-001E30603C10801C" in call["argv"]
            )
            self.assertIn("-allowProvisioningUpdates", device_call["argv"])
            self.assertIn("ONLY_ACTIVE_ARCH=YES", device_call["argv"])
            self.assertIn("DEVELOPMENT_TEAM=TEAM123", device_call["argv"])
            self.assertIn("-resultBundlePath", device_call["argv"])
            self.assertIn("physical-iphone-phase1-tests-results", by_id)
            self.assertEqual(by_id["physical-iphone-phase1-tests"]["details"]["target"], "device")

    def test_simulator_suites_run_in_parallel_unless_one_worker(self):
        def simulator_argv(**kwargs):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "mobile").mkdir()
                create_phase1_fixture(root)
                fake = FakeRunner()
                VerificationRun(root, command_runner=fake, **kwargs).run(1)
                return next(
                    call["argv"]
                    for call in fake.calls
                    if call["argv"][0:2] == ("xcodebuild", "-project")
                    and "test" in call["argv"]
                    and any(arg.startswith("platform=iOS Simulator") for arg in call["argv"])
                )

        argv = simulator_argv()
        self.assertEqual(argv[argv.index("-parallel-testing-enabled") + 1], "YES")
        self.assertEqual(argv[argv.index("-parallel-testing-worker-count") + 1], "6")
        argv = simulator_argv(parallel_workers=2)
        self.assertEqual(argv[argv.index("-parallel-testing-worker-count") + 1], "2")
        argv = simulator_argv(parallel_workers=1)
        self.assertNotIn("-parallel-testing-enabled", argv)
        self.assertNotIn("-parallel-testing-worker-count", argv)
        with self.assertRaises(ValueError):
            VerificationRun(Path("."), parallel_workers=0)

    def test_parallel_workers_option_reads_environment(self):
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(verify_module.build_parser().parse_args([]).parallel_workers, 6)
            args = verify_module.build_parser().parse_args(["--parallel-workers", "1"])
            self.assertEqual(args.parallel_workers, 1)
        with patch.dict(os.environ, {"RUNDALE_PARALLEL_WORKERS": "3"}, clear=True):
            self.assertEqual(verify_module.build_parser().parse_args([]).parallel_workers, 3)
        with patch.dict(os.environ, {"RUNDALE_PARALLEL_WORKERS": "lots"}, clear=True):
            parser = verify_module.build_parser()
            with patch("sys.stderr"), self.assertRaises(SystemExit) as raised:
                parser.parse_args([])
            self.assertEqual(raised.exception.code, 2)

    def test_command_timeout_option_reads_environment(self):
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(verify_module.build_parser().parse_args([]).command_timeout, 30)
            args = verify_module.build_parser().parse_args(["--command-timeout", "75"])
            self.assertEqual(args.command_timeout, 75)
        with patch.dict(os.environ, {"RUNDALE_COMMAND_TIMEOUT_MINUTES": "80"}, clear=True):
            self.assertEqual(verify_module.build_parser().parse_args([]).command_timeout, 80)
        for bad in ("soon", "0", "-5"):
            with patch.dict(os.environ, {"RUNDALE_COMMAND_TIMEOUT_MINUTES": bad}, clear=True):
                parser = verify_module.build_parser()
                with patch("sys.stderr"), self.assertRaises(SystemExit) as raised:
                    parser.parse_args([])
                self.assertEqual(raised.exception.code, 2)

    def test_failure_reason_names_a_timeout_before_the_last_output(self):
        killed = CommandResult(
            None, stderr="launching the app", duration_seconds=1800.4, timed_out=True
        )
        self.assertEqual(
            verify_module._failure_reason(killed, "xcodebuild failed"),
            "timed out after 1800s: launching the app",
        )
        silent = CommandResult(None, duration_seconds=60, timed_out=True)
        self.assertEqual(
            verify_module._failure_reason(silent, "xcodebuild failed"),
            "timed out after 60s: xcodebuild failed",
        )
        failed = CommandResult(65, stderr="Testing failed")
        self.assertEqual(verify_module._failure_reason(failed, "fallback"), "Testing failed")

    def test_device_is_opt_in_and_simulator_remains_default(self):
        with patch.dict(os.environ, {}, clear=True):
            args = verify_module.build_parser().parse_args([])
            self.assertIsNone(args.device)
            args = verify_module.build_parser().parse_args(["--physical-device", "phone-udid"])
            self.assertEqual(args.device, "phone-udid")

    def test_missing_physical_device_is_unavailable_and_device_suites_skip(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            fake.device_output = json.dumps({"devices": []})
            report = VerificationRun(root, command_runner=fake, device="missing").run(1)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(by_id["physical-device-selection"]["status"], "unavailable")
            self.assertEqual(by_id["physical-iphone-phase1-tests"]["status"], "skipped")
            self.assertNotEqual(by_id["physical-iphone-phase1-tests"]["status"], "failed")

    def test_performance_forces_release_and_opt_ins_require_device(self):
        with self.assertRaises(ValueError):
            VerificationRun(Path("/tmp/rundale-test"), soak=True)
        run = VerificationRun(Path("/tmp/rundale-test"), device="device", performance=True)
        self.assertEqual(run.configuration, "Release")
        env = run._xcode_env()
        self.assertEqual(env["RUNDALE_PERFORMANCE_UI_TESTS"], "1")
        self.assertEqual(env["TEST_RUNNER_RUNDALE_PERFORMANCE_UI_TESTS"], "1")
        soak = VerificationRun(Path("/tmp/rundale-test"), device="device", soak=True)
        soak_env = soak._xcode_env()
        self.assertEqual(soak_env["RUNDALE_SOAK_UI_TESTS"], "1")
        self.assertEqual(soak_env["TEST_RUNNER_RUNDALE_SOAK_UI_TESTS"], "1")

    def test_physical_phase2_excludes_simulator_only_return_key_test(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()
            report = VerificationRun(
                root,
                command_runner=fake,
                device="00008140-001E30603C10801C",
            ).run(2)
            phase2 = next(
                call for call in report["suites"] if call["id"] == "physical-iphone-phase2-tests"
            )
            self.assertEqual(phase2["status"], "passed")
            physical_call = next(
                call
                for call in fake.calls
                if "platform=iOS,id=00008140-001E30603C10801C" in call["argv"]
            )
            self.assertIn(
                "-skip-testing:RundaleUITests/RundalePhase2UITests/testSimulatorReturnKeySubmitsDraft",
                physical_call["argv"],
            )

    def test_optional_device_suite_preflight_failure_is_visible(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()
            fake.device_output = json.dumps({"devices": []})
            report = VerificationRun(root, command_runner=fake, device="missing", soak=True).run(4)
            suite = next(s for s in report["suites"] if s["id"] == "physical-iphone-soak")
            self.assertEqual(suite["status"], "skipped")
            self.assertTrue(suite["blocking"])

    def test_swift_style_gates_run_pinned_tools_and_block_on_violations(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            lint = (
                "mobile/.build/swift-tools/bin/swiftlint",
                "lint",
                "--strict",
                "--quiet",
                "--config",
                "mobile/.swiftlint.yml",
                "mobile",
            )
            fake = FakeRunner(
                {lint: CommandResult(2, stdout="Foo.swift:1:1: error: Force Try Violation")}
            )
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["swift-tools"]["status"], "passed")
            self.assertEqual(by_id["swiftlint"]["status"], "failed")
            self.assertTrue(by_id["swiftlint"]["blocking"])
            self.assertIn("Force Try Violation", by_id["swiftlint"]["reason"])
            self.assertEqual(by_id["swiftformat"]["status"], "passed")
            self.assertEqual(
                by_id["swiftformat"]["command"],
                [
                    "mobile/.build/swift-tools/bin/swiftformat",
                    "--lint",
                    "--config",
                    "mobile/.swiftformat",
                    "mobile",
                ],
            )

    def test_swift_tool_install_failure_blocks_both_style_gates(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            install = ("bash", "mobile/scripts/install-swift-tools.sh")
            fake = FakeRunner({install: CommandResult(1, stderr="digest mismatch")})
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["swift-tools"]["status"], "failed")
            for identifier in ("swiftlint", "swiftformat"):
                self.assertEqual(by_id[identifier]["status"], "skipped")
                self.assertTrue(by_id[identifier]["blocking"])
            self.assertFalse(any(call["argv"][0].endswith("/swiftlint") for call in fake.calls))

    def test_package_tests_treat_warnings_as_errors_and_collect_coverage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            report = VerificationRun(root, command_runner=fake).run(1)

            swift = next(
                call["argv"] for call in fake.calls if call["argv"][:2] == ("swift", "test")
            )
            self.assertIn("--enable-code-coverage", swift)
            self.assertEqual(swift[swift.index("-Xswiftc") + 1], "-warnings-as-errors")
            by_id = {suite["id"]: suite for suite in report["suites"]}
            coverage = by_id["swift-package-tests-coverage"]
            self.assertEqual(coverage["status"], "passed")
            # Only the package's Sources count; the test file is excluded.
            self.assertEqual(coverage["details"]["measured"]["count"], 200)
            self.assertEqual(coverage["details"]["measured"]["percent"], 80.0)

    def test_coverage_drop_beyond_tolerance_fails_and_within_tolerance_passes(self):
        for percent, status in ((79.0, "passed"), (78.5, "failed")):
            with self.subTest(percent=percent), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "mobile").mkdir()
                create_phase1_fixture(root)
                fake = FakeRunner()
                fake.coverage_percent = percent
                report = VerificationRun(root, command_runner=fake).run(1)

                by_id = {suite["id"]: suite for suite in report["suites"]}
                self.assertEqual(by_id["swift-package-tests"]["status"], "passed")
                coverage = by_id["swift-package-tests-coverage"]
                self.assertEqual(coverage["status"], status)
                self.assertEqual(coverage["blocking"], status == "failed")
                if status == "failed":
                    self.assertIn("below the 80.00% baseline", coverage["reason"])

    def test_missing_coverage_baseline_or_report_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            (root / "mobile" / "coverage-baseline.json").unlink()
            report = VerificationRun(root, command_runner=FakeRunner()).run(1)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(by_id["swift-package-tests-coverage"]["status"], "failed")
            self.assertIn(
                "no usable RundaleKit baseline", by_id["swift-package-tests-coverage"]["reason"]
            )

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            fake.coverage_percent = None
            report = VerificationRun(root, command_runner=fake).run(1)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(by_id["swift-package-tests"]["status"], "failed")
            self.assertIn("no coverage report", by_id["swift-package-tests"]["reason"])

    def test_fast_lane_runs_style_and_all_three_packages_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()
            report = VerificationRun(root, command_runner=fake).run(fast=True)

            self.assertEqual(report["exit_code"], 0)
            self.assertEqual(report["phase"], "fast")
            self.assertEqual(
                [suite["id"] for suite in report["suites"]],
                [
                    "swift-tools",
                    "swiftlint",
                    "swiftformat",
                    "swift-package-tests",
                    "swift-package-tests-coverage",
                    "swift-bridge-tests",
                    "swift-bridge-tests-coverage",
                    "swift-endpoint-kit-tests",
                    "swift-endpoint-kit-tests-coverage",
                ],
            )
            self.assertFalse(
                any(
                    call["argv"][0] in {"xcodebuild", "xcodegen", "xcrun", "rustup"}
                    for call in fake.calls
                )
            )

    def test_smoke_runs_unit_classes_and_core_ui_tests_and_records_no_full_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            fake = FakeRunner()
            with patch.object(VerificationRun, "_store_full_pass") as store:
                report = VerificationRun(root, command_runner=fake, smoke=True).run()
            store.assert_not_called()
            self.assertEqual(report["exit_code"], 0)
            self.assertEqual(report["phase"], "smoke")
            by_id = {suite["id"]: suite for suite in report["suites"]}
            builds = [
                call["argv"]
                for call in fake.calls
                if call["argv"][0] == "xcodebuild" and call["argv"][-1] == "test"
            ]
            self.assertEqual(len(builds), 1, "one combined simulator run")
            selectors = [
                a.removeprefix("-only-testing:")
                for a in builds[0]
                if a.startswith("-only-testing:")
            ]
            ui = [s for s in selectors if s.startswith("RundaleUITests")]
            self.assertEqual(sorted(ui), sorted(verify_module.SMOKE_UI_TESTS))
            self.assertIn("RundaleTests/LaunchConfigurationTests", selectors)
            self.assertIn("RundaleTests", selectors)
            # Unit classes still run under their suite; UI-only suites are
            # left to the full run without blocking.
            self.assertEqual(by_id["ios-simulator-tests"]["status"], "passed")
            self.assertIn("--smoke", by_id["ios-simulator-tests"]["name"])
            for identifier in ("phase3-ios-simulator-tests", "phase4-ios-simulator-tests"):
                self.assertEqual(by_id[identifier]["status"], "skipped", identifier)
                self.assertFalse(by_id[identifier]["blocking"], identifier)
            self.assertEqual(by_id["phase4-ios-controller-tests"]["status"], "passed")
            self.assertEqual(by_id["ios-simulator-smoke-tests"]["status"], "passed")
            self.assertEqual(by_id["ios-simulator-smoke-tests-results"]["status"], "passed")
            # Rust gates and the device build still run.
            for identifier in (
                "limerick-core-mobile-tests",
                "mobile-rust-packaging",
                "ios-device-build",
            ):
                self.assertIn(identifier, by_id)

    def test_smoke_fails_when_a_core_ui_test_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            fake = FakeRunner()
            selector = f"-only-testing:{verify_module.SMOKE_UI_TESTS[2]}"
            fake.case_results[selector] = "Failed"
            fake.case_failures[selector] = "RundalePhase2UITests.swift:9: XCTAssertTrue failed"
            report = VerificationRun(root, command_runner=fake, smoke=True).run()
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            smoke = by_id["ios-simulator-smoke-tests"]
            self.assertEqual(smoke["status"], "failed")
            self.assertTrue(smoke["blocking"])
            self.assertIn("XCTAssertTrue failed", smoke["details"]["failures"][0])

    def test_smoke_fails_when_a_named_core_ui_test_did_not_run(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            renamed = (*verify_module.SMOKE_UI_TESTS[:-1], "RundaleUITests/RundaleUITests/testGone")
            fake = FakeRunner()
            real_tree = fake.xcresult_tree

            def without_gone(*args, **kwargs):
                tree = real_tree(*args, **kwargs)
                for plan in tree["testNodes"]:
                    for bundle in plan["children"]:
                        for suite in bundle["children"]:
                            suite["children"] = [
                                c for c in suite["children"] if c["name"] != "testGone"
                            ]
                return tree

            fake.xcresult_tree = without_gone
            with patch.object(verify_module, "SMOKE_UI_TESTS", renamed):
                report = VerificationRun(root, command_runner=fake, smoke=True).run()
            by_id = {suite["id"]: suite for suite in report["suites"]}
            smoke = by_id["ios-simulator-smoke-tests"]
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(smoke["status"], "failed")
            self.assertIn("did not run: RundaleUITests/RundaleUITests/testGone", smoke["reason"])

    def test_full_run_still_records_a_full_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            with patch.object(VerificationRun, "_store_full_pass") as store:
                report = VerificationRun(root, command_runner=FakeRunner()).run()
            self.assertEqual(report["exit_code"], 0)
            store.assert_called_once()

    def test_smoke_rejects_fast_phase_and_device(self):
        with self.assertRaises(ValueError):
            VerificationRun(Path("/tmp/rundale-test"), smoke=True).run(fast=True)
        with self.assertRaises(ValueError):
            VerificationRun(Path("/tmp/rundale-test"), smoke=True).run(2)
        with self.assertRaises(ValueError):
            VerificationRun(Path("/tmp/rundale-test"), smoke=True, device="device").run()

    def test_fast_lane_rejects_phase_and_device(self):
        with self.assertRaises(ValueError):
            VerificationRun(Path("/tmp/rundale-test")).run(2, fast=True)
        with self.assertRaises(ValueError):
            VerificationRun(Path("/tmp/rundale-test"), device="device").run(fast=True)

    def test_failed_package_reason_names_the_failing_test(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            output = (
                "[11/16] Compiling RundaleKit SemanticEvent.swift\n"
                "/x/RundaleKitTests.swift:41: error: -[RundaleKitTests testX] : "
                "XCTAssertEqual failed\n"
                "Executed 29 tests, with 1 failure (0 unexpected)\n"
            )
            fake = FakeRunner()
            fake.fail_swift = True
            fake.run_swift_failure = CommandResult(1, stdout=output, stderr="build noise\n")
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(by_id["swift-package-tests"]["status"], "failed")
            self.assertEqual(
                by_id["swift-package-tests"]["reason"],
                "/x/RundaleKitTests.swift:41: error: -[RundaleKitTests testX] : XCTAssertEqual failed",
            )

    def test_opt_in_suites_reject_early_phases(self):
        run = VerificationRun(Path("/tmp/rundale-test"), device="device", soak=True)
        with self.assertRaises(ValueError):
            run.run(1)

    def test_command_failure_propagates_and_keeps_other_results(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)

            fake = FakeRunner()
            fake.fail_swift = True
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["swift-package-tests"]["status"], "failed")
            self.assertEqual(by_id["swift-package-tests"]["returncode"], 7)
            self.assertEqual(by_id["xcodegen"]["status"], "passed")
            self.assertEqual(by_id["ios-simulator-tests"]["status"], "passed")
            log = (
                root / "mobile" / ".verification" / "logs" / "swift-package-tests.log"
            ).read_text()
            self.assertIn("swift tests failed", log)

    def test_default_lists_future_phases_without_blocking_phase_one(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            report = VerificationRun(root, command_runner=FakeRunner()).run()

            future = [suite for suite in report["suites"] if suite["kind"] == "future-phase"]
            self.assertEqual([suite["phase"] for suite in future], [5, 6, 7])
            self.assertTrue(all(suite["status"] == "unavailable" for suite in future))
            self.assertTrue(all(not suite["blocking"] for suite in future))
            self.assertEqual(report["implemented_phases"], [1, 2, 3, 4])
            self.assertEqual(
                {suite["status"] for suite in report["suites"] if suite["phase"] == 2}
                - {"passed", "not_automatable", "unavailable"},
                set(),
            )
            self.assertEqual(report["exit_code"], 0)

    def test_default_does_not_reuse_failed_simulator_boot_as_phase2_ready(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()
            fake.simctl_output = json.dumps(
                {
                    "devices": {
                        "com.apple.CoreSimulator.SimRuntime.iOS-26-5": [
                            {
                                "name": "iPhone 17 Pro",
                                "udid": "11111111-1111-1111-1111-111111111111",
                                "state": "Shutdown",
                                "isAvailable": True,
                            }
                        ]
                    }
                }
            )
            fake.results[("xcrun", "simctl", "boot", "11111111-1111-1111-1111-111111111111")] = (
                CommandResult(1, stderr="simulator boot failed")
            )
            report = VerificationRun(root, command_runner=fake).run()

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(by_id["simulator-boot"]["status"], "failed")
            self.assertEqual(by_id["phase2-ios-simulator-tests"]["status"], "skipped")
            self.assertTrue(by_id["phase2-ios-simulator-tests"]["blocking"])

    def test_phase_numbers_follow_the_spec_milestones(self):
        # Product spec §17 runs to Milestone 7 (Release).
        parser = verify_module.build_parser()
        self.assertEqual(parser.parse_args(["--phase", "7"]).phase, 7)
        self.assertIsNone(parser.parse_args(["--phase", "all"]).phase)
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            parser.parse_args(["--phase", "8"])

    def test_explicit_unimplemented_phase_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            report = VerificationRun(root, command_runner=FakeRunner()).run(5)

            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(report["suites"][0]["id"], "phase-5-verification")
            self.assertEqual(report["suites"][0]["status"], "unavailable")
            self.assertTrue(report["suites"][0]["blocking"])

    def test_phase4_runs_reliability_and_prior_regressions_once(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            fake = FakeRunner()
            report = VerificationRun(root, command_runner=fake).run(4)
            self.assertEqual(report["exit_code"], 0)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            for identifier in (
                "ios-simulator-tests",
                "phase2-ios-simulator-tests",
                "phase3-ios-simulator-tests",
                "phase4-ios-simulator-tests",
                "phase4-ios-controller-tests",
            ):
                self.assertEqual(by_id[identifier]["status"], "passed")
            physical = by_id["physical-iphone-phase4-session"]
            self.assertEqual(physical["status"], "not_automatable")
            self.assertFalse(physical["blocking"])
            builds = [
                call["argv"]
                for call in fake.calls
                if call["argv"][0] == "xcodebuild" and "test" in call["argv"]
            ]
            # Every simulator suite runs in one xcodebuild, so the classes of all
            # five share one set of simulator clones and one build.
            self.assertEqual(len(builds), 1)
            selectors = {arg for arg in builds[0] if arg.startswith("-only-testing:")}
            for selector in (
                "RundaleUITests/RundaleUITests",
                "RundaleUITests/RundalePhase2UITests",
                "RundaleUITests/RundalePhase3UITests",
                "RundaleUITests/RundalePhase4UITests",
                "RundaleTests",
            ):
                self.assertIn(f"-only-testing:{selector}", selectors)
            self.assertEqual(by_id["ios-simulator-combined-run"]["status"], "passed")
            self.assertEqual(by_id["phase4-ios-simulator-tests-results"]["status"], "passed")
            self.assertEqual(by_id["phase3-ios-simulator-tests"]["details"]["totalTestCount"], 3)

    def test_combined_run_fails_only_the_suite_whose_case_failed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            fake = FakeRunner()
            selector = "-only-testing:RundaleUITests/RundalePhase3UITests"
            fake.case_results[selector] = "Failed"
            fake.case_failures[selector] = "RundalePhase3UITests.swift:42: XCTAssertTrue failed"
            report = VerificationRun(root, command_runner=fake).run(4)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["phase3-ios-simulator-tests"]["status"], "failed")
            self.assertIn("1 failed test", by_id["phase3-ios-simulator-tests"]["reason"])
            # The assertion text reaches summary.txt, which CI prints to its log.
            failure = "RundalePhase3UITests.testFixture(): RundalePhase3UITests.swift:42: XCTAssertTrue failed"
            self.assertEqual(by_id["phase3-ios-simulator-tests"]["details"]["failures"], [failure])
            self.assertNotIn("failures", by_id["phase2-ios-simulator-tests"]["details"])
            summary = (root / "mobile" / ".verification" / "summary.txt").read_text()
            self.assertIn(f"\n    {failure}\n", summary)
            self.assertEqual(by_id["phase3-ios-simulator-test-results"]["status"], "failed")
            for identifier in ("ios-simulator-tests", "phase2-ios-simulator-tests"):
                self.assertEqual(by_id[identifier]["status"], "passed", identifier)

    def test_summary_prints_a_long_failure_message_as_one_marked_line(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            fake = FakeRunner()
            selector = "-only-testing:RundaleUITests/RundalePhase3UITests"
            fake.case_results[selector] = "Failed"
            message = "A.swift:1: XCTAssertEqual failed\n" + "x" * 500
            fake.case_failures[selector] = message
            report = VerificationRun(root, command_runner=fake).run(4)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            details = by_id["phase3-ios-simulator-tests"]["details"]
            self.assertEqual(
                details["failures"], [f"RundalePhase3UITests.testFixture(): {message}"]
            )
            summary = (root / "mobile" / ".verification" / "summary.txt").read_text()
            line = next(line for line in summary.splitlines() if "A.swift:1" in line)
            self.assertIn("XCTAssertEqual failed xxx", line)
            self.assertEqual(len(line), 4 + verify_module.FAILURE_MESSAGE_CHARS)
            self.assertTrue(line.endswith("…"))

    def test_combined_run_that_exits_nonzero_fails_passing_suites(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)

            def crashed(command):
                if command[0] == "xcodebuild" and command[-1] == "test":
                    return CommandResult(65, stderr="Test runner crashed")
                return None

            fake = FakeRunner()
            fake.intercept = crashed
            report = VerificationRun(root, command_runner=fake).run(4)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            suite = by_id["phase2-ios-simulator-tests"]
            self.assertEqual(suite["status"], "failed")
            self.assertIn("combined run failed", suite["reason"])

    def test_combined_run_with_unreadable_results_fails_every_suite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)

            def unreadable(command):
                if command[:5] == ("xcrun", "xcresulttool", "get", "test-results", "tests"):
                    return CommandResult(1, stderr="no result bundle")
                return None

            fake = FakeRunner()
            fake.intercept = unreadable
            report = VerificationRun(root, command_runner=fake).run(4)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            for identifier in ("ios-simulator-tests", "phase4-ios-controller-tests"):
                self.assertEqual(by_id[identifier]["status"], "failed", identifier)
                self.assertIn("no result bundle", by_id[identifier]["reason"])

    def test_phase4_missing_suite_is_blocking(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            (root / "mobile/RundaleUITests/RundalePhase4UITests.swift").unlink()
            report = VerificationRun(root, command_runner=FakeRunner()).run(4)
            self.assertEqual(report["exit_code"], 1)
            suite = next(s for s in report["suites"] if s["id"] == "phase4-ios-simulator-tests")
            self.assertEqual(suite["status"], "unavailable")
            self.assertTrue(suite["blocking"])

    def test_phase4_missing_controller_reliability_source_is_blocking(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            (root / "mobile/RundaleTests/RundalePhase4Tests.swift").unlink()
            report = VerificationRun(root, command_runner=FakeRunner()).run(4)
            self.assertEqual(report["exit_code"], 1)
            suite = next(s for s in report["suites"] if s["id"] == "phase4-ios-controller-tests")
            self.assertEqual(suite["status"], "unavailable")
            self.assertTrue(suite["blocking"])

    def test_phase2_runs_rust_bridge_and_targeted_ui_gates(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()
            report = VerificationRun(root, command_runner=fake).run(2)

            self.assertEqual(report["exit_code"], 0)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            for identifier in (
                "limerick-core-mobile-tests",
                "limerick-persistence-tests",
                "limerick-mobile-ffi-tests",
                "mobile-dependency-graph",
                "mobile-rust-packaging",
                "swift-package-tests-phase2",
                "swift-bridge-tests",
                "swift-endpoint-kit-tests",
                "xcodegen",
                "ios-device-build",
                "phase2-ios-simulator-tests",
                "phase2-ios-simulator-test-results",
            ):
                self.assertEqual(by_id[identifier]["status"], "passed", identifier)
                self.assertEqual(by_id[identifier]["phase"], 2, identifier)

            rust_test_calls = [
                call
                for call in fake.calls
                if call["argv"][:5]
                == ("rustup", "run", verify_module.RUST_TOOLCHAIN, "cargo", "test")
            ]
            self.assertEqual(len(rust_test_calls), 3)
            for call in rust_test_calls:
                index = call["argv"].index("--manifest-path")
                self.assertEqual(call["argv"][index + 1], "limerick/Cargo.toml")
            core_call = next(c for c in rust_test_calls if "limerick-core" in c["argv"])
            # The whole portable configuration, not a filtered subset: it
            # includes the turn API and the Endpoint call tests.
            self.assertEqual(
                core_call["argv"][-5:],
                ("--no-default-features", "--features", "mobile", "--lib", "--tests"),
            )
            packaging_call = next(
                call
                for call in fake.calls
                if call["argv"][:3] == ("bash", "mobile/scripts/build-rust-mobile.sh", "all")
            )
            self.assertIn("all", packaging_call["argv"])
            phase2_ui_call = next(
                call
                for call in fake.calls
                if call["argv"][:2] == ("xcodebuild", "-project")
                and "-only-testing:RundaleUITests/RundalePhase2UITests" in call["argv"]
            )
            self.assertIn(
                "platform=iOS Simulator,id=11111111-1111-1111-1111-111111111111",
                phase2_ui_call["argv"],
            )

    def test_all_runs_rust_packaging_before_xcodegen_once(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()

            report = VerificationRun(root, command_runner=fake).run()

            self.assertEqual(report["exit_code"], 0)
            packaging_calls = [
                call
                for call in fake.calls
                if call["argv"][:3] == ("bash", "mobile/scripts/build-rust-mobile.sh", "all")
            ]
            self.assertEqual(len(packaging_calls), 1)
            packaging_index = fake.calls.index(packaging_calls[0])
            xcodegen_index = next(
                index
                for index, call in enumerate(fake.calls)
                if call["argv"][:2] == ("xcodegen", "generate")
            )
            self.assertLess(packaging_index, xcodegen_index)

    def test_phase2_rust_gate_rejects_missing_test_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()
            fake.rust_output = "Finished test profile [unoptimized + debuginfo] target(s)\n"
            report = VerificationRun(root, command_runner=fake).run(2)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["limerick-core-mobile-tests"]["status"], "failed")
            self.assertIn(
                "did not report test-result counts", by_id["limerick-core-mobile-tests"]["reason"]
            )

    def test_rust_gate_passes_named_ignored_test_and_skips_any_other(self):
        measurement = (
            "test measure_candidate_capture_cost_on_rundale ... ignored, measurement\n"
            "test result: ok. 8 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n"
        )
        for extra, expected in (
            ("", "passed"),
            ("test newly_ignored ... ignored\n", "skipped"),
        ):
            with self.subTest(extra=extra), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "mobile").mkdir()
                create_phase2_fixture(root)
                fake = FakeRunner()
                fake.rust_output = extra + measurement
                if extra:
                    fake.rust_output = fake.rust_output.replace("1 ignored", "2 ignored")
                report = VerificationRun(root, command_runner=fake, use_cache=False).run(2)

                by_id = {suite["id"]: suite for suite in report["suites"]}
                core = by_id["limerick-core-mobile-tests"]
                self.assertEqual(core["status"], expected)
                self.assertEqual(
                    core["details"]["expected_ignored"],
                    ["measure_candidate_capture_cost_on_rundale"],
                )
                if extra:
                    self.assertIn("newly_ignored", core["reason"])
                # Gates that name no expected ignore treat any ignore as a skip.
                self.assertEqual(by_id["limerick-mobile-ffi-tests"]["status"], "skipped")

    def test_phase2_dependency_gate_rejects_forbidden_crate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()
            fake.cargo_tree_output = "limerick-mobile-ffi v0.1.0\nlimerick-engine v0.1.0\n"
            report = VerificationRun(root, command_runner=fake).run(2)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["mobile-dependency-graph"]["status"], "failed")
            self.assertIn("limerick-engine", by_id["mobile-dependency-graph"]["reason"])

    def test_phase2_dependency_gate_rejects_desktop_features_but_allows_portable_diagnostics(self):
        cases = {
            # limerick-diagnostics without its features is portable (#2022).
            "limerick-mobile-ffi v0.1.0\n"
            "├── limerick-diagnostics v0.1.0\n"
            '├── limerick-core feature "mobile"\n': [],
            "limerick-mobile-ffi v0.1.0\n"
            '├── limerick-diagnostics feature "default"\n'
            '│   └── limerick-diagnostics feature "github"\n': [
                "limerick-diagnostics/default",
                "limerick-diagnostics/github",
            ],
            "limerick-mobile-ffi v0.1.0\n"
            '├── limerick-core feature "desktop"\n'
            '│   └── limerick-inference feature "desktop"\n'
            "│   └── limerick-chronicle v0.1.0\n": [
                "limerick-chronicle",
                "limerick-core/desktop",
                "limerick-inference/desktop",
            ],
        }
        for tree, forbidden in cases.items():
            with self.subTest(forbidden=forbidden), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "mobile").mkdir()
                create_phase2_fixture(root)
                fake = FakeRunner()
                fake.cargo_tree_output = tree
                report = VerificationRun(root, command_runner=fake).run(2)

                graph = {suite["id"]: suite for suite in report["suites"]}[
                    "mobile-dependency-graph"
                ]
                if forbidden:
                    self.assertEqual(graph["status"], "failed")
                    self.assertEqual(graph["details"]["forbidden_found"], forbidden)
                else:
                    self.assertEqual(graph["status"], "passed", graph.get("reason"))
                tree_command = next(call["argv"] for call in fake.calls if "tree" in call["argv"])
                self.assertIn("normal,features", tree_command)

    def test_phase2_missing_packaging_script_is_blocking(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            (root / "mobile" / "scripts" / "build-rust-mobile.sh").unlink()
            report = VerificationRun(root, command_runner=FakeRunner()).run(2)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["mobile-rust-packaging"]["status"], "unavailable")
            self.assertTrue(by_id["mobile-rust-packaging"]["blocking"])

    def test_empty_simulator_inventory_is_unavailable_and_blocks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            fake.simctl_output = json.dumps({"devices": {}})
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["simulator-selection"]["status"], "unavailable")
            self.assertEqual(by_id["ios-simulator-tests"]["status"], "skipped")
            self.assertTrue(by_id["ios-simulator-tests"]["blocking"])

    def test_zero_executed_xcresult_cannot_pass_xcodebuild(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            fake.xcresult_output = json.dumps(
                {
                    "result": "Passed",
                    "totalTestCount": 0,
                    "passedTests": 0,
                    "failedTests": 0,
                    "skippedTests": 0,
                }
            )
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["ios-simulator-tests"]["status"], "unavailable")
            self.assertEqual(by_id["ios-simulator-test-results"]["status"], "unavailable")
            self.assertIn("no executed tests", by_id["ios-simulator-tests"]["reason"])

    def test_unreconciled_xcresult_counts_cannot_pass_xcodebuild(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            fake.xcresult_output = json.dumps(
                {
                    "result": "Passed",
                    "totalTestCount": 8,
                    "passedTests": 0,
                    "failedTests": 0,
                    "skippedTests": 0,
                }
            )
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["ios-simulator-tests"]["status"], "failed")
            self.assertEqual(by_id["ios-simulator-test-results"]["status"], "failed")
            self.assertIn("do not reconcile", by_id["ios-simulator-tests"]["reason"])

    def test_expected_failures_reconcile_and_are_named_in_a_passing_result(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            fake.xcresult_output = json.dumps(
                {
                    "result": "Passed",
                    "totalTestCount": 8,
                    "passedTests": 7,
                    "failedTests": 0,
                    "skippedTests": 0,
                    "expectedFailures": 1,
                }
            )
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            suite = by_id["ios-simulator-tests"]
            self.assertEqual(suite["status"], "passed")
            self.assertEqual(suite["details"]["expectedFailures"], 1)
            self.assertIn("1 expected failure", suite["reason"])

    def test_successful_swift_command_without_executed_count_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase1_fixture(root)
            fake = FakeRunner()
            fake.swift_output = "Build complete! (0.01s)\n"
            report = VerificationRun(root, command_runner=fake).run(1)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["swift-package-tests"]["status"], "failed")
            self.assertIn(
                "did not report executed XCTest counts", by_id["swift-package-tests"]["reason"]
            )


class GitBackedRunner(FakeRunner):
    """Fake every tool except git, which runs for real in the fixture repository."""

    def run(self, argv, *, cwd, env=None, timeout_seconds=None):
        if argv and argv[0] == "git":
            self.calls.append({"argv": tuple(argv), "cwd": Path(cwd), "env": dict(env or {})})
            completed = subprocess.run(
                list(argv), cwd=cwd, env=env, capture_output=True, text=True, check=False
            )
            return CommandResult(
                completed.returncode, stdout=completed.stdout, stderr=completed.stderr
            )
        return super().run(argv, cwd=cwd, env=env, timeout_seconds=timeout_seconds)


def create_cached_repository(root: Path) -> None:
    (root / "mobile").mkdir()
    create_phase2_fixture(root)
    (root / ".gitignore").write_text(
        "/mobile/.verification/\n/mobile/Rundale.xcodeproj/\n"
        "/mobile/Rundale/Resources/GoogleService-Info.plist\n",
        encoding="utf-8",
    )
    (root / "docs").mkdir()
    (root / "docs" / "notes.md").write_text("first\n", encoding="utf-8")
    git = ["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid"]
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "add", "-A"], cwd=root, check=True)
    subprocess.run([*git, "commit", "-q", "-m", "fixture"], cwd=root, check=True)


def executed_suites(fake: FakeRunner) -> list[tuple[str, ...]]:
    return [
        call["argv"]
        for call in fake.calls
        if call["argv"][:5] == ("rustup", "run", verify_module.RUST_TOOLCHAIN, "cargo", "test")
        or call["argv"][:2] == ("swift", "test")
        or (call["argv"][0] == "xcodebuild" and call["argv"][-1] in {"test", "build"})
    ]


OLDER, NEWER = "20260101T000000000000Z", "20260102T000000000000Z"


def stamped_products(report: Path, stamp: str) -> list[Path]:
    paths = [
        report / "DerivedData" / f"simulator-{stamp}" / "Build",
        report / "xcresults" / f"simulator-{stamp}.xcresult",
        report / "swift-scratch" / stamp,
    ]
    for path in paths:
        path.mkdir(parents=True)
    (report / "device").mkdir(exist_ok=True)
    (report / "device" / f"devices-{stamp}.json").write_text("{}\n", encoding="utf-8")
    return [*paths, report / "device" / f"devices-{stamp}.json"]


def run_in_subprocess(command: str) -> tuple[subprocess.Popen, int]:
    """Start a real runner on `command` and return it with its grandchild's pid."""

    directory = tempfile.mkdtemp()
    pidfile = Path(directory) / "grandchild.pid"
    script = (
        "import signal, sys\n"
        f"sys.path.insert(0, {str(Path(verify_module.__file__).parent)!r})\n"
        "import verify\n"
        "signal.signal(signal.SIGTERM, verify._raise_interrupt)\n"
        "verify.SubprocessCommandRunner().run(\n"
        f"    ['bash', '-c', {command.replace('PIDFILE', str(pidfile))!r}], cwd='.')\n"
    )
    runner = subprocess.Popen([sys.executable, "-c", script])
    deadline = time.monotonic() + 10
    while not (pidfile.exists() and pidfile.read_text().strip()):
        if time.monotonic() > deadline or runner.poll() is not None:
            raise AssertionError("grandchild never started")
        time.sleep(0.05)
    return runner, int(pidfile.read_text())


def process_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


class PruneTests(unittest.TestCase):
    def test_a_run_deletes_earlier_runs_products_but_keeps_the_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            report = root / "mobile" / ".verification"
            earlier = stamped_products(report, OLDER) + stamped_products(report, NEWER)
            kept = [report / "cache", report / "passes", report / "logs"]
            for path in kept:
                path.mkdir(parents=True)
            VerificationRun(root, command_runner=FakeRunner()).run(1)
            for path in earlier:
                self.assertFalse(path.exists(), path)
            for path in kept:
                self.assertTrue(path.is_dir(), path)

    def test_a_killed_runs_live_build_survives_any_number_of_later_runs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            report = root / "mobile" / ".verification"
            orphaned = stamped_products(report, OLDER)
            finished = stamped_products(report, NEWER)
            # A real runner starts a builder whose arguments carry its run's
            # timestamp, as xcodebuild's -derivedDataPath does, then is killed
            # outright, leaving the builder running.
            pidfile = Path(directory) / "builder.pid"
            builder = Path(directory) / "builder.py"
            builder.write_text(
                "import os, sys, time\n"
                f"open({str(pidfile)!r}, 'w').write(str(os.getpid()))\n"
                "time.sleep(60)\n",
                encoding="utf-8",
            )
            script = (
                "import sys\n"
                f"sys.path.insert(0, {str(Path(verify_module.__file__).parent)!r})\n"
                "import verify\n"
                "verify.SubprocessCommandRunner().run(\n"
                f"    [sys.executable, {str(builder)!r}, 'simulator-{OLDER}'], cwd='.')\n"
            )
            runner = subprocess.Popen([sys.executable, "-c", script])
            deadline = time.monotonic() + 10
            while not (pidfile.exists() and pidfile.read_text().strip()):
                self.assertLess(time.monotonic(), deadline, "builder never started")
                time.sleep(0.05)
            runner.kill()
            runner.wait()
            builder_pid = int(pidfile.read_text())
            self.assertTrue(process_alive(builder_pid), "the builder should outlive SIGKILL")
            try:
                for _ in range(3):  # B, C and D, each pruning
                    VerificationRun(root, command_runner=FakeRunner()).run(1)
                    self.assertTrue(all(path.exists() for path in orphaned))
                self.assertFalse(any(path.exists() for path in finished))
            finally:
                with contextlib.suppress(ProcessLookupError):
                    os.kill(builder_pid, signal.SIGKILL)
                deadline = time.monotonic() + 5
                while process_alive(builder_pid) and time.monotonic() < deadline:
                    time.sleep(0.05)
            VerificationRun(root, command_runner=FakeRunner()).run(1)
            self.assertFalse(any(path.exists() for path in orphaned))

    def test_a_run_refuses_a_report_directory_another_run_holds(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_phase2_fixture(root)
            report = root / "mobile" / ".verification"
            older = stamped_products(report, OLDER)
            stamped_products(report, NEWER)
            with (report / ".run.lock").open("w") as held:
                verify_module.fcntl.flock(held, verify_module.fcntl.LOCK_EX)
                with self.assertRaises(verify_module.ReportDirectoryBusy):
                    VerificationRun(root, command_runner=FakeRunner()).run(fast=True)
                # The active run's build products survive the refused run.
                self.assertTrue(all(path.exists() for path in older))
            VerificationRun(root, command_runner=FakeRunner()).run(1)
            self.assertFalse(any(path.exists() for path in older))

    def test_full_runs_in_different_worktrees_take_turns_on_the_host_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "first", Path(directory) / "second"
            for root in (first, second):
                root.mkdir()
                create_phase2_fixture(root)
            host_lock = Path(directory) / "host" / "mobile-verify.lock"
            host_lock.parent.mkdir()
            # A run in another worktree (here, its own process) is already going.
            holder = subprocess.Popen(
                [
                    sys.executable,
                    "-c",
                    "import fcntl, sys, time\n"
                    f"lock = open({str(host_lock)!r}, 'w')\n"
                    "fcntl.flock(lock, fcntl.LOCK_EX)\n"
                    "print('locked', flush=True)\n"
                    "time.sleep(1.5)\n",
                ],
                stdout=subprocess.PIPE,
                text=True,
            )
            try:
                self.assertEqual(holder.stdout.readline().strip(), "locked")

                def run(root: Path, wait: float, **kwargs: object) -> dict:
                    return VerificationRun(
                        root,
                        command_runner=FakeRunner(),
                        host_lock=host_lock,
                        host_lock_wait_seconds=wait,
                    ).run(**kwargs)  # type: ignore[arg-type]

                with self.assertRaises(verify_module.HostVerificationBusy):
                    run(second, 0.2, phase=1)
                # The Swift-only lane uses no simulator and does not wait.
                run(second, 0.2, fast=True)
                started = time.monotonic()
                report = run(first, 30, phase=1)  # waits for the other run, then runs
                self.assertGreater(time.monotonic() - started, 0.5)
                self.assertIn(report["status"], (verify_module.PASSED, verify_module.FAILED))
            finally:
                holder.kill()
                holder.wait()

    def test_runs_use_the_host_lock_named_by_the_environment(self):
        with patch.dict(os.environ, {"RUNDALE_MOBILE_VERIFY_LOCK": "/tmp/x/lock"}):
            self.assertEqual(verify_module.default_host_lock(), Path("/tmp/x/lock"))
        with patch.dict(os.environ, {"RUNDALE_MOBILE_VERIFY_LOCK": ""}):
            self.assertEqual(verify_module.default_host_lock().name, "mobile-verify.lock")

    def test_sigterm_stops_the_running_commands_whole_process_group(self):
        runner, grandchild = run_in_subprocess("sleep 60 & echo $! > PIDFILE; wait")
        runner.send_signal(signal.SIGTERM)
        runner.wait(timeout=15)
        self.assertFalse(process_alive(grandchild))

    def test_a_member_that_ignores_sigterm_is_killed_before_the_runner_exits(self):
        runner, grandchild = run_in_subprocess(
            "trap '' TERM; sleep 60 >/dev/null 2>&1 & echo $! > PIDFILE; wait"
        )
        try:
            runner.send_signal(signal.SIGTERM)
            runner.wait(timeout=20)
            self.assertFalse(process_alive(grandchild))
        finally:
            with contextlib.suppress(ProcessLookupError):
                os.kill(grandchild, signal.SIGKILL)


class StopGroupTests(unittest.TestCase):
    def test_a_timed_out_command_keeps_what_it_wrote_while_being_stopped(self):
        # The trap writes on SIGTERM; the background sleep ignores SIGTERM and
        # keeps the pipe open, so the first wait times out and the group is
        # killed before its output is read.
        started = time.monotonic()
        result = verify_module.SubprocessCommandRunner().run(
            ["bash", "-c", "trap 'echo stopping' TERM; (trap '' TERM; exec sleep 60) & wait; wait"],
            cwd=Path("."),
            timeout_seconds=0.5,
        )
        self.assertTrue(result.timed_out)
        self.assertGreater(time.monotonic() - started, 5, "cleanup never escalated")
        self.assertIn("stopping", result.stdout)

    def test_a_timeout_stops_descendants_whose_parent_already_exited(self):
        result = verify_module.SubprocessCommandRunner().run(
            ["bash", "-c", "echo $$; sleep 60 & exit 0"],
            cwd=Path("."),
            timeout_seconds=0.5,
        )
        self.assertTrue(result.timed_out)
        group = int(result.stdout.split()[0])
        self.assertFalse(verify_module._group_alive(group))


class XcresultTreeTests(unittest.TestCase):
    def test_parses_a_tree_xcresulttool_wrote(self):
        # Trimmed from `xcresulttool get test-results tests` on a real combined run.
        fixture = Path(__file__).with_name("fixtures") / "xcresult-tests.json"
        cases = verify_module._xcresult_cases(json.loads(fixture.read_text(encoding="utf-8")))
        self.assertEqual(len(cases), 8)
        self.assertIn(
            (
                "RundaleUITests",
                "RundalePhase1AuditUITests",
                "testAcceptedCommandRowIsExactAndImmediatelyVisible()",
                "Passed",
            ),
            cases,
        )
        selected = [
            case
            for case in cases
            if verify_module._selected(
                case, ["RundaleUITests/RundaleUITests", "RundaleTests/Phase1AuditVolumeTests"]
            )
        ]
        self.assertEqual(len(selected), 4)
        self.assertEqual(
            len([case for case in cases if verify_module._selected(case, ["RundaleTests"])]), 4
        )
        self.assertEqual(
            verify_module._case_counts(["Passed", "Expected Failure", "Skipped", "Mixed"]),
            {
                "totalTestCount": 4,
                "passedTests": 1,
                "failedTests": 1,
                "skippedTests": 1,
                "expectedFailures": 1,
            },
        )

    def test_failures_keep_every_message_under_their_case(self):
        case = {
            "nodeType": "Test Case",
            "name": "testSlow()",
            "result": "Failed",
            "children": [
                {
                    "nodeType": "Device",
                    "name": "iPhone 17",
                    "children": [
                        {"nodeType": "Failure Message", "name": "A.swift:3: first"},
                        {"nodeType": "Failure Message", "name": "A.swift:9: second"},
                    ],
                },
                {"nodeType": "Attachment", "name": "Screenshot"},
            ],
        }
        # An XCTExpectFailure case carries its messages, but is not a failure.
        expected = {
            "nodeType": "Test Case",
            "name": "testKnownDefect()",
            "result": "Expected Failure",
            "children": [{"nodeType": "Failure Message", "name": "B.swift:5: known"}],
        }
        tree = {
            "testNodes": [
                {
                    "nodeType": "Test Plan",
                    "name": "Rundale",
                    "children": [
                        {
                            "nodeType": "UI test bundle",
                            "name": "RundaleUITests",
                            "children": [
                                {
                                    "nodeType": "Test Suite",
                                    "name": "AUITests",
                                    "children": [case, expected],
                                },
                            ],
                        },
                    ],
                }
            ]
        }
        self.assertEqual(
            verify_module._xcresult_failures(tree),
            [
                ("RundaleUITests", "AUITests", "testSlow()", "A.swift:3: first"),
                ("RundaleUITests", "AUITests", "testSlow()", "A.swift:9: second"),
            ],
        )


class VerificationCacheTests(unittest.TestCase):
    def run_phase(self, root: Path, phase: int = 2, **options):
        fake = GitBackedRunner()
        report = VerificationRun(root, command_runner=fake, **options).run(phase)
        return fake, report

    def test_identical_inputs_reuse_every_passing_suite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            first, first_report = self.run_phase(root)
            self.assertEqual(first_report["exit_code"], 0)
            self.assertEqual(first_report["summary"]["reused"], 0)
            self.assertEqual(len(executed_suites(first)), 8)

            second, report = self.run_phase(root)
            self.assertEqual(report["exit_code"], 0)
            self.assertEqual(executed_suites(second), [])
            self.assertEqual(report["summary"]["reused"], 9)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            reused = by_id["phase2-ios-simulator-tests"]["details"]["cache"]
            self.assertTrue(reused["reused"])
            self.assertEqual(reused["started_at"], first_report["started_at"])
            self.assertEqual(by_id["phase2-ios-simulator-test-results"]["status"], "passed")
            self.assertEqual(by_id["phase2-ios-simulator-tests"]["details"]["passedTests"], 8)
            summary = (root / "mobile" / ".verification" / "summary.txt").read_text()
            self.assertIn("9 passed suite(s) reused", summary)

    def test_full_run_reuses_combined_simulator_passes_per_suite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            first, first_report = self.run_phase(root, 4)
            self.assertEqual(first_report["exit_code"], 0)
            tests = [
                c
                for c in first.calls
                if c["argv"][:1] == ("xcodebuild",) and c["argv"][-1] == "test"
            ]
            self.assertEqual(len(tests), 1)

            second, report = self.run_phase(root, 4)
            self.assertEqual(report["exit_code"], 0)
            self.assertFalse(
                [
                    c
                    for c in second.calls
                    if c["argv"][:1] == ("xcodebuild",) and c["argv"][-1] == "test"
                ]
            )
            by_id = {suite["id"]: suite for suite in report["suites"]}
            for identifier in ("ios-simulator-tests", "phase4-ios-controller-tests"):
                self.assertTrue(by_id[identifier]["details"]["cache"]["reused"], identifier)
            self.assertNotIn("ios-simulator-combined-run", by_id)

    def test_reused_package_pass_still_checks_coverage_against_the_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            self.run_phase(root)
            # Raising the floor after the pass was cached must still fail.
            baseline = root / "mobile" / "coverage-baseline.json"
            payload = json.loads(baseline.read_text())
            payload["packages"]["RundaleBridge"]["line_percent"] = 95.0
            baseline.write_text(json.dumps(payload))
            subprocess.run(["git", "add", "-A"], cwd=root, check=True)
            _, report = self.run_phase(root)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["swift-endpoint-kit-tests-coverage"]["status"], "passed")
            self.assertEqual(by_id["swift-bridge-tests-coverage"]["status"], "failed")
            self.assertTrue(by_id["swift-bridge-tests"]["details"]["cache"]["reused"])

    def test_cumulative_phase_reuses_prior_phase_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            self.run_phase(root, 2)
            self.run_phase(root, 3)
            fake, report = self.run_phase(root, 4)
            self.assertEqual(report["exit_code"], 0)
            ran = executed_suites(fake)
            # Phase 1's simulator suite and Phase 4's own suites are new.
            self.assertTrue(any("RundalePhase4UITests" in " ".join(argv) for argv in ran))
            self.assertFalse(any("RundalePhase2UITests" in " ".join(argv) for argv in ran))
            self.assertFalse(any("RundalePhase3UITests" in " ".join(argv) for argv in ran))

    def test_source_changes_invalidate_but_documentation_and_commits_do_not(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            self.run_phase(root)

            (root / "docs" / "notes.md").write_text("edited\n", encoding="utf-8")
            (root / "mobile" / "README.md").write_text("new\n", encoding="utf-8")
            fake, _ = self.run_phase(root)
            self.assertEqual(executed_suites(fake), [], "documentation-only edits keep passes")

            git = ["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid"]
            subprocess.run(["git", "add", "-A"], cwd=root, check=True)
            subprocess.run([*git, "commit", "-q", "-m", "docs"], cwd=root, check=True)
            fake, _ = self.run_phase(root)
            self.assertEqual(executed_suites(fake), [], "committing identical content keeps passes")

            tracked = root / "mobile" / "RundaleUITests" / "RundalePhase2UITests.swift"
            tracked.write_text("// changed\n", encoding="utf-8")
            fake, _ = self.run_phase(root)
            self.assertEqual(len(executed_suites(fake)), 8, "tracked edit reruns every suite")

            (root / "mobile" / "Untracked.swift").write_text("// new\n", encoding="utf-8")
            fake, _ = self.run_phase(root)
            self.assertEqual(len(executed_suites(fake)), 8, "untracked source reruns every suite")

    def test_private_firebase_configuration_and_toolchain_are_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            self.run_phase(root)
            plist = root / "mobile" / "Rundale" / "Resources" / "GoogleService-Info.plist"
            plist.parent.mkdir(parents=True)
            plist.write_bytes(b"private")
            fake, _ = self.run_phase(root)
            self.assertEqual(len(executed_suites(fake)), 8)

            fake = GitBackedRunner()
            fake.results[("xcodebuild", "-version")] = CommandResult(0, stdout="Xcode 99.0\n")
            VerificationRun(root, command_runner=fake).run(2)
            self.assertEqual(len(executed_suites(fake)), 8)

    def test_failures_are_never_reused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            fake = GitBackedRunner()
            fake.fail_swift = True
            report = VerificationRun(root, command_runner=fake).run(2)
            self.assertEqual(report["exit_code"], 1)
            fake, _ = self.run_phase(root)
            self.assertEqual(
                sum(argv[:2] == ("swift", "test") for argv in executed_suites(fake)), 3
            )

    def test_no_cache_reruns_everything(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            self.run_phase(root)
            fake, report = self.run_phase(root, use_cache=False)
            self.assertEqual(len(executed_suites(fake)), 8)
            self.assertEqual(report["summary"]["reused"], 0)
            self.assertFalse(report["cache"]["enabled"])

    def test_physical_device_suites_always_run(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            device = "00008140-001E30603C10801C"
            self.run_phase(root, 1, device=device)
            fake, _ = self.run_phase(root, 1, device=device)
            physical = [
                argv for argv in executed_suites(fake) if f"platform=iOS,id={device}" in argv
            ]
            self.assertEqual(len(physical), 1)

    def test_reuse_is_disabled_outside_a_git_worktree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            GitBackedRunner().run(["git", "--version"], cwd=root)
            _, report = self.run_phase(root)
            _, report = self.run_phase(root)
            self.assertEqual(report["summary"]["reused"], 0)
            self.assertIn("disabled_reason", report["cache"])


class SplitUiClassTests(unittest.TestCase):
    """Each phase's UI tests are split over classes so parallel workers share them."""

    def test_every_phase_class_is_selected_and_base_has_no_tests(self):
        import re

        ui_dir = Path(__file__).resolve().parents[1] / "RundaleUITests"
        for phase, selected in (
            (2, verify_module.PHASE2_UI_CLASSES),
            (3, verify_module.PHASE3_UI_CLASSES),
            (4, verify_module.PHASE4_UI_CLASSES),
        ):
            text = (ui_dir / f"RundalePhase{phase}UITests.swift").read_text(encoding="utf-8")
            base = f"RundalePhase{phase}UITestCase"
            classes = re.findall(r"^(?:final )?class (\w+): (\w+) \{", text, re.MULTILINE)
            subclasses = {name for name, parent in classes if parent == base}
            self.assertGreater(len(subclasses), 1, phase)
            self.assertEqual(
                {f"RundaleUITests/{name}" for name in subclasses}, set(selected), phase
            )
            self.assertIn(f"RundaleUITests/RundalePhase{phase}UITests", selected)
            base_body = text.split(f"class {base}: XCTestCase {{", 1)[1]
            base_body = base_body.split("\nfinal class ", 1)[0]
            self.assertNotIn("func test", base_body, phase)


if __name__ == "__main__":
    unittest.main()
