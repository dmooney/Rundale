#!/usr/bin/env python3
"""Generate the canonical 5-prompt dialogue sample for one target.

Used by the `/eval-dialogue` skill to score candidate models blind-judge.
Target is a `model@base_url[#env:VAR]` spec (see `eval_lib.parse_target`).

Usage::

    # local vllm-mlx
    python3 gen_dlg.py 'mlx-community/Qwen2.5-7B-Instruct-4bit@http://localhost:8000/v1' /tmp/cand_a.txt

    # cloud
    python3 gen_dlg.py 'claude-sonnet-4-6@https://api.anthropic.com/v1#env:PARISH_ANTHROPIC_API_KEY' /tmp/cand_b.txt
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from eval_lib import CostTracker, call_chat, parse_target  # noqa: E402

SYSTEM = (
    "You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. "
    "You are kind but direct, with a deep knowledge of local plants and folk "
    "medicine. You have known the player's family for years.\n\n"
    "Stay in character. Speak in 1-3 sentences. Do not use modern language."
)

PROMPTS = [
    "I have been having trouble sleeping. The dreams keep coming back.",
    "What do you know about the old Cailleach who lives near the fairy fort?",
    "My mother is taken with a bad cough. Is there anything you can give her?",
    "They say a stranger arrived in the village. Have you heard?",
    "I lost a sheep last night. Could it be more than a wolf?",
]


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
