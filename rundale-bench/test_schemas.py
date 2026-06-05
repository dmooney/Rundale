#!/usr/bin/env python3
"""Tests asserting that each of the 5 known failure modes now raises
``pydantic.ValidationError`` by construction rather than crashing mid-format
or being silently ignored.

Failure modes under test
-------------------------
1. None-in-format crash — axes=None on a ResultItemSchema
2. bench_bug=true with nonzero axis scores
3. rubric_sha256 mismatch between result and bundle (tested via
   validate_result when rubric_sha256 is absent/empty on the result)
4. Missing required field (e.g. missing bundle_id on BundleSchema)
5. Malformed shape on ingest (e.g. items is not a list of proper objects)
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest
from pydantic import ValidationError

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

from judge_bundle import validate_result  # noqa: E402
from schemas import (  # noqa: E402
    BundleSchema,
    ResultItemSchema,
    ResultSchema,
    SummarySchema,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _minimal_bundle_dict(**overrides) -> dict:
    base = {
        "bundle_id": "test__stub",
        "slice": "dialogue",
        "candidate": {"model_id": "stub"},
        "judge_id": "judge_sonnet_v1",
        "judge_model": "claude-sonnet-4-5",
        "rubric_sha256": "a" * 64,
        "rubric": "Score the response.",
        "items": [
            {"prompt_id": "p1", "prompt": "Hello", "response": "World"},
        ],
    }
    base.update(overrides)
    return base


def _minimal_result_dict(**overrides) -> dict:
    base = {
        "rubric_sha256": "a" * 64,
        "items": [
            {
                "prompt_id": "p1",
                "axes": {"character": 3},
                "overall": 3.0,
                "flags": {},
            }
        ],
    }
    base.update(overrides)
    return base


def _minimal_result_item(**overrides) -> dict:
    base = {
        "prompt_id": "p1",
        "axes": {"character": 3, "authenticity": 3, "language": 3, "responsiveness": 3, "craft": 3},
        "overall": 3.0,
        "flags": {"bench_bug": False},
    }
    base.update(overrides)
    return base


# ---------------------------------------------------------------------------
# Failure mode 1: None-in-format crash (axes=None on ResultItemSchema)
# ---------------------------------------------------------------------------


def test_failure_mode_1_axes_none_raises():
    """axes=None must raise ValidationError rather than passing through and
    crashing later when code does ``j['axes'].get('character', 0)``."""
    with pytest.raises(ValidationError):
        ResultItemSchema(**{**_minimal_result_item(), "axes": None})


def test_failure_mode_1_axes_non_dict_raises():
    """axes as a list (wrong shape) must also raise."""
    with pytest.raises(ValidationError):
        ResultItemSchema(**{**_minimal_result_item(), "axes": [3, 3, 3]})


def test_failure_mode_1_valid_axes_accepted():
    """Sanity: a well-formed item with dict axes passes validation."""
    item = ResultItemSchema(**_minimal_result_item())
    assert item.axes["character"] == 3


# ---------------------------------------------------------------------------
# Failure mode 2: bench_bug=true with nonzero axis scores
# ---------------------------------------------------------------------------


def test_failure_mode_2_bench_bug_nonzero_axes_raises():
    """bench_bug=true but axes not all zero must raise ValidationError."""
    payload = {
        **_minimal_result_item(),
        "axes": {"character": 1, "authenticity": 0, "language": 0, "responsiveness": 0, "craft": 0},
        "overall": 0.0,
        "flags": {"bench_bug": True},
    }
    with pytest.raises(ValidationError, match="bench_bug=true but axes not all 0"):
        ResultItemSchema(**payload)


def test_failure_mode_2_bench_bug_nonzero_overall_raises():
    """bench_bug=true but overall != 0.0 must raise ValidationError."""
    payload = {
        **_minimal_result_item(),
        "axes": {"character": 0, "authenticity": 0, "language": 0, "responsiveness": 0, "craft": 0},
        "overall": 2.5,
        "flags": {"bench_bug": True},
    }
    with pytest.raises(ValidationError, match="bench_bug=true but overall != 0.0"):
        ResultItemSchema(**payload)


def test_failure_mode_2_bench_bug_all_zeros_accepted():
    """bench_bug=true with every axis and overall == 0 is valid."""
    payload = {
        **_minimal_result_item(),
        "axes": {"character": 0, "authenticity": 0, "language": 0, "responsiveness": 0, "craft": 0},
        "overall": 0.0,
        "flags": {"bench_bug": True},
    }
    item = ResultItemSchema(**payload)
    assert item.flags.bench_bug is True
    assert all(v == 0 for v in item.axes.values())


# ---------------------------------------------------------------------------
# Failure mode 3: rubric_sha256 absent/empty on result
# ---------------------------------------------------------------------------


def test_failure_mode_3_missing_rubric_sha256_raises():
    """A result with no rubric_sha256 must raise ValidationError before any
    comparison against the bundle is attempted."""
    payload = _minimal_result_dict()
    del payload["rubric_sha256"]
    with pytest.raises(ValidationError):
        ResultSchema.model_validate(payload)


def test_failure_mode_3_empty_rubric_sha256_raises():
    """Empty string rubric_sha256 is also rejected."""
    payload = {**_minimal_result_dict(), "rubric_sha256": ""}
    with pytest.raises(ValidationError, match="rubric_sha256 must not be empty"):
        ResultSchema.model_validate(payload)


def test_failure_mode_3_valid_result_accepted():
    """A well-formed result dict passes ResultSchema validation."""
    result = ResultSchema.model_validate(_minimal_result_dict())
    assert result.rubric_sha256 == "a" * 64


# ---------------------------------------------------------------------------
# Failure mode 4: missing required field on BundleSchema
# ---------------------------------------------------------------------------


def test_failure_mode_4_missing_bundle_id_raises():
    payload = _minimal_bundle_dict()
    del payload["bundle_id"]
    with pytest.raises(ValidationError):
        BundleSchema.model_validate(payload)


def test_failure_mode_4_missing_rubric_sha256_raises():
    payload = _minimal_bundle_dict()
    del payload["rubric_sha256"]
    with pytest.raises(ValidationError):
        BundleSchema.model_validate(payload)


def test_failure_mode_4_missing_items_raises():
    payload = _minimal_bundle_dict()
    del payload["items"]
    with pytest.raises(ValidationError):
        BundleSchema.model_validate(payload)


def test_failure_mode_4_empty_items_raises():
    payload = _minimal_bundle_dict(items=[])
    with pytest.raises(ValidationError, match="bundle items list must not be empty"):
        BundleSchema.model_validate(payload)


def test_failure_mode_4_valid_bundle_accepted():
    bundle = BundleSchema.model_validate(_minimal_bundle_dict())
    assert bundle.bundle_id == "test__stub"
    assert len(bundle.items) == 1


# ---------------------------------------------------------------------------
# Failure mode 5: malformed shape on ingest (bundle or result items)
# ---------------------------------------------------------------------------


def test_failure_mode_5_bundle_item_missing_response_raises():
    """A bundle item without 'response' must fail BundleItemSchema."""
    payload = _minimal_bundle_dict(
        items=[{"prompt_id": "p1", "prompt": "Hello"}]  # missing response
    )
    with pytest.raises(ValidationError):
        BundleSchema.model_validate(payload)


def test_failure_mode_5_bundle_item_missing_prompt_id_raises():
    payload = _minimal_bundle_dict(
        items=[{"prompt": "Hello", "response": "World"}]  # missing prompt_id
    )
    with pytest.raises(ValidationError):
        BundleSchema.model_validate(payload)


def test_failure_mode_5_result_items_not_a_list_raises():
    """result['items'] being a dict (wrong type) must raise."""
    payload = {**_minimal_result_dict(), "items": {"p1": {}}}
    with pytest.raises(ValidationError):
        ResultSchema.model_validate(payload)


def test_failure_mode_5_result_item_axis_out_of_range_raises():
    """An axis score of 6 (above ceiling 5) must raise on ResultItemSchema."""
    payload = {**_minimal_result_item(), "axes": {"character": 6}}
    with pytest.raises(ValidationError, match="out of range"):
        ResultItemSchema(**payload)


def test_failure_mode_5_result_item_overall_out_of_range_raises():
    """An overall of 5.5 (above ceiling) must raise."""
    payload = {**_minimal_result_item(), "overall": 5.5}
    with pytest.raises(ValidationError, match="overall must be in"):
        ResultItemSchema(**payload)


# ---------------------------------------------------------------------------
# SummarySchema sanity
# ---------------------------------------------------------------------------


def test_summary_schema_valid():
    summary = SummarySchema(
        slice="dialogue",
        records=10,
        judged=8,
        bench_bugs=1,
        overall=3.75,
        character=3.5,
        authenticity=4.0,
        language=3.8,
        responsiveness=3.7,
        craft=3.9,
    )
    assert summary.slice == "dialogue"
    assert summary.overall == 3.75


def test_summary_schema_negative_count_raises():
    with pytest.raises(ValidationError, match="non-negative"):
        SummarySchema(slice="dialogue", records=-1)


def test_summary_schema_none_scores_allowed():
    """None axis scores are valid (represents 'no judged items yet')."""
    summary = SummarySchema(slice="dialogue", records=5, judged=0, overall=None)
    assert summary.overall is None


# ---------------------------------------------------------------------------
# Malformed done/ result: validate_result must not raise, must return failures
# ---------------------------------------------------------------------------


def _minimal_bundle_for_validate() -> dict:
    return {
        "bundle_id": "test__stub",
        "slice": "dialogue",
        "candidate": {"model_id": "stub"},
        "judge_id": "judge_sonnet_v1",
        "judge_model": "claude-sonnet-4-5",
        "rubric_sha256": "a" * 64,
        "rubric": "Score the response.",
        "axes": ["character", "authenticity", "language", "responsiveness", "craft"],
        "items": [
            {"prompt_id": "p1", "prompt": "Hello", "response": "World"},
        ],
    }


def test_malformed_result_missing_rubric_sha256_returns_failures():
    """A done/ result missing rubric_sha256 must NOT raise; validate_result
    must return ([], failed) where every expected prompt_id is a failure."""
    bundle = _minimal_bundle_for_validate()
    malformed = {"items": [{"prompt_id": "p1", "axes": {"character": 3}, "overall": 3.0}]}
    # rubric_sha256 absent — pydantic ValidationError must be caught internally
    valid, failed = validate_result(malformed, bundle)
    assert valid == []
    assert len(failed) == 1
    assert failed[0]["prompt_id"] == "p1"
    assert failed[0]["flags"]["judge_retry"] is True
    assert "ValidationError" in failed[0]["error"] or "malformed" in failed[0]["error"]


def test_malformed_result_items_not_a_list_returns_failures():
    """A done/ result with items as a dict (wrong shape) must not raise."""
    bundle = _minimal_bundle_for_validate()
    malformed = {"rubric_sha256": "a" * 64, "items": {"p1": {}}}
    valid, failed = validate_result(malformed, bundle)
    assert valid == []
    assert len(failed) == 1
    assert failed[0]["flags"]["judge_retry"] is True


def test_validate_result_does_not_raise_on_valid_result():
    """A well-formed result is processed normally — not treated as malformed."""
    bundle = _minimal_bundle_for_validate()
    result = {
        "rubric_sha256": "a" * 64,
        "items": [
            {
                "prompt_id": "p1",
                "axes": {
                    "character": 3,
                    "authenticity": 3,
                    "language": 3,
                    "responsiveness": 3,
                    "craft": 3,
                },
                "overall": 3.0,
                "flags": {},
            }
        ],
    }
    valid, failed = validate_result(result, bundle)
    assert len(valid) == 1
    assert failed == []
