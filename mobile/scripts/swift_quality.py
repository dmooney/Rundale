#!/usr/bin/env python3
"""Swift quality gates for the native Rundale client (#2103).

Runs lint, format check, Swift package tests, optional Xcode lanes, and
coverage collection. Every gate reports one of:

  passed | failed | skipped | unavailable

Required gates treat both ``failed`` and ``unavailable`` as blocking. A missing
Swift toolchain on Linux therefore exits non-zero rather than pretending to
pass. Opt-in live Endpoint, performance, and soak suites are never implied by
the default fast lane.

Coordinate with #2045 (phase ``./verify``) and #2046 (device/TestFlight): this
script owns style/package/CI correctness gates, not the full phase verifier or
physical-device acceptance evidence.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol

PASSED = "passed"
FAILED = "failed"
SKIPPED = "skipped"
UNAVAILABLE = "unavailable"
STATUSES = {PASSED, FAILED, SKIPPED, UNAVAILABLE}

PACKAGE_ORDER = ("RundaleKit", "LimerickEndpointKit", "RundaleBridge")
DETERMINISTIC_UI_TEST_CLASSES = (
    "RundaleUITests",
    "RundalePhase1AuditUITests",
    "RundalePhase1TimerAuditUITests",
    "RundalePhase2UITests",
    "RundalePhase3UITests",
    "RundalePhase4UITests",
)
OPT_IN_UI_TEST_CLASSES = (
    "RundaleLiveEndpointUITests",
    "RundalePerformanceUITests",
    "RundaleSoakUITests",
)


@dataclass(frozen=True)
class CommandResult:
    returncode: int | None
    stdout: str = ""
    stderr: str = ""
    duration_seconds: float = 0.0
    unavailable: bool = False

    @property
    def output(self) -> str:
        if self.stdout and self.stderr:
            return f"{self.stdout.rstrip()}\n{self.stderr.rstrip()}\n"
        return self.stdout or self.stderr


class CommandRunner(Protocol):
    def run(
        self,
        argv: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout_seconds: float | None = None,
    ) -> CommandResult: ...


class SubprocessCommandRunner:
    def run(
        self,
        argv: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout_seconds: float | None = None,
    ) -> CommandResult:
        started = time.monotonic()
        try:
            completed = subprocess.run(
                [str(part) for part in argv],
                cwd=str(cwd),
                env=dict(env) if env is not None else None,
                capture_output=True,
                text=True,
                timeout=timeout_seconds,
                check=False,
            )
        except FileNotFoundError as exc:
            return CommandResult(
                None,
                stderr=str(exc),
                duration_seconds=time.monotonic() - started,
                unavailable=True,
            )
        except PermissionError as exc:
            return CommandResult(
                None,
                stderr=str(exc),
                duration_seconds=time.monotonic() - started,
                unavailable=True,
            )
        except subprocess.TimeoutExpired as exc:
            return CommandResult(
                124,
                stdout=_text(exc.stdout),
                stderr=_text(exc.stderr) or f"timed out after {timeout_seconds:g}s",
                duration_seconds=time.monotonic() - started,
            )
        except OSError as exc:
            return CommandResult(
                None,
                stderr=str(exc),
                duration_seconds=time.monotonic() - started,
                unavailable=True,
            )
        return CommandResult(
            completed.returncode,
            stdout=completed.stdout or "",
            stderr=completed.stderr or "",
            duration_seconds=time.monotonic() - started,
        )


@dataclass
class GateResult:
    id: str
    name: str
    status: str
    required: bool
    reason: str = ""
    duration_seconds: float = 0.0
    details: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "status": self.status,
            "required": self.required,
            "reason": self.reason,
            "duration_seconds": round(self.duration_seconds, 3),
            "details": self.details,
        }


def _text(value: Any) -> str:
    if value is None:
        return ""
    return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else str(value)


def _command_text(command: Sequence[str]) -> str:
    return " ".join(shlex.quote(str(part)) for part in command)


def _failure_reason(result: CommandResult, fallback: str) -> str:
    source = result.stderr.strip() or result.stdout.strip()
    if not source:
        return fallback
    reason = " | ".join(line.strip() for line in source.splitlines() if line.strip())
    if len(reason) > 600:
        reason = "…" + reason[-597:]
    return reason


def _read_tool_versions(path: Path) -> dict[str, str]:
    versions: dict[str, str] = {}
    if not path.is_file():
        return versions
    for line in path.read_text(encoding="utf-8").splitlines():
        match = re.match(r'^([A-Za-z0-9_-]+)\s*=\s*"([^"]+)"\s*$', line.strip())
        if match:
            versions[match.group(1)] = match.group(2)
    return versions


def _which(runner: CommandRunner, name: str, *, cwd: Path) -> str | None:
    if os.name == "nt":
        result = runner.run(["where", name], cwd=cwd)
    else:
        result = runner.run(["bash", "-lc", f"command -v {shlex.quote(name)}"], cwd=cwd)
    if result.unavailable or result.returncode not in (0,):
        return None
    path = (result.stdout or "").strip().splitlines()
    return path[0] if path else None


class SwiftQualityRun:
    def __init__(
        self,
        *,
        root: Path,
        runner: CommandRunner | None = None,
        report_dir: Path | None = None,
        simulator: str | None = None,
        configuration: str = "Debug",
    ) -> None:
        self.root = root.resolve()
        self.mobile = self.root / "mobile"
        self.runner = runner or SubprocessCommandRunner()
        self.report_dir = (
            report_dir or (self.mobile / ".verification" / "swift-quality")
        ).resolve()
        self.simulator = simulator or os.environ.get("RUNDALE_IOS_SIMULATOR")
        self.configuration = configuration
        self.tool_versions = _read_tool_versions(self.mobile / "tool-versions.toml")
        self.gates: list[GateResult] = []
        self.started_at = dt.datetime.now(dt.timezone.utc)

    def run(
        self,
        *,
        lint: bool = True,
        format_check: bool = True,
        packages: bool = True,
        xcode: bool = False,
        coverage: bool = False,
        opt_in: Sequence[str] = (),
    ) -> int:
        self.report_dir.mkdir(parents=True, exist_ok=True)
        (self.report_dir / "logs").mkdir(parents=True, exist_ok=True)

        if lint:
            self._swiftlint()
        if format_check:
            self._swiftformat()
        if packages:
            for package in PACKAGE_ORDER:
                self._package_tests(package)
        if xcode:
            self._xcode_lane(coverage=coverage)
        elif coverage:
            self._record(
                "coverage-regression",
                "Swift coverage regression policy",
                UNAVAILABLE,
                required=True,
                reason=(
                    "coverage collection requires the Xcode lane "
                    "(pass --xcode, or run just swift-quality-xcode)"
                ),
            )
        for name in opt_in:
            self._opt_in_gate(name)

        return self._finish()

    def _record(
        self,
        gate_id: str,
        name: str,
        status: str,
        *,
        required: bool,
        reason: str = "",
        duration_seconds: float = 0.0,
        details: Mapping[str, Any] | None = None,
        log_text: str = "",
    ) -> None:
        if status not in STATUSES:
            raise ValueError(f"unknown status {status!r}")
        result = GateResult(
            id=gate_id,
            name=name,
            status=status,
            required=required,
            reason=reason,
            duration_seconds=duration_seconds,
            details=dict(details or {}),
        )
        self.gates.append(result)
        if log_text:
            (self.report_dir / "logs" / f"{gate_id}.log").write_text(log_text, encoding="utf-8")
        mark = status.upper()
        suffix = f" — {reason}" if reason else ""
        print(f"[{mark}] {name}{suffix}", flush=True)

    def _swiftlint(self) -> None:
        expected = self.tool_versions.get("swiftlint")
        binary = _which(self.runner, "swiftlint", cwd=self.mobile)
        if binary is None:
            self._record(
                "swiftlint",
                "SwiftLint (non-mutating)",
                UNAVAILABLE,
                required=True,
                reason="swiftlint not found; install with bash mobile/scripts/install-swift-tools.sh",
                details={"expected_version": expected},
            )
            return
        version = self.runner.run([binary, "version"], cwd=self.mobile)
        observed = (version.stdout or version.stderr or "").strip()
        argv = [
            binary,
            "lint",
            "--strict",
            "--config",
            str(self.mobile / ".swiftlint.yml"),
            "--reporter",
            "emoji",
        ]
        result = self.runner.run(argv, cwd=self.mobile, timeout_seconds=300)
        details: dict[str, Any] = {
            "command": _command_text(argv),
            "expected_version": expected,
            "observed_version": observed,
        }
        if expected and observed and observed != expected:
            details["version_drift"] = True
        if result.unavailable:
            self._record(
                "swiftlint",
                "SwiftLint (non-mutating)",
                UNAVAILABLE,
                required=True,
                reason=_failure_reason(result, "swiftlint unavailable"),
                duration_seconds=result.duration_seconds,
                details=details,
                log_text=result.output,
            )
            return
        if result.returncode == 0:
            self._record(
                "swiftlint",
                "SwiftLint (non-mutating)",
                PASSED,
                required=True,
                duration_seconds=result.duration_seconds,
                details=details,
                log_text=result.output,
            )
            return
        self._record(
            "swiftlint",
            "SwiftLint (non-mutating)",
            FAILED,
            required=True,
            reason=_failure_reason(result, "swiftlint reported violations"),
            duration_seconds=result.duration_seconds,
            details=details,
            log_text=result.output,
        )

    def _swiftformat(self) -> None:
        expected = self.tool_versions.get("swiftformat")
        binary = _which(self.runner, "swiftformat", cwd=self.mobile)
        if binary is None:
            self._record(
                "swiftformat",
                "SwiftFormat (lint / non-mutating)",
                UNAVAILABLE,
                required=True,
                reason="swiftformat not found; install with bash mobile/scripts/install-swift-tools.sh",
                details={"expected_version": expected},
            )
            return
        version = self.runner.run([binary, "--version"], cwd=self.mobile)
        observed = (version.stdout or version.stderr or "").strip()
        argv = [
            binary,
            str(self.mobile),
            "--lint",
            "--config",
            str(self.mobile / ".swiftformat"),
        ]
        result = self.runner.run(argv, cwd=self.mobile, timeout_seconds=300)
        details: dict[str, Any] = {
            "command": _command_text(argv),
            "expected_version": expected,
            "observed_version": observed,
        }
        if expected and observed and expected not in observed:
            details["version_drift"] = True
        if result.unavailable:
            self._record(
                "swiftformat",
                "SwiftFormat (lint / non-mutating)",
                UNAVAILABLE,
                required=True,
                reason=_failure_reason(result, "swiftformat unavailable"),
                duration_seconds=result.duration_seconds,
                details=details,
                log_text=result.output,
            )
            return
        if result.returncode == 0:
            self._record(
                "swiftformat",
                "SwiftFormat (lint / non-mutating)",
                PASSED,
                required=True,
                duration_seconds=result.duration_seconds,
                details=details,
                log_text=result.output,
            )
            return
        self._record(
            "swiftformat",
            "SwiftFormat (lint / non-mutating)",
            FAILED,
            required=True,
            reason=_failure_reason(result, "swiftformat found formatting drift"),
            duration_seconds=result.duration_seconds,
            details=details,
            log_text=result.output,
        )

    def _package_tests(self, package: str) -> None:
        package_path = self.mobile / package
        gate_id = f"package-tests:{package}"
        name = f"{package} Swift package tests"
        if not (package_path / "Package.swift").is_file():
            self._record(
                gate_id,
                name,
                FAILED,
                required=True,
                reason=f"missing Package.swift under mobile/{package}",
            )
            return
        swift = _which(self.runner, "swift", cwd=package_path)
        if swift is None:
            self._record(
                gate_id,
                name,
                UNAVAILABLE,
                required=True,
                reason="swift toolchain not found (macOS + Xcode required)",
                details={"package_path": f"mobile/{package}"},
            )
            return
        argv = [swift, "test", "--package-path", str(package_path)]
        result = self.runner.run(argv, cwd=self.root, timeout_seconds=900)
        details = {"command": _command_text(argv), "package_path": f"mobile/{package}"}
        if result.unavailable:
            self._record(
                gate_id,
                name,
                UNAVAILABLE,
                required=True,
                reason=_failure_reason(result, "swift test unavailable"),
                duration_seconds=result.duration_seconds,
                details=details,
                log_text=result.output,
            )
            return
        if result.returncode == 0:
            self._record(
                gate_id,
                name,
                PASSED,
                required=True,
                duration_seconds=result.duration_seconds,
                details=details,
                log_text=result.output,
            )
            return
        self._record(
            gate_id,
            name,
            FAILED,
            required=True,
            reason=_failure_reason(result, f"{package} package tests failed"),
            duration_seconds=result.duration_seconds,
            details=details,
            log_text=result.output,
        )

    def _xcode_lane(self, *, coverage: bool) -> None:
        if _which(self.runner, "xcodebuild", cwd=self.mobile) is None:
            self._record(
                "xcode-build",
                "Xcode app build",
                UNAVAILABLE,
                required=True,
                reason="xcodebuild not found (macOS + Xcode required)",
            )
            self._record(
                "xcode-app-unit",
                "Xcode app-unit tests",
                UNAVAILABLE,
                required=True,
                reason="xcodebuild not found",
            )
            self._record(
                "xcode-ui-simulator",
                "Xcode simulator UI tests (deterministic)",
                UNAVAILABLE,
                required=True,
                reason="xcodebuild not found",
            )
            if coverage:
                self._record(
                    "coverage-collect",
                    "Swift coverage collection",
                    UNAVAILABLE,
                    required=True,
                    reason="xcodebuild not found",
                )
            return

        rust_script = self.mobile / "scripts" / "build-rust-mobile.sh"
        rust = self.runner.run(["bash", str(rust_script)], cwd=self.root, timeout_seconds=1800)
        if rust.unavailable or rust.returncode not in (0,):
            status = UNAVAILABLE if rust.unavailable or rust.returncode is None else FAILED
            self._record(
                "rust-mobile-ffi",
                "Rust mobile FFI xcframework",
                status,
                required=True,
                reason=_failure_reason(rust, "Rust mobile FFI build failed"),
                duration_seconds=rust.duration_seconds,
                log_text=rust.output,
            )
            return
        self._record(
            "rust-mobile-ffi",
            "Rust mobile FFI xcframework",
            PASSED,
            required=True,
            duration_seconds=rust.duration_seconds,
            log_text=rust.output,
        )

        if _which(self.runner, "xcodegen", cwd=self.mobile) is None:
            self._record(
                "xcodegen",
                "XcodeGen project generation",
                UNAVAILABLE,
                required=True,
                reason="xcodegen not found",
            )
            return
        gen = self.runner.run(
            ["xcodegen", "generate", "--spec", str(self.mobile / "project.yml")],
            cwd=self.mobile,
            timeout_seconds=120,
        )
        if gen.unavailable or gen.returncode not in (0,):
            status = UNAVAILABLE if gen.unavailable or gen.returncode is None else FAILED
            self._record(
                "xcodegen",
                "XcodeGen project generation",
                status,
                required=True,
                reason=_failure_reason(gen, "xcodegen failed"),
                duration_seconds=gen.duration_seconds,
                log_text=gen.output,
            )
            return
        self._record(
            "xcodegen",
            "XcodeGen project generation",
            PASSED,
            required=True,
            duration_seconds=gen.duration_seconds,
            log_text=gen.output,
        )

        destination = self._simulator_destination()
        derived = self.report_dir / "DerivedData"
        project = self.mobile / "Rundale.xcodeproj"
        result_bundle = self.report_dir / "Rundale.xcresult"
        if result_bundle.exists():
            shutil.rmtree(result_bundle)

        build_argv = [
            "xcodebuild",
            "-project",
            str(project),
            "-scheme",
            "Rundale",
            "-configuration",
            self.configuration,
            "-destination",
            destination,
            "-derivedDataPath",
            str(derived),
            "build-for-testing",
        ]
        if coverage:
            build_argv.extend(["-enableCodeCoverage", "YES"])
        build = self.runner.run(build_argv, cwd=self.mobile, timeout_seconds=2400)
        if build.unavailable or build.returncode not in (0,):
            status = UNAVAILABLE if build.unavailable or build.returncode is None else FAILED
            self._record(
                "xcode-build",
                "Xcode app build",
                status,
                required=True,
                reason=_failure_reason(build, "xcodebuild build-for-testing failed"),
                duration_seconds=build.duration_seconds,
                details={"destination": destination, "command": _command_text(build_argv)},
                log_text=build.output,
            )
            return
        self._record(
            "xcode-build",
            "Xcode app build",
            PASSED,
            required=True,
            duration_seconds=build.duration_seconds,
            details={"destination": destination},
            log_text=build.output,
        )

        unit_argv = [
            "xcodebuild",
            "test-without-building",
            "-project",
            str(project),
            "-scheme",
            "Rundale",
            "-configuration",
            self.configuration,
            "-destination",
            destination,
            "-derivedDataPath",
            str(derived),
            "-only-testing:RundaleTests",
            "-resultBundlePath",
            str(result_bundle),
        ]
        if coverage:
            unit_argv.extend(["-enableCodeCoverage", "YES"])
        unit = self.runner.run(unit_argv, cwd=self.mobile, timeout_seconds=2400)
        if unit.unavailable or unit.returncode not in (0,):
            status = UNAVAILABLE if unit.unavailable or unit.returncode is None else FAILED
            self._record(
                "xcode-app-unit",
                "Xcode app-unit tests",
                status,
                required=True,
                reason=_failure_reason(unit, "RundaleTests failed"),
                duration_seconds=unit.duration_seconds,
                details={"destination": destination},
                log_text=unit.output,
            )
        else:
            self._record(
                "xcode-app-unit",
                "Xcode app-unit tests",
                PASSED,
                required=True,
                duration_seconds=unit.duration_seconds,
                details={"destination": destination},
                log_text=unit.output,
            )

        ui_argv = [
            "xcodebuild",
            "test-without-building",
            "-project",
            str(project),
            "-scheme",
            "Rundale",
            "-configuration",
            self.configuration,
            "-destination",
            destination,
            "-derivedDataPath",
            str(derived),
            "-resultBundlePath",
            str(self.report_dir / "RundaleUI.xcresult"),
        ]
        for class_name in DETERMINISTIC_UI_TEST_CLASSES:
            ui_argv.append(f"-only-testing:RundaleUITests/{class_name}")
        ui = self.runner.run(ui_argv, cwd=self.mobile, timeout_seconds=3600)
        if ui.unavailable or ui.returncode not in (0,):
            status = UNAVAILABLE if ui.unavailable or ui.returncode is None else FAILED
            self._record(
                "xcode-ui-simulator",
                "Xcode simulator UI tests (deterministic)",
                status,
                required=True,
                reason=_failure_reason(ui, "deterministic UI tests failed"),
                duration_seconds=ui.duration_seconds,
                details={
                    "destination": destination,
                    "classes": list(DETERMINISTIC_UI_TEST_CLASSES),
                    "excluded_opt_in": list(OPT_IN_UI_TEST_CLASSES),
                },
                log_text=ui.output,
            )
        else:
            self._record(
                "xcode-ui-simulator",
                "Xcode simulator UI tests (deterministic)",
                PASSED,
                required=True,
                duration_seconds=ui.duration_seconds,
                details={
                    "destination": destination,
                    "classes": list(DETERMINISTIC_UI_TEST_CLASSES),
                    "excluded_opt_in": list(OPT_IN_UI_TEST_CLASSES),
                },
                log_text=ui.output,
            )

        if coverage:
            self._coverage_policy(result_bundle if result_bundle.exists() else None)

    def _coverage_policy(self, result_bundle: Path | None) -> None:
        baseline_path = self.mobile / "coverage-baseline.json"
        details: dict[str, Any] = {
            "baseline_path": "mobile/coverage-baseline.json",
            "result_bundle": str(result_bundle) if result_bundle else None,
            "policy": (
                "Collect coverage on Xcode runs. Do not invent a percentage floor. "
                "Once a measured baseline JSON exists, fail on a drop greater than "
                "the recorded absolute tolerance."
            ),
        }
        if result_bundle is None or not result_bundle.exists():
            self._record(
                "coverage-collect",
                "Swift coverage collection",
                UNAVAILABLE,
                required=True,
                reason="no .xcresult bundle available to read coverage from",
                details=details,
            )
            return
        self._record(
            "coverage-collect",
            "Swift coverage collection",
            PASSED,
            required=True,
            reason="coverage enabled for the Xcode result bundle; inspect in Xcode Organizer",
            details=details,
        )
        if not baseline_path.is_file():
            self._record(
                "coverage-regression",
                "Swift coverage regression policy",
                UNAVAILABLE,
                required=False,
                reason=(
                    "no measured baseline yet — run on macOS with --xcode --coverage "
                    "--write-coverage-baseline after a green suite, then commit the JSON"
                ),
                details=details,
            )
            return
        baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
        if baseline.get("status") != "measured":
            self._record(
                "coverage-regression",
                "Swift coverage regression policy",
                UNAVAILABLE,
                required=False,
                reason="coverage-baseline.json is not marked measured",
                details={**details, "baseline": baseline},
            )
            return
        # Measured comparison lands once xcresulttool parsing is wired against
        # a real macOS receipt. Until then keep the gate explicit and non-passing.
        self._record(
            "coverage-regression",
            "Swift coverage regression policy",
            UNAVAILABLE,
            required=False,
            reason=(
                "baseline present, but automated line-rate comparison still needs a "
                "macOS xcresulttool receipt; do not treat collection alone as a floor"
            ),
            details={**details, "baseline": baseline},
        )

    def _opt_in_gate(self, name: str) -> None:
        known = {
            "live-endpoint": (
                "Live Endpoint UI suite",
                "requires private Firebase config and RUNDALE_ENDPOINT_*; "
                "run RundaleLiveEndpointUITests manually / via #2046",
            ),
            "performance": (
                "Performance UI suite",
                "opt-in XCTest metrics; not a required fast gate",
            ),
            "soak": (
                "Soak UI suite",
                "opt-in long-running workload; set RUNDALE_SOAK_* explicitly",
            ),
            "physical-device": (
                "Physical iPhone acceptance",
                "human/device evidence only; record under mobile/acceptance.md (#2046)",
            ),
        }
        if name not in known:
            self._record(
                f"opt-in:{name}",
                f"Unknown opt-in gate {name}",
                FAILED,
                required=False,
                reason=f"unknown opt-in gate {name!r}",
            )
            return
        title, reason = known[name]
        self._record(
            f"opt-in:{name}",
            title,
            SKIPPED,
            required=False,
            reason=reason,
            details={"credentials_in_logs": False},
        )

    def _simulator_destination(self) -> str:
        if self.simulator:
            if re.fullmatch(
                r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}",
                self.simulator,
            ):
                return f"platform=iOS Simulator,id={self.simulator}"
            return f"platform=iOS Simulator,name={self.simulator}"
        return "platform=iOS Simulator,name=iPhone 17 Pro"

    def _finish(self) -> int:
        counts = {status: 0 for status in STATUSES}
        for gate in self.gates:
            counts[gate.status] += 1
        blocking = [
            gate for gate in self.gates if gate.required and gate.status in {FAILED, UNAVAILABLE}
        ]
        report = {
            "schema": "rundale.swift-quality.v1",
            "issue": 2103,
            "started_at": self.started_at.isoformat(),
            "finished_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "blocking": bool(blocking),
            "counts": counts,
            "gates": [gate.to_dict() for gate in self.gates],
            "notes": [
                "Skips and unavailable results are never reported as passes.",
                "Live Endpoint, performance, and soak suites are opt-in.",
                "Physical-device acceptance is documented separately from simulator evidence.",
                "Full phase ./verify remains #2045; device/TestFlight continuity remains #2046.",
            ],
        }
        report_path = self.report_dir / "swift-quality.json"
        summary_path = self.report_dir / "summary.txt"
        report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        lines = [
            f"Swift quality: {'BLOCKING' if blocking else 'ok'}",
            (
                f"passed={counts[PASSED]} failed={counts[FAILED]} "
                f"skipped={counts[SKIPPED]} unavailable={counts[UNAVAILABLE]}"
            ),
        ]
        for gate in self.gates:
            req = "required" if gate.required else "optional"
            reason = f" ({gate.reason})" if gate.reason else ""
            lines.append(f"- [{gate.status}] ({req}) {gate.name}{reason}")
        summary_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print("\n".join(lines), flush=True)
        print(f"report: {report_path}", flush=True)
        return 1 if blocking else 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run Swift lint/format/package/Xcode quality gates for mobile/."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="Repository root (default: parent of mobile/)",
    )
    parser.add_argument(
        "--report-dir",
        type=Path,
        default=None,
        help="Directory for JSON/summary/logs (default: mobile/.verification/swift-quality)",
    )
    parser.add_argument("--simulator", default=None, help="Simulator UDID or name")
    parser.add_argument("--configuration", default="Debug")
    parser.add_argument("--lint", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--format-check", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--packages", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument(
        "--xcode",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="Build the app and run app-unit + deterministic simulator UI tests",
    )
    parser.add_argument(
        "--coverage",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="Enable Xcode coverage collection (implies meaningful use with --xcode)",
    )
    parser.add_argument(
        "--opt-in",
        action="append",
        default=[],
        choices=["live-endpoint", "performance", "soak", "physical-device"],
        help="Record an opt-in gate as skipped with instructions (repeatable)",
    )
    parser.add_argument(
        "--fast",
        action="store_true",
        help="Lint + format + package tests only (default without --xcode)",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(list(argv) if argv is not None else None)
    script_root = Path(__file__).resolve().parent
    root = (args.root or script_root.parent.parent).resolve()
    run = SwiftQualityRun(
        root=root,
        report_dir=args.report_dir,
        simulator=args.simulator,
        configuration=args.configuration,
    )
    xcode = bool(args.xcode) and not args.fast
    return run.run(
        lint=args.lint,
        format_check=args.format_check,
        packages=args.packages,
        xcode=xcode,
        coverage=args.coverage,
        opt_in=args.opt_in,
    )


if __name__ == "__main__":
    sys.exit(main())
