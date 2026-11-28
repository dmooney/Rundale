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

The shipped-set check is independent of cargo-about. `limerick/notices-shipped.txt` is a
committed snapshot of `cargo tree` for every artifact that ships (`SHIPPED`: the iPhone
static library for both iOS targets, the Linux release binary, the Docker server, the
desktop app), and `check` fails when a crate in it has no notice, or when the snapshot is
older than Cargo.lock. `notices-dev-only.txt` is only what cargo-about leaves out, so
without this check a shipped crate that cargo-about wrongly omits (it dropped
`security-framework 3.7.0`: two same-named optional dependencies in `keyring`) would
be filed as dev-only without anyone noticing.

`refresh` is the half `just notices` runs after `cargo about generate`, before the new
file replaces the committed one. It refuses a generated file that lost information:

* a crate@version the previous notices listed, that Cargo.lock still holds, must still be
  listed, so a silently truncated crate list fails;
* every license text the previous file carried for such a crate must still be there, so a
  tool that drops extra license files for a crate (cargo-about 0.9.0 dropped 28 of them,
  issue 2441) fails instead of deleting real notices;
* the generated file may only name crates Cargo.lock holds.

Before those checks it applies `limerick/notices-supplement.txt`, the crates cargo-about
cannot see: each `crate@version same-as sibling@version` line adds the crate to every
list the sibling is in. An entry whose crate cargo-about now finds, or that left the
lockfile, fails, so the exception expires on its own.

It then writes `notices-dev-only.txt`: the locked crates `cargo about` left out.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from collections.abc import Iterable
from pathlib import Path

try:
    import tomllib  # type: ignore[import-not-found,unused-ignore]  # Python 3.11+
except ModuleNotFoundError:
    raise SystemExit(
        f"check_notices.py needs Python 3.11+ for tomllib; this is {sys.version.split()[0]}.\n"
        "Install a newer Python (macOS: `brew install python@3.12`) and run the script with it,\n"
        "for example `python3.12 limerick/scripts/notices/check_notices.py check`, or build the\n"
        "dev venv from it: `python3.12 -m venv .venv-dev`."
    ) from None

LIMERICK = Path(__file__).resolve().parents[2]
LOCKFILE = LIMERICK / "Cargo.lock"
NOTICES = LIMERICK / "THIRD_PARTY_NOTICES.rust.md"
DEV_ONLY = LIMERICK / "notices-dev-only.txt"
UI_LOCKFILE = LIMERICK / "apps" / "ui" / "package-lock.json"
UI_NOTICES = LIMERICK / "THIRD_PARTY_NOTICES.ui.md"
SHIPPED_SNAPSHOT = LIMERICK / "notices-shipped.txt"
SUPPLEMENT = LIMERICK / "notices-supplement.txt"

# What ships, as `cargo tree` arguments (package, target, evidence). Keep in step with the
# build that makes each artifact; a new shipped binary or target goes here, then `just notices`.
SHIPPED: tuple[tuple[str, str, str], ...] = (
    # mobile/scripts/build-rust-mobile.sh builds this package for both iOS targets.
    ("limerick-mobile-ffi", "aarch64-apple-ios", "iPhone device library"),
    ("limerick-mobile-ffi", "aarch64-apple-ios-sim", "iPhone simulator library"),
    # .github/workflows/release.yml: `cargo build -p limerick-engine --release` on Linux x86_64.
    ("limerick-engine", "x86_64-unknown-linux-gnu", "GitHub Release tarball"),
    # deploy/Dockerfile: `cargo build --release -p limerick-server` on Debian x86_64.
    ("limerick-server", "x86_64-unknown-linux-gnu", "server container image"),
    # `cargo tauri build` (docs/setup.md): the desktop app on each platform it builds for.
    ("limerick-tauri", "aarch64-apple-darwin", "macOS desktop app"),
    ("limerick-tauri", "x86_64-unknown-linux-gnu", "Linux desktop app"),
    ("limerick-tauri", "x86_64-pc-windows-msvc", "Windows desktop app"),
)
SHIPPED_HEADER = (
    "# Every locked crate@version `cargo tree -e normal,build` reaches for the artifacts that\n"
    "# ship (SHIPPED in scripts/notices/check_notices.py). `check` requires each to have a\n"
    "# notice, independent of cargo-about, and requires the fingerprint to match Cargo.lock.\n"
    "# Generated by `just notices`; do not edit by hand.\n"
)
FINGERPRINT = "# lock-fingerprint: "

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
        # Whitespace-only differences (wrapping, trailing spaces, blank lines) are not a
        # different license, so a template or cargo-about formatting change is not a loss.
        digest = hashlib.sha256(" ".join("\n".join(body).split()).encode()).hexdigest()
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
        # devOptional: needed only by dev dependencies or optional ones, so a production
        # install does not require it (it is in the notices only if an optional path brings it).
        if not any(meta.get(flag) for flag in ("dev", "optional", "devOptional")):
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


def lock_fingerprint(lock_text: str) -> str:
    """Hash of everything in Cargo.lock that decides what a build links.

    First-party versions are left out (a release bump changes them and nothing a build
    links), every registry or git package contributes name, version, source and its
    dependency list, so a new edge between existing crates changes it too.
    """
    rows = []
    for p in tomllib.loads(lock_text)["package"]:
        sourced = bool(p.get("source"))
        rows.append(
            f"{p['name']} {p['version'] if sourced else ''} {p.get('source', '')} "
            + ",".join(sorted(p.get("dependencies", [])))
        )
    return hashlib.sha256("\n".join(sorted(rows)).encode()).hexdigest()


def parse_tree(output: str, lock: set[str]) -> set[str]:
    """`name@version` of the locked third-party crates in `cargo tree --format {p}` output."""
    found = set()
    for line in output.splitlines():
        m = re.match(r"^(\S+) v(\S+)", line)
        if m and f"{m.group(1)}@{m.group(2)}" in lock:
            found.add(f"{m.group(1)}@{m.group(2)}")
    return found


def cargo_tree(package: str, target: str) -> str:
    result = subprocess.run(
        [
            "cargo",
            "tree",
            "--locked",
            "-e",
            "normal,build",
            "--target",
            target,
            "-p",
            package,
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],  # fmt: skip
        cwd=LIMERICK,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(f"cargo tree -p {package} --target {target} failed:\n{result.stderr}")
    return result.stdout


def shipped_snapshot(lock_text: str, trees: dict[tuple[str, str], str]) -> str:
    lock = locked_crates(lock_text)
    shipped: set[str] = set()
    lines = []
    for package, target, what in SHIPPED:
        crates = parse_tree(trees[(package, target)], lock)
        shipped |= crates
        lines.append(f"# {package} for {target} ({what}): {len(crates)} crates\n")
    return (
        SHIPPED_HEADER
        + FINGERPRINT
        + lock_fingerprint(lock_text)
        + "\n"
        + "".join(lines)
        + "".join(f"{c}\n" for c in sorted(shipped))
    )


def find_shipped_problems(
    snapshot: str, lock_text: str, notices: set[str], dev_only: set[str]
) -> list[str]:
    lock = locked_crates(lock_text)
    shipped = dev_only_crates(snapshot)
    stamp = next(
        (
            line[len(FINGERPRINT) :].strip()
            for line in snapshot.splitlines()
            if line.startswith(FINGERPRINT)
        ),
        "",
    )
    problems: list[str] = []
    if stamp != lock_fingerprint(lock_text):
        problems.append(
            "notices-shipped.txt was made from a different Cargo.lock "
            "(a dependency changed, so the shipped set may have too)"
        )
    if not shipped:
        problems.append("notices-shipped.txt lists no crates at all")
    stale = shipped - lock
    if stale:
        problems.append(f"notices-shipped.txt names crates not in Cargo.lock: {sample(stale)}")
    hidden = (shipped & dev_only) - notices
    if hidden:
        problems.append(
            "these crates ship (cargo tree reaches them from a shipped artifact) but "
            f"notices-dev-only.txt lists them as dev-only: {sample(hidden)}. Fix cargo-about's "
            "view (about.toml) or add them to notices-supplement.txt"
        )
    missing = shipped - notices - dev_only
    if missing:
        problems.append(f"shipped crates with no notice: {sample(missing)}")
    return problems


SUPPLEMENT_LINE = re.compile(r"^(\S+@\S+)\s+same-as\s+(\S+@\S+)\s*(#.*)?$")
LICENSE_HEADING = re.compile(r"^## .* \{#([^}]+)\}\s*$")
OVERVIEW_ROW = re.compile(r"^(- \[.+\]\(#([^)]+)\) — )(\d+) crates?$")


def parse_supplement(text: str) -> dict[str, str]:
    out: dict[str, str] = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = SUPPLEMENT_LINE.match(line)
        if not m:
            raise SystemExit(f"notices-supplement.txt: cannot read {raw!r}")
        out[m.group(1)] = m.group(2)
    return out


def apply_supplement(
    text: str, supplement: dict[str, str], lock: set[str]
) -> tuple[str, list[str]]:
    """Add each supplemented crate to every notice list its sibling is in."""
    present = set(notice_texts(text))
    problems: list[str] = []
    for crate, sibling in supplement.items():
        if crate in present:
            problems.append(
                f"{crate} is in the generated notices now; remove its notices-supplement.txt line"
            )
        if crate not in lock:
            problems.append(f"{crate} (notices-supplement.txt) is not in Cargo.lock any more")
        if sibling not in present:
            problems.append(f"{crate} is supplemented from {sibling}, which is not in the notices")
    if problems:
        return text, problems
    by_sibling: dict[str, list[str]] = {}
    for crate, sibling in supplement.items():
        by_sibling.setdefault(sibling, []).append(crate)
    out: list[str] = []
    license_id = ""
    added: dict[str, set[str]] = {}
    for line in text.split("\n"):
        out.append(line)
        if m := LICENSE_HEADING.match(line):
            license_id = m.group(1)
        if m := ENTRY.match(line):
            sibling = f"{m.group(1)}@{m.group(2)}"
            for crate in by_sibling.get(sibling, []):
                name, version = crate.split("@", 1)
                out.append(line.replace(f"[{m.group(1)} {m.group(2)}]", f"[{name} {version}]", 1))
                added.setdefault(license_id, set()).add(crate)
    for index, line in enumerate(out):
        if (m := OVERVIEW_ROW.match(line)) and m.group(2) in added:
            count = int(m.group(3)) + len(added[m.group(2)])
            out[index] = f"{m.group(1)}{count} crate{'' if count == 1 else 's'}"
    for crate in supplement:
        if crate not in notice_texts("\n".join(out)):
            problems.append(f"{crate}: no notice list was found to add it to")
    return "\n".join(out), problems


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
    if args.shipped.exists():
        problems += find_shipped_problems(
            args.shipped.read_text(encoding="utf-8"),
            args.lockfile.read_text(encoding="utf-8"),
            notices,
            dev_only,
        )
    else:
        problems.append(f"{args.shipped} is missing")
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
    # Bytes, not read_text: some license files use CRLF and the file must keep them.
    supplemented, supplement_problems = apply_supplement(
        args.generated.read_bytes().decode("utf-8"),
        parse_supplement(args.supplement.read_text(encoding="utf-8"))
        if args.supplement.exists()
        else {},
        lock,
    )
    generated = notice_texts(supplemented)
    previous = (
        notice_texts(args.previous.read_text(encoding="utf-8")) if args.previous.exists() else {}
    )
    problems = supplement_problems + find_refresh_problems(generated, previous, lock)
    if not problems and args.shipped.exists():
        # The snapshot was just regenerated by `just notices`; a shipped crate the tool
        # left out must not be filed as dev-only.
        problems += find_shipped_problems(
            args.shipped.read_text(encoding="utf-8"),
            args.lockfile.read_text(encoding="utf-8"),
            set(generated),
            lock - set(generated),
        )
    if problems:
        print("check-notices: refusing the generated Rust notices:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print(f"check-notices: {args.previous} is unchanged", file=sys.stderr)
        return 1
    dev_only = sorted(lock - set(generated))
    args.generated.write_bytes(supplemented.encode("utf-8"))
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


def cmd_shipped(args: argparse.Namespace) -> int:
    lock_text = args.lockfile.read_text(encoding="utf-8")
    trees = {(pkg, target): cargo_tree(pkg, target) for pkg, target, _ in SHIPPED}
    args.shipped.write_text(shipped_snapshot(lock_text, trees), encoding="utf-8")
    print(f"check-notices: wrote {args.shipped}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--lockfile", type=Path, default=LOCKFILE)
    parser.add_argument("--dev-only", type=Path, default=DEV_ONLY)
    parser.add_argument("--shipped", type=Path, default=SHIPPED_SNAPSHOT)
    sub = parser.add_subparsers(dest="command")
    check = sub.add_parser("check", help="compare the committed notices with Cargo.lock")
    check.add_argument("--notices", type=Path, default=NOTICES)
    check.add_argument("--ui-lockfile", type=Path, default=UI_LOCKFILE)
    check.add_argument("--ui-notices", type=Path, default=UI_NOTICES)
    refresh = sub.add_parser("refresh", help="validate a freshly generated notices file")
    refresh.add_argument("--generated", type=Path, required=True)
    refresh.add_argument("--previous", type=Path, default=NOTICES)
    refresh.add_argument("--supplement", type=Path, default=SUPPLEMENT)
    sub.add_parser("shipped", help="rewrite the shipped-crates snapshot with cargo tree")
    args = parser.parse_args(argv)
    if args.command == "refresh":
        return cmd_refresh(args)
    if args.command == "shipped":
        return cmd_shipped(args)
    if args.command is None:
        args.notices, args.ui_lockfile, args.ui_notices = NOTICES, UI_LOCKFILE, UI_NOTICES
    return cmd_check(args)


if __name__ == "__main__":
    sys.exit(main())
