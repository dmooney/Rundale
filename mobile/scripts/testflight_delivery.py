#!/usr/bin/env python3
"""Decide whether a push to main needs a TestFlight upload, and record deliveries.

The upload job used to compare a push's app input tree with the push's own
`before`. Uploads share one concurrency group, so a newer pending run replaces
an older one; when the replacing push changed no app input, it compared equal to
its own `before` and uploaded nothing, and the replaced run's app change never
shipped. The decision now compares main's input tree with the tree of the last
build delivered to TestFlight.

A delivery is recorded as a lightweight tag `testflight/<build>` on the commit
that was uploaded, pushed to origin only after the build reached Testing in
Internal Beta. The tag names the build, and the commit gives the input tree
(`verify_record.input_tree`, the same app inputs the mobile-verify attestation
uses), so no extra state is stored.

    testflight_delivery.py decide --event push [--rev SHA]
    testflight_delivery.py record --receipt PATH [--rev SHA]

`decide` prints `upload=true|false` (for $GITHUB_OUTPUT) and its reason on
stderr. A manual run (`workflow_dispatch`) always uploads. With no delivery
recorded yet, a push uploads once, and that upload records the first tag.
"""

from __future__ import annotations

import argparse
import importlib
import json
import re
import subprocess
import sys
from pathlib import Path

if __package__:
    verify_record = importlib.import_module(".verify_record", __package__)
else:  # run as a script from any directory
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    verify_record = importlib.import_module("verify_record")

TAG_PREFIX = "testflight/"
TAG = re.compile(r"testflight/(\d+)")


def _git(*args: str) -> str:
    return subprocess.run(
        ["git", *args],
        cwd=verify_record.ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def delivered_builds() -> list[tuple[int, str]]:
    """Recorded deliveries as (build, commit), the newest build first."""

    listing = _git(
        "for-each-ref", "--format=%(refname:strip=2) %(objectname)", f"refs/tags/{TAG_PREFIX}"
    )
    builds = []
    for line in listing.splitlines():
        name, sha = line.split()
        match = TAG.fullmatch(name)
        if match:
            builds.append((int(match.group(1)), sha))
    return sorted(builds, reverse=True)


def last_delivered_tree() -> str | None:
    """The input tree of the newest recorded build whose commit is still known."""

    for _build, sha in delivered_builds():
        try:
            return verify_record.input_tree(sha)
        except subprocess.CalledProcessError:
            continue  # a tag whose commit this checkout lacks
    return None


def decide(event: str, tree: str, delivered: str | None) -> tuple[bool, str]:
    """Whether to upload, and why. `tree` is the pushed commit's input tree."""

    if event != "push":
        return True, f"{event} always uploads"
    if delivered is None:
        return True, "no TestFlight delivery is recorded yet; uploading to record the first"
    if tree == delivered:
        return False, f"input tree {tree} is the last delivered one; nothing new to upload"
    return True, f"input tree {tree} differs from the last delivered {delivered}"


def build_from_receipt(receipt: dict[str, object]) -> int | None:
    """The delivered build number, or None unless the build reached Testing."""

    if receipt.get("status") != "testing":
        return None
    build = receipt.get("uploadedBuild") or receipt.get("requestedBuild")
    return build if isinstance(build, int) else None


def record(build: int, sha: str) -> None:
    tag = f"{TAG_PREFIX}{build}"
    existing = subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}^{{commit}}"],
        cwd=verify_record.ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if existing.returncode == 0:
        if existing.stdout.strip() != sha:
            raise SystemExit(f"{tag} already names {existing.stdout.strip()}, not {sha}.")
    else:
        _git("tag", tag, sha)
    _git("push", "origin", f"refs/tags/{tag}")
    print(f"Recorded {tag} on {sha[:12]}.")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    decide_parser = sub.add_parser("decide")
    decide_parser.add_argument("--event", required=True)
    decide_parser.add_argument("--rev", default="HEAD")
    record_parser = sub.add_parser("record")
    record_parser.add_argument("--receipt", type=Path, required=True)
    record_parser.add_argument("--rev", default="HEAD")
    args = parser.parse_args(argv)
    sha = _git("rev-parse", args.rev)
    if args.command == "decide":
        upload, reason = decide(args.event, verify_record.input_tree(sha), last_delivered_tree())
        print(reason, file=sys.stderr)
        print(f"upload={'true' if upload else 'false'}")
        return 0
    try:
        receipt = json.loads(args.receipt.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        receipt = {}
    build = build_from_receipt(receipt)
    if build is None:
        print(
            "The receipt does not show a build in Testing; recording nothing, "
            "so the next push uploads again.",
            file=sys.stderr,
        )
        return 0
    record(build, sha)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
