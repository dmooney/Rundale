#!/usr/bin/env python3
"""Run deterministic verification gates for the native mobile prototype."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import plistlib
import re
import shlex
import shutil
import signal
import subprocess
import tempfile
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
# Xcode spreads whole test classes across parallel simulator workers, so each
# phase's UI tests are split over several classes (sharing one base class).
PHASE2_UI_CLASSES = (
    "RundaleUITests/RundalePhase2UITests",
    "RundaleUITests/RundalePhase2DialogueUITests",
    "RundaleUITests/RundalePhase2RecoveryUITests",
)
PHASE3_UI_CLASSES = (
    "RundaleUITests/RundalePhase3UITests",
    "RundaleUITests/RundalePhase3ArrivalsUITests",
    "RundaleUITests/RundalePhase3ClarificationUITests",
)
PHASE4_UI_CLASSES = (
    "RundaleUITests/RundalePhase4UITests",
    "RundaleUITests/RundalePhase4RecoveryUITests",
    "RundaleUITests/RundalePhase4NetworkUITests",
    "RundaleUITests/RundalePhase4AccessibilityUITests",
)
IMPLEMENTED_PHASE = 4
IMPLEMENTED_PHASES = (1, 2, 3, 4)
LAST_PHASE = 6


def _pinned_rust_toolchain() -> str:
    """Return the channel `rust-toolchain.toml` pins, as CI and the build script use."""

    pin = Path(__file__).resolve().parents[2] / "rust-toolchain.toml"
    match = re.search(r'^channel\s*=\s*"([^"]+)"', pin.read_text(encoding="utf-8"), re.M)
    if match is None:
        raise RuntimeError(f"no toolchain channel in {pin}")
    return match.group(1)


RUST_TOOLCHAIN = _pinned_rust_toolchain()
CACHE_SCHEMA = 1
# Documentation that no gate compiles or reads. Edits here leave cached passes
# valid; everything else in the working tree (tracked or untracked, excluding
# ignored files) is part of the content key.
CACHE_IGNORED_PATHSPECS = (
    "docs",
    ":(glob)mobile/**/*.md",
    ":(glob)endpoints/**/*.md",
    "README.md",
    "LEARNINGS.md",
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    # Read only by the coverage gate, which reapplies it to reused passes.
    "mobile/coverage-baseline.json",
)
PRIVATE_FIREBASE_CONFIG = Path("mobile/Rundale/Resources/GoogleService-Info.plist")
# Measured Swift package line coverage and the allowed drop (#2103).
COVERAGE_BASELINE = Path("mobile/coverage-baseline.json")
SWIFT_TOOLS_BIN = Path("mobile/.build/swift-tools/bin")
FORBIDDEN_MOBILE_DEPENDENCIES = (
    "limerick-engine",
    "limerick-server",
    "limerick-tauri",
    "limerick-editor",
    "limerick-diagnostics",
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


def _worker_count(value: str) -> int:
    try:
        count = int(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("worker count must be an integer >= 1") from exc
    if count < 1:
        raise argparse.ArgumentTypeError("worker count must be an integer >= 1")
    return count


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
                    "device_type": str(entry.get("deviceTypeIdentifier") or name),
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


DEFAULT_PARALLEL_WORKERS = 4


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
        device: str | None = None,
        live_endpoint: bool = False,
        soak: bool = False,
        performance: bool = False,
        development_team: str | None = None,
        configuration: str = "Debug",
        command_timeout_seconds: float = 30 * 60,
        use_cache: bool = True,
        parallel_workers: int | None = None,
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
        if (live_endpoint or soak or performance) and not device:
            raise ValueError("--live-endpoint, --soak, and --performance require --device")
        self.scheme = scheme
        self.configuration = "Release" if performance else configuration
        self.simulator_override, self.device_override = simulator, device
        self.live_endpoint, self.soak, self.performance = live_endpoint, soak, performance
        self.development_team = development_team or os.environ.get("RUNDALE_IOS_DEVELOPMENT_TEAM")
        self.timeout = command_timeout_seconds
        self.parallel_workers = (
            DEFAULT_PARALLEL_WORKERS if parallel_workers is None else parallel_workers
        )
        if self.parallel_workers < 1:
            raise ValueError("--parallel-workers must be at least 1")
        self.runner = command_runner or SubprocessCommandRunner()
        self.records: list[dict[str, Any]] = []
        self.results: dict[str, CommandResult] = {}
        self.simulator: dict[str, Any] | None = None
        self.physical_device: dict[str, str] | None = (
            {"identifier": device, "source": "override"} if device else None
        )
        self.physical_device_ready = device is None
        self.started_at = dt.datetime.now(dt.timezone.utc).isoformat()
        self.run_stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        self.use_cache = use_cache
        # Shared by every report directory so a custom --report-dir still
        # reuses and contributes passes.
        self.cache_dir = self.root / "mobile" / ".verification" / "cache"
        self._content_key_value: str | None = None
        self._content_key_reason: str | None = None
        self._content_key_computed = False

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
        if self.soak:
            env["RUNDALE_SOAK_UI_TESTS"] = "1"
            env["TEST_RUNNER_RUNDALE_SOAK_UI_TESTS"] = "1"
            if "RUNDALE_SOAK_SMOKE" in os.environ:
                env["TEST_RUNNER_RUNDALE_SOAK_SMOKE"] = os.environ["RUNDALE_SOAK_SMOKE"]
            if "RUNDALE_SOAK_DURATION_SECONDS" in os.environ:
                env["TEST_RUNNER_RUNDALE_SOAK_DURATION_SECONDS"] = os.environ[
                    "RUNDALE_SOAK_DURATION_SECONDS"
                ]
        if self.performance:
            env["RUNDALE_PERFORMANCE_UI_TESTS"] = "1"
            env["TEST_RUNNER_RUNDALE_PERFORMANCE_UI_TESTS"] = "1"
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

    def _content_key(self) -> str | None:
        """Hash every input that can change a suite's result, or None.

        The working tree is hashed as a git tree built in a temporary index, so
        uncommitted and untracked (non-ignored) files count and committing the
        same content keeps the same key. The ignored private Firebase file and
        the Xcode, Swift, and Rust toolchains are included. Any failure disables
        reuse for the run instead of guessing.
        """

        if self._content_key_computed:
            return self._content_key_value
        self._content_key_computed = True
        index_path = self._execute(["git", "rev-parse", "--git-path", "index"])
        if index_path.returncode != 0 or not index_path.stdout.strip():
            self._content_key_reason = "git index is unavailable"
            return None
        source_index = self.root / index_path.stdout.strip()
        with tempfile.TemporaryDirectory(prefix="rundale-verify-index-") as scratch:
            temporary_index = Path(scratch) / "index"
            if source_index.is_file():
                shutil.copyfile(source_index, temporary_index)
            env = {**os.environ, "GIT_INDEX_FILE": str(temporary_index)}
            for step in (
                ["git", "add", "-A", "--", "."],
                [
                    "git",
                    "rm",
                    "-r",
                    "-q",
                    "--cached",
                    "--ignore-unmatch",
                    "--",
                    *CACHE_IGNORED_PATHSPECS,
                ],
            ):
                if self._execute(step, env=env).returncode != 0:
                    self._content_key_reason = f"could not stage the working tree ({step[1]})"
                    return None
            tree = self._execute(["git", "write-tree"], env=env)
        tree_id = tree.stdout.strip()
        if tree.returncode != 0 or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", tree_id):
            self._content_key_reason = "git write-tree did not return a tree id"
            return None
        toolchain = {}
        for label, command in (
            ("xcode", ["xcodebuild", "-version"]),
            ("swift", ["swift", "--version"]),
            ("rust", ["rustup", "run", RUST_TOOLCHAIN, "rustc", "--version"]),
        ):
            result = self._execute(command)
            if result.returncode != 0:
                self._content_key_reason = f"could not identify the {label} toolchain"
                return None
            toolchain[label] = result.output.strip()
        firebase = self.root / PRIVATE_FIREBASE_CONFIG
        try:
            firebase_digest = hashlib.sha256(firebase.read_bytes()).hexdigest()
        except FileNotFoundError:
            firebase_digest = "absent"
        except OSError:
            self._content_key_reason = "private Firebase configuration is unreadable"
            return None
        payload = {
            "schema": CACHE_SCHEMA,
            "tree": tree_id,
            "ignored_pathspecs": list(CACHE_IGNORED_PATHSPECS),
            "firebase": firebase_digest,
            "toolchain": toolchain,
        }
        self._content_key_value = hashlib.sha256(
            json.dumps(payload, sort_keys=True).encode()
        ).hexdigest()
        return self._content_key_value

    def _reuse(
        self, *, identifier: str, name: str, phase: int, inputs: Mapping[str, Any]
    ) -> tuple[dict[str, Any] | None, str | None]:
        """Return a reused passing record for identical inputs, and the suite key."""

        content = self._content_key()
        if content is None:
            return None, None
        key = hashlib.sha256(
            json.dumps({"content": content, "suite": inputs}, sort_keys=True).encode()
        ).hexdigest()
        if not self.use_cache:
            return None, key
        try:
            entry = json.loads((self.cache_dir / f"{key}.json").read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return None, key
        if (
            not isinstance(entry, Mapping)
            or entry.get("status") != PASSED
            or entry.get("key") != key
        ):
            return None, key
        source = entry.get("source")
        details = dict(entry.get("details") or {})
        details["cache"] = {
            "reused": True,
            "key": key,
            **(dict(source) if isinstance(source, Mapping) else {}),
        }
        record = self._record(
            identifier=identifier,
            name=name,
            phase=phase,
            status=PASSED,
            required=True,
            details=details,
        )
        return record, key

    def _store_cached_passes(self, report_path: Path) -> None:
        """Record each freshly passed, cacheable suite for later identical runs."""

        for record in self.records:
            details = record.get("details", {})
            key = details.get("cache_key")
            if not key or record["status"] != PASSED or details.get("cache", {}).get("reused"):
                continue
            entry = {
                "schema": CACHE_SCHEMA,
                "key": key,
                "status": PASSED,
                "name": record["name"],
                "details": {
                    name: value for name, value in details.items() if name not in {"cache_key"}
                },
                "source": {
                    "suite": record["id"],
                    "started_at": self.started_at,
                    "report": _relative(report_path, self.root),
                    "log": record.get("log"),
                },
            }
            self.cache_dir.mkdir(parents=True, exist_ok=True)
            (self.cache_dir / f"{key}.json").write_text(
                json.dumps(entry, indent=2, sort_keys=True) + "\n", encoding="utf-8"
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
        reused, key = self._reuse(
            identifier=identifier,
            name=name,
            phase=phase,
            inputs={"tool": "swift-test", "package": package, "coverage": True},
        )
        if reused is not None:
            self._coverage_gate(reused, package_path.name)
            return
        scratch = self.report_dir / "swift-scratch" / self.run_stamp / package_path.name
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
                _relative(scratch, self.root),
                "--manifest-cache",
                "local",
                "--disable-dependency-cache",
                "--skip-update",
                "--no-parallel",
                "--enable-code-coverage",
                # The compiler warning policy: package code and tests build
                # warning-free (docs/agent/swift-quality-gates.md).
                "-Xswiftc",
                "-warnings-as-errors",
            ],
            env=self._env(),
            details={"package_path": package, **({"cache_key": key} if key else {})},
        )
        if record["status"] == PASSED:
            self._validate_swift_result(record)
        if record["status"] == PASSED:
            coverage = self._package_line_coverage(scratch, package_path)
            if isinstance(coverage, str):
                self._change(record, FAILED, coverage)
            else:
                self._change(record, PASSED, details={"line_coverage": coverage})
                self._coverage_gate(record, package_path.name)

    def _package_line_coverage(self, scratch: Path, package_path: Path) -> dict[str, Any] | str:
        """Sum `swift test` line coverage over the package's own Sources."""

        reports = sorted(
            path
            for path in scratch.glob("*/debug/codecov/*.json")
            if path.stem == package_path.name
        )
        if not reports:
            return f"swift test wrote no coverage report under {_relative(scratch, self.root)}"
        try:
            payload = json.loads(reports[0].read_text(encoding="utf-8"))
            files = payload["data"][0]["files"]
        except (OSError, json.JSONDecodeError, KeyError, IndexError, TypeError) as exc:
            return f"unreadable coverage report {_relative(reports[0], self.root)}: {exc}"
        sources = (package_path / "Sources").resolve()
        count = covered = 0
        for entry in files:
            path = Path(str(entry.get("filename", ""))).resolve()
            if path.is_relative_to(sources):
                lines = entry["summary"]["lines"]
                count += int(lines["count"])
                covered += int(lines["covered"])
        if count == 0:
            return "the coverage report has no lines from the package's Sources"
        return {
            "covered": covered,
            "count": count,
            "percent": round(100 * covered / count, 2),
            "report": _relative(reports[0], self.root),
        }

    def _coverage_gate(self, test_record: dict[str, Any], package: str) -> None:
        """Fail when a package's line coverage drops below its measured baseline."""

        identifier, name = f"{test_record['id']}-coverage", f"{package} line coverage"
        if test_record["status"] != PASSED:
            return
        measured = test_record.get("details", {}).get("line_coverage")
        if not isinstance(measured, Mapping):
            self._record(
                identifier=identifier,
                name=name,
                phase=int(test_record["phase"]),
                status=FAILED,
                required=True,
                reason="the passing package run recorded no line coverage",
            )
            return
        try:
            baseline = json.loads((self.root / COVERAGE_BASELINE).read_text(encoding="utf-8"))
            floor = float(baseline["packages"][package]["line_percent"])
            tolerance = float(baseline["tolerance_percentage_points"])
        except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as exc:
            self._record(
                identifier=identifier,
                name=name,
                phase=int(test_record["phase"]),
                status=FAILED,
                required=True,
                reason=f"no usable {package} baseline in {COVERAGE_BASELINE}: {exc!r}",
                details={"measured": dict(measured)},
            )
            return
        percent = float(measured["percent"])
        details = {"measured": dict(measured), "baseline": floor, "tolerance": tolerance}
        if percent + tolerance < floor:
            status: str = FAILED
            reason: str | None = (
                f"{package} line coverage {percent:.2f}% is more than {tolerance:g} "
                f"percentage points below the {floor:.2f}% baseline"
            )
        else:
            status, reason = PASSED, None
        self._record(
            identifier=identifier,
            name=name,
            phase=int(test_record["phase"]),
            status=status,
            required=True,
            reason=reason,
            details=details,
        )

    def _swift_style(self, *, phase: int) -> None:
        """Pinned SwiftLint and SwiftFormat checks over every mobile Swift file."""

        install = self._run(
            identifier="swift-tools",
            name="Pinned SwiftLint and SwiftFormat",
            phase=phase,
            command=["bash", "mobile/scripts/install-swift-tools.sh"],
            kind="infrastructure",
        )
        checks = (
            (
                "swiftlint",
                "SwiftLint (strict)",
                ["swiftlint", "lint", "--strict", "--quiet", "--config", "mobile/.swiftlint.yml"],
            ),
            (
                "swiftformat",
                "SwiftFormat check",
                ["swiftformat", "--lint", "--config", "mobile/.swiftformat"],
            ),
        )
        for identifier, name, command in checks:
            if install["status"] != PASSED:
                self._skip(
                    identifier,
                    name,
                    phase,
                    "blocked because the pinned Swift tools did not install",
                    required=True,
                )
                continue
            tool = (SWIFT_TOOLS_BIN / command[0]).as_posix()
            self._run(
                identifier=identifier,
                name=name,
                phase=phase,
                command=[tool, *command[1:], "mobile"],
            )

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

    def _validate_rust_result(
        self, test_record: dict[str, Any], expected_ignored: Sequence[str] = ()
    ) -> None:
        """Require cargo's test harness to report a non-empty test result.

        An ignored test makes the suite a skip unless the gate names it in
        `expected_ignored` (an opt-in measurement, say), so a newly ignored
        test still blocks.
        """

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
        ignored_names = re.findall(r"^test (\S+) \.\.\. ignored", output, re.M)
        unexpected = [name for name in ignored_names if name not in expected_ignored]
        if len(ignored_names) != ignored:
            unexpected.append(f"{ignored - len(ignored_names)} unnamed")
        executed = passed + failed
        details = {
            "executed_tests": executed,
            "passed_tests": passed,
            "failed_tests": failed,
            "ignored_tests": ignored,
            "expected_ignored": [name for name in ignored_names if name in expected_ignored],
            "result_lines": len(matches),
            "toolchain": RUST_TOOLCHAIN,
        }
        if executed == 0:
            status, reason = UNAVAILABLE, "cargo test executed no tests"
        elif failed:
            status, reason = FAILED, f"cargo test reports {failed} failed test(s)"
        elif unexpected:
            status, reason = (
                SKIPPED,
                f"cargo test reports {ignored} ignored test(s): {', '.join(unexpected)}",
            )
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
        expected_ignored: Sequence[str] = (),
    ) -> None:
        command = [
            "rustup",
            "run",
            RUST_TOOLCHAIN,
            "cargo",
            "test",
            "--manifest-path",
            "limerick/Cargo.toml",
            "-p",
            package,
            *cargo_args,
        ]
        reused, key = self._reuse(
            identifier=identifier,
            name=name,
            phase=phase,
            inputs={"tool": "cargo-test", "command": command},
        )
        if reused is not None:
            return
        record = self._run(
            identifier=identifier,
            name=name,
            phase=phase,
            command=command,
            env=self._rust_env(),
            details={
                "package": package,
                "toolchain": RUST_TOOLCHAIN,
                **({"cache_key": key} if key else {}),
            },
        )
        if record["status"] == PASSED:
            self._validate_rust_result(record, expected_ignored)

    def _mobile_dependency_graph(self) -> None:
        command = [
            "rustup",
            "run",
            RUST_TOOLCHAIN,
            "cargo",
            "tree",
            "--manifest-path",
            "limerick/Cargo.toml",
            "-p",
            "limerick-mobile-ffi",
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
        only_testing: str | Sequence[str] | None = None,
        skip_testing: Sequence[str] = (),
    ) -> dict[str, Any]:
        is_physical = destination is not None and destination.startswith("platform=iOS,id=")
        target_kind = (
            "iphoneos" if destination is None else ("device" if is_physical else "simulator")
        )
        project, derived, bundle = (
            _relative(self.project, self.root),
            _relative(
                self.report_dir / "DerivedData" / f"{target_kind}-{self.run_stamp}",
                self.root,
            ),
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
        if is_physical:
            command.append("-allowProvisioningUpdates")
        if only_testing is not None:
            selectors = [only_testing] if isinstance(only_testing, str) else only_testing
            command.extend(f"-only-testing:{selector}" for selector in selectors)
        command.extend(f"-skip-testing:{target}" for target in skip_testing)
        if self.soak and is_physical:
            command.append("-test-timeouts-enabled")
            command.append("NO")
        if destination is not None and not is_physical and self.parallel_workers > 1:
            # Simulator suites clone the destination simulator per worker.
            # Every UI test launches the app with its own launch arguments and
            # a fresh app container, so classes share no state. One worker
            # keeps the run serial.
            command += [
                "-parallel-testing-enabled",
                "YES",
                "-parallel-testing-worker-count",
                str(self.parallel_workers),
            ]
        command += ["-derivedDataPath", derived, "-resultBundlePath", bundle]
        command += (
            ["CODE_SIGNING_ALLOWED=NO", "CODE_SIGNING_REQUIRED=NO", "build"]
            if destination is None
            else ["ENABLE_TESTABILITY=YES", "ONLY_ACTIVE_ARCH=YES", "test"]
        )
        if is_physical and self.development_team:
            command.insert(-1, f"DEVELOPMENT_TEAM={self.development_team}")
        identifier = identifier or (
            "ios-device-build" if destination is None else "ios-simulator-tests"
        )
        name = name or (
            "Unsigned iOS device build"
            if destination is None
            else (
                "Physical iPhone XCTest/XCUITest suite"
                if is_physical
                else "iOS simulator XCTest/XCUITest suite"
            )
        )
        key = None
        # Physical devices, soak, and performance depend on hardware and
        # sessions outside the repository, so they always run.
        if not is_physical and not self.soak and not self.performance:
            simulator = self.simulator if destination is not None else None
            reused, key = self._reuse(
                identifier=identifier,
                name=name,
                phase=phase,
                inputs={
                    "tool": "xcodebuild",
                    "target": target_kind,
                    "scheme": self.scheme,
                    "configuration": self.configuration,
                    "only_testing": [only_testing]
                    if isinstance(only_testing, str)
                    else list(only_testing or []),
                    "skip_testing": list(skip_testing),
                    "simulator": None
                    if simulator is None
                    else {
                        "runtime": simulator.get("runtime"),
                        "device_type": simulator.get("device_type"),
                    },
                },
            )
            if reused is not None:
                return reused
        record = self._run(
            identifier=identifier,
            name=name,
            phase=phase,
            command=command,
            env=self._xcode_env(),
            details={
                "scheme": self.scheme,
                "derived_data": derived,
                "result_bundle": bundle,
                "sdk": "iphoneos",
                **({"cache_key": key} if key else {}),
            }
            if destination is None
            else {
                "scheme": self.scheme,
                "destination": destination,
                "target": target_kind,
                "derived_data": derived,
                "result_bundle": bundle,
                **({"cache_key": key} if key else {}),
            },
        )
        if record["status"] == PASSED:
            app_identity = self._built_app_identity(self.root / derived)
            if app_identity:
                record.setdefault("details", {}).update(app_identity)
        return record

    def _built_app_identity(self, derived_path: Path) -> dict[str, Any]:
        for plist_path in sorted(derived_path.glob("Build/Products/*/Rundale.app/Info.plist")):
            if plist_path.parent.name != "Rundale.app":
                continue
            try:
                with plist_path.open("rb") as handle:
                    plist = plistlib.load(handle)
            except (OSError, plistlib.InvalidFileException):
                continue
            if not isinstance(plist, Mapping):
                continue
            identity = {
                key: str(plist[key])
                for key in ("CFBundleShortVersionString", "CFBundleVersion")
                if key in plist
            }
            if identity:
                return {
                    "app_identity": identity,
                    "app_info_plist": _relative(plist_path, self.root),
                }
        return {}

    def _physical_destination(self) -> str | None:
        if self.physical_device is None or not self.physical_device_ready:
            return None
        return f"platform=iOS,id={self.physical_device['identifier']}"

    def _device_preflight(self) -> None:
        if not self.device_override:
            return
        inventory_path = self.report_dir / "device" / f"devices-{self.run_stamp}.json"
        inventory_path.parent.mkdir(parents=True, exist_ok=True)
        command = [
            "xcrun",
            "devicectl",
            "list",
            "devices",
            "--json-output",
            _relative(inventory_path, self.root),
        ]
        result = self._execute(command)
        self.results["physical-device-selection"] = result
        if result.unavailable or result.returncode != 0:
            self.physical_device_ready = False
            self._record(
                identifier="physical-device-selection",
                name="Physical iPhone device selection",
                phase=1,
                status=UNAVAILABLE,
                required=True,
                kind="infrastructure",
                reason=_failure_reason(result, "devicectl is unavailable"),
                command=command,
                result=result,
            )
            return
        try:
            inventory = (
                inventory_path.read_text(encoding="utf-8")
                if inventory_path.is_file()
                else result.stdout
            )
            payload = json.loads(inventory)
        except json.JSONDecodeError as exc:
            self.physical_device_ready = False
            self._record(
                identifier="physical-device-selection",
                name="Physical iPhone device selection",
                phase=1,
                status=UNAVAILABLE,
                required=True,
                kind="infrastructure",
                reason=f"devicectl returned invalid JSON: {exc}",
                command=command,
                result=result,
            )
            return
        if isinstance(payload, Mapping) and isinstance(payload.get("result"), Mapping):
            payload = payload["result"]
        entries = payload.get("devices", []) if isinstance(payload, Mapping) else []
        selected: Mapping[str, Any] | None = None
        if isinstance(entries, list):
            for entry in entries:
                if not isinstance(entry, Mapping):
                    continue
                hardware = entry.get("hardwareProperties", {})
                hardware = hardware if isinstance(hardware, Mapping) else {}
                identifier = str(entry.get("identifier", ""))
                udid = str(hardware.get("udid", entry.get("udid", "")))
                props = entry.get("deviceProperties", {})
                props = props if isinstance(props, Mapping) else {}
                name = str(entry.get("name", props.get("name", "")))
                if (
                    self.device_override in {identifier, udid}
                    or name.lower() == self.device_override.lower()
                ):
                    selected = entry
                    break
        if selected is None:
            self.physical_device_ready = False
            self._record(
                identifier="physical-device-selection",
                name="Physical iPhone device selection",
                phase=1,
                status=UNAVAILABLE,
                required=True,
                kind="infrastructure",
                reason=f"requested physical device {self.device_override!r} is not paired and available",
                command=command,
                result=result,
            )
            return
        props = selected.get("deviceProperties", {})
        props = props if isinstance(props, Mapping) else {}
        hardware = selected.get("hardwareProperties", {})
        hardware = hardware if isinstance(hardware, Mapping) else {}
        connection = selected.get("connectionProperties", {})
        connection = connection if isinstance(connection, Mapping) else {}
        identifier = str(hardware.get("udid", selected.get("identifier", self.device_override)))
        developer_mode = str(props.get("developerModeStatus", "")).lower()
        pairing = str(connection.get("pairingState", "")).lower()
        available = str(selected.get("availability", selected.get("state", ""))).lower()
        if (
            developer_mode != "enabled"
            or pairing != "paired"
            or available
            in {
                "unavailable",
                "disconnected",
                "offline",
            }
        ):
            self.physical_device_ready = False
            reason = "physical device is not available with Developer Mode enabled"
            self._record(
                identifier="physical-device-selection",
                name="Physical iPhone device selection",
                phase=1,
                status=UNAVAILABLE,
                required=True,
                kind="infrastructure",
                reason=reason,
                command=command,
                result=result,
            )
            return
        self.physical_device = {
            "identifier": identifier,
            "name": str(props.get("name", selected.get("name", "iPhone"))),
            "os_version": str(props.get("osVersionNumber", "unknown")),
            "model": str(hardware.get("marketingName", hardware.get("productType", "unknown"))),
            "developer_mode": "enabled",
            "paired": pairing,
            "connection": str(
                connection.get("transportType", connection.get("tunnelState", "unknown"))
            ),
            "source": "devicectl",
        }
        self.physical_device_ready = True
        self._record(
            identifier="physical-device-selection",
            name="Physical iPhone device selection",
            phase=1,
            status=PASSED,
            required=True,
            kind="infrastructure",
            command=command,
            result=result,
            details={k: v for k, v in self.physical_device.items() if k != "source"},
        )

    def _physical_suite(
        self,
        *,
        identifier: str,
        name: str,
        phase: int,
        source: Path,
        target: str | Sequence[str],
        skip_testing: Sequence[str] = (),
    ) -> None:
        """Run one native suite on an explicitly selected, signed iPhone."""
        destination = self._physical_destination()
        if destination is None:
            if self.device_override:
                self._skip(
                    identifier,
                    name,
                    phase,
                    "blocked because physical device preflight was unavailable",
                    required=True,
                )
            return
        if not source.is_file():
            self._missing(
                identifier,
                name,
                phase,
                f"required native test source is missing: {_relative(source, self.root)}",
            )
            return
        record = self._xcodebuild(
            identifier,
            destination,
            phase=phase,
            identifier=identifier,
            name=name,
            only_testing=target,
            skip_testing=skip_testing,
        )
        if record["status"] == PASSED:
            self._validate_result(
                record,
                phase=phase,
                summary_identifier=f"{identifier}-results",
            )

    def _optional_physical_suites(self) -> None:
        """Run opt-in device-only suites when their sources are present."""
        optional = (
            (
                self.live_endpoint,
                "live-endpoint",
                "Live Endpoint integration",
                "RundaleLiveEndpointUITests.swift",
                "RundaleUITests/RundaleLiveEndpointUITests",
            ),
            (
                self.soak,
                "soak",
                "Soak reliability",
                "RundaleSoakUITests.swift",
                "RundaleUITests/RundaleSoakUITests",
            ),
            (
                self.performance,
                "performance",
                "Performance budgets",
                "RundalePerformanceUITests.swift",
                "RundaleUITests/RundalePerformanceUITests",
            ),
        )
        for enabled, suffix, label, source_name, target in optional:
            if not enabled:
                continue
            self._physical_suite(
                identifier=f"physical-iphone-{suffix}",
                name=f"Physical iPhone {label}",
                phase=4,
                source=self.ui_tests_path / source_name,
                target=target,
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
        result_phase = phase if phase is not None else int(test_record["phase"])
        if test_record.get("details", {}).get("cache", {}).get("reused"):
            # The run that produced this pass validated its xcresult first.
            self._record(
                identifier=summary_identifier,
                name="iOS simulator test-result validation",
                phase=result_phase,
                status=PASSED,
                required=True,
                details={
                    key: value
                    for key, value in test_record["details"].items()
                    if key
                    in {
                        "totalTestCount",
                        "passedTests",
                        "failedTests",
                        "skippedTests",
                        "expectedFailures",
                        "cache",
                    }
                },
            )
            return
        bundle = str(test_record["details"]["result_bundle"])
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
        # A test wrapped in XCTExpectFailure for a tracked defect (#2081) is
        # its own bucket. Older xcresulttool versions omit the key.
        expected_failures = summary.get("expectedFailures", 0)
        if type(expected_failures) is not int or expected_failures < 0:
            reason = "xcresulttool summary reported a non-integer expectedFailures count"
            self._change(test_record, FAILED, reason, counts)
            self._change(summary_record, FAILED, reason, counts)
            return
        counts["expectedFailures"] = expected_failures
        total = counts["totalTestCount"]
        counted = (
            counts["passedTests"]
            + counts["failedTests"]
            + counts["skippedTests"]
            + expected_failures
        )
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
        elif counts["passedTests"] + expected_failures != total:
            result_status, result_reason = (
                FAILED,
                f"xcresult reports {counts['passedTests']} of {total} test(s) passed",
            )
        elif summary.get("result") not in (None, "Passed"):
            result_status, result_reason = (
                FAILED,
                f"xcresult reports result {summary.get('result')!r}",
            )
        elif expected_failures:
            # Passing, but name the known defects so the report never hides them.
            result_status, result_reason = (
                PASSED,
                f"xcresult reports {expected_failures} expected failure(s) (XCTExpectFailure)",
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
            "simulator",
            destination,
            only_testing=[
                "RundaleUITests/RundaleUITests",
                "RundaleUITests/RundalePhase1AuditUITests",
                "RundaleUITests/RundalePhase1TimerAuditUITests",
                "RundaleTests/Phase1AuditVolumeTests",
                "RundaleTests/LaunchConfigurationTests",
                "RundaleTests/TranscriptScrollingTests",
                "RundaleTests/HeaderWeatherTests",
            ],
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
            only_testing=[
                *PHASE2_UI_CLASSES,
                "RundaleUITests/RundaleSceneUITests",
                "RundaleUITests/RundaleCommandsUITests",
                "RundaleUITests/RundaleFailureLinesUITests",
                "RundaleTests/RundaleEngineLifecycleTests",
            ],
        )
        if record["status"] == PASSED:
            self._validate_result(
                record,
                phase=2,
                summary_identifier="phase2-ios-simulator-test-results",
            )

    def _phase3_simulator_tests(self, xcodegen_ok: bool, ready: bool) -> None:
        identifier = "phase3-ios-simulator-tests"
        name = "Phase 3 canonical-world iOS simulator suite"
        phase3_sources = sorted(self.ui_tests_path.rglob("*Phase3*.swift"))
        if not xcodegen_ok:
            self._skip(
                identifier,
                name,
                3,
                "blocked because XcodeGen did not produce a usable project",
                required=True,
            )
            return
        if not self.project.exists():
            self._missing(
                identifier,
                name,
                3,
                f"required Xcode project is missing: {_relative(self.project, self.root)}",
            )
            return
        if not phase3_sources:
            self._missing(
                identifier,
                name,
                3,
                f"Phase 3 native UI test sources are missing under {_relative(self.ui_tests_path, self.root)}",
            )
            return
        if not ready or self.simulator is None:
            self._skip(
                identifier,
                name,
                3,
                "blocked because the selected simulator is not ready",
                required=True,
            )
            return
        destination = f"platform=iOS Simulator,id={self.simulator['udid']}"
        record = self._xcodebuild(
            "phase3-simulator",
            destination,
            phase=3,
            identifier=identifier,
            name=name,
            only_testing=PHASE3_UI_CLASSES,
        )
        if record["status"] == PASSED:
            self._validate_result(
                record, phase=3, summary_identifier="phase3-ios-simulator-test-results"
            )

    def _phase2_physical(self) -> None:
        self._physical_suite(
            identifier="physical-iphone-phase2-tests",
            name="Physical iPhone Phase 2 XCTest/XCUITest suite",
            phase=2,
            source=self.ui_tests_path / "RundalePhase2UITests.swift",
            target=PHASE2_UI_CLASSES,
            skip_testing=(
                "RundaleUITests/RundalePhase2UITests/testSimulatorReturnKeySubmitsDraft",
            ),
        )
        for identifier, name, reason in (
            (
                "physical-iphone-phase2-runtime",
                "Physical iPhone Phase 2 local runtime",
                "requires a human Phase 2 session on a connected iPhone",
            ),
            (
                "physical-iphone-phase2-streaming",
                "Physical iPhone Phase 2 Endpoint streaming",
                "requires a human session with the deployed Limerick Endpoint on a connected iPhone",
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
            name="Opt-in live Limerick Endpoint integration",
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
        # The shared engine in the configuration the phone links: the turn
        # API, intent and dialogue Endpoint calls, and save round trips.
        self._cargo_test(
            identifier="limerick-core-mobile-tests",
            name="Limerick core portable (mobile feature) tests",
            package="limerick-core",
            # Unit and integration tests; the crate's doc examples are not
            # tests of the portable engine.
            cargo_args=("--no-default-features", "--features", "mobile", "--lib", "--tests"),
            expected_ignored=("measure_candidate_capture_cost_on_rundale",),
        )
        self._cargo_test(
            identifier="limerick-persistence-tests",
            name="Limerick save format, journal, and kernel save-lock tests",
            package="limerick-persistence",
        )
        self._cargo_test(
            identifier="limerick-mobile-ffi-tests",
            name="Limerick mobile FFI tests",
            package="limerick-mobile-ffi",
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
        endpoint_kit_path = self.root / "mobile" / "LimerickEndpointKit"
        self._swift_package_tests(
            package_path=endpoint_kit_path,
            identifier="swift-endpoint-kit-tests",
            name="LimerickEndpointKit Swift Endpoint tests",
            phase=2,
        )
        self._phase2_simulator_tests(xcodegen_ok, ready)
        self._phase2_live_endpoint()
        self._phase2_physical()

    def _phase3(self, *, reuse_native_setup: bool) -> None:
        # mods/rundale is the canonical tiny world (#2084); its world sheet
        # is the oracle for places, people, and routes.
        self._cargo_test(
            identifier="limerick-world-sheet-tests",
            name="Canonical tiny-world sheet tests",
            package="limerick-engine",
            cargo_args=("--test", "world_sheet"),
        )
        if not reuse_native_setup:
            self._mobile_dependency_graph()
            self._mobile_rust_packaging()
            self._swift_tests(phase=3, identifier="swift-package-tests-phase3")
            xcodegen_ok = self._xcodegen(phase=3)
            self._device(xcodegen_ok, phase=3)
            selected, selection = self._select(phase=3)
            ready = selection["status"] == PASSED and self._boot(selected, phase=3)
            bridge_path = self.root / "mobile" / "RundaleBridge"
            self._swift_package_tests(
                package_path=bridge_path,
                identifier="swift-bridge-tests-phase3",
                name="RundaleBridge Phase 3 clarification tests",
                phase=3,
            )
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
        self._phase3_simulator_tests(xcodegen_ok, ready)
        self._physical_suite(
            identifier="physical-iphone-phase3-tests",
            name="Physical iPhone Phase 3 canonical-world suite",
            phase=3,
            source=self.ui_tests_path / "RundalePhase3UITests.swift",
            target=PHASE3_UI_CLASSES,
        )
        for identifier, name, reason in (
            (
                "physical-iphone-phase3-world",
                "Physical iPhone Phase 3 world traversal",
                "requires a human to visit all three canonical locations on a connected iPhone",
            ),
            (
                "physical-iphone-phase3-presence",
                "Physical iPhone Phase 3 presence and clarification",
                "requires human observation of schedule movement, availability, and ambiguity on a connected iPhone",
            ),
            (
                "physical-iphone-phase3-resume",
                "Physical iPhone Phase 3 save/resume consistency",
                "requires a human relaunch check on a connected iPhone",
            ),
        ):
            self._record(
                identifier=identifier,
                name=name,
                phase=3,
                status=NOT_AUTOMATABLE,
                required=True,
                automatable=False,
                kind="physical",
                reason=reason,
            )

    def _phase4(self) -> None:
        """Reliability builds on the earlier native/runtime regression gates."""
        xcodegen_ok = any(
            record["id"] == "xcodegen" and record["status"] == PASSED for record in self.records
        )
        ready = self.simulator is not None and any(
            record["id"] == "simulator-bootstatus"
            and record["status"] == PASSED
            or record["id"] == "simulator-boot"
            and record["status"] == SKIPPED
            and record["kind"] == "infrastructure"
            for record in self.records
        )
        for identifier, name, source, target in (
            (
                "phase4-ios-simulator-tests",
                "Phase 4 native reliability suite",
                self.ui_tests_path / "RundalePhase4UITests.swift",
                PHASE4_UI_CLASSES,
            ),
            (
                "phase4-ios-controller-tests",
                "Native controller and launch contracts",
                self.root / "mobile" / "RundaleTests" / "RundalePhase4Tests.swift",
                "RundaleTests",
            ),
        ):
            if not source.exists():
                self._missing(
                    identifier,
                    name,
                    4,
                    f"required native test source is missing: {_relative(source, self.root)}",
                )
                continue
            if not xcodegen_ok or not ready or self.simulator is None:
                self._skip(
                    identifier,
                    name,
                    4,
                    "blocked because the native project or simulator is not ready",
                    required=True,
                )
                continue
            record = self._xcodebuild(
                identifier,
                f"platform=iOS Simulator,id={self.simulator['udid']}",
                phase=4,
                identifier=identifier,
                name=name,
                only_testing=target,
            )
            if record["status"] == PASSED:
                self._validate_result(record, phase=4, summary_identifier=f"{identifier}-results")
        self._physical_suite(
            identifier="physical-iphone-phase4-tests",
            name="Physical iPhone Phase 4 native reliability suite",
            phase=4,
            source=self.ui_tests_path / "RundalePhase4UITests.swift",
            target=PHASE4_UI_CLASSES,
        )
        self._physical_suite(
            identifier="physical-iphone-phase4-controller-tests",
            name="Physical iPhone Phase 4 controller suite",
            phase=4,
            source=self.root / "mobile" / "RundaleTests" / "RundalePhase4Tests.swift",
            target="RundaleTests",
        )
        for identifier, name, reason in (
            (
                "physical-iphone-phase4-session",
                "20-minute physical iPhone reliability sessions",
                "requires recorded sessions on the primary and small-screen supported iPhones, including lifecycle, connectivity, and save recovery",
            ),
            (
                "physical-iphone-phase4-accessibility",
                "Physical keyboard, dictation, and VoiceOver",
                "requires human judgment of the core loop, focus, announcements, text entry, and accessibility Dynamic Type",
            ),
            (
                "physical-iphone-phase4-performance",
                "Physical long-history performance budgets",
                "requires device-calibrated latency and memory measurements, scroll stability, and storage/file-protection checks",
            ),
        ):
            self._record(
                identifier=identifier,
                name=name,
                phase=4,
                status=NOT_AUTOMATABLE,
                required=True,
                automatable=False,
                kind="physical",
                reason=reason,
            )

    def _phase1(self) -> None:
        self._swift_tests()
        xcodegen_ok = self._xcodegen()
        self._device(xcodegen_ok)
        selected, selection = self._select()
        ready = selection["status"] == PASSED and self._boot(selected)
        self._simulator_tests(xcodegen_ok, ready)
        self._physical_suite(
            identifier="physical-iphone-phase1-tests",
            name="Physical iPhone Phase 1 interaction suite",
            phase=1,
            source=self.ui_tests_path / "RundaleUITests.swift",
            target="RundaleUITests/RundaleUITests",
        )
        self._physical()

    def _summary(self) -> dict[str, int]:
        counts = {status: 0 for status in (PASSED, FAILED, SKIPPED, UNAVAILABLE, NOT_AUTOMATABLE)}
        for record in self.records:
            counts[record["status"]] += 1
        reused = sum(
            bool(record.get("details", {}).get("cache", {}).get("reused"))
            for record in self.records
        )
        return {
            **counts,
            "reused": reused,
            "blocking": sum(record["blocking"] for record in self.records),
        }

    def _build_identity(self) -> dict[str, Any]:
        head = self._execute(["git", "rev-parse", "HEAD"])
        dirty = self._execute(["git", "status", "--porcelain"])
        diff = self._execute(["git", "diff", "HEAD", "--binary"])
        untracked = self._execute(["git", "ls-files", "--others", "--exclude-standard", "-z"])
        digest = hashlib.sha256()
        if diff.returncode == 0:
            digest.update(diff.stdout.encode())
        if untracked.returncode == 0:
            for relative in untracked.stdout.split("\0"):
                if not relative:
                    continue
                path = self.root / relative
                if path.is_file():
                    digest.update(relative.encode())
                    with suppress(OSError):
                        digest.update(path.read_bytes())
        return {
            "git_head": head.stdout.strip() if head.returncode == 0 else None,
            "git_dirty": bool(dirty.stdout.strip()) if dirty.returncode == 0 else None,
            "source_fingerprint": digest.hexdigest()
            if diff.returncode == 0 and untracked.returncode == 0
            else None,
            "scheme": self.scheme,
            "configuration": self.configuration,
            "device": dict(self.physical_device) if self.physical_device else None,
        }

    def _fast(self) -> None:
        """The pull-request CI lane: Swift style and the three Swift packages.

        It needs no Xcode project, Rust build, or simulator, so a hosted macOS
        runner can run it on every mobile change.
        """

        self._swift_style(phase=1)
        self._swift_tests()
        for package, identifier, name in (
            ("RundaleBridge", "swift-bridge-tests", "RundaleBridge Swift bridge tests"),
            (
                "LimerickEndpointKit",
                "swift-endpoint-kit-tests",
                "LimerickEndpointKit Swift Endpoint tests",
            ),
        ):
            self._swift_package_tests(
                package_path=self.root / "mobile" / package,
                identifier=identifier,
                name=name,
                phase=2,
            )

    def _phases(self, phase: int | None) -> None:
        self._device_preflight()
        if phase is None or phase in IMPLEMENTED_PHASES:
            self._swift_style(phase=1 if phase is None else phase)
        if phase is None or phase == 4:
            # The generated iOS project consumes the Rust XCFramework. Build it
            # before Phase 1 generates/builds that project, then retain the
            # single packaging result when Phase 2 runs its remaining gates.
            self._mobile_rust_packaging()
            self._phase1()
            self._phase2(reuse_phase1_native_setup=True, rust_packaging_already_run=True)
            self._phase3(reuse_native_setup=True)
            self._phase4()
            self._optional_physical_suites()
            if phase is None:
                self._future()
        elif phase == 1:
            self._phase1()
        elif phase == 2:
            self._phase2(reuse_phase1_native_setup=False)
        elif phase == 3:
            self._phase3(reuse_native_setup=False)
        else:
            self._missing(
                f"phase-{phase}-verification",
                f"Phase {phase} verification",
                phase,
                f"Phase {phase} verification is not implemented in this checkout",
                required=True,
                kind="future-phase",
            )

    def run(self, phase: int | None = None, *, fast: bool = False) -> dict[str, Any]:
        if phase in {1, 2, 3} and (self.live_endpoint or self.soak or self.performance):
            raise ValueError("device opt-in suites require --phase 4 or all")
        if fast and (phase is not None or self.physical_device is not None):
            raise ValueError("--fast runs no phase or device suites")
        if fast:
            self._fast()
        else:
            self._phases(phase)
        summary = self._summary()
        report = {
            "schema_version": 1,
            "tool": "rundale-mobile-verify",
            "phase": "fast" if fast else "all" if phase is None else str(phase),
            "implemented_phases": list(IMPLEMENTED_PHASES),
            "started_at": self.started_at,
            "finished_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "build_identity": self._build_identity(),
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
            "cache": {
                "enabled": self.use_cache,
                "content_key": self._content_key_value,
                **(
                    {"disabled_reason": self._content_key_reason}
                    if self._content_key_reason
                    else {}
                ),
            },
            "suites": self.records,
        }
        self._write_reports(report)
        self._store_cached_passes(self.report_dir / "verify.json")
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
        if summary["reused"]:
            lines.append(
                f"{summary['reused']} passed suite(s) reused from earlier runs with identical inputs "
                "(details.cache in the JSON report; --no-cache reruns them)"
            )
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
        prog="just mobile-verify",
        description="Run deterministic Rundale native mobile verification gates.",
    )
    parser.add_argument(
        "--phase",
        type=_phase,
        default=None,
        metavar="1-6|all",
        help="phase to run; default runs all currently implemented phases",
    )
    parser.add_argument(
        "--fast",
        action="store_true",
        help="run only SwiftLint, SwiftFormat, and the Swift package tests (the PR CI lane)",
    )
    parser.add_argument("--project-spec", type=Path)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--scheme", default="Rundale")
    parser.add_argument("--package-path", type=Path)
    parser.add_argument("--ui-tests-path", type=Path)
    parser.add_argument(
        "--simulator",
        default=os.environ.get("RUNDALE_IOS_SIMULATOR"),
        help="specific available simulator UDID or name (or RUNDALE_IOS_SIMULATOR)",
    )
    parser.add_argument(
        "--device",
        "--physical-device",
        dest="device",
        default=os.environ.get("RUNDALE_IOS_DEVICE"),
        help="explicit physical iPhone UDID (or RUNDALE_IOS_DEVICE); simulator remains the default",
    )
    parser.add_argument(
        "--live-endpoint",
        action="store_true",
        help="run the opt-in live Endpoint UI suite on the device",
    )
    parser.add_argument(
        "--soak",
        action="store_true",
        help="run the opt-in long-session reliability suite on the device",
    )
    parser.add_argument(
        "--performance", action="store_true", help="run the opt-in device performance suite"
    )
    parser.add_argument(
        "--development-team",
        default=os.environ.get("RUNDALE_IOS_DEVELOPMENT_TEAM"),
        help="Apple development team for signed device tests (or RUNDALE_IOS_DEVELOPMENT_TEAM)",
    )
    parser.add_argument(
        "--parallel-workers",
        type=_worker_count,
        # A string default goes through `type`, so a bad environment value is
        # reported as a usage error rather than a traceback.
        default=os.environ.get("RUNDALE_PARALLEL_WORKERS", str(DEFAULT_PARALLEL_WORKERS)),
        metavar="N",
        help=(
            "simulator test workers (cloned simulators); 1 runs serially "
            f"(default {DEFAULT_PARALLEL_WORKERS}, or RUNDALE_PARALLEL_WORKERS)"
        ),
    )
    parser.add_argument("--configuration", default="Debug")
    parser.add_argument("--report-dir", type=Path)
    parser.add_argument(
        "--no-cache",
        dest="use_cache",
        action="store_false",
        help="rerun every suite instead of reusing passes recorded for identical inputs",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if (args.live_endpoint or args.soak or args.performance) and not args.device:
        parser.error("--live-endpoint, --soak, and --performance require --device")
    if args.phase in {1, 2, 3} and (args.live_endpoint or args.soak or args.performance):
        parser.error("device opt-in suites require --phase 4 or all")
    if args.fast and (args.phase is not None or args.device):
        parser.error("--fast runs no phase or device suites")
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
        device=args.device,
        live_endpoint=args.live_endpoint,
        soak=args.soak,
        performance=args.performance,
        development_team=args.development_team,
        configuration=args.configuration,
        use_cache=args.use_cache,
        parallel_workers=args.parallel_workers,
    )
    report = run.run(args.phase, fast=args.fast)
    print((run.report_dir / "summary.txt").read_text(encoding="utf-8"), end="")
    return int(report["exit_code"])


if __name__ == "__main__":
    raise SystemExit(main())
