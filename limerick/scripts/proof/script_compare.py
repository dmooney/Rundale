#!/usr/bin/env python3
"""Compares `--script` fixture output between base and head runs.

Each fixture output line (one JSON object per command) becomes one unit:
every field except `new_log_lines` as `field: value` (multi-line text split
into `field| line` lines), then each log line, in output order. Script runs
replay byte-identically, so one run per side is an exact comparison; with
several, a side whose runs disagree is compared as a range and reported
(see `differences.diff_units`).

    script_compare.py --base DIR [--base ...] --head DIR [--head ...]

Each DIR holds `<fixture>.jsonl` files (stdout of `limerick-engine --script`)
and `<fixture>.exit` files with the exit code.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from differences import Item, Unit, diff_units  # noqa: E402


def script_units(path: Path) -> list[Unit]:
    units: list[Unit] = []
    exit_file = path.with_suffix(".exit")
    if exit_file.exists():
        units.append(("exit", [f"exit: {exit_file.read_text().strip()}"]))
    if not path.exists():
        return units
    for index, raw in enumerate(path.read_text(errors="replace").splitlines(), 1):
        try:
            record = json.loads(raw)
        except json.JSONDecodeError:
            units.append((f"line {index}", [f"raw: {raw}"]))
            continue
        lines = []
        for key, value in record.items():
            if key == "new_log_lines":
                continue
            if isinstance(value, str) and "\n" in value:
                lines += [f"{key}| {line}" for line in value.split("\n")]
            else:
                lines.append(f"{key}: {json.dumps(value, ensure_ascii=False)}")
        lines += [
            f"log: {part}"
            for line in record.get("new_log_lines") or []
            for part in line.split("\n")
        ]
        units.append((f"cmd {index} {record.get('command', '')!r}", lines))
    return units


def fixture_items(name: str, base_dirs: list[Path], head_dirs: list[Path]) -> list[Item]:
    base_runs = [script_units(d / f"{name}.jsonl") for d in base_dirs]
    head_runs = [script_units(d / f"{name}.jsonl") for d in head_dirs]
    return diff_units("script", name, base_runs, head_runs)


def fixture_names(dirs: list[Path]) -> list[str]:
    return sorted({p.stem for d in dirs for p in d.glob("*.jsonl")})


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", type=Path, action="append", required=True)
    parser.add_argument("--head", type=Path, action="append", required=True)
    args = parser.parse_args()
    names = fixture_names(args.base + args.head)
    items: list[Item] = []
    for name in names:
        items += fixture_items(name, args.base, args.head)
    for item in items:
        print(item)
    flagged = len({item.name for item in items})
    print(
        f"fixtures={len(names)} base_runs={len(args.base)} head_runs={len(args.head)} flagged={flagged}"
    )
    sys.exit(1 if items else 0)


if __name__ == "__main__":
    main()
