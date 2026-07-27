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
    prompt_text = rec.get("user") or rec.get("prompt", "")

    # Empty / whitespace-only candidate output is a bench_bug — skip the judge
    # call entirely (it would score 1 anyway and wastes API tokens).
    if not output or not str(output).strip():
        return {
            "pass": False,
            "score": 0.0,
            "reason": "bench_bug — empty candidate output (excluded from means)",
            "namedScores": {"bench_bug": 1.0, "empty_output": 1.0},
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

    # §3.7: recompute overall from named scores using explicit weights when the
    # v2 dialogue axes are present. This removes judge arithmetic variance and
    # mirrors the live harness's coherence/character 1.5 priority.
    _DIALOGUE_V2_WEIGHTS = {
        "character": 1.5,
        "mood_fidelity": 1.5,
        "grounding": 1.5,
        "brevity": 1.25,
        "repetition": 1.25,
        "responsiveness": 1.0,
        "authenticity": 1.0,
        "language": 0.75,
        "craft": 0.5,
    }
    v2_axes = set(_DIALOGUE_V2_WEIGHTS.keys())
    if v2_axes.issubset(set(axes.keys())) and slice_name == "dialogue":
        # All v2 axes present — recompute overall in code, ignore judge's value.
        weight_sum = sum(_DIALOGUE_V2_WEIGHTS.values())
        overall = sum(_DIALOGUE_V2_WEIGHTS[k] * axes[k] for k in _DIALOGUE_V2_WEIGHTS) / weight_sum
        overall = round(overall, 1)

    # §3.7 (multiturn): recompute the multiturn overall from axes in code so it
    # doesn't depend on the judge's arithmetic — mirrors the dialogue recompute above.
    _MULTITURN_V2_WEIGHTS = {
        "continuity": 1.5,
        "name_fidelity": 1.5,
        "no_premature_farewell": 1.25,
        "persona_consistency": 1.25,
        "memory_retention": 1.0,
        "freshness": 0.5,
    }
    mt_axes = set(_MULTITURN_V2_WEIGHTS.keys())
    if mt_axes.issubset(set(axes.keys())) and slice_name == "multiturn":
        # All multiturn v2 axes present — recompute overall in code, ignore judge's value.
        weight_sum = sum(_MULTITURN_V2_WEIGHTS.values())
        overall = (
            sum(_MULTITURN_V2_WEIGHTS[k] * axes[k] for k in _MULTITURN_V2_WEIGHTS) / weight_sum
        )
        overall = round(overall, 1)

    named["overall"] = float(overall)

    if flags.get("non_latin_detected"):
        named["non_latin"] = 1.0
    if flags.get("refused"):
        named["refused"] = 1.0

    # §3.2/§3.4: hard-floor flags — degenerate_loop and fabricated force a fail
    # regardless of overall score. These are model-quality signals, not bench bugs.
    hard_fail = False
    if flags.get("degenerate_loop"):
        named["degenerate_loop"] = 1.0
        hard_fail = True
    if flags.get("fabricated"):
        named["fabricated"] = 1.0
        hard_fail = True

    passed = (overall >= 3.0) and not hard_fail

    return {
        "pass": passed,
        "score": overall / 5.0,
        "reason": (
            f"{slice_name} judged by {res.get('judge_model')}: overall {overall:.1f} "
            f"({', '.join(f'{k}={v}' for k, v in axes.items())})"
            + (
                f" [HARD FAIL: {','.join(k for k in ('degenerate_loop', 'fabricated') if flags.get(k))}]"
                if hard_fail
                else ""
            )
        ),
        "namedScores": named,
    }
