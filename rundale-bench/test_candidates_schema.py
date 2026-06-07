#!/usr/bin/env python3
"""Schema/consistency tests for the local MLX candidate catalog (TD-004 + AC7).

Validates ``candidates_local_mlx.toml`` is well-formed and that
``validate_candidates`` rejects each malformation class. Also covers the
``delete_after_bench`` disk-discipline metadata.
"""

from __future__ import annotations

import sys
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

from candidates_schema import (  # noqa: E402
    candidates_to_delete,
    load_candidates,
    validate_candidates,
)

_CANDIDATES_TOML = _BENCH_DIR / "candidates_local_mlx.toml"


def _ok_row(**overrides) -> dict:
    base = {
        "hf_repo": "mlx-community/Stub-1B-4bit",
        "slot": "tiny",
        "quant": "4bit",
        "peak_ram_gb_est": 1.0,
        "params_b": 1.0,
    }
    base.update(overrides)
    return base


# ---------------------------------------------------------------------------
# The live catalog must pass.
# ---------------------------------------------------------------------------


def test_live_catalog_is_schema_clean() -> None:
    rows = load_candidates(_CANDIDATES_TOML)
    assert rows, "expected at least one [[candidate]] in the live TOML"
    problems = validate_candidates(rows)
    assert problems == [], "\n".join(problems)


def test_live_catalog_hf_repos_unique() -> None:
    rows = load_candidates(_CANDIDATES_TOML)
    repos = [r["hf_repo"] for r in rows]
    assert len(repos) == len(set(repos))


# ---------------------------------------------------------------------------
# Each malformation class is rejected.
# ---------------------------------------------------------------------------


def test_valid_row_has_no_problems() -> None:
    assert validate_candidates([_ok_row()]) == []


def test_duplicate_hf_repo_rejected() -> None:
    rows = [_ok_row(), _ok_row()]
    problems = validate_candidates(rows)
    assert any("duplicate hf_repo" in p for p in problems)


def test_missing_required_field_rejected() -> None:
    row = _ok_row()
    del row["peak_ram_gb_est"]
    problems = validate_candidates([row])
    assert any("missing required field 'peak_ram_gb_est'" in p for p in problems)


def test_negative_ram_estimate_rejected() -> None:
    problems = validate_candidates([_ok_row(peak_ram_gb_est=-2.0)])
    assert any("peak_ram_gb_est must be positive" in p for p in problems)


def test_zero_ram_estimate_rejected() -> None:
    problems = validate_candidates([_ok_row(peak_ram_gb_est=0.0)])
    assert any("peak_ram_gb_est must be positive" in p for p in problems)


def test_bad_slot_rejected() -> None:
    problems = validate_candidates([_ok_row(slot="huge")])
    assert any("slot 'huge' not in" in p for p in problems)


def test_negative_params_rejected() -> None:
    problems = validate_candidates([_ok_row(params_b=-1.0)])
    assert any("params_b must be non-negative" in p for p in problems)


# ---------------------------------------------------------------------------
# AC7 — delete_after_bench metadata
# ---------------------------------------------------------------------------


def test_delete_after_bench_optional() -> None:
    # Absent → valid; present bool → valid.
    assert validate_candidates([_ok_row()]) == []
    assert validate_candidates([_ok_row(delete_after_bench=True)]) == []
    assert validate_candidates([_ok_row(delete_after_bench=False)]) == []


def test_delete_after_bench_must_be_bool() -> None:
    problems = validate_candidates([_ok_row(delete_after_bench="yes")])
    assert any("delete_after_bench must be a bool" in p for p in problems)


def test_candidates_to_delete() -> None:
    rows = [
        _ok_row(hf_repo="a/x", delete_after_bench=True),
        _ok_row(hf_repo="a/y"),  # no flag
        _ok_row(hf_repo="a/z", delete_after_bench=False),
        _ok_row(hf_repo="a/w", delete_after_bench=True),
    ]
    assert candidates_to_delete(rows) == ["a/x", "a/w"]


def test_candidates_to_delete_empty_when_unflagged() -> None:
    rows = load_candidates(_CANDIDATES_TOML)
    # The live catalog has no delete flags today; the helper returns [] safely.
    assert candidates_to_delete(rows) == [r["hf_repo"] for r in rows if r.get("delete_after_bench")]
