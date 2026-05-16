#!/usr/bin/env python3
"""
Player agent for Rundale inference eval.

Drives a running parish-server as a human-like player, using GitHub Models
to generate commands from observed game state. Produces a structured JSON
session log consumed by judge.py.

Usage:
    python player_agent.py --scenario smoke --turns 10
    python player_agent.py --scenario full_session --free --turns 50

Environment:
    GITHUB_TOKEN     GitHub PAT with models:read scope (required)
    PARISH_PORT      parish-server port (default: 3030)
    PLAYER_MODEL     GitHub Models slug for the player (default: microsoft/Phi-4)
"""

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

SCENARIOS_DIR = Path(__file__).parent / "scenarios"
LOGS_DIR = Path(__file__).parent / "logs"

PLAYER_MODEL = os.environ.get("PLAYER_MODEL", "microsoft/Phi-4")
GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN", "")
PARISH_PORT = int(os.environ.get("PARISH_PORT", "3030"))
GITHUB_MODELS_URL = "https://models.github.ai/inference/v1/chat/completions"

PLAYER_SYSTEM = """\
You are playing Rundale, a text adventure set in rural Ireland in 1820. You
control a character moving through an Irish village and its surroundings.

Your goal: explore the world, talk to the people you meet, and observe what
happens. Play naturally — move between locations, greet villagers, ask about
their lives and the local news.

Rules:
- Reply with a SINGLE short command or sentence (under 15 words).
- Use natural language: "go to the forge", "greet Brigid", "ask about the harvest".
- Do NOT use computer-game syntax like "N", "LOOK", "INVENTORY".
- Do NOT repeat the same command twice in a row.
- Do NOT break character or mention that you are an AI.

Current game state will be provided before each move."""


def parish_get(path: str) -> dict:
    url = f"http://127.0.0.1:{PARISH_PORT}{path}"
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read())


def parish_post(path: str, body: dict) -> dict:
    url = f"http://127.0.0.1:{PARISH_PORT}{path}"
    data = json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read())


def github_models_complete(messages: list[dict]) -> str:
    if not GITHUB_TOKEN:
        sys.exit("ERROR: GITHUB_TOKEN is not set. Generate a PAT with models:read scope.")
    payload = json.dumps({
        "model": PLAYER_MODEL,
        "messages": messages,
        "max_tokens": 60,
        "temperature": 0.8,
    }).encode()
    req = urllib.request.Request(
        GITHUB_MODELS_URL,
        data=payload,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {GITHUB_TOKEN}",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read())
            return data["choices"][0]["message"]["content"].strip()
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        sys.exit(f"ERROR: GitHub Models API returned {e.code}: {body}")


def build_state_summary(snapshot: dict, npcs: list[dict]) -> str:
    loc = snapshot.get("location", "unknown location")
    clock = snapshot.get("clock", "")
    weather = snapshot.get("weather", "")
    log_tail = snapshot.get("log_tail", [])

    lines = [f"Location: {loc}", f"Time: {clock}  Weather: {weather}"]
    if npcs:
        names = ", ".join(n.get("name", "someone") for n in npcs)
        lines.append(f"People here: {names}")
    if log_tail:
        lines.append("Recent events:")
        for entry in log_tail[-3:]:
            lines.append(f"  {entry}")
    return "\n".join(lines)


def run_session(scenario_name: str, scenario_lines: list[str], turns: int, free: bool) -> list[dict]:
    """Execute a play session and return the turn log."""
    log = []
    history: list[dict] = []  # conversation history for the player model

    # Start a new game branch
    try:
        parish_post("/api/new-game", {})
    except Exception as e:
        print(f"Warning: could not start new game: {e}", file=sys.stderr)

    scripted = [l for l in scenario_lines if l and not l.startswith("#")]
    scripted_idx = 0

    for turn in range(turns):
        # --- Gather game state ---
        try:
            snapshot = parish_get("/api/world-snapshot")
            npcs_resp = parish_get("/api/npcs-here")
            npcs = npcs_resp.get("npcs", []) if isinstance(npcs_resp, dict) else []
        except Exception as e:
            print(f"Turn {turn}: failed to read game state: {e}", file=sys.stderr)
            break

        state_text = build_state_summary(snapshot, npcs)

        # --- Choose command ---
        is_llm_turn = False
        if scripted_idx < len(scripted):
            line = scripted[scripted_idx]
            scripted_idx += 1
            if line.strip() == "# llm":
                is_llm_turn = True
            else:
                command = line.strip()
        else:
            is_llm_turn = True

        if is_llm_turn or free:
            user_msg = f"Game state:\n{state_text}\n\nWhat do you do next?"
            history.append({"role": "user", "content": user_msg})
            messages = [{"role": "system", "content": PLAYER_SYSTEM}] + history[-10:]
            command = github_models_complete(messages)
            history.append({"role": "assistant", "content": command})
            is_llm_turn = True

        # --- Submit command ---
        print(f"Turn {turn + 1:3d}: {command}")
        try:
            result = parish_post("/api/submit-input", {"input": command})
        except Exception as e:
            print(f"  Error: {e}", file=sys.stderr)
            result = {"error": str(e)}

        log.append({
            "turn": turn + 1,
            "command": command,
            "llm_generated": is_llm_turn,
            "state": {
                "location": snapshot.get("location"),
                "clock": snapshot.get("clock"),
                "weather": snapshot.get("weather"),
                "npc_count": len(npcs),
            },
            "result": result,
        })

        # Brief pause to respect rate limits
        if is_llm_turn:
            time.sleep(1)

    return log


def main():
    parser = argparse.ArgumentParser(description="Rundale player agent")
    parser.add_argument("--scenario", default="smoke",
                        help="Scenario name (file in testing/eval/scenarios/)")
    parser.add_argument("--turns", type=int, default=20,
                        help="Maximum number of turns to play")
    parser.add_argument("--free", action="store_true",
                        help="LLM generates every command (ignore scripted lines)")
    parser.add_argument("--output", default=None,
                        help="Output JSON path (default: eval/logs/<scenario>-<timestamp>.json)")
    args = parser.parse_args()

    scenario_path = SCENARIOS_DIR / f"{args.scenario}.txt"
    if not scenario_path.exists():
        sys.exit(f"ERROR: scenario not found: {scenario_path}")

    scenario_lines = scenario_path.read_text().splitlines()

    print(f"Running scenario '{args.scenario}' for up to {args.turns} turns "
          f"({'free-play' if args.free else 'scripted+llm'}) "
          f"with player model {PLAYER_MODEL}")

    session_log = run_session(args.scenario, scenario_lines, args.turns, args.free)

    output_path = args.output
    if output_path is None:
        LOGS_DIR.mkdir(parents=True, exist_ok=True)
        ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S")
        output_path = LOGS_DIR / f"{args.scenario}-{ts}.json"

    result = {
        "scenario": args.scenario,
        "player_model": PLAYER_MODEL,
        "turns": len(session_log),
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "log": session_log,
    }
    Path(output_path).write_text(json.dumps(result, indent=2))
    print(f"\nSession log written to {output_path}")


if __name__ == "__main__":
    main()
