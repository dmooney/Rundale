#!/usr/bin/env python3
"""Fail on known vulnerabilities that changed lockfiles add, as CI's scan-pr does.

CI's OSV-Scanner `scan-pr` job scans the pull request and its base and fails
on vulnerabilities the base does not have. Nothing local ran it, so a new
lockfile with a vulnerable transitive dependency passed `just check` and failed
only in CI (#2182: uuid 9.0.1 in bug-report/pnpm-lock.yaml). This runs the same
comparison for each lockfile changed against the merge base with origin/main,
including uncommitted changes.

    python3 limerick/scripts/security/osv_changed.py [--base origin/main]

Needs `osv-scanner` on PATH (`brew install osv-scanner`) and, when a lockfile
changed, network access to osv.dev; it fails without them, because a skipped
gate is not a passed one.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable, Sequence
from pathlib import Path

LOCKFILE_NAMES = {
    "Cargo.lock",
    "pnpm-lock.yaml",
    "package-lock.json",
    "yarn.lock",
    "uv.lock",
    "poetry.lock",
    "Package.resolved",
    "go.sum",
}

NO_PACKAGES = "No package sources found"

Runner = Callable[[Sequence[str]], subprocess.CompletedProcess[str]]


def run(argv: Sequence[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(list(argv), capture_output=True, text=True, check=False)


def is_lockfile(path: str) -> bool:
    name = Path(path).name
    return name in LOCKFILE_NAMES or (name.startswith("requirements") and name.endswith(".txt"))


def merge_base_with(base: str, runner: Runner) -> str:
    merge_base = runner(["git", "merge-base", base, "HEAD"]).stdout.strip()
    if not merge_base:
        raise SystemExit(f"osv-changed: cannot find the merge base with {base}")
    return merge_base


def changed_lockfiles(merge_base: str, runner: Runner) -> list[str]:
    committed = runner(["git", "diff", "--name-only", "--diff-filter=AMR", f"{merge_base}...HEAD"])
    uncommitted = runner(["git", "diff", "--name-only", "--diff-filter=AMR", "HEAD"])
    untracked = runner(["git", "ls-files", "--others", "--exclude-standard"])
    paths = {
        line.strip()
        for result in (committed, uncommitted, untracked)
        for line in result.stdout.splitlines()
        if line.strip()
    }
    return sorted(path for path in paths if is_lockfile(path) and Path(path).is_file())


def vulnerabilities(report: dict) -> dict[tuple[str, str], str]:
    """`(package, advisory)` to its version. Aliases of one advisory collapse
    to the group's first ID, so a GHSA and its CVE count once."""
    found: dict[tuple[str, str], str] = {}
    for result in report.get("results", []):
        for entry in result.get("packages", []):
            package = entry.get("package", {})
            name = f"{package.get('ecosystem', '?')}:{package.get('name', '?')}"
            groups = entry.get("groups") or [
                {"ids": [vuln.get("id")]} for vuln in entry.get("vulnerabilities", [])
            ]
            for group in groups:
                ids = sorted(str(i) for i in group.get("ids", []) if i)
                if ids:
                    found[(name, ids[0])] = str(package.get("version", "?"))
    return found


def scan(lockfile: Path, runner: Runner) -> dict[tuple[str, str], str]:
    result = runner(
        ["osv-scanner", "scan", "source", "--format", "json", "--lockfile", str(lockfile)]
    )
    # 0: none found; 1: vulnerabilities found. 128 with "No package sources
    # found" is a lockfile listing no packages (an empty Package.resolved),
    # which has nothing to report. Anything else did not scan.
    if result.returncode == 128 and NO_PACKAGES in result.stderr:
        return {}
    if result.returncode not in (0, 1):
        raise SystemExit(
            f"osv-changed: osv-scanner failed on {lockfile} ({result.returncode}): {result.stderr[-500:]}"
        )
    return vulnerabilities(json.loads(result.stdout or "{}"))


def base_version(path: str, merge_base: str, directory: Path, runner: Runner) -> Path | None:
    """The lockfile as the merge base has it, under its own name so the
    scanner recognises its format; `None` when the base has no such file."""
    shown = runner(["git", "show", f"{merge_base}:{path}"])
    if shown.returncode != 0:
        return None
    target = directory / Path(path).name
    target.write_text(shown.stdout)
    return target


def main(argv: Sequence[str] | None = None, runner: Runner = run) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", default="origin/main")
    args = parser.parse_args(argv)
    if runner is run and shutil.which("osv-scanner") is None:
        print(
            "osv-changed: osv-scanner is not installed (brew install osv-scanner)", file=sys.stderr
        )
        return 1
    merge_base = merge_base_with(args.base, runner)
    lockfiles = changed_lockfiles(merge_base, runner)
    if not lockfiles:
        print("osv-changed: no lockfile changed")
        return 0
    new: list[str] = []
    for path in lockfiles:
        head = scan(Path(path), runner)
        with tempfile.TemporaryDirectory() as directory:
            base_file = base_version(path, merge_base, Path(directory), runner)
            before = scan(base_file, runner) if base_file else {}
        for (package, advisory), version in sorted(head.items()):
            if (package, advisory) not in before:
                new.append(f"{path}: {package} {version} — https://osv.dev/{advisory}")
    if new:
        print("osv-changed: changed lockfiles add known vulnerabilities:", file=sys.stderr)
        for line in new:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"osv-changed: {len(lockfiles)} changed lockfile(s) add no known vulnerabilities")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
