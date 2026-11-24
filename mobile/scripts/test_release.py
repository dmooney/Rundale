#!/usr/bin/env python3
"""Focused stdlib tests for release orchestration; no Xcode invocation."""

from __future__ import annotations

import base64
import importlib
import io
import json
import plistlib
import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import TYPE_CHECKING
from unittest.mock import patch

if TYPE_CHECKING:
    from . import release
else:  # support both package-based pytest and direct unittest discovery
    release = (
        importlib.import_module(".release", __package__)
        if __package__
        else importlib.import_module("release")
    )


def _pad(part: str) -> str:
    return part + "=" * (-len(part) % 4)


def _der_int(value: bytes) -> bytes:
    """DER INTEGER for an unsigned big-endian value."""
    value = value.lstrip(b"\x00") or b"\x00"
    if value[0] & 0x80:
        value = b"\x00" + value
    return bytes([0x02, len(value)]) + value


class ReleaseTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "mobile" / "Rundale" / "Resources").mkdir(parents=True)
        (self.root / "mobile" / "scripts").mkdir(parents=True, exist_ok=True)
        (self.root / "mobile" / "project.yml").write_text(
            'MARKETING_VERSION: "0.1.0"\nCURRENT_PROJECT_VERSION: "8"\n', encoding="utf-8"
        )
        (self.root / "mobile" / "Rundale" / "Resources" / "GoogleService-Info.plist").write_bytes(
            b"private"
        )
        self.paths = release.Paths(self.root)
        self.commands: list[list[str]] = []

    def tearDown(self) -> None:
        self.temp.cleanup()

    def runner(self, argv, *, cwd, log):
        self.commands.append(argv)

    def test_release_identity_matches_verified_limerick_service(self):
        self.assertEqual(
            release.ENDPOINT_SETTINGS,
            {
                "RUNDALE_ENDPOINT_BASE_URL": "https://limerick-endpoints-877612517009.us-east1.run.app",
                "RUNDALE_ENDPOINT_ORGANIZATION": "limerick-demo",
            },
        )

    def test_beta_builds_report_bugs_to_the_limerick_prod_service(self):
        self.assertEqual(
            release.BETA_SETTINGS,
            {
                "RUNDALE_BETA_FEEDBACK": "YES",
                "RUNDALE_BUG_REPORT_URL": "https://limerick-bug-report-877612517009.us-east1.run.app",
            },
        )

    def test_release_build_number_is_the_commit_count(self):
        git = [
            "git",
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ]
        subprocess.run([*git, "init", "-q"], cwd=self.root, check=True)
        for message in ("one", "two", "three"):
            subprocess.run(
                [*git, "commit", "-q", "--allow-empty", "-m", message], cwd=self.root, check=True
            )
        self.assertEqual(release.release_build_number(self.root, env={}), (3, False))

        shallow = self.root / "shallow"
        subprocess.run(
            ["git", "clone", "-q", "--depth", "1", f"file://{self.root}", str(shallow)], check=True
        )
        with self.assertRaisesRegex(RuntimeError, "shallow clone"):
            release.release_build_number(shallow, env={})

    def test_release_build_number_is_app_store_connects_latest_plus_one(self):
        key = self.root / "AuthKey_TEST.p8"
        subprocess.run(
            ["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", str(key)],
            check=True,
        )
        env = {"ASC_KEY_PATH": str(key), "ASC_KEY_ID": "KEYID", "ASC_ISSUER_ID": "ISSUER"}
        tokens: list[str] = []

        def latest(token: str) -> int | None:
            tokens.append(token)
            return 1330

        self.assertEqual(
            release.release_build_number(self.root, env=env, latest=latest), (1331, True)
        )
        header, claims, signature = tokens[0].split(".")
        self.assertEqual(
            json.loads(base64.urlsafe_b64decode(_pad(header))),
            {"alg": "ES256", "kid": "KEYID", "typ": "JWT"},
        )
        body = json.loads(base64.urlsafe_b64decode(_pad(claims)))
        self.assertEqual((body["iss"], body["aud"]), ("ISSUER", "appstoreconnect-v1"))
        self.assertEqual(body["exp"] - body["iat"], 600)
        raw = base64.urlsafe_b64decode(_pad(signature))
        self.assertEqual(len(raw), 64)
        # Re-encode r || s as DER and check it with the key's public half.
        seq = _der_int(raw[:32]) + _der_int(raw[32:])
        der = bytes([0x30, len(seq)]) + seq
        (self.root / "sig.der").write_bytes(der)
        public = self.root / "pub.pem"
        subprocess.run(
            ["openssl", "ec", "-in", str(key), "-pubout", "-out", str(public)],
            check=True,
            capture_output=True,
        )
        verified = subprocess.run(
            [
                "openssl",
                "dgst",
                "-sha256",
                "-verify",
                str(public),
                "-signature",
                str(self.root / "sig.der"),
            ],
            input=f"{header}.{claims}".encode(),
            capture_output=True,
        )
        self.assertEqual(verified.returncode, 0, verified.stdout + verified.stderr)

    def test_release_build_number_falls_back_when_app_store_connect_has_no_builds(self):
        key = self.root / "AuthKey_TEST.p8"
        subprocess.run(
            ["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", str(key)],
            check=True,
        )
        env = {"ASC_KEY_PATH": str(key), "ASC_KEY_ID": "KEYID", "ASC_ISSUER_ID": "ISSUER"}
        with patch.object(release, "commit_count", return_value=42):
            self.assertEqual(
                release.release_build_number(self.root, env=env, latest=lambda _: None), (42, False)
            )

    def test_a_refused_build_lookup_reports_apples_reason(self):
        body = {
            "errors": [
                {
                    "status": "403",
                    "code": "FORBIDDEN.REQUIRED_AGREEMENTS_MISSING_OR_EXPIRED",
                    "title": "A required agreement is missing or has expired.",
                    "detail": "This request requires an in-effect agreement.",
                }
            ]
        }
        refusal = release.urllib.error.HTTPError(
            release.APP_STORE_CONNECT_BUILDS_URL,
            403,
            "Forbidden",
            {},
            io.BytesIO(json.dumps(body).encode()),
        )
        with (
            patch.object(release.urllib.request, "urlopen", side_effect=refusal),
            self.assertRaises(RuntimeError) as raised,
        ):
            release.latest_uploaded_build("TOKEN")
        message = str(raised.exception)
        self.assertIn("HTTP 403", message)
        self.assertIn("REQUIRED_AGREEMENTS_MISSING_OR_EXPIRED", message)
        self.assertIn("A required agreement is missing or has expired.", message)

    def test_a_refusal_without_a_json_body_still_names_the_status(self):
        refusal = release.urllib.error.HTTPError(
            release.APP_STORE_CONNECT_BUILDS_URL, 403, "Forbidden", {}, io.BytesIO(b"<html>")
        )
        with (
            patch.object(release.urllib.request, "urlopen", side_effect=refusal),
            self.assertRaises(RuntimeError) as raised,
        ):
            release.latest_uploaded_build("TOKEN")
        self.assertIn("(HTTP 403): no reason given", str(raised.exception))

    def test_latest_uploaded_build_takes_the_highest_number_across_pages(self):
        pages = [
            {
                "data": [{"attributes": {"version": v}} for v in ("1330", "1323", "x")],
                "links": {"next": "https://api.appstoreconnect.apple.com/v1/builds?cursor=2"},
            },
            {"data": [{"attributes": {"version": "1400"}}], "links": {}},
        ]

        class Response(io.BytesIO):
            def __enter__(self):
                return self

            def __exit__(self, *args):
                return False

        with patch.object(
            release.urllib.request,
            "urlopen",
            side_effect=[Response(json.dumps(page).encode()) for page in pages],
        ) as urlopen:
            self.assertEqual(release.latest_uploaded_build("TOKEN"), 1400)
        first, second = (call.args[0] for call in urlopen.call_args_list)
        self.assertEqual(first.get_header("Authorization"), "Bearer TOKEN")
        self.assertIn("filter[app]=6811694290", first.full_url)
        self.assertTrue(second.full_url.endswith("cursor=2"))
        self.assertEqual(second.get_header("Authorization"), "Bearer TOKEN")

    def test_testflight_requests_the_commit_count_without_editing_project_yml(self):
        before = self.paths.project_spec.read_bytes()
        runner = release.Release(self.paths, runner=self.runner)
        with (
            patch.object(release, "release_build_number", return_value=(1259, False)),
            patch.object(runner, "validate_archive") as validate,
            patch.object(runner, "validate_distribution_ipa"),
        ):
            runner.testflight()
        archive = next(c for c in self.commands if "archive" in c)
        self.assertIn("CURRENT_PROJECT_VERSION=1259", archive)
        validate.assert_called_once_with(expected_build=1259)
        self.assertEqual(self.paths.project_spec.read_bytes(), before)
        receipt = json.loads(self.paths.receipt.read_text())
        self.assertEqual(receipt["requestedBuild"], 1259)

    def test_testflight_keeps_an_app_store_connect_number_exactly(self):
        runner = release.Release(self.paths, runner=self.runner)
        with (
            patch.object(release, "release_build_number", return_value=(1331, True)),
            patch.object(release, "asc_token_factory", return_value=self.factory),
            patch.object(
                release, "wait_for_testing", return_value="VALID/IN_BETA_TESTING"
            ) as waited,
            patch.object(runner, "validate_archive") as validate,
            patch.object(runner, "validate_distribution_ipa"),
        ):
            runner.testflight()
        waited.assert_called_once_with(self.factory, 1331, timeout_minutes=45)
        archive = next(c for c in self.commands if "archive" in c)
        self.assertIn("CURRENT_PROJECT_VERSION=1331", archive)
        validate.assert_called_once_with(expected_build=1331)
        upload_options = plistlib.loads(self.paths.export_options.read_bytes())
        self.assertFalse(upload_options["manageAppVersionAndBuildNumber"])
        receipt = json.loads(self.paths.receipt.read_text())
        self.assertEqual((receipt["requestedBuild"], receipt["uploadedBuild"]), (1331, 1331))
        self.assertEqual(receipt["status"], "testing")
        self.assertEqual(receipt["testingStatus"], "VALID/IN_BETA_TESTING")

    def test_testflight_fails_and_records_the_state_when_apple_rejects_the_build(self):
        runner = release.Release(self.paths, runner=self.runner)
        failure = release.TestFlightWaitError("rejected", "VALID/MISSING_EXPORT_COMPLIANCE")
        with (
            patch.object(release, "release_build_number", return_value=(1331, True)),
            patch.object(release, "asc_token_factory", return_value=self.factory),
            patch.object(release, "wait_for_testing", side_effect=failure),
            patch.object(runner, "validate_archive"),
            patch.object(runner, "validate_distribution_ipa"),
            self.assertRaises(release.TestFlightWaitError),
        ):
            runner.testflight()
        receipt = json.loads(self.paths.receipt.read_text())
        self.assertEqual(receipt["status"], "upload_requested")
        self.assertEqual(receipt["testingStatus"], "VALID/MISSING_EXPORT_COMPLIANCE")

    def test_testflight_records_lookup_failed_for_any_other_error(self):
        runner = release.Release(self.paths, runner=self.runner)
        with (
            patch.object(release, "release_build_number", return_value=(1331, True)),
            patch.object(release, "asc_token_factory", return_value=self.factory),
            patch.object(release, "wait_for_testing", side_effect=RuntimeError("HTTP 403")),
            patch.object(runner, "validate_archive"),
            patch.object(runner, "validate_distribution_ipa"),
            self.assertRaises(RuntimeError),
        ):
            runner.testflight()
        receipt = json.loads(self.paths.receipt.read_text())
        self.assertEqual(receipt["testingStatus"], "LOOKUP_FAILED")

    def test_testflight_without_a_key_says_the_status_was_not_checked(self):
        runner = release.Release(self.paths, runner=self.runner)
        with (
            patch.object(release, "release_build_number", return_value=(1259, False)),
            patch.object(release, "wait_for_testing", side_effect=AssertionError("network")),
            patch.object(runner, "validate_archive"),
            patch.object(runner, "validate_distribution_ipa"),
            patch("sys.stdout", new_callable=io.StringIO) as out,
        ):
            runner.testflight()
        self.assertIn("could not be checked", out.getvalue())
        receipt = json.loads(self.paths.receipt.read_text())
        self.assertEqual(receipt["testingStatus"], "pending_apple_processing")

    def wait(self, states, timeout_minutes=45):
        """Run `wait_for_testing` over scripted `(state, groups)` answers.

        The clock advances only when the wait sleeps, so no real time passes.
        """

        now = [0.0]
        sleeps: list[float] = []
        answers = iter(states)

        def sleep(seconds: float) -> None:
            sleeps.append(seconds)
            now[0] += seconds

        def fetch(token: str, build: int):
            self.assertEqual((token, build), ("TOKEN", 1354))
            return next(answers)

        with patch("sys.stdout", new_callable=io.StringIO):
            result = release.wait_for_testing(
                self.factory,
                1354,
                timeout_minutes=timeout_minutes,
                fetch=fetch,
                sleep=sleep,
                clock=lambda: now[0],
            )
        return result, sleeps

    BETA = ["Internal Beta"]

    @staticmethod
    def factory() -> str:
        return "TOKEN"

    def test_wait_mints_a_fresh_token_for_every_poll(self):
        now = [0.0]
        tokens = iter(f"T{n}" for n in range(100))
        seen: list[str] = []
        answers = iter([("PROCESSING/NONE", [])] * 12 + [("VALID/IN_BETA_TESTING", self.BETA)])

        def fetch(token: str, build: int):
            seen.append(token)
            return next(answers)

        def sleep(seconds: float) -> None:
            now[0] += seconds

        with patch("sys.stdout", new_callable=io.StringIO):
            release.wait_for_testing(
                lambda: next(tokens), 1354, fetch=fetch, sleep=sleep, clock=lambda: now[0]
            )
        self.assertGreater(now[0], 600)  # past one token's lifetime
        self.assertEqual(seen, [f"T{n}" for n in range(13)])

    def wait_with_failures(self, errors, timeout_minutes=45):
        """Fail each scripted lookup with `errors`, then report Testing."""

        now = [0.0]
        queue = list(errors)

        def fetch(token: str, build: int):
            if queue:
                raise queue.pop(0)
            return "VALID/IN_BETA_TESTING", self.BETA

        def sleep(seconds: float) -> None:
            now[0] += seconds

        with patch("sys.stdout", new_callable=io.StringIO):
            return release.wait_for_testing(
                self.factory,
                1354,
                timeout_minutes=timeout_minutes,
                fetch=fetch,
                sleep=sleep,
                clock=lambda: now[0],
            )

    def test_wait_retries_a_transient_lookup_error(self):
        transient = release.TransientLookupError("HTTP 503")
        self.assertEqual(self.wait_with_failures([transient, transient]), "VALID/IN_BETA_TESTING")

    def test_wait_timeout_names_the_last_lookup_error(self):
        transient = release.TransientLookupError("HTTP 503")
        with self.assertRaises(release.TestFlightWaitError) as raised:
            self.wait_with_failures([transient] * 10, timeout_minutes=2)
        self.assertIn("last lookup error: HTTP 503", str(raised.exception))

    def test_wait_stops_on_a_fatal_refusal(self):
        with self.assertRaises(RuntimeError) as raised:
            self.wait_with_failures([RuntimeError("refused (HTTP 403)")])
        self.assertNotIsInstance(raised.exception, release.TransientLookupError)

    def lookup_error(self, error):
        with (
            patch.object(release.urllib.request, "urlopen", side_effect=error),
            self.assertRaises(RuntimeError) as raised,
        ):
            release.build_testing_state("TOKEN", 9)
        return raised.exception

    def test_lookup_treats_network_errors_as_transient(self):
        for error in (
            release.urllib.error.URLError("connection reset"),
            TimeoutError("timed out"),
            ConnectionResetError("reset"),
        ):
            with self.subTest(error=error):
                self.assertIsInstance(self.lookup_error(error), release.TransientLookupError)

    def test_lookup_treats_5xx_and_429_as_transient(self):
        for code in (429, 500, 503):
            with self.subTest(code=code):
                error = release.urllib.error.HTTPError(
                    "https://api.appstoreconnect.apple.com/v1/builds",
                    code,
                    "x",
                    {},
                    io.BytesIO(b"{}"),
                )
                self.assertIsInstance(self.lookup_error(error), release.TransientLookupError)

    def test_lookup_keeps_other_4xx_fatal_with_apples_reason(self):
        for code in (400, 401, 403, 404):
            with self.subTest(code=code):
                body = {"errors": [{"code": "NOT_AUTHORIZED", "title": "No access"}]}
                error = release.urllib.error.HTTPError(
                    "https://api.appstoreconnect.apple.com/v1/builds",
                    code,
                    "x",
                    {},
                    io.BytesIO(json.dumps(body).encode()),
                )
                raised = self.lookup_error(error)
                self.assertNotIsInstance(raised, release.TransientLookupError)
                self.assertIn(f"HTTP {code}", str(raised))
                self.assertIn("NOT_AUTHORIZED", str(raised))

    def test_wait_succeeds_once_testing_in_internal_beta(self):
        result, sleeps = self.wait(
            [
                ("NOT_LISTED", []),
                ("PROCESSING/NONE", []),
                ("VALID/READY_FOR_BETA_TESTING", []),
                ("VALID/IN_BETA_TESTING", self.BETA),
            ]
        )
        self.assertEqual(result, "VALID/IN_BETA_TESTING")
        self.assertEqual(sleeps, [60, 60, 60])

    def test_wait_keeps_polling_while_testing_but_not_in_internal_beta(self):
        result, _ = self.wait(
            [("VALID/IN_BETA_TESTING", ["External"]), ("VALID/IN_BETA_TESTING", self.BETA)]
        )
        self.assertEqual(result, "VALID/IN_BETA_TESTING")

    def test_wait_times_out_naming_the_last_state(self):
        with self.assertRaises(release.TestFlightWaitError) as raised:
            self.wait([("NOT_LISTED", [])] * 10, timeout_minutes=3)
        self.assertEqual(raised.exception.state, "NOT_LISTED")
        self.assertIn("within 3 minutes", str(raised.exception))
        self.assertIn("last state NOT_LISTED", str(raised.exception))

    def test_wait_timeout_names_a_stuck_processing_state(self):
        with self.assertRaises(release.TestFlightWaitError) as raised:
            self.wait([("VALID/READY_FOR_BETA_TESTING", [])] * 10, timeout_minutes=2)
        self.assertIn("last state VALID/READY_FOR_BETA_TESTING", str(raised.exception))

    def test_wait_fails_at_once_on_each_terminal_state(self):
        for state in (
            "FAILED/NONE",
            "INVALID/NONE",
            "VALID/PROCESSING_EXCEPTION",
            "VALID/MISSING_EXPORT_COMPLIANCE",
            "VALID/EXPIRED",
        ):
            with self.subTest(state=state):
                answers = iter([(state, []), ("VALID/IN_BETA_TESTING", self.BETA)])
                sleeps: list[float] = []
                with (
                    patch("sys.stdout", new_callable=io.StringIO),
                    self.assertRaises(release.TestFlightWaitError) as raised,
                ):
                    release.wait_for_testing(
                        lambda: "TOKEN",
                        1354,
                        fetch=lambda _token, _build, answers=answers: next(answers),
                        sleep=sleeps.append,
                        clock=lambda: 0.0,
                    )
                self.assertEqual(raised.exception.state, state)
                self.assertIn(state, str(raised.exception))
                self.assertEqual(sleeps, [])

    @staticmethod
    def response(body):
        class Response(io.BytesIO):
            def __enter__(self):
                return self

            def __exit__(self, *args):
                return False

        return Response(json.dumps(body).encode())

    def test_build_testing_state_reads_the_included_resources(self):
        body = {
            "data": [{"attributes": {"version": "1354", "processingState": "VALID"}}],
            "included": [
                {
                    "type": "buildBetaDetails",
                    "attributes": {"internalBuildState": "IN_BETA_TESTING"},
                },
                {"type": "betaGroups", "attributes": {"name": "Internal Beta"}},
            ],
        }
        with patch.object(
            release.urllib.request, "urlopen", return_value=self.response(body)
        ) as urlopen:
            state = release.build_testing_state("TOKEN", 1354)
        self.assertEqual(state, ("VALID/IN_BETA_TESTING", ["Internal Beta"]))
        request = urlopen.call_args.args[0]
        self.assertEqual(request.get_header("Authorization"), "Bearer TOKEN")
        for part in (
            "filter[app]=6811694290",
            "filter[version]=1354",
            "include=buildBetaDetail,betaGroups",
        ):
            self.assertIn(part, request.full_url)

    def test_build_testing_state_reports_an_unlisted_build(self):
        with patch.object(
            release.urllib.request, "urlopen", return_value=self.response({"data": []})
        ):
            self.assertEqual(release.build_testing_state("TOKEN", 9), ("NOT_LISTED", []))

    def test_build_testing_state_before_the_beta_detail_exists(self):
        body = {"data": [{"attributes": {"processingState": "PROCESSING"}}]}
        with patch.object(release.urllib.request, "urlopen", return_value=self.response(body)):
            self.assertEqual(release.build_testing_state("TOKEN", 9), ("PROCESSING/NONE", []))

    def test_build_testing_state_reports_a_refusal(self):
        refusal = release.urllib.error.HTTPError(
            "https://api.appstoreconnect.apple.com/v1/builds",
            401,
            "Unauthorized",
            {},
            io.BytesIO(b"{}"),
        )
        with (
            patch.object(release.urllib.request, "urlopen", side_effect=refusal),
            self.assertRaises(RuntimeError) as raised,
        ):
            release.build_testing_state("TOKEN", 9)
        self.assertIn("HTTP 401", str(raised.exception))

    def test_testflight_status_checks_an_existing_build(self):
        runner = release.Release(self.paths, wait_minutes=7)
        with (
            patch.object(release, "asc_token_factory", return_value=self.factory),
            patch.object(
                release, "wait_for_testing", return_value="VALID/IN_BETA_TESTING"
            ) as waited,
            patch("sys.stdout", new_callable=io.StringIO) as out,
        ):
            runner.testflight_status(1354)
        waited.assert_called_once_with(self.factory, 1354, timeout_minutes=7)
        self.assertIn("Build 1354: VALID/IN_BETA_TESTING", out.getvalue())

    def test_testflight_status_needs_a_key(self):
        runner = release.Release(self.paths)
        with (
            patch.object(release, "asc_token_factory", return_value=None),
            self.assertRaises(RuntimeError) as raised,
        ):
            runner.testflight_status(1354)
        self.assertIn("ASC_KEY_PATH", str(raised.exception))

    def test_main_exits_non_zero_when_the_build_never_reaches_testing(self):
        failure = release.TestFlightWaitError("stuck", "FAILED/NONE")
        with (
            patch.object(release.Release, "testflight_status", side_effect=failure),
            patch("sys.stderr", new_callable=io.StringIO),
        ):
            self.assertEqual(release.main(["testflight-status", "--build", "5"]), 1)
            self.assertEqual(release.main(["testflight-status"]), 1)

    def test_build_orders_rust_xcodegen_and_unsigned_build(self):
        runner = release.Release(self.paths, runner=self.runner)
        runner.validate_app = lambda *args, **kwargs: None
        runner.build()
        self.assertIn("build-rust-mobile.sh", self.commands[0][1])
        self.assertEqual(self.commands[1][:3], ["xcodegen", "generate", "--spec"])
        self.assertIn("CODE_SIGNING_ALLOWED=NO", self.commands[2])

    def test_testflight_stops_before_mutation_when_verify_fails(self):
        def fail(argv, *, cwd, log):
            raise __import__("subprocess").CalledProcessError(1, argv)

        runner = release.Release(self.paths, runner=fail)
        with self.assertRaises(__import__("subprocess").CalledProcessError):
            runner.testflight()
        self.assertIn('"8"', self.paths.project_spec.read_text())

    def test_export_options_and_archive_validation(self):
        app = self.paths.app
        app.mkdir(parents=True)
        info = {
            "CFBundleIdentifier": release.BUNDLE_ID,
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "8",
            "CFBundleExecutable": "Rundale",
            "ITSAppUsesNonExemptEncryption": False,
            **release.ENDPOINT_SETTINGS,
            **release.BETA_SETTINGS,
        }
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        firebase_key = "AIza" + "F" * 35
        firebase = {**release.EXPECTED_FIREBASE, "API_KEY": firebase_key}
        (app / "GoogleService-Info.plist").write_bytes(plistlib.dumps(firebase))
        # The public Firebase key may also appear in the compiled binary.
        (app / "Rundale").write_bytes(b"binary " + firebase_key.encode())
        runner = release.Release(self.paths, runner=self.runner)
        runner.validate_archive(expected_build=8)
        options_path = runner.write_export_options()
        options = plistlib.loads(options_path.read_bytes())
        self.assertEqual(options["method"], "app-store-connect")
        self.assertTrue(options["testFlightInternalTestingOnly"])
        self.assertTrue(options["manageAppVersionAndBuildNumber"])
        self.assertEqual(self.commands[-1][0], "codesign")

        for value in (None, True, "NO", 0):
            with self.subTest(encryption_declaration=value):
                invalid_info = dict(info)
                if value is None:
                    del invalid_info["ITSAppUsesNonExemptEncryption"]
                else:
                    invalid_info["ITSAppUsesNonExemptEncryption"] = value
                (app / "Info.plist").write_bytes(plistlib.dumps(invalid_info))
                with self.assertRaisesRegex(RuntimeError, "encryption exemption"):
                    runner.validate_archive(expected_build=8)

        for value in (None, "NO"):
            with self.subTest(beta_feedback=value):
                beta_off = dict(info)
                if value is None:
                    del beta_off["RUNDALE_BETA_FEEDBACK"]
                else:
                    beta_off["RUNDALE_BETA_FEEDBACK"] = value
                (app / "Info.plist").write_bytes(plistlib.dumps(beta_off))
                with self.assertRaisesRegex(RuntimeError, "RUNDALE_BETA_FEEDBACK"):
                    runner.validate_archive(expected_build=8)

        del info["CFBundleExecutable"]
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        with self.assertRaisesRegex(RuntimeError, "CFBundleExecutable"):
            runner.validate_archive(expected_build=8)

    def test_bundle_rejects_shipped_credentials_and_foreign_firebase(self):
        app = self.paths.app
        app.mkdir(parents=True)
        info = {
            "CFBundleIdentifier": release.BUNDLE_ID,
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "8",
            "CFBundleExecutable": "Rundale",
            "ITSAppUsesNonExemptEncryption": False,
            **release.ENDPOINT_SETTINGS,
            **release.BETA_SETTINGS,
        }
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        firebase = {**release.EXPECTED_FIREBASE, "API_KEY": "AIza" + "F" * 35}
        (app / "GoogleService-Info.plist").write_bytes(plistlib.dumps(firebase))
        runner = release.Release(self.paths, runner=self.runner)
        leaks = {
            "Limerick Endpoints consumer key": b"sfk_live_0123456789ab_" + b"x" * 43,
            "Anthropic API key": b"sk-ant-" + b"a" * 40,
            "OpenAI-style API key": b"sk-proj-" + b"bQ7x" * 10,
            "OpenRouter API key": b"sk-or-v1-" + b"0123456789abcdef" * 4,
            "Google API key": b"AIza" + b"G" * 35,
            "provider secret variable": b"OPENAI_API_KEY=abc",
        }
        for label, secret in leaks.items():
            with self.subTest(label=label):
                (app / "Rundale").write_bytes(b"binary " + secret)
                with self.assertRaises(RuntimeError) as raised:
                    runner.validate_archive(expected_build=8)
                self.assertIn(label, str(raised.exception))
                self.assertNotIn(
                    secret.decode(), str(raised.exception), "findings must be redacted"
                )

        # A key packed straight after other string constants is still found.
        (app / "Rundale").write_bytes(b"binary tasksk-" + b"Zr4kQ9mW" * 6)
        with self.assertRaisesRegex(RuntimeError, "OpenAI-style API key"):
            runner.validate_archive(expected_build=8)

        # The engine's packed lowercase constants are not a key (build 1271).
        packed = (
            b"blocked-weatherplayer-task-progressionarrival-reactions---break"
            b"bringcarrycleancollectcutdigfetchgatherharvesthoeplantrepairsow"
            b"stacksweeptake_care_ofsee_to'don't"
        )
        (app / "Rundale").write_bytes(b"binary " + packed)
        runner.validate_archive(expected_build=8)

        # Nor are ones that run into a capitalised word (the upload after #2325).
        (app / "Rundale").write_bytes(
            b"binary ding right here.blocked-weatherplayer-task-progression"
            b"arrival-reactions---Local time out"
        )
        runner.validate_archive(expected_build=8)

        (app / "Rundale").write_bytes(b"binary")
        foreign = {**firebase, "PROJECT_ID": "someone-else"}
        (app / "GoogleService-Info.plist").write_bytes(plistlib.dumps(foreign))
        with self.assertRaisesRegex(RuntimeError, "Firebase configuration mismatch for PROJECT_ID"):
            runner.validate_archive(expected_build=8)

    def test_archive_signs_and_validates_without_upload(self):
        import contextlib
        import io

        output = io.StringIO()
        runner = release.Release(self.paths, runner=self.runner, dry_run=True)
        with contextlib.redirect_stdout(output):
            runner.archive()
        commands = [line for line in output.getvalue().splitlines() if line.startswith("$ ")]
        self.assertTrue(any(" -allowProvisioningUpdates archive " in line for line in commands))
        self.assertFalse(any("-exportArchive" in line for line in commands))
        self.assertTrue(any("validate app:" in line for line in output.getvalue().splitlines()))

    def test_full_dry_run_needs_no_firebase_and_changes_no_files(self):
        self.paths.firebase.unlink()
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        runner = release.Release(self.paths, dry_run=True, runner=self.runner)
        runner.build()
        key = self.root.parent / "AuthKey_DRYRUN.p8"
        key.write_text("not a real key\n")
        self.addCleanup(key.unlink)
        env = {"ASC_KEY_PATH": str(key), "ASC_KEY_ID": "KEYID", "ASC_ISSUER_ID": "ISSUER"}
        with (
            patch.dict(release.os.environ, env),
            patch.object(release, "release_build_number", side_effect=AssertionError("network")),
            patch.object(release, "commit_count", return_value=1259),
        ):
            runner.testflight()
        after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        self.assertEqual(before, after)
        self.assertFalse(self.paths.output.exists())
        self.assertEqual(self.commands, [])

    def test_testflight_without_verify_builds_rust_itself(self):
        runner = release.Release(self.paths, runner=self.runner, verify=False)
        with (
            patch.object(release, "release_build_number", return_value=(1259, False)),
            patch.object(runner, "validate_archive"),
            patch.object(runner, "validate_distribution_ipa"),
        ):
            runner.testflight()
        self.assertFalse(any("verify.py" in " ".join(c) for c in self.commands))
        self.assertTrue(self.commands[0][-1].endswith("build-rust-mobile.sh"))
        self.assertEqual(self.commands[1][0], "xcodegen")
        self.assertIn("archive", self.commands[2])
        self.assertEqual(release.parse_args(["testflight"]).verify, "all")
        self.assertEqual(release.parse_args(["testflight", "--verify", "none"]).verify, "none")

    def test_signing_uses_the_xcode_account_without_an_api_key(self):
        self.assertEqual(release.signing_auth_args({}), ["-allowProvisioningUpdates"])

    def test_signing_uses_a_complete_api_key(self):
        key = self.root / "AuthKey.p8"
        key.write_text("synthetic", encoding="utf-8")
        args = release.signing_auth_args(
            {"ASC_KEY_PATH": str(key), "ASC_KEY_ID": "KEYID", "ASC_ISSUER_ID": "issuer"}
        )
        self.assertEqual(
            args,
            [
                "-allowProvisioningUpdates",
                "-authenticationKeyPath",
                str(key),
                "-authenticationKeyID",
                "KEYID",
                "-authenticationKeyIssuerID",
                "issuer",
            ],
        )

    def test_signing_rejects_a_partial_or_missing_api_key(self):
        with self.assertRaisesRegex(RuntimeError, "ASC_ISSUER_ID"):
            release.signing_auth_args({"ASC_KEY_PATH": "x", "ASC_KEY_ID": "KEYID"})
        with self.assertRaisesRegex(RuntimeError, "does not name a file"):
            release.signing_auth_args(
                {
                    "ASC_KEY_PATH": str(self.root / "missing.p8"),
                    "ASC_KEY_ID": "K",
                    "ASC_ISSUER_ID": "I",
                }
            )

    def test_every_signing_xcodebuild_call_carries_the_api_key(self):
        key = self.root / "AuthKey.p8"
        key.write_text("synthetic", encoding="utf-8")
        env = {"ASC_KEY_PATH": str(key), "ASC_KEY_ID": "KEYID", "ASC_ISSUER_ID": "issuer"}
        runner = release.Release(self.paths, runner=self.runner)
        with (
            patch.dict(release.os.environ, env),
            patch.object(release, "release_build_number", return_value=(1259, False)),
            patch.object(runner, "validate_archive"),
            patch.object(runner, "validate_distribution_ipa"),
        ):
            runner.testflight()
        signing = [c for c in self.commands if "-allowProvisioningUpdates" in c]
        self.assertEqual(len(signing), 3)
        for command in signing:
            self.assertEqual(command[command.index("-authenticationKeyID") + 1], "KEYID")

    def test_archive_signs_automatically_without_a_distribution_profile(self):
        self.assertEqual(release.archive_signing_args({}), ["CODE_SIGN_STYLE=Automatic"])

    def test_exports_sign_with_the_archives_named_distribution_profile(self):
        self.assertEqual(release.export_signing_options({}), {"signingStyle": "automatic"})
        env = {"RUNDALE_DISTRIBUTION_PROFILE": "CI App Store"}
        runner = release.Release(self.paths, runner=self.runner)
        with patch.dict(release.os.environ, env):
            for upload in (False, True):
                with self.subTest(upload=upload):
                    path = runner.write_export_options(upload=upload)
                    options = plistlib.loads(path.read_bytes())
                    self.assertEqual(options["signingStyle"], "manual")
                    self.assertEqual(options["signingCertificate"], "Apple Distribution")
                    self.assertEqual(
                        options["provisioningProfiles"], {release.BUNDLE_ID: "CI App Store"}
                    )

    def test_archive_signs_manually_with_a_named_distribution_profile(self):
        self.assertEqual(
            release.archive_signing_args({"RUNDALE_DISTRIBUTION_PROFILE": "CI App Store"}),
            [
                "CODE_SIGN_STYLE=Manual",
                "CODE_SIGN_IDENTITY=Apple Distribution",
                "RUNDALE_DISTRIBUTION_PROFILE=CI App Store",
            ],
        )

    def test_both_archives_sign_with_the_named_distribution_profile(self):
        env = {"RUNDALE_DISTRIBUTION_PROFILE": "CI App Store"}
        runner = release.Release(self.paths, runner=self.runner)
        with (
            patch.dict(release.os.environ, env),
            patch.object(release, "release_build_number", return_value=(1259, False)),
            patch.object(runner, "validate_archive"),
            patch.object(runner, "validate_distribution_ipa"),
        ):
            runner.testflight()
            runner.archive()
        archives = [c for c in self.commands if "archive" in c]
        self.assertEqual(len(archives), 2)
        for command in archives:
            self.assertIn("CODE_SIGN_STYLE=Manual", command)
            self.assertNotIn("CODE_SIGN_STYLE=Automatic", command)
            self.assertIn("RUNDALE_DISTRIBUTION_PROFILE=CI App Store", command)
        # The exports take their signing from the export options plist (manual
        # with this profile; see test_exports_sign_with_the_archives_named_
        # distribution_profile), never from build settings.
        exports = [c for c in self.commands if "-exportArchive" in c]
        self.assertEqual(len(exports), 2)
        for command in exports:
            self.assertFalse(any(part.startswith("CODE_SIGN_STYLE=") for part in command))

    def test_testflight_orders_checks_archive_validation_then_upload(self):
        runner = release.Release(self.paths, runner=self.runner)
        with (
            patch.object(release, "release_build_number", return_value=(1259, False)),
            patch.object(runner, "validate_archive") as validate,
            patch.object(runner, "validate_distribution_ipa") as validate_ipa,
        ):
            runner.testflight()
        validate.assert_called_once_with(expected_build=1259)
        validate_ipa.assert_called_once_with(expected_build=1259)
        self.assertEqual(self.commands[0][1].rsplit("/", 2)[-2:], ["scripts", "verify.py"])
        self.assertEqual(self.commands[0][2:], ["--phase", "all"])
        self.assertEqual(self.commands[1][0], "xcodegen")
        self.assertIn("archive", self.commands[2])
        # The local distribution export comes first; the upload export last.
        local, upload = self.commands[3], self.commands[4]
        self.assertIn("-exportArchive", local)
        self.assertEqual(local[local.index("-exportPath") + 1], str(self.paths.distribution_export))
        self.assertIn("-exportArchive", upload)
        self.assertEqual(upload[upload.index("-exportPath") + 1], str(self.paths.export))
        local_options = plistlib.loads(self.paths.distribution_export_options.read_bytes())
        self.assertEqual(local_options["destination"], "export")
        self.assertFalse(local_options["manageAppVersionAndBuildNumber"])
        upload_options = plistlib.loads(self.paths.export_options.read_bytes())
        self.assertEqual(upload_options["destination"], "upload")
        self.assertTrue(upload_options["manageAppVersionAndBuildNumber"])
        self.assertFalse(any("build-rust-mobile.sh" in " ".join(c) for c in self.commands))

    def write_valid_app(self, app: Path, *, build: str = "8") -> None:
        app.mkdir(parents=True)
        info = {
            "CFBundleIdentifier": release.BUNDLE_ID,
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": build,
            "CFBundleExecutable": "Rundale",
            "ITSAppUsesNonExemptEncryption": False,
            **release.ENDPOINT_SETTINGS,
            **release.BETA_SETTINGS,
        }
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        firebase = {**release.EXPECTED_FIREBASE, "API_KEY": "AIza" + "F" * 35}
        (app / "GoogleService-Info.plist").write_bytes(plistlib.dumps(firebase))
        (app / "Rundale").write_bytes(b"binary")

    def test_distribution_ipa_is_unpacked_scanned_and_must_be_distribution_signed(self):
        self.paths.distribution_export.mkdir(parents=True)
        (self.paths.distribution_export / "Rundale.ipa").write_bytes(b"zip")
        secret = b""

        def unpacking_runner(argv, *, cwd, log):
            self.commands.append(argv)
            if argv[0] == "ditto":
                app = Path(argv[-1]) / "Payload" / "Rundale.app"
                self.write_valid_app(app, build="1259")
                (app / "Rundale").write_bytes(b"binary " + secret)

        runner = release.Release(self.paths, runner=unpacking_runner)
        distribution = [
            "Apple Distribution: Rundale (MBPRPZ283R)",
            "Apple Worldwide Developer Relations",
        ]
        with patch.object(release, "signing_authorities", return_value=distribution) as authorities:
            runner.validate_distribution_ipa(expected_build=1259)
        app = self.paths.distribution_app / "Payload" / "Rundale.app"
        authorities.assert_called_once_with(app)
        self.assertEqual(self.commands[0][:3], ["ditto", "-x", "-k"])
        self.assertEqual(self.commands[-1][:2], ["codesign", "--verify"])
        self.assertEqual(self.commands[-1][-1], str(app))

        self.assertIn(
            "distribution signing chain: Apple Distribution: Rundale (MBPRPZ283R)",
            self.paths.log.read_text(),
        )

        legacy = ["iPhone Distribution: Rundale (MBPRPZ283R)", "Apple Root CA"]
        with patch.object(release, "signing_authorities", return_value=legacy):
            runner.validate_distribution_ipa(expected_build=1259)

        development = ["Apple Development: Someone (ABCDE12345)"]
        with (
            patch.object(release, "signing_authorities", return_value=development),
            self.assertRaisesRegex(
                RuntimeError, "not signed with a distribution certificate; found: Apple Development"
            ),
        ):
            runner.validate_distribution_ipa(expected_build=1259)

        with (
            patch.object(release, "signing_authorities", return_value=distribution),
            self.assertRaisesRegex(RuntimeError, "build number"),
        ):
            runner.validate_distribution_ipa(expected_build=1260)

        secret = b"sk-ant-" + b"A" * 30
        with (
            patch.object(release, "signing_authorities", return_value=distribution),
            self.assertRaisesRegex(
                RuntimeError, "forbidden credentials: Rundale: Anthropic API key"
            ),
        ):
            runner.validate_distribution_ipa(expected_build=1259)

    def test_distribution_ipa_must_be_exactly_one(self):
        runner = release.Release(self.paths, runner=self.runner)
        self.paths.distribution_export.mkdir(parents=True)
        with self.assertRaisesRegex(RuntimeError, "expected one exported .ipa, found 0"):
            runner.validate_distribution_ipa(expected_build=8)
        for name in ("a.ipa", "b.ipa"):
            (self.paths.distribution_export / name).write_bytes(b"zip")
        with self.assertRaisesRegex(RuntimeError, "expected one exported .ipa, found 2"):
            runner.validate_distribution_ipa(expected_build=8)
        self.assertEqual(self.commands, [])

    def test_invalid_archive_prevents_upload_and_preserves_verification_log(self):
        def recording_runner(argv, *, cwd, log):
            self.commands.append(argv)
            with log.open("a") as output:
                output.write("verification evidence\n")

        runner = release.Release(self.paths, runner=recording_runner)
        with (
            patch.object(release, "release_build_number", return_value=(1259, False)),
            self.assertRaisesRegex(RuntimeError, "missing Info.plist"),
        ):
            runner.testflight()
        self.assertFalse(any("-exportArchive" in c for c in self.commands))
        self.assertIn("verification evidence", self.paths.log.read_text())
        self.assertFalse(self.paths.receipt.exists())


if __name__ == "__main__":
    unittest.main()
