"""Prompt variants: what the research loop may change, and the guard on facts.

A variant holds only presentation: instructions text, layout, section order,
labels and the wording of the present-moment cues. Every fact comes from
content/ through promptlab.render, so a variant cannot change the world. The
guard below also refuses rewrites that smuggle new facts (world names, numbers)
into the editable text.
"""

from __future__ import annotations

import copy
import json
import re
from pathlib import Path
from typing import Any

from . import content

LAYOUTS = ("json", "prose")
COMMON_KNOWLEDGE = ("shared", "shaded")
SECTIONS = ("setting", "village", "character", "moment")
LABEL_KEYS = (*SECTIONS, "player_line")
WORDING_KEYS = ("stranger", "addressing", "anachronism_flag")

# The loop may change these.
EDITABLE = ("instructions", "layout", "common_knowledge", "labels", "wording")
# Experiment switches set by a person (§6.1.3, §8.3); the loop keeps them.
LOCKED = ("backstory", "anachronism_flag")

# Lower camel case, ASCII letters only, so every JSON encoder sorts them alike.
_LABEL = re.compile(r"^[a-z][A-Za-z]{0,40}$")


def section_order(labels: dict) -> list[str]:
    """Label keys in the order production sends them.

    The phone re-encodes Endpoint input with sorted keys at every level
    (mobile/RundaleBridge LimerickRuntime.swift, `.sortedKeys`), so the key
    names, not the renderer, decide the order the model reads.
    """
    return sorted(labels, key=lambda key: labels[key])


class VariantError(ValueError):
    pass


def load(path: Path) -> dict:
    variant = json.loads(path.read_text(encoding="utf-8"))
    validate(variant)
    return variant


def load_named(name: str) -> dict:
    return load(content.PROMPTS_DIR / name / "variant.json")


def validate(variant: dict) -> None:
    def need(cond: bool, msg: str) -> None:
        if not cond:
            raise VariantError(msg)

    need(isinstance(variant.get("name"), str) and variant["name"], "name must be a string")
    need(variant.get("layout") in LAYOUTS, f"layout must be one of {LAYOUTS}")
    need(
        variant.get("common_knowledge") in COMMON_KNOWLEDGE,
        f"common_knowledge must be one of {COMMON_KNOWLEDGE}",
    )
    need(isinstance(variant.get("backstory"), bool), "backstory must be a boolean")
    need(isinstance(variant.get("anachronism_flag"), bool), "anachronism_flag must be a boolean")
    labels: Any = variant.get("labels")
    need(
        isinstance(labels, dict) and set(labels) == set(LABEL_KEYS),
        f"labels must have exactly {LABEL_KEYS}",
    )
    need(
        all(isinstance(v, str) and _LABEL.match(v) for v in labels.values()),
        "labels must be short plain names",
    )
    need(len(set(labels.values())) == len(labels), "labels must be distinct")
    need(
        sorted(labels.values()) == sorted(labels.values(), key=str.lower),
        "labels must sort the same with and without case",
    )
    need(
        section_order(labels)[-2:] == ["moment", "player_line"],
        "the moment and then the player line must sort after every static section:"
        " static content first, so provider caching can reuse it (§5)",
    )
    wording: Any = variant.get("wording")
    need(
        isinstance(wording, dict) and set(wording) == set(WORDING_KEYS),
        f"wording must have exactly {WORDING_KEYS}",
    )
    need(
        isinstance(wording["stranger"], str) and wording["stranger"].strip() != "",
        "wording.stranger must be text",
    )
    need(
        isinstance(wording["anachronism_flag"], str) and wording["anachronism_flag"].strip() != "",
        "wording.anachronism_flag must be text",
    )
    addressing: Any = wording["addressing"]
    need(
        isinstance(addressing, dict) and set(addressing) == set(content.ADDRESSING),
        f"wording.addressing must have exactly {content.ADDRESSING}",
    )
    need(
        all(isinstance(v, str) and v.strip() for v in addressing.values()),
        "wording.addressing values must be text",
    )
    need(
        isinstance(variant.get("instructions"), str) and len(variant["instructions"]) >= 200,
        "instructions must be substantial text",
    )
    need(len(variant["instructions"]) <= 12000, "instructions must stay under 12000 characters")


def editable_text(variant: dict) -> str:
    wording = variant["wording"]
    parts = [
        variant["instructions"],
        *variant["labels"].values(),
        wording["stranger"],
        wording["anachronism_flag"],
        *wording["addressing"].values(),
    ]
    return "\n".join(parts)


def _strings(value: object) -> list[str]:
    if isinstance(value, str):
        return [value]
    if isinstance(value, dict):
        return [s for k, v in value.items() if not str(k).startswith("_") for s in _strings(v)]
    if isinstance(value, list):
        return [s for v in value for s in _strings(v)]
    return []


# Capitalised only at the start of a sentence or a topic heading in content/,
# or ordinary words a brief may need; not facts.
_NOT_FACTS = {
    "Their",
    "Reads",
    "Soldiers",
    "Walking",
    "Getting",
    "Land",
    "Language",
    "English",
    "Irish",
    "Mass",
    "May",
}


def fact_vocabulary() -> set[str]:
    """Proper nouns from the fixed content: words it only ever capitalises."""
    text = "\n".join(
        _strings([content.characters(), content.village(), content.common_knowledge()])
    )
    words = set(re.findall(r"[\wÁÉÍÓÚáéíóú'’-]+", text))
    vocab = {w for w in words if w[0].isupper() and w.lower() not in words and w not in _NOT_FACTS}
    # Names in the cases count too: a rewrite naming a test line's person
    # ("there is no Father Murphy") is tailoring, not a better brief. Only
    # mid-sentence capitals are taken, so sentence openers are not names.
    case_text = "\n".join(t for c in content.all_cases() for t in (c.player_line, c.expect))
    for name in re.findall(r"(?<=[\w,;] )[A-ZÁÉÍÓÚ][\wáéíóú'’-]+", case_text):
        if name.lower() not in words and name not in _NOT_FACTS:
            vocab.add(name)
    return vocab


def fact_guard(start: dict, candidate: dict) -> list[str]:
    """Problems with a rewrite: fact words or numbers the starting variant lacked."""
    problems: list[str] = []
    before, after = editable_text(start), editable_text(candidate)
    vocab = fact_vocabulary()
    have = set(re.findall(r"[\wáéíóú'’-]+", before))
    for word in sorted(set(re.findall(r"[\wáéíóú'’-]+", after)) - have):
        if word in vocab:
            problems.append(f"introduces the world fact word {word!r}")
    if set(re.findall(r"\d+", after)) - set(re.findall(r"\d+", before)):
        problems.append(
            "introduces numbers; facts with numbers belong in content/, not the instructions"
        )
    for key in LOCKED:
        if candidate.get(key) != start.get(key):
            problems.append(f"changes the locked experiment switch {key!r}")
    return problems


def apply_rewrite(start: dict, current: dict, proposal: dict, name: str) -> dict:
    """Merge a rewriter proposal onto the current variant, keeping locked fields."""
    merged = copy.deepcopy(current)
    for key in EDITABLE:
        if key in proposal:
            merged[key] = proposal[key]
    for key in LOCKED:
        merged[key] = start[key]
    merged["name"] = name
    merged["about"] = f"Research rewrite of {start['name']}."
    validate(merged)
    problems = fact_guard(start, merged)
    if problems:
        raise VariantError("; ".join(problems))
    return merged
