"""Render a case into the production request shape (§5).

The production model reads the Endpoint instructions as the system
instruction, one user part `Endpoint input values:\\n<JSON>`, and the output
schema as the response JSON schema
(endpoints/packages/providers/src/google/adapter.ts). Both prompt families
render into that shape so what works here works in production.
"""

from __future__ import annotations

import datetime as dt
import json
from dataclasses import dataclass

from . import content
from .content import Case
from .variant import section_order

BASELINE = "baseline"
CHAR_IDS = {"peig": 1, "micheal": 2, "roisin": 3}
PLACE_IDS = {"Kilteevan Village": 1, "Letter Office": 2, "Connolly Cottage": 3}


@dataclass(frozen=True)
class Request:
    system: str
    values: dict
    output_schema: dict

    @property
    def user(self) -> str:
        # JSON.stringify(values) of keys the phone sorted: compact, non-ASCII kept.
        return "Endpoint input values:\n" + json.dumps(
            self.values, ensure_ascii=False, separators=(",", ":"), sort_keys=True
        )

    @property
    def size_chars(self) -> int:
        return len(self.system) + len(self.user)


def output_schema() -> dict:
    return content.v1_endpoint()["outputSchema"]


def render(variant: dict | str, case: Case) -> Request:
    if variant == BASELINE:
        return render_baseline(case)
    assert isinstance(variant, dict)
    return render_brief(variant, case)


# --- baseline: v1 instructions and the v1 input the engine builds -------------


def _place_description(name: str, case: Case) -> str:
    world = json.loads((content.MOD_DIR / "world.json").read_text(encoding="utf-8"))
    loc = next(loc for loc in world["locations"] if loc["name"] == name)
    return (
        loc["description_template"]
        .replace("{weather}", _weather_display(case.weather))
        .replace("{time}", _time_display(content.part_of_day(case.clock)))
    )


def _weather_display(weather: str) -> str:
    """Mirror of limerick-world `description::weather_display`."""
    return {
        "Partly Cloudy": "partly cloudy",
        "Light Rain": "rainy",
        "Heavy Rain": "rain-soaked",
    }.get(weather, weather.lower())


def _time_display(part: str) -> str:
    """Mirror of limerick-world `description::time_display`."""
    return "late night" if part == "Midnight" else part.lower()


def _grounded_person(key: str) -> dict:
    char = content.characters()[key]
    return {"displayName": char["name"], "id": f"npc-{CHAR_IDS[key]}", "role": char["occupation"]}


def _exchanges(case: Case) -> list[dict]:
    """Pair each player line with the character reply after it (v1 shape)."""
    out: list[dict] = []
    turns = list(case.conversation)
    for i, turn in enumerate(turns):
        if turn.who != "player" or i + 1 >= len(turns) or turns[i + 1].who == "player":
            continue
        reply = turns[i + 1]
        out.append(
            {
                "location": PLACE_IDS[case.location],
                "npc_dialogue": reply.text,
                "player_input": turn.text,
                "speaker_id": CHAR_IDS[reply.who],
                "speaker_name": content.characters()[reply.who]["name"],
                "timestamp": f"{case.date}T{case.clock}:00Z",
            }
        )
    return out


def render_baseline(case: Case) -> Request:
    char = content.characters()[case.speaker]
    values = {
        "addressing": case.addressing,
        "attemptID": "lab-attempt",
        "authoredFacts": [
            {
                "id": f"npc-{CHAR_IDS[case.speaker]}-knowledge-{i + 1}",
                "source": "npcs.json",
                "statement": s,
            }
            for i, s in enumerate(char["knows"])
        ],
        "baseRevision": {"rawValue": 0},
        "contractVersion": {"major": 1, "minor": 0},
        "currentLocation": {
            "description": _place_description(case.location, case),
            "displayName": case.location,
            "id": f"place-{PLACE_IDS[case.location]}",
            "playable": True,
        },
        "festival": case.festival,
        "idempotencyKey": "lab-logical-request:lab-attempt",
        "knownPeople": [_grounded_person(k) for k in CHAR_IDS],
        "knownPlaces": [
            {
                "description": _place_description(name, case),
                "displayName": name,
                "id": f"place-{pid}",
                "playable": True,
            }
            for name, pid in PLACE_IDS.items()
        ],
        "logicalRequestID": "lab-logical-request",
        "maxOutputChars": 8192,
        "maxStreamBytes": 16384,
        "player": {"knowsName": False, "name": None},
        "playerInput": case.player_line,
        "present": [_grounded_person(k) for k in case.present],
        "recentConversation": _exchanges(case),
        "role": "npc_dialogue",
        "sessionID": "lab-session",
        "speaker": {
            "currentLocationId": PLACE_IDS[case.location],
            "currentLocationName": case.location,
            "displayName": char["name"],
            "id": f"npc-{CHAR_IDS[case.speaker]}",
            "mood": char["mood"],
            "role": char["occupation"],
        },
        "time": {
            "clockTime": case.clock,
            "date": case.date,
            "partOfDay": content.part_of_day(case.clock),
        },
        "weather": case.weather,
    }
    return Request(content.v1_endpoint()["instructions"], values, output_schema())


# --- brief: §4 content, laid out as the variant says ---------------------------


def _setting(variant: dict, case: Case) -> tuple[dict, str]:
    ck = content.common_knowledge()
    data: dict = {item["topic"]: item["text"] for item in ck["shared"]}
    prose = "\n".join(f"{item['topic']}: {item['text']}" for item in ck["shared"])
    if variant["common_knowledge"] == "shaded":
        note = ck["shaded"][case.speaker]
        data["How you know it"] = note
        prose += f"\nHow you know it: {note}"
    return data, prose


def _village() -> tuple[dict, str]:
    v = content.village()
    places = []
    lines = [f"Your village is {v['name']}."]
    for p in v["places"]:
        entry = {"name": p["name"], "looks": p["looks"], "ways": p["ways"]}
        line = f"{p['name']}: {p['looks']} From here: {'; '.join(p['ways'])}."
        if p["story"]:
            entry["story"] = p["story"]
            line += f" The old story: {p['story']}"
        places.append(entry)
        lines.append(line)
    people = []
    for person in v["people"]:
        people.append(dict(person))
        lines.append(
            f"{person['name']}, {person['looks']}, {person['work']}; lives at {person['home']}."
        )
    feasts = [f"{f['name']}, {f['date']}: {f['about']}" for f in v["festivals"]]
    lines.append("The feasts of the year: " + "; ".join(feasts) + ".")
    return {
        "name": v["name"],
        "places": places,
        "people": people,
        "feastsOfTheYear": feasts,
    }, "\n".join(lines)


def _character(variant: dict, case: Case) -> tuple[dict, str]:
    c = content.characters()[case.speaker]
    data = {
        "name": c["name"],
        "age": c["age"],
        "pronouns": c["pronouns"],
        "appearance": c["appearance"],
        "occupation": c["occupation"],
        "personality": c["personality"],
        "home": c["home"],
        "workplace": c["workplace"],
        "yourDayThisSeason": c["day"],
        "yourPeople": [f"{r['who']}: {r['kind']}; {r['feeling']}" for r in c["relationships"]],
        "whatYouKnow": c["knows"],
        "mood": c["mood"],
    }
    lines = [
        f"You are {c['name']}, {c['age']} ({c['pronouns']}), {c['appearance']}. {c['occupation']}.",
        c["personality"],
        f"You live at {c['home']} and work at {c['workplace']}.",
        "Your day this season: " + "; ".join(c["day"]) + ".",
        "Your people: " + "; ".join(data["yourPeople"]) + ".",
        "What you know: " + " ".join(c["knows"]),
        f"Your mood: {c['mood']}.",
    ]
    if variant["backstory"]:
        data["yourPast"] = c["backstory"]
        lines.insert(2, "Your past: " + " ".join(c["backstory"]))
    return data, "\n".join(lines)


def _moment(variant: dict, case: Case) -> tuple[dict, str]:
    chars = content.characters()
    wording = variant["wording"]
    activity, _ = content.activity_at(case.speaker, case.clock)
    date = dt.date.fromisoformat(case.date)
    when = f"{date.strftime('%A')} {date.day} {date.strftime('%B %Y')}, {case.clock} ({content.part_of_day(case.clock).lower()})"
    others = [f"{chars[k]['name']}, {content.activity_at(k, case.clock)[0]}" for k in case.present]

    def who(key: str) -> str:
        if key == "player":
            return "the stranger"
        return "you" if key == case.speaker else chars[key]["name"]

    convo = [{"speaker": who(t.who), "said": t.text} for t in case.conversation]
    data: dict = {
        "where": case.location,
        "whatYouAreDoing": activity,
        "when": when,
        "weather": case.weather,
        "feastToday": case.festival or "none",
        "alsoHere": others or ["nobody else"],
        "theStranger": wording["stranger"],
        "howYouCameToAnswer": wording["addressing"][case.addressing],
        "justSaid": convo,
    }
    lines = [
        f"You are at {case.location}, {activity}. It is {when}. The weather: {case.weather.lower()}. Feast today: {case.festival or 'none'}.",
        "Also here: " + ("; ".join(others) if others else "nobody else") + ".",
        f"The stranger: {wording['stranger']}",
        wording["addressing"][case.addressing],
    ]
    if convo:
        lines.append(
            "Just said: "
            + " ".join(
                f"{c['speaker'][:1].upper() + c['speaker'][1:]}: “{c['said']}”" for c in convo
            )
        )
    if variant["anachronism_flag"] and case.anachronism:
        data["notice"] = wording["anachronism_flag"]
        lines.append(wording["anachronism_flag"])
    return data, "\n".join(lines)


def render_brief(variant: dict, case: Case) -> Request:
    labels = variant["labels"]
    builders = {
        "setting": lambda: _setting(variant, case),
        "village": _village,
        "character": lambda: _character(variant, case),
        "moment": lambda: _moment(variant, case),
    }
    values: dict = {}
    for section in section_order(labels):
        if section == "player_line":
            values[labels[section]] = case.player_line
            continue
        data, prose = builders[section]()
        values[labels[section]] = data if variant["layout"] == "json" else prose
    return Request(variant["instructions"], values, output_schema())


def judge_reference(case: Case) -> dict:
    """Every fact the character could draw on, independent of the variant.

    Judges score against this, not against the prompt the model saw, so a
    prompt that leaves a fact out is marked down for what it misses (B3).
    """
    from . import variant as variant_mod

    canonical = variant_mod.load_named("brief")
    canonical = {
        **canonical,
        "layout": "json",
        "common_knowledge": "shaded",
        "backstory": True,
        "anachronism_flag": False,
    }
    return render_brief(canonical, case).values
