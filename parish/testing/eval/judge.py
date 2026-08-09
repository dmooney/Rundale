#!/usr/bin/env python3
"""
Judge harness for Rundale inference eval.

Reads a session log produced by player_agent.py and a rubric markdown file,
calls GitHub Models to score the session, and writes a judge result JSON.

Usage:
    python judge.py --log eval/logs/smoke-*.json --rubric rubrics/smoke.md
    python judge.py --log eval/logs/dialogue-*.json --rubric rubrics/dialogue.md --output result.json

Exit code:
    0  verdict: pass
    1  verdict: fail
    2  error (API failure, parse failure)

Environment:
    GITHUB_TOKEN    GitHub PAT with models:read scope (required)
    JUDGE_MODEL     GitHub Models slug for the judge (default: openai/gpt-4o)
"""

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

JUDGE_MODEL = os.environ.get("JUDGE_MODEL", "openai/gpt-4o")
GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN", "")
GITHUB_MODELS_URL = "https://models.github.ai/inference/chat/completions"

JUDGE_SYSTEM = """\
You are an expert evaluator for Rundale, a text adventure game set in rural
Ireland in 1820. You will receive a gameplay session log and a scoring rubric.

Respond ONLY with a single JSON object — no prose, no markdown fences.
The JSON must match the schema specified in the rubric exactly."""


def github_models_complete(system: str, prompt: str) -> str:
    if not GITHUB_TOKEN:
        sys.exit("ERROR: GITHUB_TOKEN is not set.")
    payload = json.dumps(
        {
            "model": JUDGE_MODEL,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": prompt},
            ],
            "max_tokens": 600,
            "temperature": 0.1,
            "response_format": {"type": "json_object"},
        }
    ).encode()
    req = urllib.request.Request(
        GITHUB_MODELS_URL,
        data=payload,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {GITHUB_TOKEN}",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read())
            return (data.get("choices") or [{}])[0].get("message", {}).get("content", "").strip()
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        print(f"ERROR: GitHub Models API returned {e.code}: {body}", file=sys.stderr)
        sys.exit(2)


def summarise_log(log_data: dict) -> str:
    """Produce a compact text summary of the session log for the judge prompt."""
    lines = [
        f"Scenario: {log_data.get('scenario', 'unknown')}",
        f"Player model: {log_data.get('player_model', 'unknown')}",
        f"Turns: {log_data.get('turns', 0)}",
        "",
        "Session transcript:",
    ]
    for entry in log_data.get("log", []):
        turn = entry.get("turn", "?")
        cmd = entry.get("command", "")
        state = entry.get("state", {})
        result = entry.get("result", {})
        loc = state.get("location", "")
        clock = state.get("clock", "")
        npc_count = state.get("npc_count", 0)

        # Summarise the game's response
        resp_text = ""
        if isinstance(result, dict):
            if "narrative" in result:
                resp_text = result["narrative"][:240] or "(no output)"
            elif "log_tail" in result:
                # Backward compatibility for historical eval artifacts.
                entries = result["log_tail"]
                resp_text = " | ".join(str(e) for e in entries)[:240] if entries else "(no output)"
            elif "npc_response" in result:
                resp_text = f'NPC: "{result["npc_response"]}"'
            elif "description" in result:
                resp_text = result["description"][:120]
            elif "message" in result:
                resp_text = result["message"][:120]
            elif "error" in result:
                resp_text = f"ERROR: {result['error']}"

        lines.append(
            f"Turn {turn} [{loc} / {clock} / {npc_count} NPCs here]\n"
            f"  Player: {cmd}\n"
            f"  Game:   {resp_text}"
        )
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Rundale session judge")
    parser.add_argument("--log", required=True, help="Path to session log JSON")
    parser.add_argument("--rubric", required=True, help="Path to rubric markdown file")
    parser.add_argument("--output", default=None, help="Output JSON path")
    args = parser.parse_args()

    log_path = Path(args.log)
    rubric_path = Path(args.rubric)

    if not log_path.exists():
        sys.exit(f"ERROR: log not found: {log_path}")
    if not rubric_path.exists():
        sys.exit(f"ERROR: rubric not found: {rubric_path}")

    log_data = json.loads(log_path.read_text())
    rubric_text = rubric_path.read_text()

    session_summary = summarise_log(log_data)
    prompt = f"{rubric_text}\n\n---\n\n{session_summary}"

    print(f"Judging scenario '{log_data.get('scenario')}' with {JUDGE_MODEL}...")
    raw = github_models_complete(JUDGE_SYSTEM, prompt)

    try:
        verdict_data = json.loads(raw)
    except json.JSONDecodeError:
        print(f"ERROR: judge returned non-JSON:\n{raw}", file=sys.stderr)
        sys.exit(2)

    verdict = verdict_data.get("verdict", "fail")
    notes = verdict_data.get("notes", "")

    # Compute mean score across numeric axes
    scores = {
        k: v
        for k, v in verdict_data.items()
        if isinstance(v, (int, float)) and k not in ("verdict", "notes")
    }
    mean = sum(scores.values()) / len(scores) if scores else 0.0

    result = {
        "scenario": log_data.get("scenario"),
        "judge_model": JUDGE_MODEL,
        "verdict": verdict,
        "mean_score": round(mean, 2),
        "scores": scores,
        "notes": notes,
        "log_path": str(log_path),
    }

    output_path = args.output
    if output_path is None:
        stem = log_path.stem
        output_path = log_path.parent / f"{stem}-judge.json"

    Path(output_path).write_text(json.dumps(result, indent=2))

    status = "PASS" if verdict == "pass" else "FAIL"
    print(f"{status} — mean score {mean:.2f} — {notes}")
    print(f"Judge result written to {output_path}")

    sys.exit(0 if verdict == "pass" else 1)


if __name__ == "__main__":
    main()
