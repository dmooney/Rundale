#!/usr/bin/env python3
"""rundale-bench orchestrator.

Single entrypoint for running any slice against any OpenAI-compatible target.
Outputs per-record JSON results + a summary table; supports `--slice all` to
sweep every available slice + emit an aggregate row suitable for the
leaderboard.

Usage::

    python3 parish/testing/rundale-bench/rundale_bench.py \\
        --target 'model@base_url[#env:VAR]' --suite v1 --slice <name|all> \\
        [--judge <id>] [--limit N] [--split dev|holdout]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

# Make the eval_lib loader available; lives alongside the local-eval scripts.
_REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(_REPO_ROOT / "parish" / "scripts" / "local-eval"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from eval_lib import CostTracker, Target, call_chat, load_slice, parse_target  # noqa: E402
from grade import (  # noqa: E402
    grade_dialogue,
    grade_intent,
    grade_reaction,
    grade_schema,
    grade_simulation,
)

_BENCH_DIR = Path(__file__).resolve().parent
_PROOFS_DIR = _REPO_ROOT / "docs" / "proofs" / "rundale-bench"

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
            "intent": {"type": "string", "enum": ["move", "talk", "look", "interact", "examine", "unknown"]},
            "target": {"type": ["string", "null"]},
            "dialogue": {"type": ["string", "null"]},
        },
        "required": ["intent", "target", "dialogue"],
    },
}

DIALOGUE_SYS = (
    "You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. "
    "You are kind but direct, with a deep knowledge of local plants and folk medicine. "
    "You have known the player's family for years.\n\n"
    "Stay in character. Speak in 1-3 sentences. Do not use modern language."
)


def slug(s: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "_", s).strip("_")[:80]


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def load_judge(judge_id: str, suite: str) -> dict:
    path = _BENCH_DIR / suite / f"{judge_id}.json"
    if not path.exists():
        raise FileNotFoundError(f"judge config not found: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def judge_invoker(judge: dict, tracker: CostTracker):
    """Build the `invoke(system, user, schema) -> parsed-dict` callable.

    The judge `Target` is constructed once per call to keep `call_chat`
    happy and to surface missing API keys early.
    """
    target = Target(model=judge["model"], base_url=judge["base_url"], api_key_env=judge.get("api_key_env"))
    temperature = float(judge.get("temperature", 0.0))

    def _invoke(system: str, user: str, schema: dict) -> dict:
        text, usage = call_chat(target, system, user, schema=schema, temperature=temperature)
        tracker.record(target, usage)
        return json.loads(text)

    return _invoke


# ---------------------------------------------------------------------------
# per-slice runners
# ---------------------------------------------------------------------------

def run_intent(target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    results = []
    label_matches = 0
    score_sum = 0.0
    for rec in records:
        try:
            text, usage = call_chat(target, INTENT_SYS, rec["prompt"], schema=INTENT_SCHEMA)
            tracker.record(target, usage)
            pred = json.loads(text)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e), "score": 0.0, "label_match": 0})
            continue
        graded = grade_intent(pred, rec["gold"])
        graded["id"] = rec["id"]
        graded["pred"] = pred
        graded["gold"] = rec["gold"]
        results.append(graded)
        label_matches += graded["label_match"]
        score_sum += graded["score"]
    summary = {
        "slice": "intent",
        "records": len(records),
        "label_match_rate": label_matches / max(1, len(records)),
        "mean_score": score_sum / max(1, len(records)),
    }
    return {"summary": summary, "results": results}


def run_dialogue(target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    judge = load_judge(args.judge, args.suite)
    invoke = judge_invoker(judge, tracker)
    results = []
    axis_sums = {k: 0.0 for k in ("character", "authenticity", "language", "responsiveness", "craft", "overall")}
    nl_flags = 0
    for rec in records:
        try:
            reply, usage = call_chat(target, DIALOGUE_SYS, rec["prompt"], max_tokens=200)
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e)})
            continue
        graded = grade_dialogue(reply, judge, invoke)
        graded["id"] = rec["id"]
        graded["reply"] = reply
        results.append(graded)
        for k in axis_sums:
            axis_sums[k] += graded.get(k, 0)
        if graded.get("non_latin_chars"):
            nl_flags += 1
    n = max(1, len(records))
    summary = {
        "slice": "dialogue",
        "records": len(records),
        "non_latin_rate": nl_flags / n,
        **{k: v / n for k, v in axis_sums.items()},
    }
    return {"summary": summary, "results": results}


def run_reaction(target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    judge = load_judge("judge_reaction_v1", args.suite)
    invoke = judge_invoker(judge, tracker)
    results = []
    score_sum = 0.0
    for rec in records:
        try:
            reply, usage = call_chat(
                target,
                rec["system_template"],
                rec["prompt"],
                max_tokens=rec.get("max_tokens", 100),
            )
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e), "score": 0.0})
            continue
        graded = grade_reaction(reply, rec["persona"], judge, invoke)
        graded["id"] = rec["id"]
        graded["reply"] = reply
        results.append(graded)
        score_sum += graded["score"]
    summary = {
        "slice": "reaction",
        "records": len(records),
        "mean_score": score_sum / max(1, len(records)),
    }
    return {"summary": summary, "results": results}


def run_simulation(slice_name: str, target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    judge = load_judge("judge_sim_v1", args.suite)
    invoke = judge_invoker(judge, tracker)
    results = []
    valid = 0
    score_sum = 0.0
    for rec in records:
        try:
            reply, usage = call_chat(
                target, None, rec["prompt"],
                schema=rec["schema"],
                max_tokens=600 if slice_name == "tier3-sim" else 200,
            )
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e), "score": 0.0, "schema_valid": False})
            continue
        graded = grade_simulation(reply, rec["schema"], judge, invoke)
        graded["id"] = rec["id"]
        graded["reply"] = reply
        results.append(graded)
        if graded["schema_valid"]:
            valid += 1
        score_sum += graded["score"]
    summary = {
        "slice": slice_name,
        "records": len(records),
        "schema_valid_rate": valid / max(1, len(records)),
        "mean_score": score_sum / max(1, len(records)),
    }
    return {"summary": summary, "results": results}


def run_slice(slice_name: str, target: Target, tracker: CostTracker, args) -> dict:
    records = load_slice(slice_name, version=args.suite, split=args.split)
    if args.limit:
        records = records[: args.limit]
    if slice_name == "intent":
        return run_intent(target, records, tracker, args)
    if slice_name == "dialogue":
        return run_dialogue(target, records, tracker, args)
    if slice_name == "reaction":
        return run_reaction(target, records, tracker, args)
    if slice_name in ("tier2-sim", "tier3-sim"):
        return run_simulation(slice_name, target, records, tracker, args)
    raise SystemExit(f"unknown slice: {slice_name}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--target", required=True, help="model@base_url[#env:VAR]")
    ap.add_argument("--suite", default="v1")
    ap.add_argument("--slice", required=True,
                    choices=["intent", "dialogue", "reaction", "tier2-sim", "tier3-sim", "all"])
    ap.add_argument("--judge", default="judge_v1", help="judge config id (dialogue slice)")
    ap.add_argument("--limit", type=int, default=None)
    ap.add_argument("--split", default="dev", choices=["dev", "holdout"])
    args = ap.parse_args()

    target = parse_target(args.target)
    tracker = CostTracker()
    started = time.time()

    slices = (
        ["intent", "dialogue", "reaction", "tier2-sim", "tier3-sim"]
        if args.slice == "all"
        else [args.slice]
    )

    out = {
        "suite": args.suite,
        "split": args.split,
        "target": {"model": target.model, "base_url": target.base_url},
        "run_started_utc": datetime.now(timezone.utc).isoformat(),
        "slices": {},
    }

    for s in slices:
        try:
            data = run_slice(s, target, tracker, args)
        except FileNotFoundError as e:
            print(f"[{s}] skipped: {e}")
            continue
        out["slices"][s] = data
        summary = data["summary"]
        line = " ".join(f"{k}={v:.3f}" if isinstance(v, float) else f"{k}={v}"
                        for k, v in summary.items() if k != "slice")
        print(f"[{s}] {line}")

    elapsed = time.time() - started
    out["elapsed_seconds"] = elapsed
    out["cost"] = {
        "calls": tracker.calls,
        "prompt_tokens": tracker.prompt_tokens,
        "completion_tokens": tracker.completion_tokens,
        "usd": tracker.usd,
    }
    print(f"\ntotal: {tracker.summary()} in {elapsed:.1f}s")

    _PROOFS_DIR.mkdir(parents=True, exist_ok=True)
    out_path = _PROOFS_DIR / f"run_{slug(target.model)}_{args.slice}_{utc_stamp()}.json"
    out_path.write_text(json.dumps(out, indent=2, default=str) + "\n", encoding="utf-8")
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
