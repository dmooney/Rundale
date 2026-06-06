"""Build runtime-faithful bench datasets from captured engine requests (REQ 2 + 3).

Reads the JSONL emitted by capture_server.py (one record per real engine LLM
request: system, user, response_format, max_tokens, temperature, ...), classifies
each into a slice, de-dups, and writes the promptfoo v2 datasets that the rewired
bench sends VERBATIM — so every candidate sees byte-exact live-game prompts.

Slices produced here:
  dialogue, reaction, tier2-sim, tier3-sim   — wholly from captured prompts
  intent                                      — curated gold labels + runtime system prompt
  multiturn                                   — authored failure-mode probes on captured personas

`gaeilge` is left untouched (a curated Irish-competence probe, grown separately).

Usage:
    python3 promptfoo/scripts/build_runtime_datasets.py <capture.jsonl> [more.jsonl ...]
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

PROMPTFOO_DIR = Path(__file__).resolve().parents[1]
DATASETS = PROMPTFOO_DIR / "v2" / "datasets"
# Curated gold labels + grading schemas come from the FROZEN legacy tree, which
# this builder never overwrites — so re-running is idempotent.
FROZEN = PROMPTFOO_DIR.parent / "rundale-bench" / "v1"

# Hold out ~15% of each captured slice for an unbiased re-check split.
HOLDOUT_FRAC = 0.15


def load_captures(paths: list[str]) -> list[dict]:
    rows = []
    for p in paths:
        for line in Path(p).read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def classify(d: dict) -> str | None:
    sp = d.get("system") or ""
    u = d.get("user") or ""
    if "STAY IN YOUR LANE" in sp or "PEOPLE YOU KNOW" in sp or "WORLD FACTS" in sp:
        return "dialogue"
    if "background interactions" in u:
        return "tier2-sim"
    if "background NPC activity" in u:
        return "tier3-sim"
    if "greeting or reaction" in sp or "brief greeting" in sp:
        return "reaction"  # arrival greeting (text); judged on in_character
    if "visibly react" in sp or "emoji" in sp.lower():
        return "reaction-emoji"  # not used by the in_character reaction slice
    if "input parser" in sp.lower() or "parser" in sp.lower():
        return "intent-runtime"  # used only to harvest the runtime system prompt
    return None


def _key(d: dict) -> str:
    return hashlib.sha256(
        ((d.get("system") or "") + "\x00" + (d.get("user") or "")).encode("utf-8")
    ).hexdigest()


def _persona(system: str) -> str:
    # "You are Seamus Gallagher, a 42-year-old Blacksmith in rural Ireland, 1820."
    m = re.search(r"a \d+-year-old ([A-Za-z' ]+?) in ", system)
    return m.group(1).strip().lower() if m else "villager"


def _dedup(rows: list[dict]) -> list[dict]:
    seen, out = set(), []
    for d in rows:
        k = _key(d)
        if k in seen:
            continue
        seen.add(k)
        out.append(d)
    return out


def _split(records: list[dict]) -> tuple[list[dict], list[dict]]:
    n_hold = max(1, int(len(records) * HOLDOUT_FRAC)) if len(records) > 6 else 0
    return records[n_hold:], records[:n_hold]


def _write(slice_name: str, main: list[dict], hold: list[dict]) -> None:
    (DATASETS / f"{slice_name}.jsonl").write_text(
        "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in main), encoding="utf-8"
    )
    if hold:
        (DATASETS / f"{slice_name}.holdout.jsonl").write_text(
            "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in hold), encoding="utf-8"
        )


# --- multi-turn failure-mode probes (REQ 3) --------------------------------
# Each scenario reuses a real captured dialogue system prompt (a faithful persona
# with PEOPLE YOU KNOW / anchors) and runs scripted player turns that bait the
# four known regressions. The provider chains the candidate's own replies as the
# assistant turns, so memory_retention is genuinely exercised.
MULTITURN_SCRIPTS = [
    [
        "Good day to you. My name is Sean, I'm new to the parish.",
        "Tell me, what do you do here?",
        "Do you recall my name?",
        "I'd best be getting on, but one more thing first — who else lives nearby?",
        "Right, and remind me: what was it you said your trade was?",
    ],
    [
        "Dia dhuit. I'm Sean, just arrived from Galway.",
        "Have you family in the parish?",
        "And what's my name, do you remember?",
        "Before I forget — what's the news from the fair?",
        "Grand. I'll call again tomorrow.",
    ],
    [
        "Hello there. I'm Sean.",
        "What's troubling the parish these days?",
        "You wouldn't happen to remember my name?",
        "I was telling you about Galway earlier — what did I say I did there?",
    ],
    [
        "Top of the morning. Sean's the name.",
        "Is there work to be had around here?",
        "What did I say my name was?",
        "I mentioned I keep sheep — do you recall that?",
        "Anyhow, safe home to yourself.",
    ],
    [
        "Good evening. I'm Sean, a stranger here.",
        "Who holds the land hereabouts?",
        "Tell me my name back, if you would.",
        "One last thing before I go — where might I find a bed for the night?",
    ],
    [
        "Sean is my name. Pleased to meet you.",
        "How long have you lived in the parish?",
        "Do you remember who you're speaking with?",
        "Earlier I said I came from the coast — what trade did I mention?",
        "I'll leave you to your work now.",
    ],
]

# Cap how many distinct personas become multiturn scenarios.
MULTITURN_MAX = 12


def build_multiturn(dialogue_captures: list[dict]) -> list[dict]:
    # One scenario per distinct captured persona (up to the cap), each assigned a
    # probe script round-robin so every scenario baits the four failure modes.
    seen_personas, personas = set(), []
    for d in dialogue_captures:
        p = _persona(d.get("system") or "")
        if p in seen_personas:
            continue
        seen_personas.add(p)
        personas.append(d)
        if len(personas) >= MULTITURN_MAX:
            break
    out = []
    for i, persona_cap in enumerate(personas):
        turns = MULTITURN_SCRIPTS[i % len(MULTITURN_SCRIPTS)]
        out.append(
            {
                "id": f"multiturn-{i + 1:04d}",
                "slice": "multiturn",
                "system": persona_cap["system"],
                "turns": turns,
                "persona": _persona(persona_cap["system"]),
                "response_format": persona_cap.get("response_format"),
                "temperature": persona_cap.get("temperature", 0.7),
                "frequency_penalty": persona_cap.get("frequency_penalty"),
                "player_name": "Sean",
            }
        )
    return out


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("usage: build_runtime_datasets.py <capture.jsonl> [more ...]", file=sys.stderr)
        return 2
    rows = load_captures(argv[1:])
    buckets: dict[str, list[dict]] = {}
    for d in rows:
        c = classify(d)
        if c:
            buckets.setdefault(c, []).append(d)

    # runtime intent system prompt + params (harvested from a real intent call)
    intent_caps = _dedup(buckets.get("intent-runtime", []))
    intent_sys = intent_caps[0]["system"] if intent_caps else None
    intent_rf = intent_caps[0].get("response_format") if intent_caps else {"type": "json_object"}
    intent_temp = intent_caps[0].get("temperature", 0.7) if intent_caps else 0.7
    intent_maxtok = intent_caps[0].get("max_tokens") if intent_caps else 100

    # reuse the canonical grading schemas (deterministic schema_valid) from the
    # existing curated sim datasets — request stays runtime-faithful (no schema sent).
    def _existing_schema(slice_name: str):
        p = FROZEN / f"{slice_name}.jsonl"
        for line in p.read_text(encoding="utf-8").splitlines():
            if line.strip():
                return json.loads(line).get("schema")
        return None

    tier2_schema = _existing_schema("tier2-sim")
    tier3_schema = _existing_schema("tier3-sim")

    summary = {}
    max_prompt_chars = 0

    # dialogue
    dlg = _dedup(buckets.get("dialogue", []))
    dlg_recs = [
        {
            "id": f"dialogue-{i + 1:04d}",
            "slice": "dialogue",
            "system": d["system"],
            "user": d["user"],
            "response_format": d.get("response_format"),
            "max_tokens": d.get("max_tokens"),
            "temperature": d.get("temperature", 0.7),
            "frequency_penalty": d.get("frequency_penalty"),
        }
        for i, d in enumerate(dlg)
    ]
    m, h = _split(dlg_recs)
    _write("dialogue", m, h)
    summary["dialogue"] = (len(m), len(h))

    # reaction (arrival greeting)
    rx = _dedup(buckets.get("reaction", []))
    rx_recs = [
        {
            "id": f"reaction-{i + 1:04d}",
            "slice": "reaction",
            "system": d["system"],
            "user": d["user"],
            "persona": _persona(d["system"]),
            "response_format": d.get("response_format"),
            "max_tokens": d.get("max_tokens") or 100,
            "temperature": d.get("temperature", 0.7),
        }
        for i, d in enumerate(rx)
    ]
    m, h = _split(rx_recs)
    _write("reaction", m, h)
    summary["reaction"] = (len(m), len(h))

    # tier2 / tier3 sim
    for slice_name, schema in (("tier2-sim", tier2_schema), ("tier3-sim", tier3_schema)):
        sim = _dedup(buckets.get(slice_name, []))
        recs = [
            {
                "id": f"{slice_name}-{i + 1:04d}",
                "slice": slice_name,
                "system": d.get("system"),
                "user": d["user"],
                "response_format": d.get("response_format"),
                "max_tokens": d.get("max_tokens"),
                "temperature": d.get("temperature", 0.7),
                "grade_schema": schema,
            }
            for i, d in enumerate(sim)
        ]
        m, h = _split(recs)
        _write(slice_name, m, h)
        summary[slice_name] = (len(m), len(h))

    # intent: curated gold labels + runtime system prompt + runtime response_format
    if intent_sys:
        existing = [
            json.loads(line)
            for line in (FROZEN / "intent.jsonl").read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
        intent_recs = [
            {
                "id": e["id"],
                "slice": "intent",
                "system": intent_sys,
                "user": e["prompt"],
                "gold": e["gold"],
                "response_format": intent_rf,
                "max_tokens": intent_maxtok,
                "temperature": intent_temp,
            }
            for e in existing
        ]
        _write("intent", intent_recs, [])
        summary["intent"] = (len(intent_recs), 0)

    # multiturn
    mt = build_multiturn(dlg)
    _write("multiturn", mt, [])
    summary["multiturn"] = (len(mt), 0)

    # measure the largest runtime prompt (sets the enumeration context floor)
    for d in rows:
        max_prompt_chars = max(
            max_prompt_chars, len(d.get("system") or "") + len(d.get("user") or "")
        )
    approx_tokens = int(max_prompt_chars / 3.5)

    print("[build] runtime-faithful datasets written (main, holdout):")
    for s, (a, b) in sorted(summary.items()):
        print(f"  {s:12s} {a:4d}  +{b} holdout")
    print(f"[build] largest runtime prompt ~= {max_prompt_chars} chars (~{approx_tokens} tokens)")
    print(f"[build] suggested enumeration --context-floor = {((approx_tokens // 1024) + 4) * 1024}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
