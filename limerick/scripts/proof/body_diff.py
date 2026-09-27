#!/usr/bin/env python3
"""Diffs provider request bodies logged by `scripted_openai.py`.

Each request becomes one unit: its workload, its parameters (every body key
except `messages`) and each message's lines prefixed by role. The driver
writes a turn marker before each scenario line; within a turn, foreground
requests (intent, dialogue, reaction) keep their order and background
simulation requests, which race each other, are sorted and keyed by location
identity. Several logs per side form a range (see `differences.diff_units`).

    body_diff.py --base main.jsonl [--base ...] --head branch.jsonl [--head ...]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from differences import Item, Unit, diff_units  # noqa: E402

BACKGROUND = {"simulation", "other"}
CANONICAL_LOCATION_ID = re.compile(r"Canonical location \[location_id=(\d+)\]")
LOCATION_NAME = re.compile(r"^Location: (.+)$", re.MULTILINE)


def request_unit(label: str, entry: dict[str, Any]) -> Unit:
    body = entry.get("body", {})
    params = {key: value for key, value in body.items() if key != "messages"}
    lines = [f"params {json.dumps(params, sort_keys=True)}"]
    for message in body.get("messages", []):
        role = message.get("role", "?")
        content = str(message.get("content", ""))
        lines += [f"{role}| {line}" for line in content.split("\n")]
    return f"{label} ({entry.get('workload', '?')})", lines


def _background_identity(entry: dict[str, Any]) -> str:
    """Return a stable identity for a location's race-prone background request."""
    messages = entry.get("body", {}).get("messages", [])
    prompt = "\n".join(
        str(message.get("content", "")) for message in messages if message.get("role") == "user"
    )
    match = CANONICAL_LOCATION_ID.search(prompt)
    if match:
        return f"location_id={match.group(1)}"
    match = LOCATION_NAME.search(prompt)
    if match:
        return f"location={match.group(1).strip()}"
    digest = hashlib.sha256(prompt.encode("utf-8")).hexdigest()[:12]
    return f"prompt={digest}"


def load_requests(path: Path) -> list[Unit]:
    if not path.exists():
        return []
    turns: list[tuple[str, list[dict[str, Any]]]] = [("setup", [])]
    for raw in path.read_text().splitlines():
        if not raw.strip():
            continue
        entry = json.loads(raw)
        if "turn" in entry:
            turns.append((f"turn {entry['turn']}", []))
        else:
            turns[-1][1].append(entry)
    units: list[Unit] = []
    for turn, entries in turns:
        foreground = [e for e in entries if e.get("workload") not in BACKGROUND]
        background = [e for e in entries if e.get("workload") in BACKGROUND]
        ordered = [
            request_unit(f"{turn} request #{index}", entry)
            for index, entry in enumerate(foreground, start=1)
        ]
        identified_background = sorted(
            request_unit(f"{turn} background [{_background_identity(entry)}]", entry)
            for entry in background
        )
        occurrence: dict[str, int] = {}
        for label, lines in identified_background:
            occurrence[label] = occurrence.get(label, 0) + 1
            ordered.append((f"{label} #{occurrence[label]}", lines))
        units += ordered
    return units


def request_items(name: str, base_logs: list[Path], head_logs: list[Path]) -> list[Item]:
    base_runs = [load_requests(path) for path in base_logs]
    head_runs = [load_requests(path) for path in head_logs]
    return diff_units("requests", name, base_runs, head_runs)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", type=Path, action="append", required=True)
    parser.add_argument("--head", type=Path, action="append", required=True)
    args = parser.parse_args()
    items = request_items("requests", args.base, args.head)
    for item in items:
        print(item)
    head_count = len(load_requests(args.head[0]))
    print(f"head requests={head_count} differing lines={len(items)}")
    sys.exit(1 if items else 0)


if __name__ == "__main__":
    main()
