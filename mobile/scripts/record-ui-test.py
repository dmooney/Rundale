#!/usr/bin/env python3
"""Record one XCUITest method while it runs on one iOS simulator.

The recorder is intentionally small and uses only Apple's command line tools.
It builds once with ``build-for-testing`` (unless ``--reuse-build`` is used),
then runs the selected test with ``test-without-building`` while simctl records
the simulator screen.
"""

from __future__ import annotations

import argparse
import json
import queue
import re
import signal
import subprocess
import sys
import threading
import time
from collections.abc import Sequence
from pathlib import Path
from typing import Any


class RecorderError(RuntimeError):
    """An input, simulator, or recording lifecycle error."""


IDENTIFIER = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def _validate_test_identifiers(args: argparse.Namespace) -> None:
    for label in ("target", "class_name", "method"):
        value = str(getattr(args, label, ""))
        if not value or not IDENTIFIER.fullmatch(value):
            raise RecorderError(f"{label.replace('_', ' ')} must be one exact identifier")
    if not str(args.method).startswith("test"):
        raise RecorderError("method must be an XCTest method beginning with 'test'")
    source_dir = Path(args.project).parent / args.target
    swift_files = list(source_dir.glob("*.swift")) if source_dir.is_dir() else []
    if not swift_files:
        raise RecorderError(f"cannot validate XCUITest source in {source_dir}")
    declarations = re.compile(
        r"(?m)^\s*(?:(?:final|private|internal|public|fileprivate)\s+)*(?:class|struct)\s+(\w+)\b"
    )
    method_pattern = re.compile(rf"\bfunc\s+{re.escape(args.method)}\s*\(")
    matches = 0
    for path in swift_files:
        source = path.read_text(encoding="utf-8")
        classes = list(declarations.finditer(source))
        for index, declaration in enumerate(classes):
            if declaration.group(1) != args.class_name:
                continue
            end = classes[index + 1].start() if index + 1 < len(classes) else len(source)
            matches += len(method_pattern.findall(source[declaration.end() : end]))
    if matches != 1:
        raise RecorderError("selected XCUITest method was not found unambiguously in its class")


def _run(command: Sequence[str], *, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)


def _simulators(command_runner=_run) -> list[dict[str, Any]]:
    result = command_runner(["xcrun", "simctl", "list", "devices", "available", "-j"])
    if result.returncode:
        raise RecorderError(result.stderr.strip() or "simctl could not list available devices")
    try:
        devices = json.loads(result.stdout).get("devices", {})
    except (json.JSONDecodeError, AttributeError) as exc:
        raise RecorderError("simctl returned invalid device JSON") from exc
    candidates = []
    for runtime, entries in devices.items():
        for entry in entries if isinstance(entries, list) else []:
            name = str(entry.get("name", ""))
            udid = str(entry.get("udid", ""))
            if not name or not udid or not entry.get("isAvailable", True):
                continue
            identifier = str(entry.get("deviceTypeIdentifier", ""))
            is_iphone = identifier.lower().startswith(
                "com.apple.coresimulator.simdevicetype.iphone"
            )
            if not identifier:
                is_iphone = name.lower().startswith("iphone")
            if not is_iphone:
                continue
            candidates.append(
                {
                    "name": name,
                    "udid": udid,
                    "state": str(entry.get("state", "")),
                    "runtime": runtime,
                }
            )
    return candidates


def resolve_simulator(requested: str, *, command_runner=_run) -> dict[str, Any]:
    candidates = _simulators(command_runner)
    matches = [
        item for item in candidates if item["udid"] == requested or item["name"] == requested
    ]
    if len(matches) != 1:
        if not matches:
            raise RecorderError(
                f"simulator {requested!r} was not found (use an exact UDID or name)"
            )
        raise RecorderError(f"simulator name {requested!r} is ambiguous; use its exact UDID")
    return matches[0]


def boot_simulator(simulator: dict[str, Any], *, command_runner=_run) -> None:
    udid = simulator["udid"]
    if simulator["state"].lower() != "booted":
        result = command_runner(["xcrun", "simctl", "boot", udid])
        if result.returncode:
            raise RecorderError(result.stderr.strip() or f"could not boot simulator {udid}")
    result = command_runner(["xcrun", "simctl", "bootstatus", udid, "-b"])
    if result.returncode:
        raise RecorderError(result.stderr.strip() or f"simulator {udid} did not become ready")


class VideoRecorder:
    def __init__(self, output: Path, *, popen=subprocess.Popen):
        self.output = output
        self._popen = popen
        self.process = None
        self.output_text = ""
        self.reader = None

    def start(self) -> None:
        self.process = self._popen(
            ["xcrun", "simctl", "io", self.udid, "recordVideo", "--codec=h264", str(self.output)],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        lines: queue.Queue[str] = queue.Queue()

        def read_lines() -> None:
            if self.process and self.process.stdout:
                while True:
                    line = self.process.stdout.readline()
                    if not line:
                        break
                    self.output_text += line
                    lines.put(line)

        self.reader = threading.Thread(target=read_lines, daemon=True)
        self.reader.start()
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            try:
                line = lines.get(timeout=max(0.001, min(0.2, deadline - time.monotonic())))
            except queue.Empty:
                if self.process.poll() is not None:
                    break
                continue
            if line:
                if "Recording started" in line:
                    return
            elif self.process.poll() is not None:
                break
        raise RecorderError("simctl recordVideo did not report 'Recording started'")

    def stop(self) -> None:
        if self.process is None:
            return
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGINT)
        try:
            self.process.wait(timeout=30)
        except subprocess.TimeoutExpired as exc:
            self.process.kill()
            self.process.wait(timeout=5)
            raise RecorderError("simctl recordVideo did not finalize after SIGINT") from exc
        if self.reader:
            self.reader.join(timeout=1)


def run(args: argparse.Namespace, *, command_runner=_run, popen=subprocess.Popen) -> int:
    _validate_test_identifiers(args)
    if not str(args.output).strip():
        raise RecorderError("output must be a non-empty file path")
    output = Path(args.output)
    log_path = output.with_suffix(".log")
    xcresult = output.with_suffix(".xcresult")
    if not output.name or output.exists() or log_path.exists() or xcresult.exists():
        raise RecorderError("output, log, and xcresult paths must be new files")
    output.parent.mkdir(parents=True, exist_ok=True)
    derived = Path(args.derived_data)
    simulator = resolve_simulator(args.simulator, command_runner=command_runner)
    boot_simulator(simulator, command_runner=command_runner)
    destination = f"platform=iOS Simulator,id={simulator['udid']}"
    target = f"{args.target}/{args.class_name}/{args.method}"
    common = [
        "xcodebuild",
        "-project",
        args.project,
        "-scheme",
        args.scheme,
        "-configuration",
        args.configuration,
        "-destination",
        destination,
        "-derivedDataPath",
        str(derived),
    ]
    transcript: list[str] = []
    if not args.reuse_build:
        build = command_runner(common + ["build-for-testing"])
        transcript.append(build.stdout + build.stderr)
        if build.returncode:
            log_path.write_text("".join(transcript), encoding="utf-8")
            return build.returncode
    recorder = VideoRecorder(output, popen=popen)
    recorder.udid = simulator["udid"]
    test = None
    recorder_error = None
    try:
        recorder.start()
        test = command_runner(
            common
            + [
                f"-only-testing:{target}",
                "-resultBundlePath",
                str(xcresult),
                "test-without-building",
            ]
        )
        transcript.append(test.stdout + test.stderr)
        if test.returncode == 0:
            try:
                recorder.stop()
            except RecorderError as exc:
                recorder_error = exc
        else:
            try:
                recorder.stop()
            except RecorderError as exc:
                recorder_error = exc
    finally:
        if recorder.process is not None and recorder.process.poll() is None:
            try:
                recorder.stop()
            except RecorderError as exc:
                recorder_error = recorder_error or exc
        transcript.append(recorder.output_text)
        log_path.write_text("".join(transcript), encoding="utf-8")
    if test is not None and test.returncode:
        return test.returncode
    if recorder_error:
        raise recorder_error
    if not output.is_file() or output.stat().st_size == 0:
        raise RecorderError("recording completed without a non-empty video")
    return test.returncode if test is not None else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, help="XCUITest target")
    parser.add_argument("--class", dest="class_name", required=True, help="XCUITest class")
    parser.add_argument("--method", required=True, help="XCUITest method")
    parser.add_argument("--simulator", required=True, help="exact simulator UDID or name")
    parser.add_argument("--output", required=True, help="video output path")
    parser.add_argument("--project", default="mobile/Rundale.xcodeproj")
    parser.add_argument("--scheme", default="Rundale")
    parser.add_argument("--configuration", default="Debug")
    parser.add_argument("--derived-data", default="mobile/.build/record-ui-test/DerivedData")
    parser.add_argument(
        "--reuse-build", action="store_true", help="skip build-for-testing and reuse derived data"
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    try:
        return run(build_parser().parse_args(argv))
    except RecorderError as exc:
        print(f"record-ui-test: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
