"""Normalisers for known run-to-run nondeterminism on `main`.

Each normaliser masks one source that differs between two runs of the same
build. Item 2 of #2033 removes these at the source; when a source is fixed,
delete its normaliser here so the comparison becomes exact again.
"""

from __future__ import annotations

import re
from collections.abc import Iterable
from pathlib import Path

# "Tier 3 (simulated): a, b" and "Here: a 🙂, b [mood]" debug lines list NPCs
# in HashMap order.
TIER_LIST = re.compile(r"^(\s*(?:Tier \d \([^)]*\)|Here):)\s*(.*)$")
NAME_DECORATION = re.compile(r"\s*\S*[☀-\U0001faff]\S*|\s*\[[^\]]*\]")
# Save listings carry the wall-clock save time.
SAVE_TIME = re.compile(r"saved: \d+ \w+ \d+:\d+")
SAVE_GAME_TIME = re.compile(r"#\d+ — game: \S+")
# Emoji reactions are rolled with unseeded dice.
EMOJI_TAIL = re.compile(r"[☀-\U0001faff]\s*$")
# Travel encounters are rolled from wall-clock-seeded game time; their text
# comes from fixed tables, and the lines are shown with this prefix.
ENCOUNTER_PREFIX = "  · "
QUOTED = re.compile(r'"((?:[^"\\]|\\.){20,})"')
# The live game clock runs in real time from session creation until the
# scenario's /pause, so game timestamps carry wall-clock seconds.
GAME_SECONDS = re.compile(r"(\d{4}-\d\d-\d\dT\d\d:\d\d):\d\d(\.\d+)?Z")
# Tier-3 simulation prompts list NPC blocks ("- [id] Name, ..." plus indented
# continuation lines) in HashMap order.
TIER3_HEADER = "NPCs (id in brackets"


def encounter_texts(trees: Iterable[Path]) -> set[str]:
    """Fixed encounter texts from each tree's encounter tables."""
    texts: set[str] = set()
    for tree in trees:
        for rel in (
            "limerick/crates/limerick-world/src/wayfarers.rs",
            "limerick/crates/limerick-world/src/encounter.rs",
            "mods/rundale/encounters.json",
        ):
            path = tree / rel
            if path.exists():
                texts |= set(QUOTED.findall(path.read_text()))
    return texts


def mask_times(line: str) -> str:
    """Masks wall-clock save times and game times that carry wall seconds."""
    line = SAVE_TIME.sub("saved: <wall-clock>", line)
    return SAVE_GAME_TIME.sub("#N — game: <t>", line)


def script_log_line(line: str, encounters: set[str]) -> str | None:
    """Normalises one `--script` log line; `None` drops it."""
    tier = TIER_LIST.match(line)
    if tier:
        names = sorted(
            NAME_DECORATION.sub("", name).strip() for name in tier.group(2).rstrip(",").split(",")
        )
        return f"{tier.group(1)} {', '.join(names)}"
    line = mask_times(line)
    if line.startswith(ENCOUNTER_PREFIX) or line in encounters:
        return None
    if EMOJI_TAIL.search(line):
        return None
    return line


def debug_text(text: str) -> list[str]:
    """Normalises a `/debug` response.

    Its NPC lists ("Here:", "Tier N", "/debug here" rows, relationship ties)
    come out in HashMap order, so its lines are compared as a sorted list with
    each name list sorted.
    """
    return sorted(script_log_line(line, set()) or "" for line in text.split("\n"))


def script_log(lines: list[str], encounters: set[str]) -> list[str]:
    """Normalises one command's log lines.

    NPC departure and arrival lines within one command come out in HashMap
    order, so the command's lines are compared as a sorted list.
    """
    split = [part for line in lines for part in line.split("\n")]
    kept = (script_log_line(line, encounters) for line in split)
    return sorted(line for line in kept if line is not None)


def game_seconds(text: str) -> str:
    return GAME_SECONDS.sub(r"\1:<s>Z", text)


def prompt_text(text: str) -> str:
    """Sorts the NPC blocks of a tier-3 simulation prompt."""
    if TIER3_HEADER not in text:
        return text
    out: list[str] = []
    blocks: list[list[str]] = []
    in_list = False
    for line in text.split("\n"):
        if line.startswith(TIER3_HEADER):
            in_list = True
            out.append(line)
            continue
        if in_list and line.startswith("- "):
            blocks.append([line])
            continue
        if in_list and blocks and line.startswith("  "):
            blocks[-1].append(line)
            continue
        if in_list:
            out.extend(sum(sorted(blocks), []))
            blocks = []
            in_list = False
        out.append(line)
    out.extend(sum(sorted(blocks), []))
    return "\n".join(out)
