#!/usr/bin/env python3
"""Perf + JSON-compliance probe for rundale-bench candidates.

Measures three things the rubric quality benches don't capture:

  - **ttft_ms**       — time to first content token (streaming)
  - **tok/s**         — completion tokens / generation seconds
  - **json_rate**     — fraction of "emit JSON" requests that round-trip
                       through json.loads. Two flavors:
                          (a) free-form "respond in JSON" prompt
                          (b) schema-enforced `response_format`

Usage::

    set -a; source .env; set +a
    python3 rundale-bench/bench_perf.py \\
        --target 'google/gemma-4-31b-it@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \\
        --target 'qwen/qwen-2.5-72b-instruct@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \\
        --prompts 10

Writes `rundale-bench/artifacts/perf_<UTC>.json` with per-call records
plus per-candidate medians.
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent
sys.path.insert(0, str(_REPO_ROOT / "parish" / "scripts" / "local-eval"))

from eval_lib import (  # noqa: E402
    CostTracker,
    build_dialogue_system_prompt,
    call_chat,
    call_chat_streaming,
    load_slice,
    parse_target,
)

_ARTIFACTS_DIR = _BENCH_DIR / "artifacts"

# Mirrors `parish_npc::build_tier1_system_prompt` for the Brigid persona so
# perf measurements use realistic runtime prompt lengths (issue #994).
DIALOGUE_SYS = build_dialogue_system_prompt()

JSON_FREEFORM_SYS = (
    'You respond only with a JSON object: {"answer": <string>, "confidence": <0.0-1.0>}. '
    "No prose, no markdown fences, no commentary."
)
JSON_FREEFORM_PROMPTS = [
    "What's the boiling point of water in Celsius?",
    "Name the first month of the year.",
    "What's 7 times 8?",
    "Who wrote Hamlet?",
    "What is the chemical symbol for gold?",
]

JSON_SCHEMA = {
    "name": "qa",
    "strict": True,
    "schema": {
        "type": "object",
        "additionalProperties": False,
        "properties": {
            "answer": {"type": "string"},
            "confidence": {"type": "number"},
        },
        "required": ["answer", "confidence"],
    },
}


def percentile(xs: list[float], p: float) -> float:
    if not xs:
        return 0.0
    xs_sorted = sorted(xs)
    k = (len(xs_sorted) - 1) * p
    f = int(k)
    c = min(f + 1, len(xs_sorted) - 1)
    if f == c:
        return xs_sorted[f]
    return xs_sorted[f] + (xs_sorted[c] - xs_sorted[f]) * (k - f)


def measure_streaming(t, records: list[dict], max_tokens: int) -> list[dict]:
    out = []
    for rec in records:
        t0 = time.time()
        err = None
        result: dict = {}
        try:
            result = call_chat_streaming(t, DIALOGUE_SYS, rec["prompt"], max_tokens=max_tokens)
        except Exception as e:
            err = str(e)
        out.append(
            {
                "candidate": t.model,
                "prompt_id": rec["id"],
                "ttft_ms": result.get("ttft_ms"),
                "total_ms": result.get("total_ms", int((time.time() - t0) * 1000)),
                "completion_tokens": result.get("completion_tokens"),
                "prompt_tokens": result.get("prompt_tokens"),
                "tokens_per_second": result.get("tokens_per_second"),
                "reply_len": len(result.get("text", "")),
                "error": err,
            }
        )
    return out


def measure_json_compliance(t, n: int, with_schema: bool, tracker: CostTracker) -> dict:
    valid = 0
    total = 0
    errors = []
    for i in range(n):
        prompt = JSON_FREEFORM_PROMPTS[i % len(JSON_FREEFORM_PROMPTS)]
        total += 1
        try:
            text, usage = call_chat(
                t,
                JSON_FREEFORM_SYS,
                prompt,
                schema=JSON_SCHEMA if with_schema else None,
                max_tokens=200,
                temperature=0,
            )
            tracker.record(t, usage)
            parsed = json.loads(text)
            if isinstance(parsed, dict) and "answer" in parsed:
                valid += 1
            else:
                errors.append(f"parsed but missing keys: {text[:80]}")
        except json.JSONDecodeError as e:
            errors.append(f"json_decode: {e} | text={text[:80]}")
        except Exception as e:
            errors.append(f"{type(e).__name__}: {e}")
    return {
        "n": total,
        "valid": valid,
        "rate": valid / total if total else 0.0,
        "errors": errors[:5],
    }


def main() -> None:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--target", action="append", required=True, help="model spec; repeat")
    ap.add_argument("--suite", default="v1")
    ap.add_argument("--split", default="dev", choices=["dev", "holdout"])
    ap.add_argument("--prompts", type=int, default=10, help="dialogue prompts for ttft/tps")
    ap.add_argument("--json-trials", type=int, default=10, help="JSON-compliance trials per mode")
    ap.add_argument("--max-tokens", type=int, default=200)
    ap.add_argument("--output", default=None)
    args = ap.parse_args()

    targets = [parse_target(t) for t in args.target]
    records = load_slice("dialogue", version=args.suite, split=args.split)[: args.prompts]
    tracker = CostTracker()
    started = time.time()

    per_target: dict[str, dict] = {}
    for t in targets:
        print(f"[perf] {t.model}", flush=True)
        print(f"  streaming dialogue ({len(records)} prompts)…", flush=True)
        stream_records = measure_streaming(t, records, args.max_tokens)
        # Tokens from streaming are recorded out-of-band; estimate cost via known token totals
        for rec in stream_records:
            usage = {
                "prompt_tokens": rec.get("prompt_tokens") or 0,
                "completion_tokens": rec.get("completion_tokens") or 0,
            }
            tracker.record(t, usage)
            tt = rec.get("ttft_ms")
            tps = rec.get("tokens_per_second")
            err = " ERR" if rec.get("error") else ""
            print(
                f"    {rec['prompt_id']}  ttft={str(tt) + 'ms':>8}  total={rec['total_ms']:5d}ms  "
                f"tok/s={tps:5.1f}{err}"
                if tps
                else f"    {rec['prompt_id']}  ttft={str(tt) + 'ms':>8}  total={rec['total_ms']:5d}ms{err}",
                flush=True,
            )

        print(f"  json-compliance free-form ({args.json_trials})…", flush=True)
        json_free = measure_json_compliance(t, args.json_trials, with_schema=False, tracker=tracker)
        print(
            f"    free-form  rate={json_free['rate']:.2%}  ({json_free['valid']}/{json_free['n']})",
            flush=True,
        )

        print(f"  json-compliance schema-enforced ({args.json_trials})…", flush=True)
        json_schema = measure_json_compliance(
            t, args.json_trials, with_schema=True, tracker=tracker
        )
        print(
            f"    schema     rate={json_schema['rate']:.2%}  ({json_schema['valid']}/{json_schema['n']})",
            flush=True,
        )

        ttfts = [r["ttft_ms"] for r in stream_records if r.get("ttft_ms") is not None]
        tpss = [r["tokens_per_second"] for r in stream_records if r.get("tokens_per_second")]
        totals = [r["total_ms"] for r in stream_records if not r.get("error")]
        ok_n = sum(1 for r in stream_records if not r.get("error"))
        per_target[t.model] = {
            "n_streamed": len(stream_records),
            "n_ok": ok_n,
            "ttft_ms_median": int(statistics.median(ttfts)) if ttfts else None,
            "ttft_ms_p90": int(percentile(ttfts, 0.9)) if ttfts else None,
            "total_ms_median": int(statistics.median(totals)) if totals else None,
            "tokens_per_second_median": round(statistics.median(tpss), 1) if tpss else None,
            "tokens_per_second_p90": round(percentile(tpss, 0.9), 1) if tpss else None,
            "json_freeform": json_free,
            "json_schema": json_schema,
            "stream_records": stream_records,
        }
        s = per_target[t.model]
        print(
            f"  → ttft p50={s['ttft_ms_median']}ms  total p50={s['total_ms_median']}ms  "
            f"tok/s p50={s['tokens_per_second_median']}  "
            f"json: free={json_free['rate']:.0%} / schema={json_schema['rate']:.0%}",
            flush=True,
        )

    elapsed = time.time() - started
    out = {
        "ran_at_utc": datetime.now(timezone.utc).isoformat(),
        "suite": args.suite,
        "split": args.split,
        "dialogue_prompts": len(records),
        "json_trials": args.json_trials,
        "per_target": per_target,
        "elapsed_seconds": elapsed,
        "cost": {"usd": tracker.usd, "calls": tracker.calls},
    }

    if args.output:
        out_path = Path(args.output)
    else:
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        out_path = _ARTIFACTS_DIR / f"perf_{stamp}.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    print()
    print(f"{'candidate':50s}  ttft50  total50  tok/s50  jf%   js%")
    for cand, s in per_target.items():
        ttft = s["ttft_ms_median"] or 0
        total = s["total_ms_median"] or 0
        tps = s["tokens_per_second_median"] or 0
        jf = s["json_freeform"]["rate"]
        js = s["json_schema"]["rate"]
        print(f"{cand:50s} {ttft:5d}ms {total:5d}ms  {tps:6.1f}  {jf:4.0%}  {js:4.0%}")
    print(f"\n{tracker.summary()} in {elapsed:.1f}s")
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
