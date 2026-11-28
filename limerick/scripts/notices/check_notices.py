#!/usr/bin/env python3
"""Keep the third-party notices in step with the lockfiles they are generated from.

Rust: limerick/THIRD_PARTY_NOTICES.rust.md against limerick/Cargo.lock. UI:
limerick/THIRD_PARTY_NOTICES.ui.md against limerick/apps/ui/package-lock.json.

`check` (the default, stdlib only, no network, no cargo) fails when the committed
notices do not account for exactly the registry/git crates in Cargo.lock. Every locked
`name@version` must appear either in the notices (a crate that ships) or in
`limerick/notices-dev-only.txt` (a crate `cargo about` leaves out because only dev
dependencies reach it), never both, and neither file may name a crate the lockfile no
longer holds. A dependency bump that leaves the notices behind (`rustix 1.1.4` after the
lock moved to 1.1.6, issue 2441) therefore fails without a human noticing. The UI check
requires every row of the UI notices to exist in package-lock.json and every locked
production, non-optional package to have a row (optional platform binaries are only in
the notices for the targets the generator installs, so they are not required).

`refresh` is the half `just notices` runs after `cargo about generate`, before the new
file replaces the committed one. It refuses a generated file that lost information:

* a crate@version the previous notices listed, that Cargo.lock still holds, must still be
  listed, so a silently truncated crate list fails;
* every license text the previous file carried for such a crate must still be there, so a
  tool that drops extra license files for a crate (cargo-about 0.9.0 dropped 28 of them,
  issue 2441) fails instead of deleting real notices;
* the generated file may only name crates Cargo.lock holds.

It then writes `notices-dev-only.txt`: the locked crates `cargo about` left out.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections.abc import Iterable
from pathlib import Path

import tomllib

LIMERICK = Path(__file__).resolve().parents[2]
LOCKFILE = LIMERICK / "Cargo.lock"
NOTICES = LIMERICK / "THIRD_PARTY_NOTICES.rust.md"
DEV_ONLY = LIMERICK / "notices-dev-only.txt"
UI_LOCKFILE = LIMERICK / "apps" / "ui" / "package-lock.json"
UI_NOTICES = LIMERICK / "THIRD_PARTY_NOTICES.ui.md"

DEV_ONLY_HEADER = (
    "# Crates in Cargo.lock that THIRD_PARTY_NOTICES.rust.md leaves out because cargo-about\n"
    "# does not count them as shipped (dev-dependencies only, or platform-gated). Generated\n"
    "# by `just notices`; do not edit by hand. `scripts/notices/check_notices.py` fails\n"
    "# when this list and the notices together stop matching Cargo.lock.\n"
)

ENTRY = re.compile(r"^- \[([^ \]]+) ([^\]]+)\]\(")
RERUN = "run `just notices` in limerick/ and commit the regenerated files"
UI_ROW = re.compile(r"^- \[([^\]]+)\]\(")


def locked_crates(lock_text: str) -> set[str]:
    """`name@version` of every package in Cargo.lock that has a source (not first-party)."""
    data = tomllib.loads(lock_text)
    return {f"{p['name']}@{p['version']}" for p in data["package"] if p.get("source")}


def notice_texts(notices: str) -> dict[str, set[str]]:
    """Map each `name@version` in the notices to the hashes of the license texts it is under."""
    lines = notices.split("\n")
    out: dict[str, set[str]] = {}
    i = 0
    while i < len(lines):
        if lines[i] != "**Used by:**":
            i += 1
            continue
        i += 1
        crates: list[str] = []
        while i < len(lines) and not lines[i].startswith("```"):
            m = ENTRY.match(lines[i])
            if m:
                crates.append(f"{m.group(1)}@{m.group(2)}")
            i += 1
        i += 1  # opening fence
        body: list[str] = []
        while i < len(lines) and lines[i] != "```":
            body.append(lines[i])
            i += 1
        digest = hashlib.sha256("\n".join(body).encode()).hexdigest()
        for crate in crates:
            out.setdefault(crate, set()).add(digest)
        i += 1
    return out


def dev_only_crates(text: str) -> set[str]:
    return {
        line.strip()
        for line in text.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }


def sample(items: Iterable[str], limit: int = 12) -> str:
    ordered = sorted(items)
    more = f" (+{len(ordered) - limit} more)" if len(ordered) > limit else ""
    return ", ".join(ordered[:limit]) + more


def find_problems(lock: set[str], notices: set[str], dev_only: set[str]) -> list[str]:
    problems: list[str] = []
    both = notices & dev_only
    if both:
        problems.append(f"in both the notices and notices-dev-only.txt: {sample(both)}")
    stale = (notices | dev_only) - lock
    if stale:
        problems.append(f"named by the notices files but not in Cargo.lock: {sample(stale)}")
    missing = lock - notices - dev_only
    if missing:
        problems.append(f"in Cargo.lock but in neither notices file: {sample(missing)}")
    return problems


def ui_locked(lock_text: str) -> tuple[set[str], set[str]]:
    """(every `name@version` in package-lock.json, those a production install must carry)."""
    packages = json.loads(lock_text)["packages"]
    every: set[str] = set()
    required: set[str] = set()
    for path, meta in packages.items():
        if not path or meta.get("link"):
            continue
        key = f"{path.rsplit('node_modules/', 1)[-1]}@{meta['version']}"
        every.add(key)
        if not meta.get("dev") and not meta.get("optional"):
            required.add(key)
    return every, required


def ui_notice_rows(notices: str) -> set[str]:
    return {m.group(1) for line in notices.splitlines() if (m := UI_ROW.match(line))}


def find_ui_problems(every: set[str], required: set[str], rows: set[str]) -> list[str]:
    problems: list[str] = []
    stale = rows - every
    if stale:
        problems.append(f"UI notices name packages not in package-lock.json: {sample(stale)}")
    missing = required - rows
    if missing:
        problems.append(
            f"production packages in package-lock.json with no UI notice: {sample(missing)}"
        )
    return problems


def cmd_check(args: argparse.Namespace) -> int:
    for needed in (args.lockfile, args.notices, args.dev_only):
        if not needed.exists():
            print(f"check-notices: {needed} is missing; {RERUN}", file=sys.stderr)
            return 1
    lock = locked_crates(args.lockfile.read_text(encoding="utf-8"))
    notices = set(notice_texts(args.notices.read_text(encoding="utf-8")))
    dev_only = dev_only_crates(args.dev_only.read_text(encoding="utf-8"))
    if not notices:
        print(f"check-notices: {args.notices} lists no crates at all", file=sys.stderr)
        return 1
    problems = find_problems(lock, notices, dev_only)
    if args.ui_lockfile.exists() and args.ui_notices.exists():
        every, required = ui_locked(args.ui_lockfile.read_text(encoding="utf-8"))
        rows = ui_notice_rows(args.ui_notices.read_text(encoding="utf-8"))
        problems += find_ui_problems(every, required, rows)
    else:
        problems.append(f"{args.ui_lockfile} or {args.ui_notices} is missing")
    if problems:
        print("check-notices: the third-party notices are stale:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print(f"check-notices: {RERUN}", file=sys.stderr)
        return 1
    print(
        f"check-notices: ok ({len(notices)} Rust crates in the notices, "
        f"{len(dev_only)} dev-only, {len(lock)} locked; {len(rows)} UI packages)"
    )
    return 0


def find_refresh_problems(
    generated: dict[str, set[str]],
    previous: dict[str, set[str]],
    lock: set[str],
) -> list[str]:
    if not generated:
        return ["the generated notices list no crates at all"]
    problems: list[str] = []
    unlocked = set(generated) - lock
    if unlocked:
        problems.append(
            f"the generated notices name {len(unlocked)} crate(s) not in Cargo.lock: "
            f"{sample(unlocked)}"
        )
    dropped = (set(previous) & lock) - set(generated)
    if dropped:
        problems.append(
            f"{len(dropped)} crate(s) the previous notices listed, still locked at the same "
            f"version, are missing: {sample(dropped)}"
        )
    lost = {
        crate: len(previous[crate] - generated[crate])
        for crate in previous.keys() & generated.keys()
        if previous[crate] - generated[crate]
    }
    if lost:
        problems.append(
            f"{sum(lost.values())} license text(s) the previous notices carried are gone for "
            f"{len(lost)} crate(s) that are still present: {sample(lost)}. The tool dropped "
            "license files; use the pinned cargo-about version (cargo_about_version in the "
            "justfile)"
        )
    return problems


def cmd_refresh(args: argparse.Namespace) -> int:
    lock = locked_crates(args.lockfile.read_text(encoding="utf-8"))
    generated = notice_texts(args.generated.read_text(encoding="utf-8"))
    previous = (
        notice_texts(args.previous.read_text(encoding="utf-8")) if args.previous.exists() else {}
    )
    problems = find_refresh_problems(generated, previous, lock)
    if problems:
        print("check-notices: refusing the generated Rust notices:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print(f"check-notices: {args.previous} is unchanged", file=sys.stderr)
        return 1
    dev_only = sorted(lock - set(generated))
    args.dev_only.write_text(
        DEV_ONLY_HEADER + "".join(f"{c}\n" for c in dev_only), encoding="utf-8"
    )
    removed = sorted(set(previous) - set(generated))
    added = sorted(set(generated) - set(previous))
    print(
        f"check-notices: {len(generated)} crates ({len(added)} added, {len(removed)} "
        f"removed or re-versioned), {len(dev_only)} dev-only"
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--lockfile", type=Path, default=LOCKFILE)
    parser.add_argument("--dev-only", type=Path, default=DEV_ONLY)
    sub = parser.add_subparsers(dest="command")
    check = sub.add_parser("check", help="compare the committed notices with Cargo.lock")
    check.add_argument("--notices", type=Path, default=NOTICES)
    check.add_argument("--ui-lockfile", type=Path, default=UI_LOCKFILE)
    check.add_argument("--ui-notices", type=Path, default=UI_NOTICES)
    refresh = sub.add_parser("refresh", help="validate a freshly generated notices file")
    refresh.add_argument("--generated", type=Path, required=True)
    refresh.add_argument("--previous", type=Path, default=NOTICES)
    args = parser.parse_args(argv)
    if args.command == "refresh":
        return cmd_refresh(args)
    if args.command is None:
        args.notices, args.ui_lockfile, args.ui_notices = NOTICES, UI_LOCKFILE, UI_NOTICES
    return cmd_check(args)


if __name__ == "__main__":
    sys.exit(main())
