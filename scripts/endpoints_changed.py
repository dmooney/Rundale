#!/usr/bin/env python3
"""Runs CI's Endpoints job locally when this branch would trigger it.

CI runs `pnpm check` in `endpoints/` (format, lint, typecheck, tests, build)
when a pull request changes any path in its `endpoints` filter and at least
one changed file is not documentation (`.github/workflows/ci.yml`, jobs
`changes` and `endpoints-quality`). This gate applies the same rule to the
changes against the merge base with origin/main, including uncommitted and
untracked files, so `just check` fails where that CI job would.

    endpoints_changed.py [--base origin/main]
"""

from __future__ import annotations

import argparse
import fnmatch
import subprocess
import sys
from collections.abc import Callable, Iterable, Sequence

# CI's `endpoints` path filter, in fnmatch form.
ENDPOINT_INPUTS = (
    "endpoints/*",
    "mods/rundale/endpoints/*",
    "mods/rundale/npcs.json",
    "mods/rundale/world.json",
    "mobile/endpoint/*",
    ".github/workflows/ci.yml",
)

Runner = Callable[..., subprocess.CompletedProcess[str]]


def run(argv: Sequence[str], cwd: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(list(argv), capture_output=True, text=True, check=False, cwd=cwd)


def is_endpoint_input(path: str) -> bool:
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in ENDPOINT_INPUTS)


def is_documentation(path: str) -> bool:
    """CI's docs-only detector: Markdown anywhere, or anything under docs/."""
    return path.endswith(".md") or path.startswith("docs/")


def triggers(paths: Iterable[str]) -> bool:
    """Whether CI's Endpoints job runs for this set of changed paths."""
    changed = list(paths)
    return any(is_endpoint_input(path) for path in changed) and any(
        not is_documentation(path) for path in changed
    )


def git(argv: Sequence[str], runner: Runner) -> str:
    result = runner(["git", *argv])
    if result.returncode != 0:
        raise SystemExit(
            f"endpoints-changed: `git {' '.join(argv)}` failed: {result.stderr.strip()}"
        )
    return result.stdout


def changed_paths(base: str, runner: Runner) -> list[str]:
    merge_base = git(["merge-base", base, "HEAD"], runner).strip()
    if not merge_base:
        raise SystemExit(f"endpoints-changed: cannot find the merge base with {base}")
    # Without rename detection a move lists both its old and new paths, as
    # CI's path filter does, so moving a file out of a watched directory
    # still runs the job.
    outputs = (
        git(["diff", "--name-only", "--no-renames", f"{merge_base}...HEAD"], runner),
        git(["diff", "--name-only", "--no-renames", "HEAD"], runner),
        git(["ls-files", "--others", "--exclude-standard"], runner),
    )
    return sorted(
        {line.strip() for output in outputs for line in output.splitlines() if line.strip()}
    )


def main(argv: Sequence[str] | None = None, runner: Runner = run) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", default="origin/main")
    args = parser.parse_args(argv)
    if not triggers(changed_paths(args.base, runner)):
        print("endpoints-changed: no change CI's Endpoints job runs for; skipped.")
        return 0
    for step in (["pnpm", "install", "--frozen-lockfile", "--silent"], ["pnpm", "check"]):
        result = runner(step, cwd="endpoints")
        sys.stdout.write(result.stdout)
        sys.stderr.write(result.stderr)
        if result.returncode != 0:
            print(f"endpoints-changed: `{' '.join(step)}` failed.", file=sys.stderr)
            return result.returncode
    return 0


if __name__ == "__main__":
    sys.exit(main())
