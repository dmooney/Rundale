#!/usr/bin/env python3
"""Build and request an internal TestFlight upload for the native client.

This is intentionally a small stdlib-only orchestration layer.  It keeps the
Firebase configuration private, writes receipts under ignored build output, and
never treats an upload request as proof that Apple has processed the build.
"""

from __future__ import annotations

import argparse
import json
import plistlib
import re
import shlex
import shutil
import subprocess
import sys
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Sequence

TEAM_ID = "MBPRPZ283R"
SCHEME = "Rundale"
BUNDLE_ID = "com.rundale.mobile"
ENDPOINT_SETTINGS = {
    "RUNDALE_ENDPOINT_BASE_URL": "https://parish-server-24861210203.us-east1.run.app",
    "RUNDALE_ENDPOINT_ORGANIZATION": "parish-demo",
    "RUNDALE_ENDPOINT_SLUG": "rundale-dialogue",
    "RUNDALE_ENDPOINT_VERSION": "1",
}


@dataclass(frozen=True)
class Paths:
    root: Path

    @property
    def mobile(self) -> Path:
        return self.root / "mobile"

    @property
    def output(self) -> Path:
        return self.mobile / ".build" / "release"

    @property
    def archive(self) -> Path:
        return self.output / "Rundale.xcarchive"

    @property
    def app(self) -> Path:
        return self.archive / "Products" / "Applications" / "Rundale.app"

    @property
    def export(self) -> Path:
        return self.output / "testflight-export"

    @property
    def firebase(self) -> Path:
        return self.mobile / "Rundale" / "Resources" / "GoogleService-Info.plist"

    @property
    def project_spec(self) -> Path:
        return self.mobile / "project.yml"

    @property
    def project(self) -> Path:
        return self.mobile / "Rundale.xcodeproj"

    @property
    def export_options(self) -> Path:
        return self.output / "ExportOptions.plist"

    @property
    def receipt(self) -> Path:
        return self.output / "receipt.json"

    @property
    def log(self) -> Path:
        return self.output / "release.log"


def command_text(argv: Sequence[str]) -> str:
    return " ".join(shlex.quote(str(part)) for part in argv)


def endpoint_args() -> list[str]:
    return [f"{key}={value}" for key, value in ENDPOINT_SETTINGS.items()]


def increment_build_number(project_spec: Path, *, dry_run: bool = False) -> tuple[int, int]:
    text = project_spec.read_text(encoding="utf-8")
    pattern = re.compile(r"^(\s*CURRENT_PROJECT_VERSION:\s*[\"']?)(\d+)([\"']?\s*)$", re.MULTILINE)
    matches = list(pattern.finditer(text))
    if len(matches) != 1:
        raise ValueError("project.yml must contain exactly one numeric CURRENT_PROJECT_VERSION")
    match = matches[0]
    old = int(match.group(2))
    new = old + 1
    if not dry_run:
        replacement = f"{match.group(1)}{new}{match.group(3)}"
        project_spec.write_text(text[: match.start()] + replacement + text[match.end() :], encoding="utf-8")
    return old, new


def project_value(project_spec: Path, key: str) -> str:
    match = re.search(rf"^\s*{re.escape(key)}:\s*[\"']?([^\"'\s]+)", project_spec.read_text(encoding="utf-8"), re.MULTILINE)
    if match is None:
        raise ValueError(f"project.yml is missing {key}")
    return match.group(1)


class Release:
    def __init__(self, paths: Paths, *, dry_run: bool = False,
                 runner: Callable[..., None] | None = None) -> None:
        self.paths = paths
        self.dry_run = dry_run
        self._runner = runner

    def run_command(self, argv: Sequence[str], *, cwd: Path | None = None) -> None:
        print(f"$ {command_text(argv)}", flush=True)
        if self.dry_run:
            return
        self.paths.output.mkdir(parents=True, exist_ok=True)
        if self._runner is not None:
            self._runner(list(argv), cwd=cwd or self.paths.root, log=self.paths.log)
            return
        with self.paths.log.open("a", encoding="utf-8") as log:
            log.write(f"$ {command_text(argv)}\n")
            log.flush()
            subprocess.run(
                [str(part) for part in argv],
                cwd=str(cwd or self.paths.root),
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
            )

    @contextmanager
    def locked(self):
        if self.dry_run:
            yield
            return
        self.paths.output.parent.mkdir(parents=True, exist_ok=True)
        lock_path = self.paths.output.parent / "release.lock"
        with lock_path.open("w", encoding="utf-8") as lock_file:
            try:
                import fcntl
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX)
            except ImportError:
                pass
            yield
            try:
                import fcntl
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)
            except ImportError:
                pass

    def clear_output(self) -> None:
        if self.dry_run:
            return
        # Keep compiler/package caches; discard only prior release artifacts.
        for path in (self.paths.archive, self.paths.export):
            if path.exists():
                shutil.rmtree(path)
        for path in (self.paths.receipt, self.paths.log, self.paths.export_options):
            path.unlink(missing_ok=True)

    def firebase_preflight(self) -> None:
        if self.dry_run:
            return
        if not self.paths.firebase.is_file():
            raise RuntimeError(
                f"missing ignored Firebase configuration: {self.paths.firebase}; "
                "see mobile/phase2-auth.md"
            )

    def common_prepare(self, *, build_rust: bool = True) -> None:
        if build_rust:
            self.run_command(["bash", str(self.paths.mobile / "scripts" / "build-rust-mobile.sh")])
        self.run_command(["xcodegen", "generate", "--spec", str(self.paths.project_spec)], cwd=self.paths.root)

    def build(self) -> None:
        with self.locked():
            self.firebase_preflight()
            self.clear_output()
            self.common_prepare()
            self.run_command(
                [
                    "xcodebuild", "-project", str(self.paths.project), "-scheme", SCHEME,
                    "-configuration", "Release", "-destination", "generic/platform=iOS",
                    "-derivedDataPath", str(self.paths.output / "DerivedData"),
                    "CODE_SIGNING_ALLOWED=NO", "build", *endpoint_args(),
                ],
                cwd=self.paths.root,
            )
            self.validate_app(
                self.paths.output / "DerivedData" / "Build" / "Products" / "Release-iphoneos" / "Rundale.app",
                expected_build=int(project_value(self.paths.project_spec, "CURRENT_PROJECT_VERSION")),
            )
            if not self.dry_run:
                self.paths.receipt.write_text(json.dumps({
                    "status": "built_unsigned",
                    "version": project_value(self.paths.project_spec, "MARKETING_VERSION"),
                    "build": int(project_value(self.paths.project_spec, "CURRENT_PROJECT_VERSION")),
                }, indent=2) + "\n", encoding="utf-8")
                print(f"Built unsigned iPhone app: {self.paths.output / 'DerivedData/Build/Products/Release-iphoneos/Rundale.app'}")
                print(f"Build log: {self.paths.log}")

    def testflight(self) -> None:
        with self.locked():
            self.firebase_preflight()
            self.clear_output()
            self.run_command([str(self.paths.root / "verify"), "--phase", "all"], cwd=self.paths.root)
            old, new = increment_build_number(self.paths.project_spec, dry_run=self.dry_run)
            print(f"CURRENT_PROJECT_VERSION: {old} -> {new}")
            # verify already built the Rust framework for all implemented phases.
            self.common_prepare(build_rust=False)
            self.run_command(
                [
                    "xcodebuild", "-project", str(self.paths.project), "-scheme", SCHEME,
                    "-configuration", "Release", "-destination", "generic/platform=iOS",
                    "-derivedDataPath", str(self.paths.output / "DerivedData"),
                    "-archivePath", str(self.paths.archive), "-allowProvisioningUpdates", "archive",
                    f"DEVELOPMENT_TEAM={TEAM_ID}", "CODE_SIGN_STYLE=Automatic", *endpoint_args(),
                ],
                cwd=self.paths.root,
            )
            self.validate_archive(expected_build=new)
            options = self.write_export_options()
            self.run_command(
                [
                    "xcodebuild", "-exportArchive", "-archivePath", str(self.paths.archive),
                    "-exportOptionsPlist", str(options), "-exportPath", str(self.paths.export),
                    "-allowProvisioningUpdates",
                ],
                cwd=self.paths.root,
            )
            if not self.dry_run:
                self.paths.receipt.write_text(json.dumps({
                    "status": "upload_requested",
                    "version": project_value(self.paths.project_spec, "MARKETING_VERSION"),
                    "archiveBuild": new,
                    "uploadedBuild": None,
                    "appStoreConnectAppID": "6811694290",
                    "testingStatus": "pending_apple_processing",
                }, indent=2) + "\n", encoding="utf-8")
                print("Upload succeeded; Apple processing/compliance and Testing status remain pending.")
                print("Xcode manages the uploaded build number; confirm it in App Store Connect.")
                print(f"Upload log and receipt: {self.paths.output}")

    def validate_app(self, app: Path, *, expected_build: int | None = None, verify_code_sign: bool = False) -> None:
        if self.dry_run:
            print(f"validate app: {app}")
            return
        info_path = app / "Info.plist"
        if not info_path.is_file() or not (app / "GoogleService-Info.plist").is_file():
            raise RuntimeError("packaged app is missing Info.plist or GoogleService-Info.plist")
        info = plistlib.loads(info_path.read_bytes())
        expected = {
            "CFBundleIdentifier": BUNDLE_ID,
            "CFBundleShortVersionString": project_value(self.paths.project_spec, "MARKETING_VERSION"),
            "RUNDALE_ENDPOINT_BASE_URL": ENDPOINT_SETTINGS["RUNDALE_ENDPOINT_BASE_URL"],
            "RUNDALE_ENDPOINT_ORGANIZATION": ENDPOINT_SETTINGS["RUNDALE_ENDPOINT_ORGANIZATION"],
            "RUNDALE_ENDPOINT_SLUG": ENDPOINT_SETTINGS["RUNDALE_ENDPOINT_SLUG"],
            "RUNDALE_ENDPOINT_VERSION": ENDPOINT_SETTINGS["RUNDALE_ENDPOINT_VERSION"],
        }
        for key, value in expected.items():
            if info.get(key) != value:
                raise RuntimeError(f"archive metadata mismatch for {key}")
        if expected_build is not None and str(info.get("CFBundleVersion")) != str(expected_build):
            raise RuntimeError("packaged app build number does not match project.yml")
        executable_name = info.get("CFBundleExecutable")
        if not isinstance(executable_name, str) or not executable_name:
            raise RuntimeError("packaged app is missing CFBundleExecutable")
        executable = app / executable_name
        if not executable.is_file():
            raise RuntimeError("archive is missing the application executable")
        if verify_code_sign:
            self.run_command(["codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app)])

    def validate_archive(self, *, expected_build: int | None = None) -> None:
        self.validate_app(self.paths.app, expected_build=expected_build, verify_code_sign=True)

    def write_export_options(self) -> Path:
        options = {
            "method": "app-store-connect",
            "destination": "upload",
            "teamID": TEAM_ID,
            "signingStyle": "automatic",
            "testFlightInternalTestingOnly": True,
            "manageAppVersionAndBuildNumber": True,
            "uploadSymbols": True,
        }
        if not self.dry_run:
            self.paths.output.mkdir(parents=True, exist_ok=True)
            self.paths.export_options.write_bytes(plistlib.dumps(options, sort_keys=False))
        else:
            print(f"write export options: {self.paths.export_options}")
        return self.paths.export_options


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "testflight"))
    parser.add_argument("--dry-run", action="store_true", help="print commands without prerequisites or side effects")
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    release = Release(Paths(Path(__file__).resolve().parents[2]), dry_run=args.dry_run)
    try:
        getattr(release, args.command)()
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"release failed: {error}", file=sys.stderr)
        print(f"See {release.paths.log} for command output.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
