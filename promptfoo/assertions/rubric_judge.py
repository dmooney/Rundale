"""Configurable-API-judge assertion for the LLM-graded slices.

Slices: dialogue (5 axes), reaction (in_character), tier2-sim / tier3-sim
(plausibility), gaeilge (5 axes). Calls the judge configured in
config/judge.yaml (env-overridable) with the copied rubric, parses the batched
single-item envelope, and returns a promptfoo GradingResult whose namedScores
are the per-axis scores + overall. bench_bug / judge_failure items carry a
marker namedScore so the report excludes them from means (matching v1's
`_dialogue_aggregate`).

The slice is taken from the RB_SLICE env var (set by each promptfooconfig).
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402


def get_assert(output, context):
    vars_ = (context or {}).get("vars", {}) or {}
    rec = json.loads(vars_.get("record", "{}"))
    slice_name = os.environ.get("RB_SLICE")
    if not slice_name:
        raise ValueError("RB_SLICE env var required for rubric_judge")
    prompt_id = rec.get("id", vars_.get("rb_id", "?"))
    prompt_text = rec.get("prompt", "")

    # Empty / whitespace-only candidate output is a bench_bug — skip the judge
    # call entirely (it would score 1 anyway and wastes API tokens).
    if not output or not str(output).strip():
        return {
            "pass": False,
            "score": 0.0,
            "reason": "bench_bug — empty candidate output (excluded from means)",
            "namedScores": {"bench_bug": 1.0},
        }

    res = rb.judge_item(slice_name, prompt_id, prompt_text, output, rec)

    if res.get("judge_failure"):
        return {
            "pass": False,
            "score": 0.0,
            "reason": f"judge failure: {res['judge_failure']}",
            "namedScores": {"judge_failure": 1.0},
        }

    flags = res.get("flags", {})
    axes = res.get("axes", {})
    overall = res.get("overall", 0.0)

    if flags.get("bench_bug"):
        return {
            "pass": False,
            "score": 0.0,
            "reason": "bench_bug — no usable candidate output (excluded from means)",
            "namedScores": {"bench_bug": 1.0},
        }

    named = {k: float(v) for k, v in axes.items()}
    named["overall"] = float(overall)
    if flags.get("non_latin_detected"):
        named["non_latin"] = 1.0
    return {
        "pass": overall >= 3.0,
        "score": overall / 5.0,
        "reason": (
            f"{slice_name} judged by {res.get('judge_model')}: overall {overall:.1f} "
            f"({', '.join(f'{k}={v}' for k, v in axes.items())})"
        ),
        "namedScores": named,
    }
