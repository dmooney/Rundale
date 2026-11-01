"""Fixed lab content: characters, village, common knowledge, cases, requirements.

Paths resolve from this file, never from the working directory.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from functools import cache
from pathlib import Path

LAB_DIR = Path(__file__).resolve().parent.parent
REPO_DIR = LAB_DIR.parents[1]
CONTENT_DIR = LAB_DIR / "content"
CASES_PATH = LAB_DIR / "cases" / "cases.json"
PROMPTS_DIR = LAB_DIR / "prompts"
REQUIREMENTS_DOC = REPO_DIR / "docs" / "design" / "npc-dialogue-prompt.md"
V1_ENDPOINT = REPO_DIR / "mods" / "rundale" / "endpoints" / "rundale-dialogue.v1.json"
MOD_DIR = REPO_DIR / "mods" / "rundale"

SPLITS = ("train", "heldout")


def _load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


@cache
def characters() -> dict[str, dict]:
    return _load(CONTENT_DIR / "characters.json")["characters"]


@cache
def village() -> dict:
    return _load(CONTENT_DIR / "village.json")


@cache
def common_knowledge() -> dict:
    return _load(CONTENT_DIR / "common-knowledge.json")


@cache
def v1_endpoint() -> dict:
    return _load(V1_ENDPOINT)


@dataclass(frozen=True)
class Turn:
    who: str  # "player" or a character key
    text: str


@dataclass(frozen=True)
class Case:
    id: str
    split: str
    tags: tuple[str, ...]
    speaker: str
    location: str
    clock: str
    player_line: str
    expect: str
    date: str
    weather: str
    festival: str | None
    addressing: str
    anachronism: bool
    present: tuple[str, ...] = ()
    conversation: tuple[Turn, ...] = field(default_factory=tuple)
    probes: tuple[int, ...] = ()


ADDRESSING = ("spoken_to", "part_of_all", "joining_mentioned")


def _case(raw: dict, defaults: dict) -> Case:
    merged = {**defaults, **raw}
    return Case(
        id=merged["id"],
        split=merged["split"],
        tags=tuple(merged["tags"]),
        speaker=merged["speaker"],
        location=merged["location"],
        clock=merged["clock"],
        player_line=merged["player_line"],
        expect=merged["expect"],
        date=merged["date"],
        weather=merged["weather"],
        festival=merged["festival"],
        addressing=merged["addressing"],
        anachronism=bool(merged["anachronism"]),
        present=tuple(merged.get("present", ())),
        conversation=tuple(Turn(t["who"], t["text"]) for t in merged.get("conversation", ())),
        probes=tuple(merged.get("probes", ())),
    )


def all_cases() -> list[Case]:
    data = _load(CASES_PATH)
    return [_case(raw, data["defaults"]) for raw in data["cases"]]


def cases(split: str, *, tags: set[str] | None = None, ids: set[str] | None = None) -> list[Case]:
    """Cases of one split, optionally narrowed. Callers pick the split explicitly."""
    if split not in SPLITS:
        raise ValueError(f"unknown split {split!r}")
    chosen = [c for c in all_cases() if c.split == split]
    if tags:
        chosen = [c for c in chosen if tags & set(c.tags)]
    if ids:
        chosen = [c for c in chosen if c.id in ids]
    return chosen


_REQ_ROW = re.compile(r"^\|\s*(B\d+)\s*\|\s*\*\*(.+?)\*\*\s*(.+?)\s*\|")


@cache
def requirements() -> dict[str, str]:
    """B1..B20 from the requirements doc's §3 table, the single source."""
    reqs: dict[str, str] = {}
    for line in REQUIREMENTS_DOC.read_text(encoding="utf-8").splitlines():
        match = _REQ_ROW.match(line)
        if match:
            reqs[match.group(1)] = f"{match.group(2)} {match.group(3)}"
    return reqs


def part_of_day(clock: str) -> str:
    """Mirror of limerick-types `time_of_day_from_hour`."""
    hour = int(clock.split(":")[0])
    if 5 <= hour <= 6:
        return "Dawn"
    if 7 <= hour <= 11:
        return "Morning"
    if 12 <= hour <= 13:
        return "Midday"
    if 14 <= hour <= 16:
        return "Afternoon"
    if 17 <= hour <= 18:
        return "Dusk"
    if 19 <= hour <= 22:
        return "Night"
    return "Midnight"


def activity_at(key: str, clock: str) -> tuple[str, str]:
    """(activity, location) from the character's day at this clock time."""
    minutes = _minutes(clock)
    for entry in characters()[key]["day"]:
        span, rest = entry.split(" ", 1)
        start, end = span.split("-")
        if _minutes(start) <= minutes <= _minutes(end):
            activity, _, place = rest.rpartition(" (")
            return activity, place.rstrip(")")
    raise ValueError(f"{key} has no day entry at {clock}")


def _minutes(clock: str) -> int:
    hours, minutes = clock.split(":")
    return int(hours) * 60 + int(minutes)
