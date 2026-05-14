#!/usr/bin/env python3
"""Iterate on the judging rubric against cached dialogue samples.

Pairs with `cache_dialogue_replies.py`. Load a cache JSON, score every sample
with a candidate rubric (single-axis 1-10 or 5-axis), inspect score
distributions per candidate, find ties / outliers, then edit the rubric
and re-run — all without re-querying candidate models.

Modes
-----

`absolute` — judge scores each reply individually on the rubric.
`pairwise` — judge picks A vs B between same-prompt replies; aggregate
            into per-candidate win-rate and (optionally) ELO.

Examples::

    # Score every sample on a single-axis 1-10 rubric
    python3 parish/testing/rundale-bench/rubric_lab.py \\
        --samples docs/proofs/rundale-bench/dialogue_samples_<ts>.json \\
        --rubric-file my_rubric.txt --mode absolute --axis-scale 10

    # Pairwise: enumerate every (a, b) pair per prompt, ELO-rank
    python3 parish/testing/rundale-bench/rubric_lab.py \\
        --samples ... --rubric-file pairwise_rubric.txt --mode pairwise

Rubric files
------------

`--rubric-file <path>` reads the rubric text and uses qwen3-235b at temp 0
via OpenRouter (OPENROUTER_API_KEY in env). Override with `--judge-model`.

For absolute mode the script expects the rubric to instruct the judge to
emit `{"score": <int>}` (single-axis); for pairwise it expects
`{"winner": "A"|"B"|"tie", "reason": "..."}`.
"""
from __future__ import annotations

import argparse
import itertools
import json
import random
import sys
from collections import defaultdict
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(_REPO_ROOT / "parish" / "scripts" / "local-eval"))

from eval_lib import CostTracker, Target, call_chat  # noqa: E402


def load_samples(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def absolute_judge(rubric: str, reply: str, judge_target: Target, tracker: CostTracker, scale: int) -> dict:
    schema = {
        "name": "score",
        "strict": True,
        "schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {"score": {"type": "integer"}, "reason": {"type": "string"}},
            "required": ["score", "reason"],
        },
    }
    user = f"Reply to score: {reply}"
    try:
        text, usage = call_chat(judge_target, rubric, user, schema=schema, temperature=0)
        tracker.record(judge_target, usage)
        out = json.loads(text)
        return {"score": int(out.get("score") or 0), "reason": str(out.get("reason", ""))[:200]}
    except Exception as e:
        return {"score": 0, "reason": "", "error": str(e)}


def pairwise_judge(rubric: str, a: str, b: str, prompt: str, judge_target: Target, tracker: CostTracker) -> dict:
    schema = {
        "name": "verdict",
        "strict": True,
        "schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "winner": {"type": "string", "enum": ["A", "B", "tie"]},
                "reason": {"type": "string"},
            },
            "required": ["winner", "reason"],
        },
    }
    user = f"Player prompt: {prompt}\n\n=== Reply A ===\n{a}\n\n=== Reply B ===\n{b}\n"
    try:
        text, usage = call_chat(judge_target, rubric, user, schema=schema, temperature=0)
        tracker.record(judge_target, usage)
        out = json.loads(text)
        w = out.get("winner")
        if w not in ("A", "B", "tie"):
            raise ValueError(f"invalid winner: {w!r}")
        return {"winner": w, "reason": str(out.get("reason", ""))[:200]}
    except Exception as e:
        return {"winner": "tie", "reason": "", "error": str(e)}


def _elo_update(ra: float, rb: float, score_a: float, k: float) -> tuple[float, float]:
    expected_a = 1.0 / (1.0 + 10.0 ** ((rb - ra) / 400.0))
    return ra + k * (score_a - expected_a), rb + k * ((1.0 - score_a) - (1.0 - expected_a))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--samples", required=True, help="path to a dialogue_samples_*.json cache")
    ap.add_argument("--rubric-file", required=True, help="path to a rubric text file")
    ap.add_argument("--mode", default="absolute", choices=["absolute", "pairwise"])
    ap.add_argument("--axis-scale", type=int, default=10, help="absolute mode: max score (1..N)")
    ap.add_argument("--judge-model", default="qwen/qwen3-235b-a22b-2507")
    ap.add_argument("--judge-url", default="https://openrouter.ai/api/v1")
    ap.add_argument("--judge-env", default="OPENROUTER_API_KEY")
    ap.add_argument("--limit", type=int, default=None, help="cap samples (per candidate in absolute mode)")
    ap.add_argument("--output", default=None)
    args = ap.parse_args()

    data = load_samples(Path(args.samples))
    rubric = Path(args.rubric_file).read_text(encoding="utf-8")
    judge_target = Target(model=args.judge_model, base_url=args.judge_url, api_key_env=args.judge_env)
    tracker = CostTracker()

    if args.mode == "absolute":
        per_cand: dict[str, list[int]] = defaultdict(list)
        results: list[dict] = []
        for s in data["samples"]:
            if s.get("error"):
                continue
            if args.limit and len(per_cand[s["candidate"]]) >= args.limit:
                continue
            r = absolute_judge(rubric, s["reply"], judge_target, tracker, args.axis_scale)
            per_cand[s["candidate"]].append(r["score"])
            results.append({"candidate": s["candidate"], "prompt_id": s["prompt_id"], **r})
        print(f"\nabsolute mode — N={args.axis_scale}")
        print(f"{'candidate':50s}  n   mean   median   min   max")
        for cand in sorted(per_cand):
            xs = per_cand[cand]
            mean = sum(xs) / len(xs) if xs else 0
            xs_sorted = sorted(xs)
            median = xs_sorted[len(xs) // 2] if xs_sorted else 0
            print(f"{cand:50s} {len(xs):3d}  {mean:5.2f}  {median:6.1f}  {min(xs, default=0):4d}  {max(xs, default=0):4d}")
        out = {"mode": "absolute", "axis_scale": args.axis_scale, "rubric": rubric,
               "per_candidate": {k: v for k, v in per_cand.items()}, "results": results,
               "cost": {"usd": tracker.usd, "calls": tracker.calls}}
    else:
        # pairwise: group samples by prompt_id, enumerate all (a, b) pairs
        by_prompt: dict[str, list[dict]] = defaultdict(list)
        for s in data["samples"]:
            if not s.get("error") and s.get("reply"):
                by_prompt[s["prompt_id"]].append(s)
        candidates = sorted({s["candidate"] for s in data["samples"]})
        ratings = {c: 1500.0 for c in candidates}
        match_count = {c: 0 for c in candidates}
        match_log = []
        rng = random.Random(0xe10)
        for prompt_id, group in by_prompt.items():
            if len(group) < 2:
                continue
            prompt_text = group[0]["prompt"]
            for sa, sb in itertools.combinations(group, 2):
                swap = rng.random() < 0.5
                shown_a, shown_b = (sb, sa) if swap else (sa, sb)
                v = pairwise_judge(rubric, shown_a["reply"], shown_b["reply"], prompt_text, judge_target, tracker)
                w = v["winner"]
                if w == "tie":
                    score_a = 0.5
                elif (w == "A" and not swap) or (w == "B" and swap):
                    score_a = 1.0
                else:
                    score_a = 0.0
                k = 32.0 if min(match_count[sa["candidate"]], match_count[sb["candidate"]]) < 50 else 16.0
                ra, rb = ratings[sa["candidate"]], ratings[sb["candidate"]]
                ratings[sa["candidate"]], ratings[sb["candidate"]] = _elo_update(ra, rb, score_a, k)
                match_count[sa["candidate"]] += 1
                match_count[sb["candidate"]] += 1
                match_log.append({"prompt": prompt_id, "a": sa["candidate"], "b": sb["candidate"],
                                  "winner": w, "reason": v.get("reason", "")[:120]})
        print("\npairwise ELO standings:")
        for cand, r in sorted(ratings.items(), key=lambda kv: -kv[1]):
            print(f"  {r:7.1f}  n={match_count[cand]:3d}  {cand}")
        out = {"mode": "pairwise", "rubric": rubric, "ratings": ratings,
               "match_counts": match_count, "match_log": match_log,
               "cost": {"usd": tracker.usd, "calls": tracker.calls}}

    print(f"\n{tracker.summary()}")
    if args.output:
        Path(args.output).write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
