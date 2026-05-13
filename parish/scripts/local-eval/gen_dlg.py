#!/usr/bin/env python3
"""Generate the canonical 5-prompt dialogue sample for one target.

Used by the `/eval-dialogue` skill to score candidate models blind-judge.
Target is a `model@base_url[#env:VAR]` spec (see `eval_lib.parse_target`).

Usage::

    # local vllm-mlx
    python3 gen_dlg.py 'mlx-community/Qwen2.5-7B-Instruct-4bit@http://localhost:8000/v1' /tmp/cand_a.txt

    # cloud
    python3 gen_dlg.py 'claude-sonnet-4-6@https://api.anthropic.com/v1#env:ANTHROPIC_API_KEY' /tmp/cand_b.txt
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from eval_lib import CostTracker, call_chat, load_slice, parse_target  # noqa: E402

SYSTEM = (
    "You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. "
    "You are kind but direct, with a deep knowledge of local plants and folk "
    "medicine. You have known the player's family for years.\n\n"
    "Stay in character. Speak in 1-3 sentences. Do not use modern language."
)

# Core 5-prompt set drawn from the frozen rundale-bench v1 dialogue slice.
# Edit parish/testing/rundale-bench/v1/dialogue.jsonl (tier=core) and rebuild
# MANIFEST.json to change them.
PROMPTS = [r["prompt"] for r in load_slice("dialogue", version="v1", tier="core")]
assert len(PROMPTS) == 5, f"core tier must have 5 prompts, has {len(PROMPTS)}"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("target", help="Target spec: 'model@base_url[#env:VAR]'")
    ap.add_argument("output", help="Output path for the transcript")
    args = ap.parse_args()

    target = parse_target(args.target)
    out_path = Path(args.output)

    tracker = CostTracker()
    lines = [f"=== Target: {target.model} @ {target.base_url} ===\n"]
    for p in PROMPTS:
        text, usage = call_chat(target, SYSTEM, p, max_tokens=200)
        tracker.record(target, usage)
        lines.append(f"\nPROMPT: {p}\nREPLY:  {text.strip()}\n")
    lines.append(f"\n=== Cost: {tracker.summary()} ===\n")
    out_path.write_text("".join(lines))
    print(f"wrote {out_path}")
    print(f"cost: {tracker.summary()}")


if __name__ == "__main__":
    main()
