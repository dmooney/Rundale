"""Deterministic intent grader (no judge) — exact label + Jaccard.

Reuses rundale-bench/grade.py::grade_intent so scoring is byte-identical to v1.
Returns a promptfoo GradingResult; namedScores carry label_match + score for
the report to roll up label_match_rate / mean_score.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402


def get_assert(output, context):
    vars_ = (context or {}).get("vars", {}) or {}
    rec = json.loads(vars_.get("record", "{}"))
    gold = rec.get("gold", {})

    try:
        pred = json.loads(output) if isinstance(output, str) else output
        if not isinstance(pred, dict):
            raise ValueError("prediction is not a JSON object")
    except Exception as e:  # noqa: BLE001
        return {
            "pass": False,
            "score": 0.0,
            "reason": f"intent prediction not parseable: {e}",
            "namedScores": {"label_match": 0.0, "intent_score": 0.0},
        }

    graded = rb.rb_grade.grade_intent(pred, gold)
    return {
        "pass": graded["label_match"] == 1,
        "score": graded["score"],
        "reason": (
            f"label {'match' if graded['label_match'] else 'MISS'} "
            f"(pred={pred.get('intent')!r} gold={gold.get('intent')!r}); "
            f"target_j={graded['target_jaccard']:.2f} dialogue_j={graded['dialogue_jaccard']:.2f}"
        ),
        "namedScores": {
            "label_match": float(graded["label_match"]),
            "intent_score": graded["score"],
        },
    }
