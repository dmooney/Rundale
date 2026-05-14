#!/usr/bin/env python3
"""Multi-axis 0-10 scorer over cached dialogue samples.

For each cached reply the judge emits one integer per axis (0-10) plus an
explicit `total` (0.0-10.0) that the judge computes as a weighted mean.
Per-candidate aggregates are written alongside the per-record scores so a
human can scan the table without re-deriving the rollups.

Default axes mirror the 5-axis dialogue rubric (character, authenticity,
language, responsiveness, craft) — same dimensions that judge_v1 used on
the 1-5 scale, now stretched to 0-10 for finer discrimination.

Usage::

    set -a; source .env; set +a
    python3 parish/testing/rundale-bench/score_multiaxis.py \\
        --samples docs/proofs/rundale-bench/dialogue_samples_<ts>.json \\
        --judge-model mistralai/mistral-large-2512 \\
        --output docs/proofs/rundale-bench/multiaxis_<ts>.json

Or use the justfile recipe `multiaxis-from-cache`.
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(_REPO_ROOT / "parish" / "scripts" / "local-eval"))

from eval_lib import CostTracker, Target, call_chat  # noqa: E402

DEFAULT_AXES = ["character", "authenticity", "language", "responsiveness", "craft"]

DEFAULT_RUBRIC = """You are an impartial judge scoring a single piece of fictional dialogue from rural Ireland in 1820. The model is playing Brigid O'Brien, a 42-year-old midwife.

Score the reply on each axis independently on an integer 0-10 scale, where:

  - 0  = broken, off-topic, or non-Latin script anywhere in the reply
  - 3  = recognisable midwife voice but flat, anachronistic, or unresponsive
  - 6  = period-plausible, on-topic, no glaring tells
  - 8  = strong period voice with idiomatic Hiberno-English
  - 10 = a sentence a real 1820 Irish midwife could have spoken

Axes:
  - CHARACTER         — reads as the specific persona (rural Irish midwife, 1820, woman of folk-medicine experience)
  - AUTHENTICITY      — period-appropriate vocabulary, no anachronisms or modern terms ("hi", "okay", "guys")
  - LANGUAGE          — idiomatic Hiberno-English; optional Irish (ga-IE) code-switching is encouraged; non-Latin scripts = 0
  - RESPONSIVENESS    — actually addresses the player's prompt rather than deflecting or generic-folksy-padding
  - CRAFT             — concise, evocative, 1-3 sentences as instructed; no monologuing

After the five axes, emit `total` as the unweighted mean of the five integer axes (0.0-10.0, one decimal). Use a strict integer for axis scores; the total is a float.

Output ONLY a JSON object: {"character":<0-10>, "authenticity":<0-10>, "language":<0-10>, "responsiveness":<0-10>, "craft":<0-10>, "total":<0.0-10.0>, "reason":"<one short sentence, ≤25 words>"}. No prose, no markdown."""

_JUDGE_MAX_TOKENS = 2000


def build_schema(axes: list[str]) -> dict:
    props = {a: {"type": "integer"} for a in axes}
    props["total"] = {"type": "number"}
    props["reason"] = {"type": "string"}
    return {
        "name": "multiaxis_score",
        "strict": True,
        "schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": props,
            "required": axes + ["total", "reason"],
        },
    }


def score_one(rubric: str, reply: str, judge_target: Target, tracker: CostTracker, axes: list[str]) -> dict:
    schema = build_schema(axes)
    user = f"Reply to score:\n\n{reply}\n"
    try:
        text, usage = call_chat(judge_target, rubric, user, schema=schema,
                                temperature=0, max_tokens=_JUDGE_MAX_TOKENS)
        tracker.record(judge_target, usage)
        if not text:
            raise ValueError("empty content (reasoning model truncated?)")
        out = json.loads(text)
        scored = {a: int(out.get(a) or 0) for a in axes}
        scored["total"] = float(out.get("total") or 0.0)
        scored["reason"] = str(out.get("reason", ""))[:200]
        return scored
    except Exception as e:
        return {a: 0 for a in axes} | {"total": 0.0, "reason": "", "error": str(e)}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--samples", required=True, help="cached dialogue_samples_*.json")
    ap.add_argument("--rubric-file", default=None,
                    help="path to rubric text; defaults to DEFAULT_RUBRIC bundled here")
    ap.add_argument("--axes", default=",".join(DEFAULT_AXES),
                    help="comma-separated axis names (must match the rubric's expectations)")
    ap.add_argument("--judge-model", default="mistralai/mistral-large-2512")
    ap.add_argument("--judge-url", default="https://openrouter.ai/api/v1")
    ap.add_argument("--judge-env", default="OPENROUTER_API_KEY")
    ap.add_argument("--limit", type=int, default=None,
                    help="cap samples per candidate")
    ap.add_argument("--output", default=None)
    args = ap.parse_args()

    axes = [a.strip() for a in args.axes.split(",") if a.strip()]
    rubric = Path(args.rubric_file).read_text(encoding="utf-8") if args.rubric_file else DEFAULT_RUBRIC

    data = json.loads(Path(args.samples).read_text(encoding="utf-8"))
    judge_target = Target(model=args.judge_model, base_url=args.judge_url, api_key_env=args.judge_env)
    tracker = CostTracker()
    started = time.time()

    per_cand: dict[str, dict] = defaultdict(lambda: {a: [] for a in axes + ["total"]})
    per_record: list[dict] = []
    eligible = [s for s in data["samples"] if not s.get("error") and s.get("reply", "").strip()]
    total_n = len(eligible)
    print(f"scoring {total_n} replies × {len(axes)} axes with {args.judge_model}", flush=True)

    for i, s in enumerate(eligible, 1):
        if args.limit and len(per_cand[s["candidate"]]["total"]) >= args.limit:
            continue
        t0 = time.time()
        scored = score_one(rubric, s["reply"], judge_target, tracker, axes)
        dt = time.time() - t0
        for a in axes:
            per_cand[s["candidate"]][a].append(scored[a])
        per_cand[s["candidate"]]["total"].append(scored["total"])
        per_record.append({
            "candidate": s["candidate"],
            "prompt_id": s["prompt_id"],
            "reply": s["reply"],
            **scored,
        })
        tag = " ERR" if scored.get("error") else ""
        print(f"  [{i:4d}/{total_n}] {dt:5.1f}s  total={scored['total']:4.1f}{tag}  "
              f"${tracker.usd:7.4f} cum  {s['candidate'][:30]:30s} {s['prompt_id']}", flush=True)

    aggregates = {}
    for cand, bucket in per_cand.items():
        ag = {}
        for a in axes:
            xs = bucket[a]
            ag[a + "_mean"] = sum(xs) / len(xs) if xs else 0.0
            ag[a + "_n"] = len(xs)
        ts = bucket["total"]
        ag["total_mean"] = sum(ts) / len(ts) if ts else 0.0
        ag["total_n"] = len(ts)
        aggregates[cand] = ag

    ranked = sorted(aggregates.items(), key=lambda kv: -kv[1]["total_mean"])
    print(f"\n0-10 multiaxis standings (total = mean of axes):")
    header = f"{'candidate':50s}  n   total " + " ".join(f"{a[:5]:>5}" for a in axes)
    print(header)
    for cand, ag in ranked:
        line = f"{cand:50s} {ag['total_n']:3d}  {ag['total_mean']:5.2f} " + " ".join(f"{ag[a + '_mean']:5.2f}" for a in axes)
        print(line)
    print(f"\n{tracker.summary()} in {time.time() - started:.1f}s")

    out = {
        "scored_at_utc": datetime.now(timezone.utc).isoformat(),
        "samples_file": args.samples,
        "judge": {"model": args.judge_model, "url": args.judge_url},
        "axes": axes,
        "rubric": rubric,
        "aggregates": aggregates,
        "ranked": [{"candidate": c, **ag} for c, ag in ranked],
        "per_record": per_record,
        "cost": {"calls": tracker.calls, "usd": tracker.usd,
                 "prompt_tokens": tracker.prompt_tokens,
                 "completion_tokens": tracker.completion_tokens},
    }
    if args.output:
        out_path = Path(args.output)
    else:
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        out_path = _REPO_ROOT / "docs" / "proofs" / "rundale-bench" / f"multiaxis_{stamp}.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
