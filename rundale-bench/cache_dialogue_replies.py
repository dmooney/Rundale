#!/usr/bin/env python3
"""Generate + cache dialogue replies from a set of candidate models.

The goal is rubric iteration without re-querying: hit each (candidate, prompt)
once, save replies to JSON, then prototype judge rubrics offline against the
cache. Subsequent rubric experiments cost zero API spend.

Usage::

    set -a; source .env; set +a
    python3 rundale-bench/cache_dialogue_replies.py \\
        --target 'openai/gpt-oss-120b:free@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \\
        --target 'qwen/qwen3-next-80b-a3b-instruct:free@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \\
        --target 'meta-llama/llama-3.3-70b-instruct:free@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \\
        --target 'google/gemma-4-31b-it:free@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \\
        --target 'qwen/qwen3-235b-a22b-2507@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \\
        --target 'mistralai/mistral-small-24b-instruct-2501@https://openrouter.ai/api/v1#env:OPENROUTER_API_KEY' \\
        --prompts 15

Output: `rundale-bench/artifacts/dialogue_samples_<UTC>.json` containing
one record per (candidate, prompt) with reply text, usage, latency, and
any error. Re-runs append a new file rather than mutating an existing one.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent
sys.path.insert(0, str(_REPO_ROOT / "parish" / "scripts" / "local-eval"))

from eval_lib import (  # noqa: E402
    CostTracker,
    build_dialogue_system_prompt,
    call_chat,
    load_slice,
    parse_target,
)

_ARTIFACTS_DIR = _BENCH_DIR / "artifacts"

# Mirrors `parish_npc::build_tier1_system_prompt` for the Brigid persona so
# bench scores track the runtime tier-1 grounding (issue #994).
DIALOGUE_SYS = build_dialogue_system_prompt()


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def main() -> None:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument(
        "--target",
        action="append",
        required=True,
        help="model@base_url[#env:VAR]; pass multiple times",
    )
    ap.add_argument("--suite", default="v1")
    ap.add_argument("--split", default="dev", choices=["dev", "holdout"])
    ap.add_argument("--prompts", type=int, default=15, help="prompts per candidate")
    ap.add_argument("--max-tokens", type=int, default=200)
    ap.add_argument(
        "--output",
        default=None,
        help="output path (default: auto-stamped under rundale-bench/artifacts/)",
    )
    args = ap.parse_args()

    targets = [parse_target(t) for t in args.target]
    records = load_slice("dialogue", version=args.suite, split=args.split)[: args.prompts]

    tracker = CostTracker()
    started = time.time()
    samples: list[dict] = []
    for t in targets:
        print(f"[cache] {t.model}")
        for rec in records:
            t0 = time.time()
            err = None
            reply = ""
            usage: dict[str, Any] = {}
            try:
                reply, usage = call_chat(t, DIALOGUE_SYS, rec["prompt"], max_tokens=args.max_tokens)
                tracker.record(t, usage)
            except Exception as e:
                err = str(e)
            samples.append(
                {
                    "candidate": t.model,
                    "base_url": t.base_url,
                    "prompt_id": rec["id"],
                    "prompt": rec["prompt"],
                    "tier": rec.get("tier"),
                    "reply": reply,
                    "usage": usage,
                    "latency_ms": int((time.time() - t0) * 1000),
                    "error": err,
                }
            )

    elapsed = time.time() - started
    out = {
        "suite": args.suite,
        "split": args.split,
        "system_prompt": DIALOGUE_SYS,
        "max_tokens": args.max_tokens,
        "run_started_utc": datetime.now(timezone.utc).isoformat(),
        "candidates": [t.model for t in targets],
        "prompts": len(records),
        "samples": samples,
        "elapsed_seconds": elapsed,
        "cost": {
            "calls": tracker.calls,
            "prompt_tokens": tracker.prompt_tokens,
            "completion_tokens": tracker.completion_tokens,
            "usd": tracker.usd,
        },
    }
    _ARTIFACTS_DIR.mkdir(parents=True, exist_ok=True)
    out_path = (
        Path(args.output)
        if args.output
        else _ARTIFACTS_DIR / f"dialogue_samples_{utc_stamp()}.json"
    )
    out_path.write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"\n{tracker.summary()} in {elapsed:.1f}s")
    print(
        f"wrote {out_path} ({len(samples)} samples from {len(targets)} candidates over {len(records)} prompts)"
    )


if __name__ == "__main__":
    main()
