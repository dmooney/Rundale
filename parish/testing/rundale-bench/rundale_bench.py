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
    grade_gaeilge,
    grade_intent,
    grade_pairwise,
    grade_reaction,
    grade_schema,
    grade_simulation,
)

import itertools
import random

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

GAEILGE_SYS = (
    "You are being evaluated for fluency in Irish Gaeilge.\n\n"
    "Follow the task exactly. Unless the task explicitly says otherwise, answer only "
    "in Irish Gaeilge, not English. Do not explain your choices. Prefer natural Irish "
    "syntax and idiom over word-for-word translation from English."
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


def _gaeilge_candidate_prompt(rec: dict) -> str:
    constraints = "\n".join(f"- {c}" for c in rec.get("constraints", []))
    return (
        f"Task type: {rec['task_type']}\n\n"
        f"Prompt:\n{rec['prompt']}\n\n"
        f"Constraints:\n{constraints}\n\n"
        "Respond now."
    )


def run_gaeilge(target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    judge = load_judge("judge_gaeilge_v1", args.suite)
    invoke = judge_invoker(judge, tracker)
    results = []
    axis_sums = {
        k: 0.0
        for k in ("fluency", "grammar", "idiom", "task_fulfillment", "english_leakage", "overall")
    }
    leakage_flags = 0
    error_count = 0
    for rec in records:
        try:
            reply, usage = call_chat(
                target,
                GAEILGE_SYS,
                _gaeilge_candidate_prompt(rec),
                max_tokens=rec.get("max_tokens", 300),
                temperature=0.2,
            )
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e)})
            error_count += 1
            continue
        graded = grade_gaeilge(reply, rec, judge, invoke)
        graded["id"] = rec["id"]
        graded["reply"] = reply
        results.append(graded)
        if graded.get("error"):
            error_count += 1
        for k in axis_sums:
            axis_sums[k] += graded.get(k, 0)
        if graded.get("english_leakage", 0) < 4:
            leakage_flags += 1
    n = max(1, len(records))
    summary = {
        "slice": "gaeilge",
        "records": len(records),
        "errors": error_count,
        "english_leakage_flag_rate": leakage_flags / n,
        **{f"{k}_mean": v / n for k, v in axis_sums.items()},
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
    if slice_name == "gaeilge":
        return run_gaeilge(target, records, tracker, args)
    raise SystemExit(f"unknown slice: {slice_name}")


# ---------------------------------------------------------------------------
# ELO mode (pairwise judge)
# ---------------------------------------------------------------------------

def _elo_update(rating_a: float, rating_b: float, score_a: float, k: float) -> tuple[float, float]:
    """Standard ELO update. `score_a` ∈ {0, 0.5, 1}. Returns (new_a, new_b)."""
    expected_a = 1.0 / (1.0 + 10.0 ** ((rating_b - rating_a) / 400.0))
    expected_b = 1.0 - expected_a
    return (
        rating_a + k * (score_a - expected_a),
        rating_b + k * ((1.0 - score_a) - expected_b),
    )


def _bootstrap_ci(
    matches: list[tuple[str, str, float]],
    targets: list[str],
    k_initial: float,
    k_settled: float = 16.0,
    settle_threshold: int = 50,
    iters: int = 500,
) -> dict:
    """Bootstrap 5/95 percentile ratings by resampling matches with replacement.

    Mirrors the main accumulator's dynamic K: K = k_initial until either
    candidate in a match has at least `settle_threshold` matches under its
    belt, then drops to k_settled. Using a constant K here understated CI
    width for late matches (where the actual ratings move slowly).
    """
    rng = random.Random(0xb1a40)
    all_ratings: dict[str, list[float]] = {t: [] for t in targets}
    for _ in range(iters):
        ratings = {t: 1500.0 for t in targets}
        match_count = {t: 0 for t in targets}
        sample = [matches[rng.randrange(len(matches))] for _ in range(len(matches))]
        for a, b, score_a in sample:
            k = k_initial if min(match_count[a], match_count[b]) < settle_threshold else k_settled
            new_a, new_b = _elo_update(ratings[a], ratings[b], score_a, k)
            ratings[a] = new_a
            ratings[b] = new_b
            match_count[a] += 1
            match_count[b] += 1
        for t in targets:
            all_ratings[t].append(ratings[t])
    ci = {}
    for t in targets:
        rs = sorted(all_ratings[t])
        ci[t] = (rs[int(0.05 * iters)], rs[int(0.95 * iters)])
    return ci


def run_elo(targets: list[Target], tracker: CostTracker, args) -> dict:
    """Pairwise ELO ranking over the dialogue slice.

    For each prompt, every (a, b) pair plays one match. A and B labels are
    randomized per match to absorb judge first-position bias. Replies are
    generated once per (target, prompt) and cached.
    """
    if len(targets) < 2:
        raise SystemExit("--mode elo requires at least 2 --target flags")

    records = load_slice("dialogue", version=args.suite, split=args.split)
    if args.limit:
        records = records[: args.limit]

    judge = load_judge(args.judge, args.suite)
    invoke = judge_invoker(judge, tracker)

    rng = random.Random(0xe10)
    # Use `model@base_url` as the canonical id so two `--target` flags with
    # the same model name but different providers / urls don't collide.
    def _target_id(t: Target) -> str:
        return f"{t.model}@{t.base_url}"
    target_ids = [_target_id(t) for t in targets]
    if len(set(target_ids)) != len(target_ids):
        raise SystemExit(f"--target flags must be unique on model+base_url; got {target_ids}")

    # Reply cache: (target_id, prompt_id) -> (reply, error)
    replies: dict[tuple[str, str], tuple[str, Optional[str]]] = {}
    for t in targets:
        tid = _target_id(t)
        for rec in records:
            try:
                reply, usage = call_chat(t, DIALOGUE_SYS, rec["prompt"], max_tokens=200)
                tracker.record(t, usage)
                replies[(tid, rec["id"])] = (reply, None)
            except Exception as e:
                replies[(tid, rec["id"])] = ("", str(e))
        print(f"[elo] candidate replies ready: {tid}")

    matches: list[tuple[str, str, float]] = []  # (winner_id, loser_id, score_a-as-listed-first)
    match_log: list[dict] = []
    pairs = list(itertools.combinations(target_ids, 2))
    for rec in records:
        prompt_id = rec["id"]
        for a, b in pairs:
            reply_a, err_a = replies[(a, prompt_id)]
            reply_b, err_b = replies[(b, prompt_id)]
            if err_a and err_b:
                continue  # both failed; no signal
            if err_a:
                # Canonical (a, b, score_a) ordering. A errored → score_a=0
                # (A loses, B gains). Earlier (b, a, 0.0) recorded B losing.
                matches.append((a, b, 0.0))
                match_log.append({"prompt": prompt_id, "a": a, "b": b, "winner": "B", "reason": f"A error: {err_a[:60]}"})
                continue
            if err_b:
                matches.append((a, b, 1.0))
                match_log.append({"prompt": prompt_id, "a": a, "b": b, "winner": "A", "reason": f"B error: {err_b[:60]}"})
                continue
            # Position randomization: 50% of the time swap A/B
            swap = rng.random() < 0.5
            shown_a, shown_b = (reply_b, reply_a) if swap else (reply_a, reply_b)
            judgment = grade_pairwise(shown_a, shown_b, rec["prompt"], judge, invoke)
            w = judgment["winner"]
            # Map judge's verdict back to canonical (a, b) labels
            if w == "tie":
                score_a = 0.5
            elif (w == "A" and not swap) or (w == "B" and swap):
                score_a = 1.0
            else:
                score_a = 0.0
            matches.append((a, b, score_a))
            match_log.append({
                "prompt": prompt_id, "a": a, "b": b, "swap": swap,
                "winner": w, "score_a": score_a, "reason": judgment.get("reason", "")[:120],
            })

    # ELO accumulation
    k_initial = 32.0
    ratings = {t: 1500.0 for t in target_ids}
    match_count = {t: 0 for t in target_ids}
    for a, b, score_a in matches:
        k = k_initial if min(match_count[a], match_count[b]) < 50 else 16.0
        new_a, new_b = _elo_update(ratings[a], ratings[b], score_a, k)
        ratings[a] = new_a
        ratings[b] = new_b
        match_count[a] += 1
        match_count[b] += 1

    ci = _bootstrap_ci(matches, target_ids, k_initial)

    standings = sorted(
        [(t, ratings[t], match_count[t], ci[t]) for t in target_ids],
        key=lambda r: -r[1],
    )
    print("\n[elo] standings:")
    for tid, rating, n, (lo, hi) in standings:
        print(f"  {rating:7.1f}  [CI {lo:6.1f}–{hi:6.1f}]  n={n:3d}  {tid}")

    return {
        "mode": "elo",
        "judge": judge["judge_id"],
        "rubric_sha256": judge["rubric_sha256"],
        "targets": target_ids,
        "prompts": len(records),
        "matches": len(matches),
        "ratings": ratings,
        "match_counts": match_count,
        "ci": {t: list(ci[t]) for t in target_ids},
        "standings": [
            {"target": t, "rating": r, "matches": n, "ci_lo": lo, "ci_hi": hi}
            for t, r, n, (lo, hi) in standings
        ],
        "match_log": match_log,
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--target", action="append", required=True,
                    help="model@base_url[#env:VAR]; pass multiple times in --mode elo")
    ap.add_argument("--suite", default="v1")
    ap.add_argument("--slice", default=None,
                    choices=["intent", "dialogue", "reaction", "tier2-sim", "tier3-sim", "gaeilge", "all"],
                    help="absolute-score mode: one slice to run (omit when --mode elo)")
    ap.add_argument("--mode", default="absolute", choices=["absolute", "elo"],
                    help="absolute: per-slice graders; elo: pairwise ELO over dialogue slice")
    ap.add_argument("--judge", default=None,
                    help="judge config id (default judge_v1 in absolute mode, judge_pairwise_v1 in elo mode)")
    ap.add_argument("--limit", type=int, default=None)
    ap.add_argument("--split", default="dev", choices=["dev", "holdout"])
    args = ap.parse_args()

    if args.mode == "elo":
        if args.slice is not None and args.slice != "dialogue":
            raise SystemExit("--mode elo currently only supports the dialogue slice")
        if args.judge is None:
            args.judge = "judge_pairwise_v1"
        targets = [parse_target(t) for t in args.target]
        tracker = CostTracker()
        started = time.time()
        out = {
            "suite": args.suite, "split": args.split, "mode": "elo",
            "run_started_utc": datetime.now(timezone.utc).isoformat(),
            "elo": run_elo(targets, tracker, args),
        }
        elapsed = time.time() - started
        out["elapsed_seconds"] = elapsed
        out["cost"] = {"calls": tracker.calls, "prompt_tokens": tracker.prompt_tokens,
                       "completion_tokens": tracker.completion_tokens, "usd": tracker.usd}
        print(f"\ntotal: {tracker.summary()} in {elapsed:.1f}s")
        _PROOFS_DIR.mkdir(parents=True, exist_ok=True)
        out_path = _PROOFS_DIR / f"elo_{utc_stamp()}.json"
        out_path.write_text(json.dumps(out, indent=2, default=str) + "\n", encoding="utf-8")
        print(f"wrote {out_path}")
        return

    # absolute mode
    if args.slice is None:
        raise SystemExit("--slice is required in absolute mode")
    if args.judge is None:
        args.judge = "judge_v1"
    if len(args.target) > 1:
        raise SystemExit("absolute mode takes one --target; pass --mode elo for multi-target sweeps")
    target = parse_target(args.target[0])
    tracker = CostTracker()
    started = time.time()

    slices = (
        ["intent", "dialogue", "reaction", "tier2-sim", "tier3-sim", "gaeilge"]
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
