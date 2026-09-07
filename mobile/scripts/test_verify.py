#!/usr/bin/env python3
"""Regression tests for the mobile verification orchestrator."""

from __future__ import annotations

import importlib
import json
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import TYPE_CHECKING

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
        self.swift_output = "Executed 8 tests, with 0 failures (0 unexpected) in 0.01 seconds\n"
        self.rust_output = (
            "test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        )
        self.cargo_tree_output = "parish-mobile-ffi v0.1.0\n"
        self.simctl_output = SIMCTL_JSON
        self.xcresult_output = XCRESULT_JSON

    def run(self, argv, *, cwd, env=None, timeout_seconds=None):
        command = tuple(str(part) for part in argv)
        self.calls.append({"argv": command, "cwd": Path(cwd)})
        result = self.results.get(command)
        if callable(result):
            return result(command)
        if result is not None:
            return result
        if command[:5] == ("xcrun", "simctl", "list", "devices", "available"):
            return CommandResult(0, stdout=self.simctl_output)
        if command[:5] == ("xcrun", "xcresulttool", "get", "test-results", "summary"):
            return CommandResult(0, stdout=self.xcresult_output)
        if command[:5] == ("rustup", "run", "1.98.0", "cargo", "test"):
            return CommandResult(0, stdout=self.rust_output)
        if command[:5] == ("rustup", "run", "1.98.0", "cargo", "tree"):
            return CommandResult(0, stdout=self.cargo_tree_output)
        if self.fail_swift and command[:2] == ("swift", "test"):
            return CommandResult(7, stderr="swift tests failed")
        if command[:2] == ("swift", "test"):
            return CommandResult(0, stdout=self.swift_output)
        return CommandResult(0, stdout="ok\n")


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
    endpoint_kit = root / "mobile" / "ParishEndpointKit"
    (endpoint_kit / "Tests" / "ParishEndpointKitTests").mkdir(parents=True)
    (endpoint_kit / "Package.swift").write_text("// fixture\n", encoding="utf-8")
    (endpoint_kit / "Tests" / "ParishEndpointKitTests" / "FixtureTests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    packaging = root / "mobile" / "scripts" / "build-rust-mobile.sh"
    packaging.parent.mkdir(parents=True, exist_ok=True)
    packaging.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")


class VerificationRunnerTests(unittest.TestCase):
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
            self.assertTrue(
                (root / "mobile" / ".verification" / "logs" / "ios-simulator-tests.log").is_file()
            )

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
            self.assertEqual([suite["phase"] for suite in future], [3, 4, 5, 6])
            self.assertTrue(all(suite["status"] == "unavailable" for suite in future))
            self.assertTrue(all(not suite["blocking"] for suite in future))
            self.assertEqual(report["implemented_phases"], [1, 2])
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

    def test_explicit_unimplemented_phase_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            report = VerificationRun(root, command_runner=FakeRunner()).run(3)

            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(report["suites"][0]["id"], "phase-3-verification")
            self.assertEqual(report["suites"][0]["status"], "unavailable")
            self.assertTrue(report["suites"][0]["blocking"])

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
                "parish-core-mobile-tests",
                "parish-persistence-mobile-tests",
                "parish-mobile-ffi-tests",
                "mobile-dependency-graph",
                "mobile-rust-packaging",
                "swift-package-tests-phase2",
                "parish-mobile-endpoint-contract",
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
                if call["argv"][:5] == ("rustup", "run", "1.98.0", "cargo", "test")
            ]
            self.assertEqual(len(rust_test_calls), 4)
            for call in rust_test_calls:
                index = call["argv"].index("--manifest-path")
                self.assertEqual(call["argv"][index + 1], "parish/Cargo.toml")
            self.assertTrue(
                any("mobile_endpoint_fixture" in call["argv"] for call in rust_test_calls)
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
            self.assertEqual(by_id["parish-core-mobile-tests"]["status"], "failed")
            self.assertIn(
                "did not report test-result counts", by_id["parish-core-mobile-tests"]["reason"]
            )

    def test_phase2_dependency_gate_rejects_forbidden_crate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mobile").mkdir()
            create_phase2_fixture(root)
            fake = FakeRunner()
            fake.cargo_tree_output = "parish-mobile-ffi v0.1.0\nparish-engine v0.1.0\n"
            report = VerificationRun(root, command_runner=fake).run(2)

            by_id = {suite["id"]: suite for suite in report["suites"]}
            self.assertEqual(report["exit_code"], 1)
            self.assertEqual(by_id["mobile-dependency-graph"]["status"], "failed")
            self.assertIn("parish-engine", by_id["mobile-dependency-graph"]["reason"])

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


if __name__ == "__main__":
    unittest.main()
