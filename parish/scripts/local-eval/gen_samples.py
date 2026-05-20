#!/usr/bin/env python3
"""Generate 2 production-faithful samples per inference category.

Mirrors the prompts from `parish/crates/parish-inference/examples/inf_bench.rs`.

Two targets — `small` and `large` — handle Intent/Reaction/Simulation and
Dialogue respectively. Each target is a `model@base_url[#env:VAR]` spec
(see `eval_lib.parse_target`). Defaults reproduce the original two-slot
Apple Silicon vllm-mlx loadout.

Examples::

    # default vllm-mlx two-slot loadout
    python3 gen_samples.py

    # cross-provider: Groq small slot, Claude large slot
    python3 gen_samples.py \\
        --small 'llama-3.1-8b-instant@https://api.groq.com/openai/v1#env:PARISH_GROQ_API_KEY' \\
        --large 'claude-sonnet-4-6@https://api.anthropic.com/v1#env:ANTHROPIC_API_KEY' \\
        --output docs/proofs/local-perf/category_samples_xprovider.md
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from eval_lib import (  # noqa: E402
    CostTracker,
    Target,
    build_dialogue_system_prompt,
    call_chat,
    parse_target,
)

DEFAULT_SMALL = "mlx-community/Qwen2.5-1.5B-Instruct-4bit@http://localhost:8001/v1"
DEFAULT_LARGE = "mlx-community/Qwen2.5-7B-Instruct-4bit@http://localhost:8000/v1"
DEFAULT_OUTPUT = (
    Path(__file__).resolve().parents[3]
    / "docs"
    / "proofs"
    / "local-perf"
    / "category_samples.md"
)

INTENT_SYS = (
    "You are a text adventure input parser. Given the player's natural language input, "
    "determine their intent. Respond with valid JSON containing:\n"
    '- "intent": one of "move", "talk", "look", "interact", "examine", "unknown"\n'
    '- "target": what the action is directed at (string or null)\n'
    '- "dialogue": what the player is saying, if talking (string or null)\n\n'
    'IMPORTANT: "move" is ONLY for when the player expresses a present desire to '
    "navigate somewhere (imperative or future intent). Narrative, past-tense, or "
    'reflective statements that merely mention a place name are "talk", not "move".\n\n'
    "Examples:\n"
    'Input: "go to the pub" → {"intent": "move", "target": "the pub", "dialogue": null}\n'
    'Input: "talk to Mary" → {"intent": "talk", "target": "Mary", "dialogue": null}\n'
    'Input: "tell Padraig I saw his cow" → {"intent": "talk", "target": "Padraig", "dialogue": "I saw his cow"}\n'
    'Input: "look around" → {"intent": "look", "target": null, "dialogue": null}\n'
    'Input: "pick up the stone" → {"intent": "interact", "target": "the stone", "dialogue": null}\n'
    'Input: "I came from the coast" → {"intent": "talk", "target": null, "dialogue": "I came from the coast"}\n'
    'Input: "I was at the shore yesterday" → {"intent": "talk", "target": null, "dialogue": "I was at the shore yesterday"}\n\n'
    "Respond ONLY with valid JSON. No explanation."
)

INTENT_SCHEMA = {
    "name": "intent",
    "strict": True,
    "schema": {
        "type": "object",
        "additionalProperties": False,
        "properties": {
            "intent": {"type": "string"},
            "target": {"type": ["string", "null"]},
            "dialogue": {"type": ["string", "null"]},
        },
        "required": ["intent", "target", "dialogue"],
    },
}

REACTION_SYS = (
    "You are Padraig Darcy, a 58-year-old Publican in rural Ireland, 1820.\n"
    "A gruff but warm-hearted publican who has run Darcy's Pub for thirty years. "
    "Known for his dry wit.\n"
    "Current mood: content\n\n"
    "Write a single brief greeting or reaction (1-2 sentences max). "
    "Dialogue only, no narration or action descriptions. "
    "Do not use any modern language."
)

TIER2_USER = (
    "You are simulating background interactions between characters in a small "
    "Irish parish in 1820.\n\n"
    "Location: Darcy's Pub\n"
    "Time: Evening\n"
    "Weather: Clear.\n\n"
    "Dramatis personae (id in brackets — reuse these in your JSON):\n"
    "- [1] Padraig Darcy, Publican. Currently content. He is even-tempered and well-spoken. "
    "He's known Niamh his whole life.\n"
    "- [2] Niamh Darcy, Barmaid. Currently tired. She is quick-witted and observant. "
    "She is Padraig's daughter.\n"
    "- [3] Sean Murphy, Farmer. Currently hungry. He is plain-spoken and stubborn.\n\n"
    "Write one short sentence (max 20 words) describing what these characters are "
    "doing right now. Most exchanges are uneventful — leave mood_changes and "
    "relationship_changes as empty arrays unless a character's mood has clearly "
    "shifted or a relationship has meaningfully strengthened or strained.\n\n"
    "Respond with a JSON object, using the bracketed ids. Default shape (use this "
    "when nothing notable changes):\n"
    '{"summary": "...", "mood_changes": [], "relationship_changes": []}\n\n'
    "Only when something actually changes, include entries:\n"
    '  mood_changes:        {"npc_id": <id>, "new_mood": "<mood>"}\n'
    '  relationship_changes: {"from": <id>, "to": <id>, "delta": <-0.1 to 0.1>}'
)

TIER2_SCHEMA = {
    "name": "tier2_simulation",
    "strict": True,
    "schema": {
        "type": "object",
        "additionalProperties": False,
        "properties": {
            "summary": {"type": "string"},
            "mood_changes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": False,
                    "properties": {
                        "npc_id": {"type": "integer"},
                        "new_mood": {"type": "string"},
                    },
                    "required": ["npc_id", "new_mood"],
                },
            },
            "relationship_changes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": False,
                    "properties": {
                        "from": {"type": "integer"},
                        "to": {"type": "integer"},
                        "delta": {"type": "number"},
                    },
                    "required": ["from", "to", "delta"],
                },
            },
        },
        "required": ["summary", "mood_changes", "relationship_changes"],
    },
}

TIER3_USER = (
    "You are simulating background NPC activity in a rural Irish parish in 1820. "
    "Simulate 6 hours of activity for the people below. "
    "The weather is Clear, the season is Summer, the time is afternoon.\n\n"
    "NPCs (id in brackets — reuse these in your JSON):\n"
    "- [1] Padraig Darcy, 58, Publican — at Darcy's Pub, content (even-tempered, well-spoken).\n"
    "  Known Niamh his whole life; long-standing friendship with Tommy Maguire.\n"
    "- [2] Niamh Darcy, 24, Barmaid — at Darcy's Pub, tired (quick-witted, observant).\n"
    "  Daughter of Padraig.\n"
    "- [3] Sean Murphy, 41, Farmer — at the bog, hungry (plain-spoken, stubborn).\n"
    "- [4] Tommy Maguire, 62, Farmer — at the crossroads, restless (storyteller).\n"
    "- [5] Brigid O'Brien, 42, Midwife — at her cottage, focused (kind, direct, knowledgeable).\n"
    "- [6] Father Cathal, 51, Priest — at the church, contemplative (eloquent, severe).\n\n"
    "For each NPC, return one update describing their mood, what they did, "
    "whether they moved, and any relationship shifts. Respond with JSON, "
    "using the bracketed ids:\n"
    '{"updates":[{"npc_id":<id>,"mood":"...","activity_summary":"...",'
    '"new_location":<id|null>,'
    '"relationship_changes":[{"from":<id>,"to":<id>,"delta":<-0.1..0.1>}]}]}'
)

TIER3_SCHEMA = {
    "name": "tier3_batch",
    "strict": True,
    "schema": {
        "type": "object",
        "additionalProperties": False,
        "properties": {
            "updates": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": False,
                    "properties": {
                        "npc_id": {"type": "integer"},
                        "mood": {"type": "string"},
                        "activity_summary": {"type": "string"},
                        "new_location": {"type": ["integer", "null"]},
                        "relationship_changes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": False,
                                "properties": {
                                    "from": {"type": "integer"},
                                    "to": {"type": "integer"},
                                    "delta": {"type": "number"},
                                },
                                "required": ["from", "to", "delta"],
                            },
                        },
                    },
                    "required": [
                        "npc_id",
                        "mood",
                        "activity_summary",
                        "new_location",
                        "relationship_changes",
                    ],
                },
            },
        },
        "required": ["updates"],
    },
}

# Mirrors `parish_npc::build_tier1_system_prompt` for the Brigid persona so
# bench scores track the runtime tier-1 grounding (issue #994).
DIALOGUE_SYS = build_dialogue_system_prompt()


def pretty_json(text: str) -> str:
    try:
        return json.dumps(json.loads(text), indent=2)
    except Exception:
        return text


def cases(small: Target, large: Target) -> list[tuple]:
    """Returns (category, n, slot_label, target, system, user, schema, max_tokens)."""
    return [
        ("Intent", 1, "small", small, INTENT_SYS, "go to the pub", INTENT_SCHEMA, None),
        ("Intent", 2, "small", small, INTENT_SYS,
         "tell Padraig I saw his cow wandering near the bog", INTENT_SCHEMA, None),
        ("Reaction", 1, "small", small, REACTION_SYS,
         "A newcomer has just arrived at Darcy's Pub. It is evening, Clear.\n"
         "You have not met this person before. You are working here as the Publican. "
         "Introduce yourself briefly.", None, 100),
        ("Reaction", 2, "small", small, REACTION_SYS,
         "A newcomer has just arrived at Darcy's Pub. It is morning, Light Rain.\n"
         "You have met this person before.", None, 100),
        ("Simulation (Tier 2)", 1, "small", small, None, TIER2_USER, TIER2_SCHEMA, 200),
        ("Simulation (Tier 3 batch)", 2, "small", small, None, TIER3_USER, TIER3_SCHEMA, 600),
        ("Dialogue", 1, "large", large, DIALOGUE_SYS,
         "I've been having trouble sleeping. The dreams keep coming back.", None, None),
        ("Dialogue", 2, "large", large, DIALOGUE_SYS,
         "What do you know about the old Cailleach who lives near the fairy fort?", None, None),
    ]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--small", default=DEFAULT_SMALL,
                    help=f"Target for Intent/Reaction/Simulation (default: {DEFAULT_SMALL})")
    ap.add_argument("--large", default=DEFAULT_LARGE,
                    help=f"Target for Dialogue (default: {DEFAULT_LARGE})")
    ap.add_argument("--output", default=str(DEFAULT_OUTPUT),
                    help=f"Markdown report path (default: {DEFAULT_OUTPUT})")
    args = ap.parse_args()

    small = parse_target(args.small)
    large = parse_target(args.large)

    out_lines = ["# Inference category samples\n"]
    out_lines.append(
        f"Targets — small: `{small.model}` at `{small.base_url}` "
        f"(Intent/Reaction/Simulation); large: `{large.model}` at "
        f"`{large.base_url}` (Dialogue).\n"
    )

    tracker = CostTracker()
    last_cat = None
    for cat, n, slot_label, target, sys_p, user, schema, mt in cases(small, large):
        print(f"# {cat} #{n} ({slot_label}) {target.model}")
        try:
            output, usage = call_chat(target, sys_p, user, schema=schema, max_tokens=mt)
            tracker.record(target, usage)
        except Exception as e:
            output = f"ERROR: {e}"
        if cat != last_cat:
            out_lines.append(f"\n## {cat}\n")
            last_cat = cat
        out_lines.append(f"### Sample {n}  (slot: {slot_label}, model: `{target.label()}`)\n")
        if sys_p:
            out_lines.append("**System prompt:**\n")
            out_lines.append("```\n" + sys_p.strip() + "\n```\n")
        out_lines.append("**User prompt:**\n")
        out_lines.append("```\n" + user.strip() + "\n```\n")
        out_lines.append("**Output:**\n")
        rendered = pretty_json(output) if schema else output.strip()
        if schema:
            out_lines.append("```json\n" + rendered + "\n```\n")
        else:
            out_lines.append("> " + rendered.replace("\n", "\n> ") + "\n")

    out_lines.append(f"\n---\n\n_Run cost: {tracker.summary()}._\n")
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text("\n".join(out_lines))
    print(f"wrote {args.output}")
    print(f"cost: {tracker.summary()}")


if __name__ == "__main__":
    main()
