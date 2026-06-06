"""Deterministic JSON-schema validity for tier2-sim / tier3-sim (no judge).

Reuses rundale-bench/grade.py::grade_schema. Pairs with rubric_judge.py on the
sim slices: this gates schema_valid (deterministic), the judge scores
plausibility. namedScores.schema_valid feeds the report's schema_valid_rate.
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
    # Runtime-faithful sim records carry `grade_schema` (the request itself sends
    # no schema, matching the live engine); legacy records used `schema`.
    schema = rec.get("grade_schema") or rec.get("schema", {})

    try:
        res = rb.rb_grade.grade_schema(output, schema)
        valid = bool(res.get("schema_valid"))
        errors = res.get("errors", [])
    except Exception as e:  # noqa: BLE001 — never crash the eval on a bad row
        valid = False
        errors = [f"grading error: {e}"]
    return {
        "pass": valid,
        "score": 1.0 if valid else 0.0,
        "reason": "schema valid" if valid else f"schema invalid: {errors[:3]}",
        "namedScores": {"schema_valid": 1.0 if valid else 0.0},
    }
