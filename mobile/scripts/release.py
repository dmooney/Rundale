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
from collections.abc import Callable, Sequence
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path

TEAM_ID = "MBPRPZ283R"
SCHEME = "Rundale"
BUNDLE_ID = "com.rundale.mobile"
ENDPOINT_SETTINGS = {
    "RUNDALE_ENDPOINT_BASE_URL": "https://limerick-endpoints-877612517009.us-east1.run.app",
    "RUNDALE_ENDPOINT_ORGANIZATION": "limerick-demo",
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
    def distribution_export(self) -> Path:
        return self.output / "distribution-export"

    @property
    def distribution_app(self) -> Path:
        return self.output / "distribution-ipa"

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
    def distribution_export_options(self) -> Path:
        return self.output / "DistributionExportOptions.plist"

    @property
    def receipt(self) -> Path:
        return self.output / "receipt.json"

    @property
    def log(self) -> Path:
        return self.output / "release.log"


# The only Firebase identity the shipped client may carry. The Firebase API key
# inside GoogleService-Info.plist is a public, App-Check-restricted identifier,
# not a provider or Endpoint credential; it is the one key-shaped value allowed.
EXPECTED_FIREBASE = {
    "PROJECT_ID": "limerick-prod",
    "BUNDLE_ID": BUNDLE_ID,
    "GOOGLE_APP_ID": "1:877612517009:ios:586f98a2cc3e7d0c676130",
}

# Credentials that must never ship in the app bundle (P2-F08). Findings report
# only the file and pattern name, never the matched value.
FORBIDDEN_CREDENTIALS: tuple[tuple[str, re.Pattern[bytes]], ...] = (
    ("Limerick Endpoints consumer key", re.compile(rb"sfk_live_[0-9a-f]{12}_[A-Za-z0-9_-]{20,}")),
    ("Anthropic API key", re.compile(rb"sk-ant-[A-Za-z0-9_-]{20,}")),
    # A key is random, so its body has a capital or a digit; the lookahead
    # demands one. Without it, "ta" + "sk-" + the engine's lowercase string
    # constants, packed side by side in the binary, read as a key.
    ("OpenAI-style API key", re.compile(rb"sk-(?:proj-)?(?=[a-z_-]*[A-Z0-9])[A-Za-z0-9_-]{32,}")),
    ("Google API key", re.compile(rb"AIza[0-9A-Za-z_-]{35}")),
    ("private key block", re.compile(rb"-----BEGIN (?:RSA |EC )?PRIVATE KEY-----")),
    (
        "provider secret variable",
        re.compile(rb"(?:OPENAI|ANTHROPIC|GEMINI|GOOGLE_GENAI|OPENROUTER)_API_KEY\s*[=:]\s*\S"),
    ),
)


def scan_bundle_credentials(app: Path, allowed: set[bytes]) -> list[str]:
    """Return redacted findings for forbidden credentials anywhere in the bundle."""
    findings: list[str] = []
    for path in sorted(app.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        data = path.read_bytes()
        for label, pattern in FORBIDDEN_CREDENTIALS:
            if any(match.group(0) not in allowed for match in pattern.finditer(data)):
                findings.append(f"{path.relative_to(app)}: {label}")
    return findings


# Leaf certificates that sign for App Store and TestFlight distribution:
# today's "Apple Distribution" and the legacy "iPhone Distribution".
DISTRIBUTION_AUTHORITIES = ("Apple Distribution:", "iPhone Distribution:")


def signing_authorities(app: Path) -> list[str]:
    """Return the certificate chain codesign reports for a signed bundle."""
    details = subprocess.run(
        ["codesign", "-dvv", str(app)], check=True, capture_output=True, text=True
    )
    # codesign -d writes its report to stderr.
    return [
        line.removeprefix("Authority=")
        for line in details.stderr.splitlines()
        if line.startswith("Authority=")
    ]


def command_text(argv: Sequence[str]) -> str:
    return " ".join(shlex.quote(str(part)) for part in argv)


def endpoint_args() -> list[str]:
    return [f"{key}={value}" for key, value in ENDPOINT_SETTINGS.items()]


def release_build_number(root: Path) -> int:
    """Return HEAD's commit count, the build number an upload requests.

    It rises with every commit on main, so each upload asks for a higher
    number than the last without editing project.yml. App Store Connect may
    still renumber the build (manageAppVersionAndBuildNumber).
    """
    shallow = subprocess.run(
        ["git", "rev-parse", "--is-shallow-repository"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if shallow == "true":
        # A shallow clone counts only its depth, which would request a
        # lower build number than earlier uploads.
        raise RuntimeError("cannot take a build number from a shallow clone; fetch full history")
    count = subprocess.run(
        ["git", "rev-list", "--count", "HEAD"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    return int(count)


def project_value(project_spec: Path, key: str) -> str:
    match = re.search(
        rf"^\s*{re.escape(key)}:\s*[\"']?([^\"'\s]+)",
        project_spec.read_text(encoding="utf-8"),
        re.MULTILINE,
    )
    if match is None:
        raise ValueError(f"project.yml is missing {key}")
    return match.group(1)


class Release:
    def __init__(
        self, paths: Paths, *, dry_run: bool = False, runner: Callable[..., None] | None = None
    ) -> None:
        self.paths = paths
        self.dry_run = dry_run
        self._runner = runner

    def note(self, message: str) -> None:
        """Print a line and keep it in the release log beside command output."""
        print(message, flush=True)
        if self.dry_run:
            return
        self.paths.output.mkdir(parents=True, exist_ok=True)
        with self.paths.log.open("a", encoding="utf-8") as log:
            log.write(message + "\n")

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
        for path in (
            self.paths.archive,
            self.paths.export,
            self.paths.distribution_export,
            self.paths.distribution_app,
        ):
            if path.exists():
                shutil.rmtree(path)
        for path in (
            self.paths.receipt,
            self.paths.log,
            self.paths.export_options,
            self.paths.distribution_export_options,
        ):
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
        self.run_command(
            ["xcodegen", "generate", "--spec", str(self.paths.project_spec)], cwd=self.paths.root
        )

    def build(self) -> None:
        with self.locked():
            self.firebase_preflight()
            self.clear_output()
            self.common_prepare()
            self.run_command(
                [
                    "xcodebuild",
                    "-project",
                    str(self.paths.project),
                    "-scheme",
                    SCHEME,
                    "-configuration",
                    "Release",
                    "-destination",
                    "generic/platform=iOS",
                    "-derivedDataPath",
                    str(self.paths.output / "DerivedData"),
                    "CODE_SIGNING_ALLOWED=NO",
                    "build",
                    *endpoint_args(),
                ],
                cwd=self.paths.root,
            )
            self.validate_app(
                self.paths.output
                / "DerivedData"
                / "Build"
                / "Products"
                / "Release-iphoneos"
                / "Rundale.app",
                expected_build=int(
                    project_value(self.paths.project_spec, "CURRENT_PROJECT_VERSION")
                ),
            )
            if not self.dry_run:
                self.paths.receipt.write_text(
                    json.dumps(
                        {
                            "status": "built_unsigned",
                            "version": project_value(self.paths.project_spec, "MARKETING_VERSION"),
                            "build": int(
                                project_value(self.paths.project_spec, "CURRENT_PROJECT_VERSION")
                            ),
                        },
                        indent=2,
                    )
                    + "\n",
                    encoding="utf-8",
                )
                print(
                    f"Built unsigned iPhone app: {self.paths.output / 'DerivedData/Build/Products/Release-iphoneos/Rundale.app'}"
                )
                print(f"Build log: {self.paths.log}")

    def archive(self) -> None:
        """Create and inspect a signed Release archive without uploading it."""
        with self.locked():
            self.firebase_preflight()
            self.clear_output()
            self.common_prepare()
            self.run_command(
                [
                    "xcodebuild",
                    "-project",
                    str(self.paths.project),
                    "-scheme",
                    SCHEME,
                    "-configuration",
                    "Release",
                    "-destination",
                    "generic/platform=iOS",
                    "-derivedDataPath",
                    str(self.paths.output / "DerivedData"),
                    "-archivePath",
                    str(self.paths.archive),
                    "-allowProvisioningUpdates",
                    "archive",
                    f"DEVELOPMENT_TEAM={TEAM_ID}",
                    "CODE_SIGN_STYLE=Automatic",
                    *endpoint_args(),
                ],
                cwd=self.paths.root,
            )
            build = int(project_value(self.paths.project_spec, "CURRENT_PROJECT_VERSION"))
            self.validate_archive(expected_build=build)
            if not self.dry_run:
                self.paths.receipt.write_text(
                    json.dumps({"status": "archived_signed_not_uploaded", "build": build}, indent=2)
                    + "\n",
                    encoding="utf-8",
                )
                print(f"Signed archive validated, not uploaded: {self.paths.archive}")

    def testflight(self) -> None:
        with self.locked():
            self.firebase_preflight()
            self.clear_output()
            self.run_command(
                [
                    sys.executable,
                    str(self.paths.mobile / "scripts" / "verify.py"),
                    "--phase",
                    "all",
                ],
                cwd=self.paths.root,
            )
            build = release_build_number(self.paths.root)
            print(f"Requested build number: {build} (commit count of HEAD)")
            # The verifier already built the Rust framework for all implemented phases.
            self.common_prepare(build_rust=False)
            self.run_command(
                [
                    "xcodebuild",
                    "-project",
                    str(self.paths.project),
                    "-scheme",
                    SCHEME,
                    "-configuration",
                    "Release",
                    "-destination",
                    "generic/platform=iOS",
                    "-derivedDataPath",
                    str(self.paths.output / "DerivedData"),
                    "-archivePath",
                    str(self.paths.archive),
                    "-allowProvisioningUpdates",
                    "archive",
                    f"DEVELOPMENT_TEAM={TEAM_ID}",
                    "CODE_SIGN_STYLE=Automatic",
                    *endpoint_args(),
                    f"CURRENT_PROJECT_VERSION={build}",
                ],
                cwd=self.paths.root,
            )
            self.validate_archive(expected_build=build)
            # Export the distribution-signed .ipa locally and scan it before
            # the upload export, which re-signs the same archive the same way
            # but leaves no local copy to inspect.
            self.run_command(
                [
                    "xcodebuild",
                    "-exportArchive",
                    "-archivePath",
                    str(self.paths.archive),
                    "-exportOptionsPlist",
                    str(self.write_export_options(upload=False)),
                    "-exportPath",
                    str(self.paths.distribution_export),
                    "-allowProvisioningUpdates",
                ],
                cwd=self.paths.root,
            )
            self.validate_distribution_ipa(expected_build=build)
            options = self.write_export_options()
            self.run_command(
                [
                    "xcodebuild",
                    "-exportArchive",
                    "-archivePath",
                    str(self.paths.archive),
                    "-exportOptionsPlist",
                    str(options),
                    "-exportPath",
                    str(self.paths.export),
                    "-allowProvisioningUpdates",
                ],
                cwd=self.paths.root,
            )
            if not self.dry_run:
                self.paths.receipt.write_text(
                    json.dumps(
                        {
                            "status": "upload_requested",
                            "version": project_value(self.paths.project_spec, "MARKETING_VERSION"),
                            "requestedBuild": build,
                            "uploadedBuild": None,
                            "appStoreConnectAppID": "6811694290",
                            "testingStatus": "pending_apple_processing",
                        },
                        indent=2,
                    )
                    + "\n",
                    encoding="utf-8",
                )
                print(
                    "Upload succeeded; Apple processing/compliance and Testing status remain pending."
                )
                print("Xcode manages the uploaded build number; confirm it in App Store Connect.")
                print(f"Upload log and receipt: {self.paths.output}")

    def validate_app(
        self, app: Path, *, expected_build: int | None = None, verify_code_sign: bool = False
    ) -> None:
        if self.dry_run:
            print(f"validate app: {app}")
            return
        info_path = app / "Info.plist"
        if not info_path.is_file() or not (app / "GoogleService-Info.plist").is_file():
            raise RuntimeError("packaged app is missing Info.plist or GoogleService-Info.plist")
        info = plistlib.loads(info_path.read_bytes())
        try:
            firebase = plistlib.loads((app / "GoogleService-Info.plist").read_bytes())
        except (plistlib.InvalidFileException, ValueError) as error:
            raise RuntimeError(
                "packaged GoogleService-Info.plist is not a property list"
            ) from error
        for key, value in EXPECTED_FIREBASE.items():
            if firebase.get(key) != value:
                raise RuntimeError(f"packaged Firebase configuration mismatch for {key}")
        allowed = (
            {firebase["API_KEY"].encode()} if isinstance(firebase.get("API_KEY"), str) else set()
        )
        findings = scan_bundle_credentials(app, allowed)
        if findings:
            raise RuntimeError(
                "packaged app contains forbidden credentials: " + "; ".join(findings)
            )
        # Reassess the declaration when crypto or distribution changes; see testflight.md.
        if info.get("ITSAppUsesNonExemptEncryption") is not False:
            raise RuntimeError(
                "packaged app is missing the reviewed encryption exemption declaration"
            )
        expected = {
            "CFBundleIdentifier": BUNDLE_ID,
            "CFBundleShortVersionString": project_value(
                self.paths.project_spec, "MARKETING_VERSION"
            ),
            "RUNDALE_ENDPOINT_BASE_URL": ENDPOINT_SETTINGS["RUNDALE_ENDPOINT_BASE_URL"],
            "RUNDALE_ENDPOINT_ORGANIZATION": ENDPOINT_SETTINGS["RUNDALE_ENDPOINT_ORGANIZATION"],
        }
        for key, value in expected.items():
            if info.get(key) != value:
                raise RuntimeError(f"archive metadata mismatch for {key}")
        if expected_build is not None and str(info.get("CFBundleVersion")) != str(expected_build):
            raise RuntimeError("packaged app build number does not match the expected build")
        executable_name = info.get("CFBundleExecutable")
        if not isinstance(executable_name, str) or not executable_name:
            raise RuntimeError("packaged app is missing CFBundleExecutable")
        executable = app / executable_name
        if not executable.is_file():
            raise RuntimeError("archive is missing the application executable")
        if verify_code_sign:
            self.run_command(
                ["codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app)]
            )

    def validate_archive(self, *, expected_build: int | None = None) -> None:
        self.validate_app(self.paths.app, expected_build=expected_build, verify_code_sign=True)

    def validate_distribution_ipa(self, *, expected_build: int) -> None:
        """Check the distribution-signed .ipa TestFlight receives (P2-F08)."""
        if self.dry_run:
            print(f"validate distribution ipa: {self.paths.distribution_export}")
            return
        ipas = sorted(self.paths.distribution_export.glob("*.ipa"))
        if len(ipas) != 1:
            raise RuntimeError(f"expected one exported .ipa, found {len(ipas)}")
        if self.paths.distribution_app.exists():
            shutil.rmtree(self.paths.distribution_app)
        # ditto keeps the bundle's permissions and symlinks, which codesign checks.
        self.run_command(["ditto", "-x", "-k", str(ipas[0]), str(self.paths.distribution_app)])
        apps = sorted((self.paths.distribution_app / "Payload").glob("*.app"))
        if len(apps) != 1:
            raise RuntimeError(f"expected one app in the exported .ipa, found {len(apps)}")
        self.validate_app(apps[0], expected_build=expected_build, verify_code_sign=True)
        authorities = signing_authorities(apps[0])
        self.note("distribution signing chain: " + "; ".join(authorities))
        if not any(authority.startswith(DISTRIBUTION_AUTHORITIES) for authority in authorities):
            raise RuntimeError(
                "exported app is not signed with a distribution certificate; found: "
                + ("; ".join(authorities) or "no signing authority")
            )

    def write_export_options(self, *, upload: bool = True) -> Path:
        options = {
            "method": "app-store-connect",
            "destination": "upload" if upload else "export",
            "teamID": TEAM_ID,
            "signingStyle": "automatic",
            "testFlightInternalTestingOnly": True,
            # Only the upload may renumber; the local export keeps the
            # archive's build so its number can be checked.
            "manageAppVersionAndBuildNumber": upload,
            "uploadSymbols": upload,
        }
        path = self.paths.export_options if upload else self.paths.distribution_export_options
        if not self.dry_run:
            self.paths.output.mkdir(parents=True, exist_ok=True)
            path.write_bytes(plistlib.dumps(options, sort_keys=False))
        else:
            print(f"write export options: {path}")
        return path


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "archive", "testflight"))
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print commands without prerequisites or side effects",
    )
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
