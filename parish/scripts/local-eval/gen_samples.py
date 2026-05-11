#!/usr/bin/env python3
"""Generate 2 production-faithful samples per inference category.

Mirrors the prompts from `parish/crates/parish-inference/examples/inf_bench.rs`.
Hits the two-slot vllm-mlx loadout:
  Intent / Reaction / Simulation → :8001 (Qwen2.5-1.5B-Instruct-4bit)
  Dialogue                        → :8000 (Qwen2.5-7B-Instruct-4bit)
"""
import json
import urllib.request

SMALL = "mlx-community/Qwen2.5-1.5B-Instruct-4bit"
LARGE = "mlx-community/Qwen2.5-7B-Instruct-4bit"

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

DIALOGUE_SYS = (
    "You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. "
    "You are kind but direct, with a deep knowledge of local plants and folk medicine. "
    "You have known the player's family for years.\n\n"
    "Stay in character. Speak in 1-3 sentences. Do not use modern language."
)


def call(port, model, system, user, schema=None, max_tokens=None):
    msgs = []
    if system:
        msgs.append({"role": "system", "content": system})
    msgs.append({"role": "user", "content": user})
    body = {"model": model, "messages": msgs, "stream": False, "temperature": 0.7}
    if max_tokens:
        body["max_tokens"] = max_tokens
    if schema:
        body["response_format"] = {"type": "json_schema", "json_schema": schema}
    req = urllib.request.Request(
        f"http://localhost:{port}/v1/chat/completions",
        data=json.dumps(body).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        data = json.load(resp)
    return data["choices"][0]["message"]["content"]


def pretty_json(text):
    """Try to re-emit JSON as pretty-printed; fall back to raw on parse fail."""
    try:
        return json.dumps(json.loads(text), indent=2)
    except Exception:
        return text


cases = [
    # Intent x 2
    ("Intent", 1, "small", SMALL, INTENT_SYS,
     "go to the pub", INTENT_SCHEMA, None),
    ("Intent", 2, "small", SMALL, INTENT_SYS,
     "tell Padraig I saw his cow wandering near the bog", INTENT_SCHEMA, None),

    # Reaction x 2
    ("Reaction", 1, "small", SMALL, REACTION_SYS,
     "A newcomer has just arrived at Darcy's Pub. It is evening, Clear.\n"
     "You have not met this person before. You are working here as the Publican. "
     "Introduce yourself briefly.", None, 100),
    ("Reaction", 2, "small", SMALL, REACTION_SYS,
     "A newcomer has just arrived at Darcy's Pub. It is morning, Light Rain.\n"
     "You have met this person before.", None, 100),

    # Simulation x 2 (Tier 2 + Tier 3)
    ("Simulation (Tier 2)", 1, "small", SMALL, None, TIER2_USER, TIER2_SCHEMA, 200),
    ("Simulation (Tier 3 batch)", 2, "small", SMALL, None, TIER3_USER, TIER3_SCHEMA, 600),

    # Dialogue x 2
    ("Dialogue", 1, "large", LARGE, DIALOGUE_SYS,
     "I've been having trouble sleeping. The dreams keep coming back.", None, None),
    ("Dialogue", 2, "large", LARGE, DIALOGUE_SYS,
     "What do you know about the old Cailleach who lives near the fairy fort?",
     None, None),
]


def main():
    out_lines = ["# Inference category samples (May 2026)\n"]
    out_lines.append(
        "Production-faithful prompts mirroring "
        "`parish-inference/examples/inf_bench.rs`. Two-slot Apple Silicon "
        "loadout: small slot = `mlx-community/Qwen2.5-1.5B-Instruct-4bit` "
        "on :8001 (Intent, Reaction, Simulation); large slot = "
        "`mlx-community/Qwen2.5-7B-Instruct-4bit` on :8000 (Dialogue). "
        "Generated via `/tmp/gen_samples.py`.\n"
    )
    last_cat = None
    for cat, n, slot, model, sys_p, user, schema, mt in cases:
        port = 8000 if slot == "large" else 8001
        print(f"# {cat} #{n} on :{port} {model}")
        try:
            output = call(port, model, sys_p, user, schema, mt)
        except Exception as e:
            output = f"ERROR: {e}"
        if cat != last_cat:
            out_lines.append(f"\n## {cat}\n")
            last_cat = cat
        out_lines.append(f"### Sample {n}  (slot: {slot}, model: `{model.split('/')[-1]}`)\n")
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
    out_path = "/Users/dmooney/Rundale/.claude/worktrees/piped-imagining-meerkat/docs/proofs/local-perf/category_samples.md"
    with open(out_path, "w") as f:
        f.write("\n".join(out_lines))
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
