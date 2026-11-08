#!/usr/bin/env python3
"""Check that Dependabot covers every tracked lockfile in the repository.

A lockfile Dependabot never looks at is never bumped, and GitHub's security-update
jobs for it fail with `downgrades_dependencies` without opening a PR (issue 2340).
This check lists every tracked lockfile (`git ls-files`), derives the Dependabot
`package-ecosystem` that owns it, and fails when `.github/dependabot.yml` has no
`updates` entry with that ecosystem whose `directory` or `directories` matches the
lockfile's directory, unless the lockfile is in `EXCLUSIONS` with a written reason.

`directories` globs are honoured the way Dependabot reads them: `*` matches within one
path segment, `**` matches across segments (a trailing `/**` includes the directory itself), and `/` is the repository root only. A
stale exclusion (the file is no longer tracked, or is now covered) also fails, so the
list cannot rot.

Run it with `.venv-dev/bin/python scripts/check_dependabot_coverage.py` after
`just setup-py` (it needs PyYAML). CI runs it in the `python-quality` job, and
`just check` runs it through `just dependabot-coverage`.
"""

from __future__ import annotations

import argparse
import posixpath
import re
import subprocess
import sys
from collections.abc import Iterable, Sequence
from pathlib import Path
from typing import Any

import yaml

ROOT = Path(__file__).resolve().parents[1]
CONFIG = ".github/dependabot.yml"

# Lockfile basename pattern -> Dependabot package-ecosystem. `requirements*.txt` is the
# pip manifest that doubles as this repository's pin file.
LOCKFILE_ECOSYSTEMS: tuple[tuple[re.Pattern[str], str], ...] = (
    (re.compile(r"package-lock\.json|npm-shrinkwrap\.json|pnpm-lock\.yaml|yarn\.lock"), "npm"),
    (re.compile(r"Cargo\.lock"), "cargo"),
    (re.compile(r"requirements[^/]*\.txt|poetry\.lock|Pipfile\.lock"), "pip"),
    (re.compile(r"uv\.lock"), "uv"),
    (re.compile(r"Package\.resolved"), "swift"),
    (re.compile(r"go\.sum"), "gomod"),
    (re.compile(r"Gemfile\.lock"), "bundler"),
    (re.compile(r"composer\.lock"), "composer"),
    (re.compile(r"Podfile\.lock"), "cocoapods"),
)

# Tracked lockfiles deliberately left out of Dependabot: path -> why. Adding a line here
# is a decision a reviewer must read; keep the reason specific.
EXCLUSIONS: dict[str, str] = {
    "mobile/Rundale.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved": (
        "Xcode-managed SwiftPM pins (Firebase and its transitive packages) with no "
        "Package.swift manifest beside them for Dependabot's swift ecosystem to read; "
        "they move with deliberate Firebase upgrades verified by mobile-verify."
    ),
}


def ecosystem_for(path: str) -> str | None:
    """Return the Dependabot ecosystem that owns `path`, or None if it is no lockfile."""
    name = posixpath.basename(path)
    for pattern, ecosystem in LOCKFILE_ECOSYSTEMS:
        if pattern.fullmatch(name):
            return ecosystem
    return None


def normalize_dir(directory: str) -> str:
    """Return `directory` as an absolute, slash-trimmed repository path ('/' for root)."""
    return "/" + directory.strip("/")


def glob_to_regex(pattern: str) -> re.Pattern[str]:
    """Compile a Dependabot directory glob (`*`, `**`, `?`) to a full-match regex."""
    out: list[str] = []
    pattern = normalize_dir(pattern)
    i = 0
    while i < len(pattern):
        if pattern.startswith("/**", i) and i + 3 == len(pattern):
            out.append("(?:/.*)?")  # a trailing `/**` also matches the directory itself
            i += 3
        elif pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif pattern[i] == "*":
            out.append("[^/]*")
            i += 1
        elif pattern[i] == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(pattern[i]))
            i += 1
    return re.compile("".join(out))


def covered_directories(config: dict[str, Any]) -> list[tuple[str, re.Pattern[str]]]:
    """Return (ecosystem, directory matcher) for every `updates` entry directory."""
    matchers: list[tuple[str, re.Pattern[str]]] = []
    for entry in config.get("updates") or []:
        ecosystem = str(entry.get("package-ecosystem"))
        dirs: list[str] = []
        # Dependabot reads wildcards only in `directories`; `directory` is literal.
        if "directory" in entry:
            literal = normalize_dir(str(entry["directory"]))
            matchers.append((ecosystem, re.compile(re.escape(literal))))
        dirs.extend(entry.get("directories") or [])
        matchers.extend((ecosystem, glob_to_regex(str(d))) for d in dirs)
    return matchers


def find_problems(
    tracked: Iterable[str],
    config: dict[str, Any],
    exclusions: dict[str, str] | None = None,
) -> list[str]:
    """Return one message per uncovered lockfile or stale/unreasoned exclusion."""
    exclusions = EXCLUSIONS if exclusions is None else exclusions
    matchers = covered_directories(config)
    problems: list[str] = []
    lockfiles = sorted(p for p in tracked if ecosystem_for(p))
    covered: set[str] = set()
    for path in lockfiles:
        ecosystem = ecosystem_for(path)
        directory = normalize_dir(posixpath.dirname(path))
        if any(eco == ecosystem and rx.fullmatch(directory) for eco, rx in matchers):
            covered.add(path)
        elif path not in exclusions:
            problems.append(
                f"{path}: no {CONFIG} update entry with package-ecosystem {ecosystem!r} "
                f"covers directory {directory!r}. Add it to `directory`/`directories`, or "
                "list the file in EXCLUSIONS in scripts/check_dependabot_coverage.py with "
                "a written reason."
            )
    tracked_lockfiles = set(lockfiles)
    for path, reason in sorted(exclusions.items()):
        if not reason.strip():
            problems.append(f"{path}: exclusion has no reason.")
        elif path not in tracked_lockfiles:
            problems.append(f"{path}: stale exclusion, the lockfile is no longer tracked.")
        elif path in covered:
            problems.append(f"{path}: stale exclusion, {CONFIG} now covers it.")
    return problems


def tracked_files(root: Path) -> list[str]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"],
        check=True,
        capture_output=True,
        text=True,
    )
    return [p for p in result.stdout.split("\0") if p]


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check Dependabot lockfile coverage.")
    parser.add_argument("--root", type=Path, default=ROOT, help="repository root")
    args = parser.parse_args(argv)
    root: Path = args.root
    config = yaml.safe_load((root / CONFIG).read_text(encoding="utf-8")) or {}
    problems = find_problems(tracked_files(root), config)
    for problem in problems:
        print(f"error: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("Dependabot covers or explicitly excludes every tracked lockfile.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
