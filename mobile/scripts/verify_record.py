#!/usr/bin/env python3
"""Record a full `mobile-verify` pass on GitHub and check for one.

The full simulator gate runs once, on the author's Mac. A pass is identified by
its input tree: the git tree of the commit with the paths no gate reads
removed (`verify.ATTESTATION_IGNORED_PATHSPECS`), so later documentation-only
commits keep it valid. `record` posts that tree as a `mobile-verify` commit status on
HEAD; `check` finds a status whose tree matches a pull request or a commit, so
CI and the TestFlight workflow need neither Xcode nor a second run.

    verify_record.py tree [REV]            # print REV's input tree (default HEAD)
    verify_record.py record                # after pushing a verified HEAD
    verify_record.py check --pr N          # CI: a matching pass for PR N's head
    verify_record.py check --commit SHA    # TestFlight: for a merge commit

The status is the author's attestation that `mobile-verify --phase all` passed
on that tree with their toolchain; it is not an independent run.
"""

from __future__ import annotations

import argparse
import importlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

if __package__:
    verify = importlib.import_module(".verify", __package__)
else:  # run as a script from any directory
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    verify = importlib.import_module("verify")

ROOT = Path(__file__).resolve().parents[2]
CONTEXT = "mobile-verify"
PASSES = ROOT / "mobile" / ".verification" / "passes"
DESCRIPTION = re.compile(r"input tree ([0-9a-f]{40}|[0-9a-f]{64})")


def _git(*args: str, env: dict[str, str] | None = None) -> str:
    return subprocess.run(
        ["git", *args], cwd=ROOT, env=env, check=True, capture_output=True, text=True
    ).stdout.strip()


def input_tree(rev: str = "HEAD") -> str:
    """Return REV's tree with the paths no gate reads removed."""

    with tempfile.TemporaryDirectory(prefix="rundale-input-tree-") as scratch:
        env = {**os.environ, "GIT_INDEX_FILE": str(Path(scratch) / "index")}
        _git("read-tree", f"{rev}^{{tree}}", env=env)
        # -f: in CI's merge checkout an excluded file can match neither the
        # checkout's HEAD nor its work tree, which plain `rm --cached` refuses.
        _git(
            "rm", "-r", "-q", "-f", "--cached", "--ignore-unmatch", "--",
            *verify.ATTESTATION_IGNORED_PATHSPECS,
            env=env,
        )  # fmt: skip
        return _git("write-tree", env=env)


def _gh(*args: str) -> Any:
    out = subprocess.run(["gh", *args], cwd=ROOT, check=True, capture_output=True, text=True)
    return json.loads(out.stdout) if out.stdout.strip() else None


def _repo() -> str:
    return (
        os.environ.get("GITHUB_REPOSITORY")
        or _gh("repo", "view", "--json", "nameWithOwner")["nameWithOwner"]
    )


def _recorded_trees(repo: str, sha: str) -> set[str]:
    statuses = _gh("api", "--paginate", "--slurp", f"repos/{repo}/commits/{sha}/statuses")
    trees = set()
    for page in statuses or []:
        for status in page:
            if status.get("context") == CONTEXT and status.get("state") == "success":
                match = DESCRIPTION.fullmatch(status.get("description") or "")
                if match:
                    trees.add(match.group(1))
    return trees


def _pull_commits(repo: str, number: int) -> list[str]:
    pages = _gh("api", "--paginate", "--slurp", f"repos/{repo}/pulls/{number}/commits")
    return [commit["sha"] for page in pages or [] for commit in page]


def record() -> int:
    sha = _git("rev-parse", "HEAD")
    tree = input_tree(sha)
    if not (PASSES / f"{tree}.json").is_file():
        print(
            f"No full mobile-verify pass for HEAD's input tree {tree}. Run "
            "`just mobile-verify --phase all` on a clean checkout of HEAD first; a run "
            "with uncommitted or untracked inputs tests a different tree.",
            file=sys.stderr,
        )
        return 1
    repo = _repo()
    _gh(
        "api", "--method", "POST", f"repos/{repo}/statuses/{sha}",
        "-f", "state=success", "-f", f"context={CONTEXT}",
        "-f", f"description=input tree {tree}",
    )  # fmt: skip
    print(f"Recorded {CONTEXT} for {sha[:12]} (input tree {tree}).")
    return 0


def check(*, pr: int | None, commit: str | None, wait: int) -> int:
    repo = _repo()
    if pr is not None:
        head = _gh("api", f"repos/{repo}/pulls/{pr}")["head"]["sha"]
        tree = input_tree(head)
    else:
        assert commit is not None
        tree = input_tree(commit)
    deadline = time.monotonic() + wait
    while True:
        if pr is not None:
            commits = _pull_commits(repo, pr)
        else:
            pulls = _gh("api", f"repos/{repo}/commits/{commit}/pulls") or []
            assert commit is not None
            commits = [commit] + [c for p in pulls for c in _pull_commits(repo, p["number"])]
        for sha in reversed(commits):
            if tree in _recorded_trees(repo, sha):
                print(f"{CONTEXT} passed on input tree {tree} (recorded on {sha[:12]}).")
                return 0
        if time.monotonic() >= deadline:
            break
        time.sleep(15)
    print(
        f"No {CONTEXT} pass is recorded for input tree {tree}. Run "
        "`just mobile-verify --phase all`, push, then `just mobile-verify-record`.",
        file=sys.stderr,
    )
    return 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    tree_parser = sub.add_parser("tree")
    tree_parser.add_argument("rev", nargs="?", default="HEAD")
    sub.add_parser("record")
    check_parser = sub.add_parser("check")
    target = check_parser.add_mutually_exclusive_group(required=True)
    target.add_argument("--pr", type=int)
    target.add_argument("--commit")
    check_parser.add_argument(
        "--wait", type=int, default=0, help="seconds to keep looking for a status"
    )
    args = parser.parse_args(argv)
    if args.command == "tree":
        print(input_tree(args.rev))
        return 0
    if args.command == "record":
        return record()
    return check(pr=args.pr, commit=args.commit, wait=args.wait)


if __name__ == "__main__":
    raise SystemExit(main())
