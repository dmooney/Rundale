#!/usr/bin/env python3
"""Run deterministic verification gates for the native mobile prototype."""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
import plistlib
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
from collections.abc import Iterator, Mapping, Sequence
from contextlib import ExitStack, contextmanager, suppress
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
)
PHASE3_UI_CLASSES = (
    "RundaleUITests/RundalePhase3UITests",
)
PHASE4_UI_CLASSES = (
    "RundaleUITests/RundalePhase4UITests",
    "RundaleUITests/RundalePhase4RecoveryUITests",
    "RundaleUITests/RundalePhase4AccessibilityUITests",
)
# The hosted pull-request gate (--smoke) runs these core-loop UI tests in place
# of the full UI suites, which take most of an hour on a hosted runner and run
# nightly instead (testflight.yml): launch, a fixture stream and Stop, the engine booting the
# canonical world, travel by place name, and a save surviving termination.
SMOKE_UI_TESTS = (
    "RundaleUITests/RundaleUITests/testLaunchShowsOnlyPrimaryPlayRegions",
    "RundaleUITests/RundaleUITests/testManualStreamCanBeStoppedWithPartialOutputRetained",
    "RundaleUITests/RundalePhase2UITests/testPhase2FoundationBootsCanonicalWorldAndPeig",
    "RundaleUITests/RundalePhase3UITests/testPlaceNameTravelWithoutEndpointInference",
    "RundaleUITests/RundalePhase4UITests/testDraftAndCommittedTravelSurviveBackgroundAndTermination",
)
# Tests whose result depends on the machine's speed, which hosted CI leaves to
# local runs with --skip-time-sensitive (#2287). The list is empty on purpose.
# It grew to 15 entries (#2292) because the hosted simulator suites ran in
# parallel simulator clones on a small shared VM, where fixed waits raced each
# other (#2301); hosted runs now use one worker (HOSTED_PARALLEL_WORKERS), and
# the rules behind most of those tests moved below XCUITest (#2319). Add a test
# here only with evidence that it still fails when run alone on the hosted
# runner, and name the issue that will fix it.
TIME_SENSITIVE_TESTS: tuple[str, ...] = ()
UI_TEST_BUNDLE = "RundaleUITests"
IMPLEMENTED_PHASE = 4
IMPLEMENTED_PHASES = (1, 2, 3, 4)
LAST_PHASE = 7


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
    # The gate scripts' own tests, which CI's pytest runs and no gate reads.
    ":(glob)mobile/scripts/test_*.py",
    "mobile/scripts/fixtures",
    # Read only by the coverage gate, which reapplies it to reused passes.
    "mobile/coverage-baseline.json",
)
PRIVATE_FIREBASE_CONFIG = Path("mobile/Rundale/Resources/GoogleService-Info.plist")
# Measured Swift package line coverage and the allowed drop (#2103).
COVERAGE_BASELINE = Path("mobile/coverage-baseline.json")
# A recorded full pass (verify_record.py) is never re-checked against the
# coverage baseline the way a reused suite is, so the baseline stays in its key.
ATTESTATION_IGNORED_PATHSPECS = (
    *(spec for spec in CACHE_IGNORED_PATHSPECS if spec != COVERAGE_BASELINE.as_posix()),
    # Nested agent instructions explain how to work in a directory; they are
    # not inputs read by the mobile build or verification gates.
    ":(glob)**/AGENTS.md",
    # The verification tools themselves, which their unit tests cover; they
    # change nothing the app ships. release.py stays in: it writes Info.plist
    # settings into the app.
    "mobile/scripts/verify.py",
    "mobile/scripts/verify_record.py",
    "mobile/scripts/record-ui-test.py",
    "mobile/scripts/__init__.py",
)
# Everything a full run builds or reads. A recorded pass covers only these, so
# a merge that changes CI, Endpoints or desktop code leaves it valid. CI's
# `mobile_app` path filter and the TestFlight triggers list the same paths.
ATTESTATION_PATHSPECS = (
    "mobile",
    "mods",
    "limerick/crates",
    "limerick/testing",
    "limerick/Cargo.toml",
    "limerick/Cargo.lock",
    # include_str! in limerick-inference, which the mobile build compiles.
    "limerick/config",
    "rust-toolchain.toml",
    ".cargo",
    # RundaleBugReportTests reads the shared phone report fixture.
    "bug-report/test/fixtures",
)


def attestation_removals() -> list[list[str]]:
    """`git rm --cached` arguments that leave only a recorded pass's inputs."""

    base = ["rm", "-r", "-q", "-f", "--cached", "--ignore-unmatch", "--"]
    return [
        [*base, ".", *(f":(exclude){spec}" for spec in ATTESTATION_PATHSPECS)],
        [*base, *ATTESTATION_IGNORED_PATHSPECS],
    ]


SWIFT_TOOLS_BIN = Path("mobile/.build/swift-tools/bin")
FORBIDDEN_MOBILE_DEPENDENCIES = (
    "limerick-engine",
    "limerick-server",
    "limerick-tauri",
    "limerick-editor",
    "limerick-chronicle",
    "tauri",
    "axum",
)
# Desktop-only features of crates the phone may link. `limerick-diagnostics`
# is portable without its `github` feature (GitHub filing, the `gh` subprocess,
# token lookup); its default enables it (#2022).
FORBIDDEN_MOBILE_FEATURES = (
    ("limerick-core", "desktop"),
    ("limerick-inference", "desktop"),
    ("limerick-diagnostics", "github"),
    ("limerick-diagnostics", "default"),
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


def _stop_group(process: subprocess.Popen[str]) -> tuple[str, str]:
    """Stop a command's whole process group and return its remaining output.

    The group is gone when this returns: a member that ignores SIGTERM gets
    SIGKILL. Ctrl+C and SIGTERM are held off meanwhile, so a second interrupt
    cannot abandon the cleanup halfway.
    """

    blocked = {signal.SIGINT, signal.SIGTERM}
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, blocked) if os.name == "posix" else None
    try:
        if os.name != "posix":
            process.terminate()
            out, err = process.communicate()
            return _text(out), _text(err)
        with suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGTERM)
        drained = True
        try:
            out, err = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            out, err, drained = "", "", False
        # poll() reaps the direct child: a zombie still counts as a member.
        deadline = time.monotonic() + 5
        while process.poll() is None or _group_alive(process.pid):
            if time.monotonic() >= deadline:
                break
            time.sleep(0.05)
        if process.poll() is None or _group_alive(process.pid):
            with suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGKILL)
            while process.poll() is None or _group_alive(process.pid):
                time.sleep(0.05)
        if not drained:
            # Every member has exited, so the pipes are closed: read what the
            # command wrote while it was being stopped.
            more_out, more_err = process.communicate()
            out, err = _text(out) + _text(more_out), _text(err) + _text(more_err)
        return _text(out), _text(err)
    finally:
        if previous is not None:
            signal.pthread_sigmask(signal.SIG_SETMASK, previous)


class SubprocessCommandRunner:
    """Run each command in a process group so a timeout or interrupt cleans up children."""

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
            # The leader may have exited while a descendant keeps the pipes
            # open; stop whatever is left of the group either way.
            if process is not None:
                tail_out, tail_err = _stop_group(process)
                stdout += tail_out
                stderr += tail_err
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
        except BaseException:
            # Ctrl+C or SIGTERM (see main): stop the child's group before the
            # run's report-directory lock is released, so no build outlives it.
            if process is not None:
                _stop_group(process)
            raise
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
    # A killed command's last output rarely says why it stopped; lead with that.
    prefix = f"timed out after {result.duration_seconds:.0f}s: " if result.timed_out else ""
    source = result.stderr.strip() or result.stdout.strip()
    if not source:
        return prefix + fallback
    reason = " | ".join(line.strip() for line in source.splitlines() if line.strip())
    if len(reason) > 600:
        reason = "…" + reason[-597:]
    return prefix + reason


def _swift_errors(output: str, limit: int = 3) -> list[str]:
    """Return the first distinct `error:` diagnostics from swift build or XCTest output."""

    errors: list[str] = []
    for line in output.splitlines():
        line = line.strip()
        if re.search(r"(^|: )error: ", line) and line not in errors:
            errors.append(line)
            if len(errors) == limit:
                break
    return errors


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


def _timeout_minutes(value: str) -> float:
    try:
        minutes = float(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("timeout must be a number of minutes > 0") from exc
    if not minutes > 0:
        raise argparse.ArgumentTypeError("timeout must be a number of minutes > 0")
    return minutes


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
        raise argparse.ArgumentTypeError(f"phase must be 1-{LAST_PHASE} or all") from exc
    if result < 1 or result > LAST_PHASE:
        raise argparse.ArgumentTypeError(f"phase must be 1-{LAST_PHASE} or all")
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


# Measured on an 18-core M5 Pro: one combined run of every simulator suite took
# 448 s with 4 workers and 353 s with 6.
DEFAULT_PARALLEL_WORKERS = 6
# GitHub-hosted runners are small shared VMs. Parallel simulator clones there
# raced the suites' fixed waits and flaked a different test every night (#2301),
# so a hosted run uses one worker, and asking for more is an error rather than
# a silent return to parallel testing. Local runs keep DEFAULT_PARALLEL_WORKERS:
# a developer Mac has the cores for it, and a failure there is not a shared-VM
# timing race.
HOSTED_PARALLEL_WORKERS = 1


def _hosted_runner() -> bool:
    """Whether this process runs on a GitHub-hosted runner."""

    return os.environ.get("GITHUB_ACTIONS") == "true"


def _default_parallel_workers() -> int:
    return HOSTED_PARALLEL_WORKERS if _hosted_runner() else DEFAULT_PARALLEL_WORKERS

# Longest "Suite.test: message" line summary.txt prints per failed test; CI
# prints summary.txt to its log, and verify.json keeps the full message.
FAILURE_MESSAGE_CHARS = 400
# Returned for a simulator suite queued for the combined run; never recorded.
DEFERRED = "deferred"
# Bounds on the evidence kept for failed tests (see `_collect_failure_evidence`).
# Past EVIDENCE_MAX_TESTS failed tests, only the first get a digest; index.md
# says how many more the result bundle holds.
FAILURES_DIR = "failures"
EVIDENCE_MAX_TESTS = 20
EVIDENCE_MAX_FILE_BYTES = 3 * 1024 * 1024
EVIDENCE_MAX_TEST_BYTES = 10 * 1024 * 1024
EVIDENCE_LOG_TAIL_BYTES = 512 * 1024
EVIDENCE_TIMELINE_LINES = 80
# Failed runs whose evidence (and result bundle) survive the next run's prune.
EVIDENCE_KEPT_RUNS = 3


def _xcresult_case_nodes(
    tree: Mapping[str, Any],
) -> list[tuple[str, str, str, Mapping[str, Any]]]:
    """Return (bundle, class, test, node) for every test case node in an xcresult tree."""

    cases: list[tuple[str, str, str, Mapping[str, Any]]] = []

    def walk(node: Mapping[str, Any], bundle: str, suite: str) -> None:
        kind = str(node.get("nodeType", ""))
        name = str(node.get("name", ""))
        if kind.endswith("test bundle"):
            bundle, suite = name, ""
        elif kind == "Test Suite" and not suite:
            suite = name
        elif kind == "Test Case":
            cases.append((bundle, suite, name, node))
            return
        for child in node.get("children") or ():
            if isinstance(child, Mapping):
                walk(child, bundle, suite)

    for node in tree.get("testNodes") or ():
        if isinstance(node, Mapping):
            walk(node, "", "")
    return cases


def _xcresult_cases(tree: Mapping[str, Any]) -> list[tuple[str, str, str, str]]:
    """Return (bundle, class, test, result) for every test case in an xcresult tree."""

    return [
        (bundle, suite, name, str(node.get("result", "")))
        for bundle, suite, name, node in _xcresult_case_nodes(tree)
    ]


def _xcresult_failures(tree: Mapping[str, Any]) -> list[tuple[str, str, str, str]]:
    """Return (bundle, class, test, message) for every failure message of a failed case.

    The messages carry the file, line and assertion text, so a CI log shows why a
    test failed without the result bundle. A case `_case_counts` does not count
    as failed (passed, skipped, or an XCTExpectFailure) contributes none.
    """

    return [
        (bundle, suite, name, message)
        for bundle, suite, name, node in _xcresult_case_nodes(tree)
        if _case_counts([str(node.get("result", ""))])["failedTests"]
        for message in _case_messages(node)
    ]


def _failed_case_nodes(
    tree: Mapping[str, Any],
) -> list[tuple[str, str, str, Mapping[str, Any]]]:
    """Return (bundle, class, test, node) for every case `_case_counts` counts as failed."""

    return [
        case
        for case in _xcresult_case_nodes(tree)
        if _case_counts([str(case[3].get("result", ""))])["failedTests"]
    ]


def _case_messages(node: Mapping[str, Any]) -> list[str]:
    """The `file:line: text` failure messages nested under a test case node."""

    found = [str(node.get("name", ""))] if node.get("nodeType") == "Failure Message" else []
    for child in node.get("children") or ():
        if isinstance(child, Mapping):
            found += _case_messages(child)
    return found


def _evidence_name(text: str, limit: int = 80) -> str:
    """A path-safe name for a test or attachment."""

    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "_", text).strip("._") or "unnamed"
    return cleaned[:limit]


def _activity_timeline(activities: Mapping[str, Any] | None) -> list[str]:
    """Flatten `xcresulttool get test-results activities` into indented timeline lines.

    Each line is `+seconds title`, offset from the first activity; a step Xcode
    ties to a failure ends with `<-- FAILURE`. Only the last
    EVIDENCE_TIMELINE_LINES lines are kept, the lead-up to the failure.
    """

    lines: list[str] = []
    starts: list[float] = []

    def collect(items: Any) -> None:
        for item in items or ():
            if isinstance(item, Mapping):
                if isinstance(item.get("startTime"), (int, float)):
                    starts.append(float(item["startTime"]))
                collect(item.get("childActivities"))

    runs = [r for r in (activities or {}).get("testRuns") or () if isinstance(r, Mapping)]
    for run in runs:
        collect(run.get("activities"))
    origin = min(starts) if starts else 0.0

    def walk(items: Any, depth: int) -> None:
        for item in items or ():
            if not isinstance(item, Mapping):
                continue
            start = item.get("startTime")
            offset = (
                f"+{float(start) - origin:7.2f}s" if isinstance(start, (int, float)) else " " * 9
            )
            mark = "  <-- FAILURE" if item.get("isAssociatedWithFailure") else ""
            title = " ".join(str(item.get("title", "")).split())
            lines.append(f"{offset} {'  ' * depth}{title}{mark}")
            walk(item.get("childActivities"), depth + 1)

    for run in runs:
        walk(run.get("activities"), 0)
    if len(lines) > EVIDENCE_TIMELINE_LINES:
        dropped = len(lines) - EVIDENCE_TIMELINE_LINES
        lines = [f"... {dropped} earlier step(s) omitted ..."] + lines[-EVIDENCE_TIMELINE_LINES:]
    return lines


def _attachment_entries(manifest: Any, test_id: str) -> list[Mapping[str, Any]]:
    """The attachment records `xcresulttool export attachments` listed for one test."""

    entries: list[Mapping[str, Any]] = []
    if isinstance(manifest, list):
        for group in manifest:
            if isinstance(group, Mapping) and group.get("testIdentifier") == test_id:
                entries += [a for a in group.get("attachments") or () if isinstance(a, Mapping)]
    return entries


def _failure_digest(
    *,
    test: str,
    bundle: str,
    messages: Sequence[str],
    details: Mapping[str, Any] | None,
    timeline: Sequence[str],
    saved: Sequence[str],
    skipped: Sequence[str],
    diagnostics: str | None,
    notes: Sequence[str] = (),
) -> str:
    """The short `failure.md` that points a reader at everything kept for one failed test."""

    device = ""
    if details and isinstance(details.get("devices"), list) and details["devices"]:
        first = details["devices"][0]
        if isinstance(first, Mapping):
            device = f"{first.get('deviceName', '')} ({first.get('platform', '')} {first.get('osVersion', '')})"
    out = [f"# Failed: {test}", ""]
    out.append(f"- Result bundle: `{bundle}` (open with Xcode, or `xcrun xcresulttool`)")
    if device.strip(" ()"):
        out.append(f"- Device: {device}")
    if details and details.get("durationInSeconds") is not None:
        out.append(f"- Duration: {float(details['durationInSeconds']):.1f}s")
    out += ["", "## Failure messages", ""]
    out += [f"- {' '.join(m.split())}" for m in messages] or [
        "- (none recorded in the result bundle)"
    ]
    out += ["", "## Timeline before the failure", ""]
    out += ["```", *timeline, "```"] if timeline else ["(activities unavailable)"]
    out += ["", "## Attachments", ""]
    out += [f"- `{name}`" for name in saved] or ["- (none exported)"]
    out += [f"- skipped: {entry}" for entry in skipped]
    if diagnostics:
        out += ["", "## Runner and crash logs", "", f"- `{diagnostics}`"]
    if notes:
        out += ["", "## Notes", ""] + [f"- {note}" for note in notes]
    return "\n".join(out) + "\n"


def _json_output(result: CommandResult) -> Any:
    """The parsed JSON of a successful command, or None."""

    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def _copy_tail(source: Path, target: Path, limit: int) -> None:
    """Copy `source` to `target`, keeping only the last `limit` bytes of a larger file."""

    size = source.stat().st_size
    if size <= limit:
        shutil.copyfile(source, target)
        return
    with source.open("rb") as handle:
        handle.seek(size - limit)
        tail = handle.read()
    target.write_bytes(f"[first {size - limit} bytes omitted]\n".encode() + tail)


def _copy_runner_diagnostics(exported: Path, target: Path) -> list[str]:
    """Copy the test runner's logs and any crash reports out of an exported diagnostics tree.

    `xcresulttool export diagnostics` writes the whole simulator's logs (over
    150 MB). Only the test runner's own output (`<Bundle>-<id>/...` folders),
    testmanagerd's log, and `.ips`/`.crash` reports naming the app, the runner
    or xctest are worth keeping; the rest stays out. Returns the copied names.
    """

    copied: list[str] = []
    for source in sorted(exported.rglob("*")):
        if not source.is_file():
            continue
        parts = source.relative_to(exported).parts
        # `<TestBundle>-<id>` folders: RundaleUITests-..., RundaleTests-...
        runner_log = any(re.match(r"[A-Za-z]+Tests-", part) for part in parts[:-1])
        crash = source.suffix in {".ips", ".crash"} and re.search(
            r"Rundale|xctest|Runner", source.name, re.IGNORECASE
        )
        if not (runner_log or crash or source.name == "testmanagerd.log"):
            continue
        # The file name carries the meaning; a short parent keeps names apart.
        name = _evidence_name(parts[-1], limit=100)
        if len(parts) > 1:
            name = f"{_evidence_name(parts[-2], limit=24)}__{name}"
        target.mkdir(parents=True, exist_ok=True)
        unique, count = name, 2
        while unique in copied:
            unique, count = f"{count}-{name}", count + 1
        _copy_tail(source, target / unique, EVIDENCE_LOG_TAIL_BYTES)
        copied.append(unique)
    return copied


def _evidence_summary(entry: Mapping[str, Any]) -> dict[str, Any]:
    """The part of a failed test's evidence entry that verify.json and summary.txt carry."""

    return {key: entry[key] for key in ("test", "directory", "digest", "files")}


def _selected(case: tuple[str, str, str, str], selectors: Sequence[str]) -> bool:
    """Whether an `-only-testing` selector (bundle, bundle/class or bundle/class/test) names a case."""

    path = (case[0], case[1], case[2].removesuffix("()"))
    for selector in selectors:
        parts = tuple(part.removesuffix("()") for part in selector.split("/"))
        if path[: len(parts)] == parts:
            return True
    return False


def _named_case(test: str) -> tuple[str, str, str, str]:
    """A `bundle/class/test` identifier as an xcresult case, for `_selected`."""

    bundle, cls, name = test.split("/")
    return bundle, cls, name, ""


def _case_counts(results: Sequence[str]) -> dict[str, int]:
    """Count xcresult case results into the summary's buckets; anything unknown fails."""

    passed = sum(result == "Passed" for result in results)
    skipped = sum(result == "Skipped" for result in results)
    expected = sum(result == "Expected Failure" for result in results)
    return {
        "totalTestCount": len(results),
        "passedTests": passed,
        "failedTests": len(results) - passed - skipped - expected,
        "skippedTests": skipped,
        "expectedFailures": expected,
    }


def _result_status(counts: Mapping[str, int], result: Any = None) -> tuple[str, str | None]:
    """Map reconciled xcresult counts to a suite status and reason."""

    total, expected_failures = counts["totalTestCount"], counts["expectedFailures"]
    if total == 0:
        return UNAVAILABLE, "xcresult contains no executed tests"
    if counts["failedTests"]:
        return FAILED, f"xcresult reports {counts['failedTests']} failed test(s)"
    if counts["skippedTests"]:
        return SKIPPED, f"xcresult reports {counts['skippedTests']} skipped test(s)"
    if counts["passedTests"] + expected_failures != total:
        return FAILED, f"xcresult reports {counts['passedTests']} of {total} test(s) passed"
    if result not in (None, "Passed"):
        return FAILED, f"xcresult reports result {result!r}"
    if expected_failures:
        # Passing, but name the known defects so the report never hides them.
        return (
            PASSED,
            f"xcresult reports {expected_failures} expected failure(s) (XCTExpectFailure)",
        )
    return PASSED, None


def _running_command_lines() -> str | None:
    """Every running process's full command line, or None if `ps` fails."""

    try:
        listed = subprocess.run(
            ["ps", "-axww", "-o", "command="], capture_output=True, text=True, timeout=30
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    return listed.stdout if listed.returncode == 0 else None


def _raise_interrupt(signum: int, frame: Any) -> None:
    raise KeyboardInterrupt


def _group_alive(pgid: int) -> bool:
    try:
        os.killpg(pgid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def default_host_lock() -> Path:
    """The lock every non-fast run on this machine shares (`RUNDALE_MOBILE_VERIFY_LOCK`)."""

    override = os.environ.get("RUNDALE_MOBILE_VERIFY_LOCK")
    if override:
        return Path(override)
    caches = Path.home() / ("Library/Caches" if sys.platform == "darwin" else ".cache")
    return caches / "rundale" / "mobile-verify.lock"


# How long a run waits for another worktree's run to release the host lock.
HOST_LOCK_WAIT_SECONDS = 6 * 3600


class HostVerificationBusy(RuntimeError):
    """Another mobile-verify run on this machine kept the host lock past the wait."""

    def __init__(self, lock: Path, waited: float) -> None:
        super().__init__(
            f"another mobile-verify run on this machine held {lock} for {waited:.0f}s; "
            "only one simulator gate runs at a time"
        )


class ReportDirectoryBusy(RuntimeError):
    """Another run holds the report directory's lock."""

    def __init__(self, report_dir: Path) -> None:
        super().__init__(
            f"another mobile-verify run is using {report_dir}; wait for it, or pass "
            "--report-dir for a separate directory"
        )


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
        host_lock: Path | None = None,
        host_lock_wait_seconds: float = HOST_LOCK_WAIT_SECONDS,
        smoke: bool = False,
        skip_time_sensitive: bool = False,
    ) -> None:
        self.root = repo_root.resolve()
        # Every gate but the full UI suites, which SMOKE_UI_TESTS stand in for.
        self.smoke = smoke
        # Leaves TIME_SENSITIVE_TESTS out of the combined simulator run.
        self.skip_time_sensitive = skip_time_sensitive
        # Every worktree's non-fast run (just mobile-verify, release.py, the
        # Dependabot upkeep routine) shares the simulator and the Cargo target.
        self.host_lock = host_lock if host_lock is not None else default_host_lock()
        self.host_lock_wait_seconds = host_lock_wait_seconds
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
            _default_parallel_workers() if parallel_workers is None else parallel_workers
        )
        if self.parallel_workers < 1:
            raise ValueError("--parallel-workers must be at least 1")
        if _hosted_runner() and self.parallel_workers > HOSTED_PARALLEL_WORKERS:
            raise ValueError(
                f"hosted runners run the simulator suites with {HOSTED_PARALLEL_WORKERS} worker, "
                f"not {self.parallel_workers}: parallel simulator clones flaked the nightly "
                "gate (#2301). Unset RUNDALE_PARALLEL_WORKERS / --parallel-workers."
            )
        self.runner = command_runner or SubprocessCommandRunner()
        self.records: list[dict[str, Any]] = []
        self.results: dict[str, CommandResult] = {}
        # Simulator suites a full run queues for one combined xcodebuild.
        self._deferred: list[dict[str, Any]] | None = None
        self.simulator: dict[str, Any] | None = None
        self.physical_device: dict[str, str] | None = (
            {"identifier": device, "source": "override"} if device else None
        )
        self.physical_device_ready = device is None
        self.started_at = dt.datetime.now(dt.timezone.utc).isoformat()
        self.run_stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        self.use_cache = use_cache
        # Failed-test evidence already written, by result bundle.
        self._failure_evidence: dict[str, list[dict[str, Any]]] = {}
        # The index.md kept for each bundle that has failure evidence.
        self._evidence_index: dict[str, str] = {}
        self._evidence_written = False
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

    def _evidence_details(
        self, bundle: str, entries: Sequence[Mapping[str, Any]]
    ) -> dict[str, Any]:
        """The record details that point a failed run at its evidence."""

        return {
            **(
                {
                    "failures": [
                        f"{entry['test']}: {message}"
                        for entry in entries
                        for message in entry["messages"]
                    ],
                    "failure_evidence": [_evidence_summary(entry) for entry in entries],
                }
                if entries
                else {}
            ),
            **(
                {"failure_evidence_index": self._evidence_index[bundle]}
                if bundle in self._evidence_index
                else {}
            ),
        }

    def _collect_failure_evidence(
        self,
        bundle: str,
        tree: Mapping[str, Any] | None = None,
        *,
        invocation_failed: bool = False,
    ) -> list[dict[str, Any]]:
        """Keep the diagnosis of every failed test in `failures/<run>/<test>/`.

        A failure line alone cannot root-cause a flaky UI test, and the result
        bundle is pruned with the run, so the gate reads the bundle before
        anything is thrown away. Per failed case it writes `failure.md` (the
        digest), the exported attachments (failure screenshot, UI hierarchy,
        short screen recording) within size bounds, and once per run the test
        runner's logs and crash reports. Returns one entry per case: test,
        directory, digest, messages. Done once per bundle; later calls reuse it.
        A bundle whose xcodebuild invocation failed (`invocation_failed`) is kept
        and given an index even when it lists no failed case (a runner that
        never started) or cannot be read.
        """

        if bundle in self._failure_evidence and (
            bundle in self._evidence_index or not invocation_failed
        ):
            return self._failure_evidence[bundle]
        entries: list[dict[str, Any]] = []
        self._failure_evidence[bundle] = entries
        bundle_path = self.root / bundle
        if tree is None:
            tree = _json_output(
                self._execute(
                    [
                        "xcrun",
                        "xcresulttool",
                        "get",
                        "test-results",
                        "tests",
                        "--path",
                        bundle,
                        "--compact",
                    ]
                )
            )
        run_dir = self.report_dir / FAILURES_DIR / self.run_stamp
        failed = _failed_case_nodes(tree) if isinstance(tree, Mapping) else []
        if not failed and isinstance(tree, Mapping) and not invocation_failed:
            return entries
        if not failed:
            if bundle_path.exists():
                # The tests could not be listed, but the bundle is still the
                # only evidence: mark the run so the next prune keeps it, and
                # keep the runner logs.
                run_dir.mkdir(parents=True, exist_ok=True)
                self._evidence_written = True
                diagnostics = self._export_runner_diagnostics(bundle, run_dir)
                reason = (
                    "xcresulttool listed no failed test case"
                    if isinstance(tree, Mapping)
                    else "xcresulttool could not list the tests"
                )
                (run_dir / "index.md").write_text(
                    f"# Failed run {self.run_stamp}\n\n"
                    f"{reason} in `{bundle}`; open it in Xcode.\n"
                    + (f"Runner and crash logs: `{diagnostics}`\n" if diagnostics else ""),
                    encoding="utf-8",
                )
                self._evidence_index[bundle] = _relative(run_dir / "index.md", self.root)
            return entries
        assert isinstance(tree, Mapping)
        run_dir.mkdir(parents=True, exist_ok=True)
        self._evidence_written = True
        diagnostics = self._export_runner_diagnostics(bundle, run_dir)
        for case_bundle, suite, name, node in failed[:EVIDENCE_MAX_TESTS]:
            test_id = str(node.get("nodeIdentifier") or f"{suite}/{name}")
            directory = run_dir / _evidence_name(f"{suite}.{name.removesuffix('()')}", limit=120)
            suffix = 2
            while directory.exists():
                directory = directory.with_name(f"{directory.name}-{suffix}")
                suffix += 1
            directory.mkdir(parents=True)
            messages = _case_messages(node)
            notes: list[str] = []
            base = ["xcrun", "xcresulttool", "get", "test-results"]
            details = _json_output(
                self._execute(
                    [*base, "test-details", "--path", bundle, "--test-id", test_id, "--compact"]
                )
            )
            activities = _json_output(
                self._execute(
                    [*base, "activities", "--path", bundle, "--test-id", test_id, "--compact"]
                )
            )
            if activities is None:
                notes.append("xcresulttool could not read the activity timeline")
            saved, skipped = self._export_attachments(bundle, test_id, directory, notes)
            (directory / "failure.md").write_text(
                _failure_digest(
                    test=f"{case_bundle}/{suite}/{name}",
                    bundle=_relative(bundle_path, self.root),
                    messages=messages,
                    details=details if isinstance(details, Mapping) else None,
                    timeline=_activity_timeline(activities),
                    saved=saved,
                    skipped=skipped,
                    diagnostics=diagnostics,
                    notes=notes,
                ),
                encoding="utf-8",
            )
            entries.append(
                {
                    "case": (case_bundle, suite, name, ""),
                    "test": f"{suite}.{name}",
                    "directory": _relative(directory, self.root),
                    "digest": _relative(directory / "failure.md", self.root),
                    "files": saved,
                    "messages": messages,
                }
            )
        index = [f"# Failed tests of run {self.run_stamp}", ""]
        index += [f"- {e['test']}: `{e['digest']}`" for e in entries]
        if len(failed) > len(entries):
            index.append(f"- ... {len(failed) - len(entries)} more failed test(s); see `{bundle}`")
        (run_dir / "index.md").write_text("\n".join(index) + "\n", encoding="utf-8")
        self._evidence_index[bundle] = _relative(run_dir / "index.md", self.root)
        return entries

    def _export_runner_diagnostics(self, bundle: str, run_dir: Path) -> str | None:
        """Copy the runner logs and crash reports of `bundle` into `run_dir/diagnostics`."""

        with tempfile.TemporaryDirectory(prefix="xcresult-diagnostics-") as scratch:
            result = self._execute(
                [
                    "xcrun",
                    "xcresulttool",
                    "export",
                    "diagnostics",
                    "--path",
                    bundle,
                    "--output-path",
                    scratch,
                ]
            )
            if result.returncode != 0:
                return None
            copied = _copy_runner_diagnostics(Path(scratch), run_dir / "diagnostics")
        return _relative(run_dir / "diagnostics", self.root) if copied else None

    def _export_attachments(
        self, bundle: str, test_id: str, directory: Path, notes: list[str]
    ) -> tuple[list[str], list[str]]:
        """Export one test's attachments into `directory`; return (saved, skipped) names."""

        saved: list[str] = []
        skipped: list[str] = []
        with tempfile.TemporaryDirectory(prefix="xcresult-attachments-") as scratch:
            result = self._execute(
                [
                    "xcrun",
                    "xcresulttool",
                    "export",
                    "attachments",
                    "--path",
                    bundle,
                    "--test-id",
                    test_id,
                    "--output-path",
                    scratch,
                ]
            )
            manifest_path = Path(scratch) / "manifest.json"
            try:
                manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                notes.append(
                    "xcresulttool exported no attachments"
                    + (
                        ""
                        if result.returncode == 0
                        else f": {_failure_reason(result, 'export failed')}"
                    )
                )
                return saved, skipped
            total = 0
            entries = sorted(
                _attachment_entries(manifest, test_id), key=lambda a: a.get("timestamp") or 0
            )
            for entry in entries:
                exported = Path(scratch) / str(entry.get("exportedFileName", ""))
                human = re.sub(
                    r"_\d+_[0-9A-Fa-f-]{36}", "", str(entry.get("suggestedHumanReadableName", ""))
                )
                stem = _evidence_name(Path(human).stem if Path(human).suffix else human)
                name = stem + (exported.suffix or ".bin")
                if not exported.is_file():
                    continue
                size = exported.stat().st_size
                if size > EVIDENCE_MAX_FILE_BYTES or total + size > EVIDENCE_MAX_TEST_BYTES:
                    skipped.append(f"{name} ({size} bytes, over the evidence size bound)")
                    continue
                target, count = directory / name, 2
                while target.exists():
                    target = directory / f"{Path(name).stem}-{count}{Path(name).suffix}"
                    count += 1
                shutil.copyfile(exported, target)
                total += size
                saved.append(target.name)
        return saved, skipped

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
        if self.smoke:
            # CI's Linux Swift lane (`--fast`) runs every package suite on each
            # pull request; the full gate runs them against Apple's Foundation.
            self._skip(identifier, name, phase, "--smoke leaves Swift packages to --fast")
            return
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
        if record["status"] == FAILED:
            # Name the failing test or compiler error, not the build log tail.
            errors = _swift_errors(self.results[identifier].output)
            if errors:
                self._change(record, FAILED, " | ".join(errors))
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

    def _validate_rust_result(self, test_record: dict[str, Any]) -> None:
        """Require cargo's test harness to report a non-empty test result.

        An ignored test makes the suite a skip, so a newly ignored test blocks.
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
        unexpected = list(ignored_names)
        if len(ignored_names) != ignored:
            unexpected.append(f"{ignored - len(ignored_names)} unnamed")
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
            self._validate_rust_result(record)

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
            "normal,features",
        ]
        record = self._run(
            identifier="mobile-dependency-graph",
            name="Portable mobile dependency graph",
            phase=2,
            command=command,
            env=self._rust_env(),
            details={
                "forbidden_dependencies": list(FORBIDDEN_MOBILE_DEPENDENCIES),
                "forbidden_features": [f"{c}/{f}" for c, f in FORBIDDEN_MOBILE_FEATURES],
            },
        )
        if record["status"] != PASSED:
            return
        output = self.results[record["id"]].output
        found = sorted(
            [
                dependency
                for dependency in FORBIDDEN_MOBILE_DEPENDENCIES
                if re.search(rf"\b{re.escape(dependency)}\b", output)
            ]
            + [
                f"{crate}/{feature}"
                for crate, feature in FORBIDDEN_MOBILE_FEATURES
                if re.search(rf"\b{re.escape(crate)} feature \"{re.escape(feature)}\"", output)
            ]
        )
        if not output.strip():
            self._change(record, FAILED, "cargo tree produced no dependency graph")
        elif found:
            self._change(
                record,
                FAILED,
                f"portable dependency graph contains forbidden crate(s) or feature(s): {', '.join(found)}",
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
        summary_identifier: str | None = None,
        smoke_ui: bool = False,
    ) -> dict[str, Any]:
        is_physical = destination is not None and destination.startswith("platform=iOS,id=")
        if self.smoke and destination is not None and not is_physical and not smoke_ui:
            # --smoke keeps a suite's unit test classes and leaves its UI
            # classes to the full run; SMOKE_UI_TESTS run as their own suite.
            requested = [only_testing] if isinstance(only_testing, str) else only_testing or []
            kept = [s for s in requested if s.split("/")[0] != UI_TEST_BUNDLE]
            identifier = identifier or "ios-simulator-tests"
            name = name or "iOS simulator XCTest/XCUITest suite"
            if not kept:
                return self._skip(
                    identifier, name, phase, "--smoke leaves UI suites to the full run"
                )
            if len(kept) < len(requested):
                name = f"{name} (unit test classes; --smoke)"
            only_testing = kept
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
        if destination is not None and not is_physical:
            if self.parallel_workers > 1:
                # Simulator suites clone the destination simulator per worker.
                # Every UI test launches the app with its own launch arguments
                # and a fresh app container, so classes share no state.
                command += [
                    "-parallel-testing-enabled",
                    "YES",
                    "-parallel-testing-worker-count",
                    str(self.parallel_workers),
                ]
            else:
                # Say so rather than rely on the scheme: one worker is serial.
                command += ["-parallel-testing-enabled", "NO"]
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
                    **(
                        {"skip_time_sensitive": True}
                        if self.skip_time_sensitive and destination is not None
                        else {}
                    ),
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
        if self._deferred is not None and destination is not None and not is_physical:
            self._deferred.append(
                {
                    "identifier": identifier,
                    "name": name,
                    "phase": phase,
                    "selectors": [only_testing]
                    if isinstance(only_testing, str)
                    else list(only_testing or []),
                    "skip_testing": list(skip_testing),
                    "summary_identifier": summary_identifier or f"{identifier}-results",
                    "command": command,
                    "destination": destination,
                    "derived": derived,
                    "key": key,
                }
            )
            return {"id": identifier, "status": DEFERRED}
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
        elif destination is not None and not is_physical and (self.root / bundle).exists():
            # xcodebuild failed: its tests may have failed, and nothing else
            # validates this bundle, so read the evidence now.
            evidence = self._collect_failure_evidence(bundle, invocation_failed=True)
            record.setdefault("details", {}).update(self._evidence_details(bundle, evidence))
        return record

    def _run_deferred_simulator_suites(self) -> None:
        """Run the queued simulator suites as one xcodebuild and record each suite.

        Separate runs each clone and boot their own simulators and wait on their
        slowest test class; one run spreads every class across the workers.
        Each suite still passes or fails on its own test cases.
        """

        pending, self._deferred = self._deferred or [], None
        if not pending:
            return
        if any(spec["skip_testing"] for spec in pending):
            raise ValueError("a combined simulator run cannot skip tests per suite")
        selectors: list[str] = []
        for spec in pending:
            selectors += [s for s in spec["selectors"] if s not in selectors]
        bundle = self._bundle("simulator-combined")
        command: list[str] = []
        parts = iter(pending[0]["command"])
        for part in parts:
            if part.startswith("-only-testing:"):
                continue
            command.append(part)
            if part == "-resultBundlePath":
                next(parts)
                command.append(bundle)
        skipped = [
            test
            for test in TIME_SENSITIVE_TESTS
            if self.skip_time_sensitive and _selected(_named_case(test), selectors)
        ]
        at = command.index("-derivedDataPath")
        command[at:at] = [f"-only-testing:{selector}" for selector in selectors] + [
            f"-skip-testing:{test}" for test in skipped
        ]
        if skipped:
            self._record(
                identifier="time-sensitive-tests",
                name="Time-sensitive simulator tests",
                phase=pending[0]["phase"],
                status=SKIPPED,
                required=False,
                kind="automated",
                reason="--skip-time-sensitive leaves these to local runs",
                details={"tests": skipped},
            )
        run = self._run(
            identifier="ios-simulator-combined-run",
            name="Combined iOS simulator test run",
            phase=pending[0]["phase"],
            command=command,
            required=False,
            kind="infrastructure",
            env=self._xcode_env(),
            details={
                "suites": [spec["identifier"] for spec in pending],
                "destination": pending[0]["destination"],
                "derived_data": pending[0]["derived"],
                "result_bundle": bundle,
            },
        )
        tree = self._execute(
            ["xcrun", "xcresulttool", "get", "test-results", "tests", "--path", bundle, "--compact"]
        )
        cases: list[tuple[str, str, str, str]] | None = None
        failures: list[tuple[str, str, str, str]] = []
        evidence: list[dict[str, Any]] = []
        unreadable = _failure_reason(tree, "xcresulttool could not read the combined result bundle")
        if tree.returncode == 0:
            try:
                parsed = json.loads(tree.stdout)
            except json.JSONDecodeError as exc:
                unreadable = f"xcresulttool returned invalid JSON: {exc}"
            else:
                if isinstance(parsed, Mapping):
                    cases = _xcresult_cases(parsed)
                    failures = _xcresult_failures(parsed)
                    # Read the failed tests' evidence before anything else can discard it.
                    evidence = self._collect_failure_evidence(
                        bundle, parsed, invocation_failed=run["status"] != PASSED
                    )
                else:
                    unreadable = "xcresulttool returned a JSON value that is not an object"
        if cases is None:
            # Unreadable results: still keep the bundle and the runner logs.
            self._collect_failure_evidence(bundle, invocation_failed=run["status"] != PASSED)
        app_identity = (
            self._built_app_identity(self.root / pending[0]["derived"])
            if run["status"] == PASSED
            else {}
        )
        for spec in pending:
            counts: dict[str, int] | None = None
            reason: str | None
            if cases is None:
                status, reason = FAILED, unreadable
            else:
                counts = _case_counts([c[3] for c in cases if _selected(c, spec["selectors"])])
                status, reason = _result_status(counts)
                # A selector naming one test (bundle/class/test) must match a
                # case it ran: a renamed or deleted test would otherwise drop
                # out of the suite silently.
                missing = [
                    selector
                    for selector in spec["selectors"]
                    if selector.count("/") == 2 and not any(_selected(c, [selector]) for c in cases)
                ]
                if status == PASSED and missing:
                    status, reason = FAILED, f"selected test(s) did not run: {', '.join(missing)}"
                if status == PASSED and run["status"] != PASSED:
                    status, reason = FAILED, f"the combined run failed: {run.get('reason')}"
            messages = [
                f"{failure[1]}.{failure[2]}: {failure[3]}"
                for failure in failures
                if _selected(failure, spec["selectors"])
            ]
            if status != PASSED:
                # Whatever blocked the suite (an empty tree, a test that never
                # ran), keep the bundle for the next prune.
                self._collect_failure_evidence(bundle, invocation_failed=True)
            suite_evidence = [
                _evidence_summary(entry)
                for entry in evidence
                if _selected(entry["case"], spec["selectors"])
            ]
            details: dict[str, Any] = {
                "scheme": self.scheme,
                "destination": spec["destination"],
                "target": "simulator",
                "derived_data": spec["derived"],
                "result_bundle": bundle,
                "combined_run": "ios-simulator-combined-run",
                **(counts or {}),
                **({"failures": messages} if messages else {}),
                **({"failure_evidence": suite_evidence} if suite_evidence else {}),
                **(
                    {"failure_evidence_index": self._evidence_index[bundle]}
                    if status != PASSED and bundle in self._evidence_index
                    else {}
                ),
                **({"cache_key": spec["key"]} if spec["key"] else {}),
                **(app_identity if status == PASSED else {}),
            }
            self._record(
                identifier=spec["identifier"],
                name=spec["name"],
                phase=spec["phase"],
                status=status,
                required=True,
                reason=reason,
                details=details,
            )
            self._record(
                identifier=spec["summary_identifier"],
                name="iOS simulator test-result validation",
                phase=spec["phase"],
                status=status,
                required=True,
                reason=reason,
                details={"result_bundle": bundle, **(counts or {})},
            )

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
        self._check_result(test_record, phase=phase, summary_identifier=summary_identifier)
        bundle = str(test_record.get("details", {}).get("result_bundle", ""))
        if (
            test_record["status"] != PASSED
            and bundle
            and bundle not in self._evidence_index
            and (self.root / bundle).exists()
        ):
            # Validation failed before it could name a failed test (unreadable
            # or malformed results): the bundle is still the evidence to keep.
            evidence = self._collect_failure_evidence(bundle, invocation_failed=True)
            self._change(
                test_record,
                test_record["status"],
                None,
                self._evidence_details(bundle, evidence),
            )

    def _check_result(
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
        result_status, result_reason = _result_status(counts, summary.get("result"))
        test_details: dict[str, Any] = dict(counts)
        if counts["failedTests"]:
            # Read the failed tests' evidence before anything else can discard it.
            evidence = self._collect_failure_evidence(bundle)
            test_details = {**counts, **self._evidence_details(bundle, evidence)}
        self._change(test_record, result_status, result_reason, test_details)
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
            summary_identifier="ios-simulator-test-results",
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
                "RundaleUITests/RundaleCommandsUITests",
                "RundaleTests/EngineScenarioTests",
                "RundaleTests/RundaleEngineLifecycleTests",
                "RundaleTests/RundaleBugReportTests",
            ],
            summary_identifier="phase2-ios-simulator-test-results",
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
            summary_identifier="phase3-ios-simulator-test-results",
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
        # The other Rust crates' tests build for the host, not iOS, so CI runs
        # them on Linux (full-ci.yml). limerick-persistence stays: its save
        # paths and kernel save lock have Darwin-only code and semantics.
        self._cargo_test(
            identifier="limerick-persistence-tests",
            name="Limerick save format, journal, and kernel save-lock tests",
            package="limerick-persistence",
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
        self._swift_package_tests(
            package_path=self.root / "mobile" / "RundaleSession",
            identifier="swift-session-tests",
            name="RundaleSession Swift session tests",
            phase=2,
        )
        self._phase2_simulator_tests(xcodegen_ok, ready)
        self._phase2_live_endpoint()
        self._phase2_physical()

    def _phase3(self, *, reuse_native_setup: bool) -> None:
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
                summary_identifier=f"{identifier}-results",
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

    def _smoke_simulator_tests(self) -> None:
        identifier, name = "ios-simulator-smoke-tests", "Core-loop UI smoke tests (--smoke)"
        xcodegen_ok = any(
            record["id"] == "xcodegen" and record["status"] == PASSED for record in self.records
        )
        ready = self.simulator is not None and any(
            (record["id"] == "simulator-bootstatus" and record["status"] == PASSED)
            or (
                record["id"] == "simulator-boot"
                and record["status"] == SKIPPED
                and record["kind"] == "infrastructure"
            )
            for record in self.records
        )
        if not xcodegen_ok or not ready or self.simulator is None:
            self._skip(
                identifier,
                name,
                1,
                "blocked because the native project or simulator is not ready",
                required=True,
            )
            return
        record = self._xcodebuild(
            "smoke-simulator",
            f"platform=iOS Simulator,id={self.simulator['udid']}",
            identifier=identifier,
            name=name,
            only_testing=SMOKE_UI_TESTS,
            summary_identifier=f"{identifier}-results",
            smoke_ui=True,
        )
        if record["status"] == PASSED:
            self._validate_result(record, phase=1, summary_identifier=f"{identifier}-results")

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
        """Swift style and the four Swift packages: CI's Linux Swift lane.

        It needs no Xcode project, Rust build, or simulator, and runs on Linux
        as on macOS. The phase gates leave out the style checks, and --smoke
        the packages too; the full gate keeps the packages for Apple's
        Foundation.
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
            ("RundaleSession", "swift-session-tests", "RundaleSession Swift session tests"),
        ):
            self._swift_package_tests(
                package_path=self.root / "mobile" / package,
                identifier=identifier,
                name=name,
                phase=2,
            )

    def _phases(self, phase: int | None) -> None:
        self._device_preflight()
        # SwiftLint and SwiftFormat check source text, so they run only in
        # --fast (CI's Linux Swift lane), not on a Mac.
        if phase is None or phase == 4:
            # The generated iOS project consumes the Rust XCFramework. Build it
            # before Phase 1 generates/builds that project, then retain the
            # single packaging result when Phase 2 runs its remaining gates.
            self._mobile_rust_packaging()
            # Every simulator suite runs in one xcodebuild after Phase 4 queues
            # its own, instead of one xcodebuild per suite.
            self._deferred = []
            self._phase1()
            self._phase2(reuse_phase1_native_setup=True, rust_packaging_already_run=True)
            self._phase3(reuse_native_setup=True)
            self._phase4()
            if self.smoke:
                self._smoke_simulator_tests()
            self._run_deferred_simulator_suites()
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
        if self.smoke and (fast or phase is not None or self.physical_device is not None):
            raise ValueError("--smoke runs every phase on the simulator, without --fast")
        if self.skip_time_sensitive and (
            fast or phase not in {None, 4} or self.physical_device is not None
        ):
            raise ValueError(
                "--skip-time-sensitive applies to the simulator run of --phase 4 or all, without --device"
            )
        self.report_dir.mkdir(parents=True, exist_ok=True)
        with ExitStack() as held:
            if not fast:  # --fast runs no simulator suite and builds no Rust
                held.enter_context(self._host_lock())
            lock = held.enter_context((self.report_dir / ".run.lock").open("w"))
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise ReportDirectoryBusy(self.report_dir) from None
            # Held until the report is written: pruning must never remove what
            # another run sharing this directory still builds into.
            return self._locked_run(phase, fast=fast)

    @contextmanager
    def _host_lock(self) -> Iterator[None]:
        """Wait, up to the bound, until no other run on this machine holds the host lock."""

        self.host_lock.parent.mkdir(parents=True, exist_ok=True)
        started = time.monotonic()
        with self.host_lock.open("w") as lock:
            announced = False
            while True:
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    waited = time.monotonic() - started
                    if waited >= self.host_lock_wait_seconds:
                        raise HostVerificationBusy(self.host_lock, waited) from None
                    if not announced:
                        print(
                            f"mobile-verify: waiting for another run on this machine ({self.host_lock})",
                            file=sys.stderr,
                            flush=True,
                        )
                        announced = True
                    time.sleep(min(5.0, max(0.05, self.host_lock_wait_seconds - waited)))
            yield

    def _locked_run(self, phase: int | None, *, fast: bool) -> dict[str, Any]:
        self._prune_previous_runs()
        if fast:
            self._fast()
        else:
            self._phases(phase)
        summary = self._summary()
        report = {
            "schema_version": 1,
            "tool": "rundale-mobile-verify",
            "phase": "fast"
            if fast
            else "smoke"
            if self.smoke
            else "all"
            if phase is None
            else str(phase),
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
        # A smoke pass leaves the UI suites untested, so it never stands in
        # for a full pass.
        if phase is None and not fast and not self.smoke and report["status"] == PASSED:
            self._store_full_pass(report)
        return report

    def _attestation_tree(self) -> str | None:
        """Return the tested working tree's ATTESTATION_PATHSPECS, less the ignored paths."""

        index_path = self._execute(["git", "rev-parse", "--git-path", "index"])
        if index_path.returncode != 0 or not index_path.stdout.strip():
            return None
        source_index = self.root / index_path.stdout.strip()
        with tempfile.TemporaryDirectory(prefix="rundale-verify-attest-") as scratch:
            temporary_index = Path(scratch) / "index"
            if source_index.is_file():
                shutil.copyfile(source_index, temporary_index)
            env = {**os.environ, "GIT_INDEX_FILE": str(temporary_index)}
            for step in (
                ["git", "add", "-A", "--", "."],
                *(["git", *removal] for removal in attestation_removals()),
            ):
                if self._execute(step, env=env).returncode != 0:
                    return None
            tree = self._execute(["git", "write-tree"], env=env)
        tree_id = tree.stdout.strip()
        if tree.returncode != 0 or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", tree_id):
            return None
        return tree_id

    def _store_full_pass(self, report: Mapping[str, Any]) -> None:
        """Note a full pass by input tree, for `verify_record.py record` to post."""

        tree = self._attestation_tree()
        if tree is None:
            return
        passes = self.root / "mobile" / ".verification" / "passes"
        passes.mkdir(parents=True, exist_ok=True)
        (passes / f"{tree}.json").write_text(
            json.dumps(
                {
                    "input_tree": tree,
                    "content_key": self._content_key_value,
                    "skipped_time_sensitive": self.skip_time_sensitive,
                    "started_at": report["started_at"],
                    "finished_at": report["finished_at"],
                    "summary": report["summary"],
                    "report": report["reports"]["json"],
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )

    def _prune_previous_runs(self) -> None:
        """Delete earlier runs' build products and result bundles, unless in use.

        Each run writes Xcode DerivedData, result bundles and Swift scratch
        directories under its own timestamp, and nothing reads an earlier
        run's: reused passes come from the cache. Kept, they filled the disk.
        A run killed outright (SIGKILL) can leave its xcodebuild or swift build
        running; every such command carries its run's timestamp in its
        arguments, so a timestamp that appears in a running process's command
        line is kept, however many runs later.
        """

        stamp = re.compile(r"(\d{8}T\d{12}Z)")
        # A failed run's result bundle is the evidence behind its failures/
        # folder: keep the newest few such runs whole, drop the older ones.
        failed_runs = sorted(
            (
                entry
                for entry in (self.report_dir / FAILURES_DIR).glob("*")
                if entry.is_dir() and stamp.fullmatch(entry.name)
            ),
            key=lambda entry: entry.name,
        )
        kept_failed = {entry.name for entry in failed_runs[-EVIDENCE_KEPT_RUNS:]}
        for entry in failed_runs[:-EVIDENCE_KEPT_RUNS]:
            shutil.rmtree(entry, ignore_errors=True)
        entries: dict[str, list[Path]] = {}
        for name in ("DerivedData", "xcresults", "swift-scratch", "device"):
            folder = self.report_dir / name
            for entry in folder.iterdir() if folder.is_dir() else ():
                match = stamp.search(entry.name)
                if match:
                    entries.setdefault(match.group(1), []).append(entry)
        if not entries:
            return
        in_use = _running_command_lines()
        if in_use is None:
            return  # without a process list, nothing is known to be free
        for run, paths in entries.items():
            if run in in_use:
                continue
            for entry in paths:
                # A kept failed run keeps only its result bundle, not its build products.
                if run in kept_failed and entry.parent.name == "xcresults":
                    continue
                if entry.is_dir():
                    shutil.rmtree(entry, ignore_errors=True)
                else:
                    entry.unlink(missing_ok=True)

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
        if self._evidence_written:
            lines += [
                "",
                f"FAILURE EVIDENCE: {FAILURES_DIR}/{self.run_stamp}/index.md under the report "
                f"directory ({_relative(self.report_dir / FAILURES_DIR / self.run_stamp, self.root)}).",
                "Read each failure.md, its screenshot and UI hierarchy, and the .xcresult, and "
                "diagnose the failure before re-running or calling a test flaky.",
                "",
            ]
        for r in self.records:
            if not (r["blocking"] or r["status"] == FAILED):
                continue
            lines.append(
                f"- {r['status'].upper()}: {r['name']}"
                + (f" — {r['reason']}" if r.get("reason") else "")
            )
            if index := r.get("details", {}).get("failure_evidence_index"):
                lines.append(f"    evidence index: {index}")
            for kept in r.get("details", {}).get("failure_evidence", ()):
                lines.append(
                    f"    evidence: {kept['digest']} ({', '.join(kept['files']) or 'no attachments'})"
                )
            for failure in r.get("details", {}).get("failures", ()):
                # One log line each; verify.json keeps the full text.
                line = " ".join(failure.split())
                if len(line) > FAILURE_MESSAGE_CHARS:
                    line = line[: FAILURE_MESSAGE_CHARS - 1] + "…"
                lines.append(f"    {line}")
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
        metavar=f"1-{LAST_PHASE}|all",
        help="phase to run; default runs all currently implemented phases",
    )
    parser.add_argument(
        "--fast",
        action="store_true",
        help="run only SwiftLint, SwiftFormat, and the Swift package tests (the PR CI lane)",
    )
    parser.add_argument(
        "--smoke",
        action="store_true",
        help=(
            "run every gate but the full UI suites and the Swift packages (--fast "
            "runs those), plus a few core-loop UI tests (the hosted PR gate); "
            "never records a full pass"
        ),
    )
    parser.add_argument(
        "--skip-time-sensitive",
        action="store_true",
        help=(
            "leave the tests whose result depends on machine speed "
            "(TIME_SENSITIVE_TESTS) to local runs; for hosted CI only"
        ),
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
        default=os.environ.get("RUNDALE_PARALLEL_WORKERS", str(_default_parallel_workers())),
        metavar="N",
        help=(
            "simulator test workers (cloned simulators); 1 runs serially "
            f"(default {DEFAULT_PARALLEL_WORKERS}, or {HOSTED_PARALLEL_WORKERS} on a "
            "GitHub-hosted runner, where more is an error; or RUNDALE_PARALLEL_WORKERS)"
        ),
    )
    parser.add_argument(
        "--command-timeout",
        type=_timeout_minutes,
        default=os.environ.get("RUNDALE_COMMAND_TIMEOUT_MINUTES", "30"),
        metavar="MINUTES",
        help=(
            "stop any single build or test command after this long "
            "(default 30, or RUNDALE_COMMAND_TIMEOUT_MINUTES)"
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
    if args.smoke and (args.fast or args.phase is not None or args.device):
        parser.error("--smoke runs every phase on the simulator, without --fast")
    if args.skip_time_sensitive and (args.fast or args.phase not in {None, 4} or args.device):
        parser.error(
            "--skip-time-sensitive applies to the simulator run of --phase 4 or all, without --device"
        )
    if _hosted_runner() and args.parallel_workers > HOSTED_PARALLEL_WORKERS:
        parser.error(
            f"a hosted runner runs the simulator suites with {HOSTED_PARALLEL_WORKERS} worker, "
            f"not {args.parallel_workers} (#2301); unset RUNDALE_PARALLEL_WORKERS"
        )
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
        command_timeout_seconds=args.command_timeout * 60,
        smoke=args.smoke,
        skip_time_sensitive=args.skip_time_sensitive,
    )
    # SIGTERM unwinds like Ctrl+C, so the runner stops its child's process
    # group before the report-directory lock is released.
    signal.signal(signal.SIGTERM, _raise_interrupt)
    try:
        report = run.run(args.phase, fast=args.fast)
    except (ReportDirectoryBusy, HostVerificationBusy) as exc:
        print(f"mobile-verify: {exc}", file=sys.stderr)
        return 2
    print((run.report_dir / "summary.txt").read_text(encoding="utf-8"), end="")
    return int(report["exit_code"])


if __name__ == "__main__":
    raise SystemExit(main())
