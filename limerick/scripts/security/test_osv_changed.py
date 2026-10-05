"""Tests for osv_changed: only vulnerabilities the base lacks fail the gate."""

from __future__ import annotations

import json
import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))
import osv_changed  # noqa: E402


def report(*vulns: tuple[str, str, list[str]]) -> str:
    """An osv-scanner JSON report: (package, version, advisory aliases)."""
    return json.dumps(
        {
            "results": [
                {
                    "packages": [
                        {
                            "package": {"name": name, "version": version, "ecosystem": "npm"},
                            "groups": [{"ids": ids}],
                        }
                        for name, version, ids in vulns
                    ]
                }
            ]
        }
    )


class FakeRunner:
    def __init__(self, changed: list[str], head: str, base: str | None) -> None:
        self.changed = changed
        self.head = head
        self.base = base
        self.calls: list[list[str]] = []

    def __call__(self, argv: Sequence[str]) -> subprocess.CompletedProcess[str]:
        argv = list(argv)
        self.calls.append(argv)
        ok = lambda out="": subprocess.CompletedProcess(argv, 0, out, "")  # noqa: E731
        if argv[:2] == ["git", "merge-base"]:
            return ok("abc123\n")
        if argv[:2] == ["git", "diff"] and "abc123...HEAD" in argv:
            return ok("\n".join(self.changed))
        if argv[:2] in (["git", "diff"], ["git", "ls-files"]):
            return ok()
        if argv[:2] == ["git", "show"]:
            if self.base is None:
                return subprocess.CompletedProcess(argv, 128, "", "fatal: path does not exist")
            return ok("base lockfile")
        if argv[0] == "osv-scanner":
            # The base version is scanned from a temporary directory.
            output = self.base if Path(argv[-1]).is_absolute() else self.head
            return subprocess.CompletedProcess(
                argv, 1 if output and "groups" in output else 0, output, ""
            )
        raise AssertionError(argv)


@pytest.fixture
def lockfile(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> str:
    monkeypatch.chdir(tmp_path)
    (tmp_path / "svc").mkdir()
    (tmp_path / "svc" / "pnpm-lock.yaml").write_text("head lockfile")
    return "svc/pnpm-lock.yaml"


def test_a_new_lockfile_with_a_vulnerable_dependency_fails(lockfile, capsys):
    runner = FakeRunner(
        [lockfile],
        head=report(("uuid", "9.0.1", ["GHSA-w5hq-g745-h8pq", "CVE-2026-41907"])),
        base=None,
    )
    assert osv_changed.main([], runner) == 1
    err = capsys.readouterr().err
    assert "npm:uuid 9.0.1" in err
    assert "https://osv.dev/CVE-2026-41907" in err


def test_a_vulnerability_the_base_already_has_does_not_fail(lockfile):
    known = report(("lodash", "4.17.20", ["GHSA-aaaa"]))
    runner = FakeRunner([lockfile], head=known, base=known)
    assert osv_changed.main([], runner) == 0


def test_a_clean_change_passes_and_unchanged_lockfiles_are_not_scanned(lockfile):
    runner = FakeRunner([lockfile, "README.md"], head=report(), base=report())
    assert osv_changed.main([], runner) == 0
    scanned = [call[-1] for call in runner.calls if call[0] == "osv-scanner"]
    assert scanned[0] == lockfile
    assert len(scanned) == 2  # the head and base versions of the one lockfile


def test_no_changed_lockfile_scans_nothing(lockfile):
    runner = FakeRunner(["README.md"], head=report(), base=report())
    assert osv_changed.main([], runner) == 0
    assert not [call for call in runner.calls if call[0] == "osv-scanner"]


def test_a_scanner_failure_is_not_a_pass(lockfile):
    def broken(argv: Sequence[str]) -> subprocess.CompletedProcess[str]:
        argv = list(argv)
        if argv[0] == "osv-scanner":
            return subprocess.CompletedProcess(argv, 127, "", "boom")
        return FakeRunner([lockfile], head="", base=None)(argv)

    with pytest.raises(SystemExit, match="osv-scanner failed"):
        osv_changed.main([], broken)


def test_a_lockfile_with_no_packages_has_nothing_to_report(lockfile):
    def empty(argv: Sequence[str]) -> subprocess.CompletedProcess[str]:
        argv = list(argv)
        if argv[0] == "osv-scanner":
            return subprocess.CompletedProcess(
                argv, 128, "", "No package sources found, --help for usage information."
            )
        return FakeRunner([lockfile], head="", base=None)(argv)

    assert osv_changed.main([], empty) == 0


def test_the_merge_base_is_computed_once(lockfile):
    runner = FakeRunner([lockfile], head=report(), base=report())
    osv_changed.main([], runner)
    assert sum(call[:2] == ["git", "merge-base"] for call in runner.calls) == 1


@pytest.mark.parametrize(
    "path, expected",
    [
        ("bug-report/pnpm-lock.yaml", True),
        ("limerick/Cargo.lock", True),
        ("requirements-dev.txt", True),
        (
            "mobile/Rundale.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved",
            True,
        ),
        ("pnpm-workspace.yaml", False),
        ("docs/requirements.md", False),
    ],
)
def test_lockfile_names(path, expected):
    assert osv_changed.is_lockfile(path) is expected
