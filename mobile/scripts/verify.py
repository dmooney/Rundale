#!/usr/bin/env python3
"""Run deterministic verification gates for the native mobile prototype."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import shlex
import signal
import subprocess
import time
import xml.etree.ElementTree as ET
from collections.abc import Mapping, Sequence
from contextlib import suppress
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol, cast

PASSED = "passed"
FAILED = "failed"
SKIPPED = "skipped"
UNAVAILABLE = "unavailable"
NOT_AUTOMATABLE = "not_automatable"
STATUSES = {PASSED, FAILED, SKIPPED, UNAVAILABLE, NOT_AUTOMATABLE}
IMPLEMENTED_PHASE = 2
IMPLEMENTED_PHASES = (1, 2)
LAST_PHASE = 6
RUST_TOOLCHAIN = "1.98.0"
FORBIDDEN_MOBILE_DEPENDENCIES = (
    "parish-engine",
    "parish-server",
    "parish-tauri",
    "parish-editor",
    "parish-diagnostics",
    "tauri",
    "axum",
)


@dataclass(frozen=True)
class CommandResult:
    returncode: int | None
    stdout: str = ""
    stderr: str = ""
    duration_seconds: float = 0.0
    unavailable: bool = False
    timed_out: bool = False

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
    """Run each command in a process group so a timeout cleans up children."""

    def run(
        self,
        argv: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout_seconds: float | None = None,
    ) -> CommandResult:
        started = time.monotonic()
        process: subprocess.Popen[str] | None = None
        try:
            process = subprocess.Popen(
                [str(part) for part in argv],
                cwd=str(cwd),
                env=dict(env) if env is not None else None,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                start_new_session=(os.name == "posix"),
            )
            stdout, stderr = process.communicate(timeout=timeout_seconds)
        except FileNotFoundError as exc:
            return CommandResult(
                None, stderr=str(exc), duration_seconds=time.monotonic() - started, unavailable=True
            )
        except PermissionError as exc:
            return CommandResult(
                None, stderr=str(exc), duration_seconds=time.monotonic() - started, unavailable=True
            )
        except subprocess.TimeoutExpired as exc:
            stdout, stderr = _text(exc.stdout), _text(exc.stderr)
            if process is not None and process.poll() is None:
                if os.name == "posix":
                    with suppress(ProcessLookupError):
                        os.killpg(process.pid, signal.SIGTERM)
                else:
                    process.terminate()
                try:
                    tail_out, tail_err = process.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    if os.name == "posix":
                        with suppress(ProcessLookupError):
                            os.killpg(process.pid, signal.SIGKILL)
                    else:
                        process.kill()
                    tail_out, tail_err = process.communicate()
                stdout += _text(tail_out)
                stderr += _text(tail_err)
            return CommandResult(
                process.returncode if process is not None else None,
                stdout=stdout,
                stderr=stderr or f"command timed out after {timeout_seconds:g}s",
                duration_seconds=time.monotonic() - started,
                timed_out=True,
            )
        except OSError as exc:
            return CommandResult(
                None, stderr=str(exc), duration_seconds=time.monotonic() - started, unavailable=True
            )
        return CommandResult(
            process.returncode if process is not None else None,
            stdout=_text(stdout),
            stderr=_text(stderr),
            duration_seconds=time.monotonic() - started,
        )


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


def _relative(path: Path, root: Path) -> str:
    try:
        return path.resolve().relative_to(root.resolve()).as_posix()
    except ValueError:
        return str(path)


def _under_root(value: Path | None, default: Path, root: Path) -> Path:
    path = value or default
    return path if path.is_absolute() else root / path


def _safe_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "_", value).strip("_") or "command"


def _phase(value: str) -> int | None:
    if value.lower() == "all":
        return None
    try:
        result = int(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("phase must be 1-6 or all") from exc
    if result < 1 or result > LAST_PHASE:
        raise argparse.ArgumentTypeError("phase must be 1-6 or all")
    return result


def _simulator_candidates(payload: Mapping[str, Any]) -> list[dict[str, Any]]:
    devices = payload.get("devices")
    if not isinstance(devices, Mapping):
        return []
    candidates: list[dict[str, Any]] = []
    for runtime, entries in devices.items():
        if not isinstance(runtime, str) or not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, Mapping):
                continue
            name, udid = str(entry.get("name", "")), str(entry.get("udid", ""))
            available = entry.get("isAvailable", True)
            if isinstance(available, str):
                available = available.lower() not in {"no", "false", "unavailable"}
            if "deviceTypeIdentifier" in entry:
                device_type = entry["deviceTypeIdentifier"]
                is_iphone = isinstance(device_type, str) and bool(
                    re.search(r"(?:^|\.)iphone(?:-|$)", device_type, re.IGNORECASE)
                )
            else:
                is_iphone = name.lower().startswith("iphone")
            if not is_iphone or not udid or not available:
                continue
            candidates.append(
                {
                    "name": name,
                    "udid": udid,
                    "state": str(entry.get("state", "Unknown")),
                    "runtime": runtime,
                    "runtime_key": tuple(int(n) for n in re.findall(r"\d+", runtime)),
                }
            )
    return candidates


def _select_simulator(
    payload: Mapping[str, Any], override: str | None = None
) -> tuple[dict[str, Any] | None, str | None]:
    candidates = _simulator_candidates(payload)
    if override:
        needle = override.lower()
        matches = [
            c for c in candidates if c["udid"].lower() == needle or c["name"].lower() == needle
        ]
        matches = matches or [c for c in candidates if needle in c["name"].lower()]
        if not matches:
            return None, f"requested simulator {override!r} is not available"
    else:
        if not candidates:
            return None, "no available iPhone simulator was found"
        matches = candidates
    matches.sort(
        key=lambda c: (
            c["state"].lower() != "booted",
            tuple(-n for n in c["runtime_key"]),
            c["name"],
            c["udid"],
        )
    )
    return matches[0], None


class VerificationRun:
    def __init__(
        self,
        repo_root: Path,
        *,
        command_runner: CommandRunner | None = None,
        report_dir: Path | None = None,
        project_spec: Path | None = None,
        project: Path | None = None,
        scheme: str = "Rundale",
        package_path: Path | None = None,
        ui_tests_path: Path | None = None,
        simulator: str | None = None,
        configuration: str = "Debug",
        command_timeout_seconds: float = 30 * 60,
    ) -> None:
        self.root = repo_root.resolve()
        mobile = self.root / "mobile"
        self.report_dir = _under_root(report_dir, mobile / ".verification", self.root).resolve()
        self.project_spec = _under_root(project_spec, mobile / "project.yml", self.root).resolve()
        self.project = _under_root(project, mobile / "Rundale.xcodeproj", self.root).resolve()
        self.package_path = _under_root(package_path, mobile / "RundaleKit", self.root).resolve()
        self.ui_tests_path = _under_root(
            ui_tests_path, mobile / "RundaleUITests", self.root
        ).resolve()
        self.scheme, self.configuration = scheme, configuration
        self.simulator_override, self.timeout = simulator, command_timeout_seconds
        self.runner = command_runner or SubprocessCommandRunner()
        self.records: list[dict[str, Any]] = []
        self.results: dict[str, CommandResult] = {}
        self.simulator: dict[str, Any] | None = None
        self.started_at = dt.datetime.now(dt.timezone.utc).isoformat()
        self.run_stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")

    def _env(self) -> dict[str, str]:
        cache = self.report_dir / "tool-cache"
        dirs = {name: cache / name for name in ("swiftpm", "clang", "swift")}
        for path in dirs.values():
            path.mkdir(parents=True, exist_ok=True)
        env = os.environ.copy()
        env.update(
            {
                "CLANG_MODULE_CACHE_PATH": str(dirs["clang"]),
                "SWIFT_MODULECACHE_PATH": str(dirs["swift"]),
                "SWIFTPM_PACKAGECACHE_PATH": str(dirs["swiftpm"]),
                "XDG_CACHE_HOME": str(cache),
            }
        )
        return env

    def _xcode_env(self) -> dict[str, str]:
        """Keep compiler caches local without overriding Swift package discovery."""

        cache = self.report_dir / "tool-cache"
        clang, swift = cache / "clang", cache / "swift"
        clang.mkdir(parents=True, exist_ok=True)
        swift.mkdir(parents=True, exist_ok=True)
        env = os.environ.copy()
        env.update({"CLANG_MODULE_CACHE_PATH": str(clang), "SWIFT_MODULECACHE_PATH": str(swift)})
        return env

    def _rust_env(self) -> dict[str, str]:
        """Pin Rust build outputs to the report and avoid host sccache state."""

        env = os.environ.copy()
        env.update(
            {
                "CARGO_TARGET_DIR": str(self.report_dir / "rust-target"),
                "RUSTC_WRAPPER": "",
            }
        )
        return env

    def _bundle(self, name: str) -> str:
        return _relative(
            self.report_dir / "xcresults" / f"{name}-{self.run_stamp}.xcresult", self.root
        )

    def _record(
        self,
        *,
        identifier: str,
        name: str,
        phase: int,
        status: str,
        required: bool,
        automatable: bool = True,
        kind: str = "automated",
        reason: str | None = None,
        command: Sequence[str] | None = None,
        cwd: Path | None = None,
        result: CommandResult | None = None,
        details: Mapping[str, Any] | None = None,
    ) -> dict[str, Any]:
        if status not in STATUSES:
            raise ValueError(f"unsupported verification status: {status}")
        record: dict[str, Any] = {
            "id": identifier,
            "name": name,
            "phase": phase,
            "kind": kind,
            "status": status,
            "required": required,
            "automatable": automatable,
            "blocking": bool(required and automatable and status != PASSED),
        }
        if reason:
            record["reason"] = reason
        if command is not None:
            record["command"] = [str(part) for part in command]
            record["command_text"] = _command_text(command)
            record["cwd"] = _relative(cwd or self.root, self.root)
        if result is not None:
            record["returncode"] = result.returncode
            record["duration_seconds"] = round(max(result.duration_seconds, 0), 3)
            if result.timed_out:
                record["timed_out"] = True
            if result.unavailable:
                record["runner_unavailable"] = True
        if details:
            record["details"] = dict(details)
        if reason or result is not None:
            log_dir = self.report_dir / "logs"
            log_dir.mkdir(parents=True, exist_ok=True)
            log = log_dir / f"{_safe_name(identifier)}.log"
            lines = [f"$ {_command_text(command)}"] if command is not None else []
            if reason:
                lines.append(reason)
            if result is not None and result.output:
                lines += ([""] if lines else []) + [result.output.rstrip("\n")]
            log.write_text("\n".join(lines) + ("\n" if lines else ""), encoding="utf-8")
            record["log"] = _relative(log, self.root)
        self.records.append(record)
        return record

    def _change(
        self,
        record: dict[str, Any],
        status: str,
        reason: str | None = None,
        details: Mapping[str, Any] | None = None,
    ) -> None:
        record["status"] = status
        record["blocking"] = bool(
            record.get("required") and record.get("automatable", True) and status != PASSED
        )
        if reason:
            record["reason"] = reason
            if isinstance(record.get("log"), str):
                with (self.root / record["log"]).open("a", encoding="utf-8") as handle:
                    handle.write(f"{reason}\n")
        if details:
            record.setdefault("details", {}).update(details)

    def _execute(
        self, command: Sequence[str], *, env: Mapping[str, str] | None = None
    ) -> CommandResult:
        started = time.monotonic()
        try:
            return self.runner.run(command, cwd=self.root, env=env, timeout_seconds=self.timeout)
        except FileNotFoundError as exc:
            return CommandResult(
                None, stderr=str(exc), duration_seconds=time.monotonic() - started, unavailable=True
            )
        except OSError as exc:
            return CommandResult(
                None, stderr=str(exc), duration_seconds=time.monotonic() - started, unavailable=True
            )
        except RuntimeError as exc:
            return CommandResult(
                None,
                stderr=f"command runner error: {exc}",
                duration_seconds=time.monotonic() - started,
            )

    def _run(
        self,
        *,
        identifier: str,
        name: str,
        phase: int,
        command: Sequence[str],
        required: bool = True,
        kind: str = "automated",
        env: Mapping[str, str] | None = None,
        details: Mapping[str, Any] | None = None,
    ) -> dict[str, Any]:
        result = self._execute(command, env=env)
        self.results[identifier] = result
        if result.unavailable:
            status, reason = UNAVAILABLE, _failure_reason(result, "command runner is unavailable")
        elif result.returncode == 0:
            status, reason = PASSED, None
        else:
            status, reason = (
                FAILED,
                _failure_reason(result, f"command exited with status {result.returncode}"),
            )
        return self._record(
            identifier=identifier,
            name=name,
            phase=phase,
            status=status,
            required=required,
            kind=kind,
            reason=reason,
            command=command,
            result=result,
            details=details,
        )

    def _missing(
        self,
        identifier: str,
        name: str,
        phase: int,
        reason: str,
        *,
        required: bool = True,
        kind: str = "automated",
    ) -> dict[str, Any]:
        return self._record(
            identifier=identifier,
            name=name,
            phase=phase,
            status=UNAVAILABLE,
            required=required,
            kind=kind,
            reason=reason,
        )

    def _skip(
        self,
        identifier: str,
        name: str,
        phase: int,
        reason: str,
        *,
        required: bool = False,
        kind: str = "automated",
    ) -> dict[str, Any]:
        return self._record(
            identifier=identifier,
            name=name,
            phase=phase,
            status=SKIPPED,
            required=required,
            kind=kind,
            reason=reason,
        )

    def _swift_package_tests(
        self,
        *,
        package_path: Path,
        identifier: str,
        name: str,
        phase: int,
        required: bool = True,
    ) -> None:
        manifest, tests = package_path / "Package.swift", package_path / "Tests"
        if not manifest.is_file():
            self._missing(
                identifier,
                name,
                phase,
                f"required package manifest is missing: {_relative(manifest, self.root)}",
                required=required,
            )
            return
        if not tests.is_dir() or not any(tests.rglob("*.swift")):
            self._missing(
                identifier,
                name,
                phase,
                f"required Swift test sources are missing: {_relative(tests, self.root)}",
                required=required,
            )
            return
        package = _relative(package_path, self.root)
        record = self._run(
            identifier=identifier,
            name=name,
            phase=phase,
            command=[
                "swift",
                "test",
                "--package-path",
                package,
                "--cache-path",
                _relative(self.report_dir / "tool-cache" / "swiftpm", self.root),
                "--scratch-path",
                _relative(self.report_dir / "swift-scratch" / self.run_stamp, self.root),
                "--manifest-cache",
                "local",
                "--disable-dependency-cache",
                "--skip-update",
                "--no-parallel",
            ],
            env=self._env(),
            details={"package_path": package},
        )
        if record["status"] == PASSED:
            self._validate_swift_result(record)

    def _swift_tests(self, *, phase: int = 1, identifier: str = "swift-package-tests") -> None:
        self._swift_package_tests(
            package_path=self.package_path,
            identifier=identifier,
            name="RundaleKit Swift package tests",
            phase=phase,
        )

    def _validate_swift_result(self, test_record: dict[str, Any]) -> None:
        """Require Swift's XCTest runner to report executed tests."""

        output = self.results[test_record["id"]].output
        matches = list(
            re.finditer(
                r"^[ \t]*Executed[ \t]+(\d+)[ \t]+tests?,[ \t]+with[ \t]+(\d+)[ \t]+failures?\b",
                output,
                re.MULTILINE,
            )
        )
        if not matches:
            self._change(
                test_record, FAILED, "swift test output did not report executed XCTest counts"
            )
            return
        executed, failures = (int(value) for value in matches[-1].groups())
        details = {"executed_tests": executed, "failed_tests": failures}
        if executed == 0:
            status, reason = UNAVAILABLE, "swift test executed no tests"
        elif failures:
            status, reason = FAILED, f"swift test reports {failures} failed test(s)"
        else:
            status, reason = PASSED, None
        self._change(test_record, status, reason, details)

    def _validate_rust_result(self, test_record: dict[str, Any]) -> None:
        """Require cargo's test harness to report a non-empty test result."""

        output = self.results[test_record["id"]].output
        matches = list(
            re.finditer(
                r"test result:\s+\w+\.\s+(\d+) passed;\s+(\d+) failed;\s+(\d+) ignored;",
                output,
            )
        )
        if not matches:
            self._change(
                test_record,
                FAILED,
                "cargo test output did not report test-result counts",
            )
            return
        passed = sum(int(match.group(1)) for match in matches)
        failed = sum(int(match.group(2)) for match in matches)
        ignored = sum(int(match.group(3)) for match in matches)
        executed = passed + failed
        details = {
            "executed_tests": executed,
            "passed_tests": passed,
            "failed_tests": failed,
            "ignored_tests": ignored,
            "result_lines": len(matches),
            "toolchain": RUST_TOOLCHAIN,
        }
        if executed == 0:
            status, reason = UNAVAILABLE, "cargo test executed no tests"
        elif failed:
            status, reason = FAILED, f"cargo test reports {failed} failed test(s)"
        elif ignored:
            status, reason = SKIPPED, f"cargo test reports {ignored} ignored test(s)"
        else:
            status, reason = PASSED, None
        self._change(test_record, status, reason, details)

    def _cargo_test(
        self,
        *,
        identifier: str,
        name: str,
        package: str,
        cargo_args: Sequence[str] = (),
        phase: int = 2,
    ) -> None:
        command = [
            "rustup",
            "run",
            RUST_TOOLCHAIN,
            "cargo",
            "test",
            "--manifest-path",
            "parish/Cargo.toml",
            "-p",
            package,
            *cargo_args,
        ]
        record = self._run(
            identifier=identifier,
            name=name,
            phase=phase,
            command=command,
            env=self._rust_env(),
            details={"package": package, "toolchain": RUST_TOOLCHAIN},
        )
        if record["status"] == PASSED:
            self._validate_rust_result(record)

    def _mobile_dependency_graph(self) -> None:
        command = [
            "rustup",
            "run",
            RUST_TOOLCHAIN,
            "cargo",
            "tree",
            "--manifest-path",
            "parish/Cargo.toml",
            "-p",
            "parish-mobile-ffi",
            "--edges",
            "normal",
        ]
        record = self._run(
            identifier="mobile-dependency-graph",
            name="Portable mobile dependency graph",
            phase=2,
            command=command,
            env=self._rust_env(),
            details={"forbidden_dependencies": list(FORBIDDEN_MOBILE_DEPENDENCIES)},
        )
        if record["status"] != PASSED:
            return
        output = self.results[record["id"]].output
        found = sorted(
            dependency
            for dependency in FORBIDDEN_MOBILE_DEPENDENCIES
            if re.search(rf"\b{re.escape(dependency)}\b", output)
        )
        if not output.strip():
            self._change(record, FAILED, "cargo tree produced no dependency graph")
        elif found:
            self._change(
                record,
                FAILED,
                f"portable dependency graph contains forbidden crate(s): {', '.join(found)}",
                {"forbidden_found": found},
            )
        else:
            self._change(record, PASSED, details={"forbidden_found": []})

    def _mobile_rust_packaging(self) -> None:
        script = self.root / "mobile" / "scripts" / "build-rust-mobile.sh"
        if not script.is_file():
            self._missing(
                "mobile-rust-packaging",
                "Rust mobile device and simulator packaging",
                2,
                f"required packaging script is missing: {_relative(script, self.root)}",
            )
            return
        self._run(
            identifier="mobile-rust-packaging",
            name="Rust mobile device and simulator packaging",
            phase=2,
            command=["bash", _relative(script, self.root), "all"],
            env=self._rust_env(),
            details={"toolchain": RUST_TOOLCHAIN, "architectures": ["device", "simulator"]},
        )

    def _xcodegen(self, *, phase: int = 1) -> bool:
        if not self.project_spec.is_file():
            self._missing(
                "xcodegen",
                "XcodeGen project generation",
                phase,
                f"required XcodeGen specification is missing: {_relative(self.project_spec, self.root)}",
            )
            return False
        spec = _relative(self.project_spec, self.root)
        record = self._run(
            identifier="xcodegen",
            name="XcodeGen project generation",
            phase=phase,
            command=["xcodegen", "generate", "--spec", spec],
            details={"spec": spec, "project": _relative(self.project, self.root)},
        )
        if record["status"] == PASSED and not self.project.exists():
            self._change(
                record,
                FAILED,
                f"xcodegen exited successfully but did not produce the expected project: {_relative(self.project, self.root)}",
            )
        return record["status"] == PASSED

    def _xcodebuild(
        self,
        kind: str,
        destination: str | None = None,
        *,
        phase: int = 1,
        identifier: str | None = None,
        name: str | None = None,
        only_testing: str | None = None,
    ) -> dict[str, Any]:
        project, derived, bundle = (
            _relative(self.project, self.root),
            _relative(self.report_dir / "DerivedData" / f"{kind}-{self.run_stamp}", self.root),
            self._bundle(kind),
        )
        command = [
            "xcodebuild",
            "-project",
            project,
            "-scheme",
            self.scheme,
            "-configuration",
            self.configuration,
        ]
        command += ["-sdk", "iphoneos"] if destination is None else ["-destination", destination]
        if only_testing is not None:
            command.append(f"-only-testing:{only_testing}")
        command += ["-derivedDataPath", derived, "-resultBundlePath", bundle]
        command += (
            ["CODE_SIGNING_ALLOWED=NO", "CODE_SIGNING_REQUIRED=NO", "build"]
            if destination is None
            else ["test"]
        )
        identifier = identifier or (
            "ios-device-build" if destination is None else "ios-simulator-tests"
        )
        return self._run(
            identifier=identifier,
            name=name
            or (
                "Unsigned iOS device build"
                if destination is None
                else "iOS simulator XCTest/XCUITest suite"
            ),
            phase=phase,
            command=command,
            env=self._xcode_env(),
            details={
                "scheme": self.scheme,
                "derived_data": derived,
                "result_bundle": bundle,
                "sdk": "iphoneos",
            }
            if destination is None
            else {
                "scheme": self.scheme,
                "destination": destination,
                "derived_data": derived,
                "result_bundle": bundle,
            },
        )

    def _device(self, xcodegen_ok: bool, *, phase: int = 1) -> None:
        if not xcodegen_ok:
            self._skip(
                "ios-device-build",
                "Unsigned iOS device build",
                phase,
                "blocked because XcodeGen did not produce a usable project",
                required=True,
            )
        elif self.project.exists():
            self._xcodebuild("device", phase=phase)
        else:
            self._missing(
                "ios-device-build",
                "Unsigned iOS device build",
                phase,
                f"required Xcode project is missing: {_relative(self.project, self.root)}",
            )

    def _select(self, *, phase: int = 1) -> tuple[dict[str, Any] | None, dict[str, Any]]:
        command = ["xcrun", "simctl", "list", "devices", "available", "-j"]
        result = self._execute(command)
        self.results["simulator-selection"] = result
        name = "Available iPhone simulator selection"
        if result.unavailable or result.returncode != 0:
            status = UNAVAILABLE
            reason = _failure_reason(
                result,
                "xcrun/simctl is unavailable"
                if result.unavailable
                else f"simctl exited with status {result.returncode}",
            )
            return None, self._record(
                identifier="simulator-selection",
                name=name,
                phase=phase,
                status=status,
                required=True,
                reason=reason,
                command=command,
                result=result,
            )
        try:
            payload = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            return None, self._record(
                identifier="simulator-selection",
                name=name,
                phase=phase,
                status=FAILED,
                required=True,
                reason=f"simctl returned invalid JSON: {exc}",
                command=command,
                result=result,
            )
        if not isinstance(payload, Mapping):
            return None, self._record(
                identifier="simulator-selection",
                name=name,
                phase=phase,
                status=FAILED,
                required=True,
                reason="simctl returned a JSON value that is not an object",
                command=command,
                result=result,
            )
        selected, selection_reason = _select_simulator(payload, self.simulator_override)
        if selected is None:
            return None, self._record(
                identifier="simulator-selection",
                name=name,
                phase=phase,
                status=UNAVAILABLE,
                required=True,
                reason=selection_reason or "no simulator selected",
                command=command,
                result=result,
            )
        self.simulator = selected
        return selected, self._record(
            identifier="simulator-selection",
            name=name,
            phase=phase,
            status=PASSED,
            required=True,
            command=command,
            result=result,
            details={
                "name": selected["name"],
                "udid": selected["udid"],
                "runtime": selected["runtime"],
                "state": selected["state"],
                "source": "override" if self.simulator_override else "automatic",
            },
        )

    def _boot(self, selected: dict[str, Any] | None, *, phase: int = 1) -> bool:
        if selected is None:
            self._skip(
                "simulator-boot",
                "iPhone simulator boot",
                phase,
                "blocked because simulator selection was unavailable",
                required=True,
            )
            return False
        if selected["state"].lower() == "booted":
            self._skip(
                "simulator-boot",
                "iPhone simulator boot",
                phase,
                f"simulator {selected['name']} is already booted",
                kind="infrastructure",
            )
            return True
        udid = selected["udid"]
        boot = self._run(
            identifier="simulator-boot",
            name="iPhone simulator boot",
            phase=phase,
            command=["xcrun", "simctl", "boot", udid],
            kind="infrastructure",
            details={"name": selected["name"], "udid": udid},
        )
        if boot["status"] != PASSED:
            return False
        ready = self._run(
            identifier="simulator-bootstatus",
            name="iPhone simulator boot readiness",
            phase=phase,
            command=["xcrun", "simctl", "bootstatus", udid, "-b"],
            kind="infrastructure",
            details={"name": selected["name"], "udid": udid},
        )
        return ready["status"] == PASSED

    def _validate_result(
        self,
        test_record: dict[str, Any],
        *,
        phase: int | None = None,
        summary_identifier: str = "ios-simulator-test-results",
    ) -> None:
        bundle = str(test_record["details"]["result_bundle"])
        result_phase = phase if phase is not None else int(test_record["phase"])
        summary_record = self._run(
            identifier=summary_identifier,
            name="iOS simulator test-result validation",
            phase=result_phase,
            command=[
                "xcrun",
                "xcresulttool",
                "get",
                "test-results",
                "summary",
                "--path",
                bundle,
                "--compact",
            ],
            details={"result_bundle": bundle},
        )
        if summary_record["status"] != PASSED:
            self._change(test_record, summary_record["status"], summary_record.get("reason"))
            return
        try:
            summary = json.loads(self.results[summary_identifier].stdout)
        except json.JSONDecodeError as exc:
            reason = f"xcresulttool returned invalid JSON: {exc}"
            self._change(test_record, FAILED, reason)
            self._change(summary_record, FAILED, reason)
            return
        if not isinstance(summary, Mapping):
            reason = "xcresulttool returned a JSON value that is not an object"
            self._change(test_record, FAILED, reason)
            self._change(summary_record, FAILED, reason)
            return
        keys = ("totalTestCount", "passedTests", "failedTests", "skippedTests")
        raw_counts: dict[str, int | None] = {key: summary.get(key) for key in keys}
        if not all(type(value) is int and value >= 0 for value in raw_counts.values()):
            reason = "xcresulttool summary omitted integer test counts"
            self._change(test_record, FAILED, reason, raw_counts)
            self._change(summary_record, FAILED, reason, raw_counts)
            return
        counts = cast(dict[str, int], raw_counts)
        total = counts["totalTestCount"]
        counted = counts["passedTests"] + counts["failedTests"] + counts["skippedTests"]
        if total != counted:
            reason = (
                f"xcresult test counts do not reconcile: totalTestCount={total}, counted={counted}"
            )
            self._change(test_record, FAILED, reason, counts)
            self._change(summary_record, FAILED, reason, counts)
            return
        if total == 0:
            result_status, result_reason = UNAVAILABLE, "xcresult contains no executed tests"
        elif counts["failedTests"]:
            result_status, result_reason = (
                FAILED,
                f"xcresult reports {counts['failedTests']} failed test(s)",
            )
        elif counts["skippedTests"]:
            result_status, result_reason = (
                SKIPPED,
                f"xcresult reports {counts['skippedTests']} skipped test(s)",
            )
        elif counts["passedTests"] != total:
            result_status, result_reason = (
                FAILED,
                f"xcresult reports {counts['passedTests']} of {total} test(s) passed",
            )
        elif summary.get("result") not in (None, "Passed"):
            result_status, result_reason = (
                FAILED,
                f"xcresult reports result {summary.get('result')!r}",
            )
        else:
            result_status, result_reason = PASSED, None
        self._change(test_record, result_status, result_reason, counts)
        self._change(summary_record, result_status, result_reason, counts)

    def _simulator_tests(self, xcodegen_ok: bool, ready: bool) -> None:
        identifier, name = "ios-simulator-tests", "iOS simulator XCTest/XCUITest suite"
        if not xcodegen_ok:
            self._skip(
                identifier,
                name,
                1,
                "blocked because XcodeGen did not produce a usable project",
                required=True,
            )
            return
        if not self.project.exists():
            self._missing(
                identifier,
                name,
                1,
                f"required Xcode project is missing: {_relative(self.project, self.root)}",
            )
            return
        if not self.ui_tests_path.is_dir() or not any(self.ui_tests_path.rglob("*.swift")):
            self._missing(
                identifier,
                name,
                1,
                f"required native UI test sources are missing: {_relative(self.ui_tests_path, self.root)}",
            )
            return
        if not ready or self.simulator is None:
            self._skip(
                identifier,
                name,
                1,
                "blocked because the selected simulator is not ready",
                required=True,
            )
            return
        destination = f"platform=iOS Simulator,id={self.simulator['udid']}"
        record = self._xcodebuild(
            "simulator", destination, only_testing="RundaleUITests/RundaleUITests"
        )
        if record["status"] == PASSED:
            self._validate_result(record, phase=1)

    def _phase2_simulator_tests(self, xcodegen_ok: bool, ready: bool) -> None:
        identifier = "phase2-ios-simulator-tests"
        name = "Phase 2 iOS simulator XCTest/XCUITest suite"
        phase2_sources = sorted(self.ui_tests_path.rglob("*Phase2*.swift"))
        if not xcodegen_ok:
            self._skip(
                identifier,
                name,
                2,
                "blocked because XcodeGen did not produce a usable project",
                required=True,
            )
            return
        if not self.project.exists():
            self._missing(
                identifier,
                name,
                2,
                f"required Xcode project is missing: {_relative(self.project, self.root)}",
            )
            return
        if not phase2_sources:
            self._missing(
                identifier,
                name,
                2,
                f"Phase 2 native UI test sources are missing under {_relative(self.ui_tests_path, self.root)}",
            )
            return
        if not ready or self.simulator is None:
            self._skip(
                identifier,
                name,
                2,
                "blocked because the selected simulator is not ready",
                required=True,
            )
            return
        destination = f"platform=iOS Simulator,id={self.simulator['udid']}"
        record = self._xcodebuild(
            "phase2-simulator",
            destination,
            phase=2,
            identifier=identifier,
            name=name,
            only_testing="RundaleUITests/RundalePhase2UITests",
        )
        if record["status"] == PASSED:
            self._validate_result(
                record,
                phase=2,
                summary_identifier="phase2-ios-simulator-test-results",
            )

    def _phase2_physical(self) -> None:
        for identifier, name, reason in (
            (
                "physical-iphone-phase2-runtime",
                "Physical iPhone Phase 2 local runtime",
                "requires a human Phase 2 session on a connected iPhone",
            ),
            (
                "physical-iphone-phase2-streaming",
                "Physical iPhone Phase 2 Endpoint streaming",
                "requires a human session with the deployed Parish Endpoint on a connected iPhone",
            ),
        ):
            self._record(
                identifier=identifier,
                name=name,
                phase=2,
                status=NOT_AUTOMATABLE,
                required=True,
                automatable=False,
                kind="physical",
                reason=reason,
            )

    def _phase2_live_endpoint(self) -> None:
        self._record(
            identifier="live-endpoint-integration",
            name="Opt-in live Parish Endpoint integration",
            phase=2,
            status=UNAVAILABLE,
            required=False,
            automatable=True,
            kind="live-integration",
            reason="opt-in live Endpoint verification is not configured in this checkout",
        )

    def _physical(self) -> None:
        for identifier, name, reason in (
            (
                "physical-iphone-interaction",
                "Physical iPhone Phase 1 interaction",
                "requires a human session on a connected iPhone; automation cannot establish usability",
            ),
            (
                "physical-iphone-accessibility",
                "Physical iPhone Dynamic Type and VoiceOver",
                "requires human VoiceOver and accessibility-size judgment on a connected iPhone",
            ),
            (
                "physical-iphone-lifecycle",
                "Physical iPhone lifecycle and safe-area checks",
                "requires a human to exercise keyboard, interruption, relaunch, and safe-area behavior on a connected iPhone",
            ),
        ):
            self._record(
                identifier=identifier,
                name=name,
                phase=1,
                status=NOT_AUTOMATABLE,
                required=True,
                automatable=False,
                kind="physical",
                reason=reason,
            )

    def _future(self) -> None:
        for phase in range(IMPLEMENTED_PHASE + 1, LAST_PHASE + 1):
            self._missing(
                f"phase-{phase}-verification",
                f"Phase {phase} verification",
                phase,
                f"Phase {phase} verification is not implemented in this checkout",
                required=False,
                kind="future-phase",
            )

    def _phase2(
        self,
        *,
        reuse_phase1_native_setup: bool,
        rust_packaging_already_run: bool = False,
    ) -> None:
        self._cargo_test(
            identifier="parish-core-mobile-tests",
            name="Parish core portable mobile tests",
            package="parish-core",
            cargo_args=("--no-default-features", "--features", "mobile", "--lib", "mobile::"),
        )
        self._cargo_test(
            identifier="parish-mobile-endpoint-contract",
            name="Production engine Endpoint wire fixture",
            package="parish-core",
            cargo_args=(
                "--no-default-features",
                "--features",
                "mobile",
                "--test",
                "mobile_endpoint_fixture",
            ),
        )
        self._cargo_test(
            identifier="parish-persistence-mobile-tests",
            name="Parish mobile persistence tests",
            package="parish-persistence",
            cargo_args=("mobile::",),
        )
        self._cargo_test(
            identifier="parish-mobile-ffi-tests",
            name="Parish mobile FFI tests",
            package="parish-mobile-ffi",
        )
        self._mobile_dependency_graph()
        if not rust_packaging_already_run:
            self._mobile_rust_packaging()

        # Phase 1 already runs the shared RundaleKit package in an all-phase
        # invocation.  An explicit Phase 2 invocation still runs it so Phase
        # 2 remains independently reproducible; all-phase does not duplicate
        # the package build/test work.
        if not reuse_phase1_native_setup:
            self._swift_tests(phase=2, identifier="swift-package-tests-phase2")
            xcodegen_ok = self._xcodegen(phase=2)
            self._device(xcodegen_ok, phase=2)
            selected, selection = self._select(phase=2)
            ready = selection["status"] == PASSED and self._boot(selected, phase=2)
        else:
            xcodegen_ok = any(
                record["id"] == "xcodegen" and record["status"] == PASSED for record in self.records
            )
            selected = self.simulator
            selection_passed = any(
                record["id"] == "simulator-selection" and record["status"] == PASSED
                for record in self.records
            )
            boot_ready = any(
                record["id"] == "simulator-bootstatus" and record["status"] == PASSED
                for record in self.records
            ) or any(
                record["id"] == "simulator-boot"
                and record["status"] == SKIPPED
                and record["kind"] == "infrastructure"
                for record in self.records
            )
            ready = selected is not None and selection_passed and boot_ready
        bridge_path = self.root / "mobile" / "RundaleBridge"
        self._swift_package_tests(
            package_path=bridge_path,
            identifier="swift-bridge-tests",
            name="RundaleBridge Swift bridge tests",
            phase=2,
        )
        endpoint_kit_path = self.root / "mobile" / "ParishEndpointKit"
        self._swift_package_tests(
            package_path=endpoint_kit_path,
            identifier="swift-endpoint-kit-tests",
            name="ParishEndpointKit Swift Endpoint tests",
            phase=2,
        )
        self._phase2_simulator_tests(xcodegen_ok, ready)
        self._phase2_live_endpoint()
        self._phase2_physical()

    def _phase1(self) -> None:
        self._swift_tests()
        xcodegen_ok = self._xcodegen()
        self._device(xcodegen_ok)
        selected, selection = self._select()
        ready = selection["status"] == PASSED and self._boot(selected)
        self._simulator_tests(xcodegen_ok, ready)
        self._physical()

    def _summary(self) -> dict[str, int]:
        counts = {status: 0 for status in (PASSED, FAILED, SKIPPED, UNAVAILABLE, NOT_AUTOMATABLE)}
        for record in self.records:
            counts[record["status"]] += 1
        return {**counts, "blocking": sum(record["blocking"] for record in self.records)}

    def run(self, phase: int | None = None) -> dict[str, Any]:
        if phase is None:
            # The generated iOS project consumes the Rust XCFramework. Build it
            # before Phase 1 generates/builds that project, then retain the
            # single packaging result when Phase 2 runs its remaining gates.
            self._mobile_rust_packaging()
            self._phase1()
            self._phase2(reuse_phase1_native_setup=True, rust_packaging_already_run=True)
            self._future()
        elif phase == 1:
            self._phase1()
        elif phase == 2:
            self._phase2(reuse_phase1_native_setup=False)
        else:
            self._missing(
                f"phase-{phase}-verification",
                f"Phase {phase} verification",
                phase,
                f"Phase {phase} verification is not implemented in this checkout",
                required=True,
                kind="future-phase",
            )
        summary = self._summary()
        report = {
            "schema_version": 1,
            "tool": "rundale-mobile-verify",
            "phase": "all" if phase is None else str(phase),
            "implemented_phases": list(IMPLEMENTED_PHASES),
            "started_at": self.started_at,
            "finished_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "status": FAILED if summary["blocking"] else PASSED,
            "exit_code": 1 if summary["blocking"] else 0,
            "summary": summary,
            "reports": {
                key: _relative(self.report_dir / filename, self.root)
                for key, filename in (
                    ("json", "verify.json"),
                    ("junit", "verify.junit.xml"),
                    ("summary", "summary.txt"),
                )
            },
            "suites": self.records,
        }
        self._write_reports(report)
        return report

    def _write_reports(self, report: Mapping[str, Any]) -> None:
        self.report_dir.mkdir(parents=True, exist_ok=True)
        (self.report_dir / "verify.json").write_text(
            json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        root = ET.Element(
            "testsuite",
            {
                "name": "Rundale mobile verification",
                "tests": str(len(self.records)),
                "failures": str(sum(r["status"] == FAILED for r in self.records)),
                "errors": str(sum(r["status"] == UNAVAILABLE for r in self.records)),
                "skipped": str(
                    sum(r["status"] in {SKIPPED, NOT_AUTOMATABLE} for r in self.records)
                ),
            },
        )
        for record in self.records:
            case = ET.SubElement(
                root,
                "testcase",
                {
                    "classname": f"phase{record['phase']}.{record['kind']}",
                    "name": record["name"],
                    "status": record["status"],
                    "time": str(record.get("duration_seconds", 0)),
                },
            )
            reason = record.get("reason", "")
            if record["status"] == FAILED:
                node = ET.SubElement(case, "failure", {"message": reason})
                node.text = reason
            elif record["status"] == UNAVAILABLE:
                node = ET.SubElement(case, "error", {"message": reason})
                node.text = reason
            elif record["status"] in {SKIPPED, NOT_AUTOMATABLE}:
                node = ET.SubElement(case, "skipped", {"message": reason})
                node.text = reason
        ET.indent(root, space="  ")
        ET.ElementTree(root).write(
            self.report_dir / "verify.junit.xml", encoding="utf-8", xml_declaration=True
        )
        summary = report["summary"]
        lines = [
            f"Rundale mobile verification (phase {report['phase']})",
            f"{'PASS' if report['exit_code'] == 0 else 'FAIL'} automated gate: {summary['passed']} passed, {summary['failed']} failed, {summary['skipped']} skipped, {summary['unavailable']} unavailable, {summary['not_automatable']} not automatable",
        ]
        lines += [
            f"- {r['status'].upper()}: {r['name']}"
            + (f" — {r['reason']}" if r.get("reason") else "")
            for r in self.records
            if r["blocking"] or r["status"] == FAILED
        ]
        lines += ["", f"JSON: {report['reports']['json']}", f"JUnit: {report['reports']['junit']}"]
        (self.report_dir / "summary.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run deterministic Rundale native mobile verification gates."
    )
    parser.add_argument(
        "--phase",
        type=_phase,
        default=None,
        metavar="1-6|all",
        help="phase to run; default runs all currently implemented phases",
    )
    parser.add_argument("--project-spec", type=Path)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--scheme", default="Rundale")
    parser.add_argument("--package-path", type=Path)
    parser.add_argument("--ui-tests-path", type=Path)
    parser.add_argument("--simulator", help="specific available simulator UDID or name")
    parser.add_argument("--configuration", default="Debug")
    parser.add_argument("--report-dir", type=Path)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root = Path(__file__).resolve().parents[2]
    run = VerificationRun(
        root,
        report_dir=args.report_dir,
        project_spec=args.project_spec,
        project=args.project,
        scheme=args.scheme,
        package_path=args.package_path,
        ui_tests_path=args.ui_tests_path,
        simulator=args.simulator,
        configuration=args.configuration,
    )
    report = run.run(args.phase)
    print((run.report_dir / "summary.txt").read_text(encoding="utf-8"), end="")
    return int(report["exit_code"])


if __name__ == "__main__":
    raise SystemExit(main())
