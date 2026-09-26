"""Difference items and the author's list of intended differences.

Every comparator turns a base and a head observation into `Item`s: one per
changed line, labelled with the surface (`script`, `requests`, `responses`),
the fixture or scenario name, and the unit (command, request, or turn).
`check` then matches each item against the intended-differences file; an
item nothing declares fails, and so does a required declaration that matches
nothing.

Intended-differences file (TOML):

    [[intended]]
    surface = "script"               # optional: script | requests | responses
    name = "test_walkthrough"        # optional fnmatch pattern on the name
    match = 'Tier 3 \\(simulated\\)' # regex searched in the item's text
    reason = "tier-3 lists are sorted by NPC id now"
    required = false                 # optional, default true: false only when
                                     # the change replaces random base output
                                     # that can match the head by chance
"""

from __future__ import annotations

import difflib
import fnmatch
import re
import sys
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path

if sys.version_info >= (3, 11):
    import tomllib
else:  # pragma: no cover - CI and the dev venv run 3.11+
    import tomli as tomllib

INTENDED_FENCE = "```toml intended-diffs"

# A unit is one comparable step (a command, a request, a turn): a short label
# and the normalised lines that describe it.
Unit = tuple[str, list[str]]


@dataclass(frozen=True)
class Item:
    surface: str
    name: str
    unit: str
    sign: str  # "+" only in head, "-" only in base
    text: str

    def __str__(self) -> str:
        return f"{self.surface}/{self.name} {self.unit} {self.sign} {self.text}"


@dataclass
class Intended:
    match: str
    reason: str
    surface: str | None = None
    name: str | None = None
    required: bool = True
    hits: int = 0
    pattern: re.Pattern[str] = field(init=False)

    def __post_init__(self) -> None:
        self.pattern = re.compile(self.match)

    def covers(self, item: Item) -> bool:
        if self.surface and self.surface != item.surface:
            return False
        if self.name and not fnmatch.fnmatch(item.name, self.name):
            return False
        return bool(self.pattern.search(str(item)))


def load_intended(path: Path | None) -> list[Intended]:
    if path is None:
        return []
    return parse_intended(path.read_text(), str(path))


def load_intended_markdown(path: Path) -> list[Intended]:
    """Reads every fenced ```toml intended-diffs block in a Markdown file (a
    PR body; `render-proof-comment.sh` writes the block from a bundle's
    `intended-diffs.toml`)."""
    blocks: list[str] = []
    current: list[str] | None = None
    for line in path.read_text().splitlines():
        if current is None:
            if line.strip() == INTENDED_FENCE:
                current = []
        elif line.strip() == "```":
            blocks.append("\n".join(current))
            current = None
        else:
            current.append(line)
    return [entry for block in blocks for entry in parse_intended(block, str(path))]


def parse_intended(text: str, source: str) -> list[Intended]:
    data = tomllib.loads(text)
    entries = []
    for raw in data.get("intended", []):
        unknown = set(raw) - {"match", "reason", "surface", "name", "required"}
        if unknown or "match" not in raw or not raw.get("reason"):
            raise SystemExit(f"{source}: each [[intended]] needs match and reason; got {raw}")
        entries.append(Intended(**raw))
    return entries


def check(items: Sequence[Item], intended: Sequence[Intended]) -> list[Item]:
    """Returns the undeclared items and counts hits on each declaration."""
    undeclared = []
    for item in items:
        matched = [entry for entry in intended if entry.covers(item)]
        for entry in matched:
            entry.hits += 1
        if not matched:
            undeclared.append(item)
    return undeclared


def diff_units(
    surface: str,
    name: str,
    base_runs: Sequence[Sequence[Unit]],
    head_runs: Sequence[Sequence[Unit]],
) -> list[Item]:
    """Line-level differences between the base runs and the head runs.

    Units are paired by label (command index, turn, request slot), first run
    to first run, so repetitive fixtures cannot mis-pair. A pair is unchanged
    when some base run and some head run agree on it. Where every run of
    both sides agrees, the pair is compared line by line, order included.
    Where a side's runs disagree (nondeterminism), each side is a range: a
    line is added only if every head run has it and no base run does, and
    removed only if every base run has it and no head run does.
    """
    base_options = _options(base_runs)
    head_options = _options(head_runs)
    base_ref = list(base_runs[0]) if base_runs else []
    head_ref = list(head_runs[0]) if head_runs else []

    items: list[Item] = []
    matcher = difflib.SequenceMatcher(
        a=[unit[0] for unit in base_ref], b=[unit[0] for unit in head_ref], autojunk=False
    )
    for _tag, a0, a1, b0, b1 in matcher.get_opcodes():
        pairs = list(zip(range(a0, a1), range(b0, b1), strict=False))
        for a, b in pairs:
            old_keys = {_key(unit) for unit in base_options[a]}
            if old_keys & {_key(unit) for unit in head_options[b]}:
                continue
            old = [unit[1] for unit in base_options[a]]
            new = [unit[1] for unit in head_options[b]]
            items += _range_items(surface, name, head_ref[b][0], old, new)
        for a in range(a0 + len(pairs), a1):
            items += _line_items(surface, name, base_ref[a][0], base_ref[a][1], [])
        for b in range(b0 + len(pairs), b1):
            items += _line_items(surface, name, head_ref[b][0], [], head_ref[b][1])
    return items


def unstable_units(base_runs: Sequence[Sequence[Unit]]) -> int:
    """Counts unit positions where the base runs disagree with each other."""
    if len(base_runs) < 2:
        return 0
    longest = max(len(run) for run in base_runs)
    return sum(
        1
        for index in range(longest)
        if len({_key(run[index]) if index < len(run) else None for run in base_runs}) > 1
    )


def _key(unit: Unit) -> str:
    return "\n".join(unit[1])


def _options(runs: Sequence[Sequence[Unit]]) -> list[list[Unit]]:
    """Per position, the unit each run has there (runs may differ in length)."""
    longest = max((len(run) for run in runs), default=0)
    return [[run[index] for run in runs if index < len(run)] for index in range(longest)]


def _range_items(
    surface: str, name: str, label: str, old: list[list[str]], new: list[list[str]]
) -> list[Item]:
    if all(o == old[0] for o in old) and all(n == new[0] for n in new):
        return _line_items(surface, name, label, old[0], new[0])
    old_any: set[str] = set().union(*old)
    new_any: set[str] = set().union(*new)
    old_all = set.intersection(*map(set, old))
    new_all = set.intersection(*map(set, new))
    items = [Item(surface, name, label, "-", line) for line in old[0] if line in old_all - new_any]
    items += [Item(surface, name, label, "+", line) for line in new[0] if line in new_all - old_any]
    return items


def _line_items(surface: str, name: str, label: str, old: list[str], new: list[str]) -> list[Item]:
    items = []
    for line in difflib.ndiff(old, new):
        sign = line[:1]
        if sign in "+-":
            items.append(Item(surface, name, label, sign, line[2:]))
    return items
