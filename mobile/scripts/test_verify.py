#!/usr/bin/env python3
"""Regression tests for the mobile verification orchestrator."""

from __future__ import annotations

import importlib
import json
import os
import plistlib
import subprocess
import tempfile
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
        self.cargo_tree_output = "limerick-mobile-ffi v0.1.0\n"
        self.simctl_output = SIMCTL_JSON
        self.xcresult_output = XCRESULT_JSON
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
        result = self.results.get(command)
        if callable(result):
            return result(command)
        if result is not None:
            return result
        if command[:5] == ("xcrun", "simctl", "list", "devices", "available"):
            return CommandResult(0, stdout=self.simctl_output)
        if command[:5] == ("xcrun", "xcresulttool", "get", "test-results", "summary"):
            return CommandResult(0, stdout=self.xcresult_output)
        if command[:5] == ("xcrun", "devicectl", "list", "devices", "--json-output"):
            return CommandResult(0, stdout=self.device_output)
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
    (root / "mobile" / "RundaleUITests" / "RundalePhase3UITests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    (root / "mobile" / "RundaleUITests" / "RundalePhase4UITests.swift").write_text(
        "// fixture\n", encoding="utf-8"
    )
    (root / "mobile" / "RundaleTests").mkdir()
    (root / "mobile" / "RundaleTests" / "RundalePhase4Tests.swift").write_text("// fixture\n")
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
            self.assertEqual([suite["phase"] for suite in future], [5, 6])
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
            # All simulator phases share products within a run, not separate
            # dependency recompilations for every test class.
            self.assertEqual(len({b[b.index("-derivedDataPath") + 1] for b in builds}), 1)
            self.assertEqual(len(builds), 5)

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
                "limerick-persistence-mobile-tests",
                "limerick-mobile-ffi-tests",
                "mobile-dependency-graph",
                "mobile-rust-packaging",
                "swift-package-tests-phase2",
                "limerick-mobile-endpoint-contract",
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
                self.assertEqual(call["argv"][index + 1], "limerick/Cargo.toml")
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
            self.assertEqual(by_id["limerick-core-mobile-tests"]["status"], "failed")
            self.assertIn(
                "did not report test-result counts", by_id["limerick-core-mobile-tests"]["reason"]
            )

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
        if call["argv"][:5] == ("rustup", "run", "1.98.0", "cargo", "test")
        or call["argv"][:2] == ("swift", "test")
        or (call["argv"][0] == "xcodebuild" and call["argv"][-1] in {"test", "build"})
    ]


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
            self.assertEqual(len(executed_suites(first)), 9)

            second, report = self.run_phase(root)
            self.assertEqual(report["exit_code"], 0)
            self.assertEqual(executed_suites(second), [])
            self.assertEqual(report["summary"]["reused"], 10)
            by_id = {suite["id"]: suite for suite in report["suites"]}
            reused = by_id["phase2-ios-simulator-tests"]["details"]["cache"]
            self.assertTrue(reused["reused"])
            self.assertEqual(reused["started_at"], first_report["started_at"])
            self.assertEqual(by_id["phase2-ios-simulator-test-results"]["status"], "passed")
            self.assertEqual(by_id["phase2-ios-simulator-tests"]["details"]["passedTests"], 8)
            summary = (root / "mobile" / ".verification" / "summary.txt").read_text()
            self.assertIn("10 passed suite(s) reused", summary)

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
            self.assertEqual(len(executed_suites(fake)), 9, "tracked edit reruns every suite")

            (root / "mobile" / "Untracked.swift").write_text("// new\n", encoding="utf-8")
            fake, _ = self.run_phase(root)
            self.assertEqual(len(executed_suites(fake)), 9, "untracked source reruns every suite")

    def test_private_firebase_configuration_and_toolchain_are_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            create_cached_repository(root)
            self.run_phase(root)
            plist = root / "mobile" / "Rundale" / "Resources" / "GoogleService-Info.plist"
            plist.parent.mkdir(parents=True)
            plist.write_bytes(b"private")
            fake, _ = self.run_phase(root)
            self.assertEqual(len(executed_suites(fake)), 9)

            fake = GitBackedRunner()
            fake.results[("xcodebuild", "-version")] = CommandResult(0, stdout="Xcode 99.0\n")
            VerificationRun(root, command_runner=fake).run(2)
            self.assertEqual(len(executed_suites(fake)), 9)

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
            self.assertEqual(len(executed_suites(fake)), 9)
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


if __name__ == "__main__":
    unittest.main()
