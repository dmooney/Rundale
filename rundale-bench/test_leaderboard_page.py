#!/usr/bin/env python3
"""Tests for build_leaderboard_page.py — pending-judge marker and bench-bug rate column.

Run: python3 rundale-bench/test_leaderboard_page.py
or via pytest from the repo root.
"""

from __future__ import annotations

import json
import sys
import tempfile
from pathlib import Path
from unittest import mock

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import build_leaderboard_page as blp  # noqa: E402


def _write(d: Path, name: str, obj: dict) -> None:
    d.mkdir(parents=True, exist_ok=True)
    (d / name).write_text(json.dumps(obj), encoding="utf-8")


def _quality_run(
    model: str,
    overall: float | None,
    *,
    pending_judge: bool = False,
    bench_bugs: int = 0,
    records: int = 10,
    judged: int = 10,
    ts: str = "2026-06-01T00:00:00Z",
) -> dict:
    """Minimal run file matching the shape build_leaderboard_page.build_data expects."""
    summary: dict = {
        "overall": overall,
        "judged": judged,
        "records": records,
        "bench_bugs": bench_bugs,
        "character": overall,
        "authenticity": overall,
        "language": overall,
        "responsiveness": overall,
        "craft": overall,
        "judge_model": "claude-sonnet-4-6",
        "judge": "judge_sonnet_v1",
    }
    if pending_judge:
        summary["pending_judge"] = True
        summary["overall"] = None
        for ax in ("character", "authenticity", "language", "responsiveness", "craft"):
            summary[ax] = None
    return {
        "target": {"model": model, "base_url": "https://example.com"},
        "split": "dev",
        "run_started_utc": ts,
        "slices": {"dialogue": {"summary": summary}},
    }


def _patch_artifacts(tmp: Path) -> tuple:
    """Return context-manager patches to redirect build_leaderboard_page constants."""
    return (mock.patch.object(blp, "_ARTIFACTS_DIR", tmp / "artifacts"),)


def test_pending_judge_row_appears_in_quality():
    """A pending-judge run must appear in quality data with pending_judge=True."""
    with tempfile.TemporaryDirectory() as d:
        art = Path(d) / "artifacts"
        _write(art, "run_a_all_1.json", _quality_run("model-a", None, pending_judge=True))
        with _patch_artifacts(Path(d))[0]:
            data = blp.build_data()
    assert len(data["quality"]) == 1
    assert data["quality"][0]["pending_judge"] is True
    assert data["quality"][0]["total"] is None


def test_pending_judge_row_excluded_from_quality_when_no_score_and_not_pending():
    """A run with overall=None and no pending_judge flag is still excluded."""
    with tempfile.TemporaryDirectory() as d:
        art = Path(d) / "artifacts"
        _write(art, "run_b_all_1.json", _quality_run("model-b", None, pending_judge=False))
        with _patch_artifacts(Path(d))[0]:
            data = blp.build_data()
    assert data["quality"] == []


def test_pending_judge_marker_in_markdown():
    """A pending-judge row renders '(pending judge)' in the Overall column of leaderboard.md."""
    with tempfile.TemporaryDirectory() as d:
        art = Path(d) / "artifacts"
        _write(art, "run_p_all_1.json", _quality_run("pending-model", None, pending_judge=True))
        with _patch_artifacts(Path(d))[0]:
            data = blp.build_data()
        md = blp.build_markdown(data)
    assert "(pending judge)" in md
    assert "pending-model" in md


def test_scored_row_does_not_show_pending_marker():
    """A fully-judged row shows a numeric Overall and no pending marker."""
    with tempfile.TemporaryDirectory() as d:
        art = Path(d) / "artifacts"
        _write(art, "run_s_all_1.json", _quality_run("scored-model", 3.75))
        with _patch_artifacts(Path(d))[0]:
            data = blp.build_data()
        md = blp.build_markdown(data)
    assert "(pending judge)" not in md
    assert "3.75" in md


def test_bench_bug_rate_column_in_markdown():
    """Bench-bug % column appears in the quality table with correct fraction."""
    with tempfile.TemporaryDirectory() as d:
        art = Path(d) / "artifacts"
        _write(art, "run_bb_all_1.json", _quality_run("buggy-model", 2.5, bench_bugs=3, records=10))
        with _patch_artifacts(Path(d))[0]:
            data = blp.build_data()
        md = blp.build_markdown(data)
    assert "Bench-bug %" in md
    assert "3/10" in md
    assert "30%" in md


def test_bench_bug_rate_zero_bugs():
    """A model with no bench bugs shows '0/N (0%)' not a blank."""
    with tempfile.TemporaryDirectory() as d:
        art = Path(d) / "artifacts"
        _write(art, "run_nb_all_1.json", _quality_run("clean-model", 4.0, bench_bugs=0, records=10))
        with _patch_artifacts(Path(d))[0]:
            data = blp.build_data()
        md = blp.build_markdown(data)
    assert "0/10 (0%)" in md


def test_bench_bug_rate_zero_records():
    """When records is 0, the cell shows '-' not an error."""
    with tempfile.TemporaryDirectory() as d:
        art = Path(d) / "artifacts"
        _write(art, "run_nr_all_1.json", _quality_run("empty-model", 3.0, bench_bugs=0, records=0))
        with _patch_artifacts(Path(d))[0]:
            data = blp.build_data()
        md = blp.build_markdown(data)
    assert "Bench-bug %" in md
    # The '-' placeholder appears for the zero-records row.
    assert " - |" in md or "| - " in md


def test_quality_row_carries_bench_bugs_and_records():
    """build_data() stores bench_bugs and records on each quality entry."""
    with tempfile.TemporaryDirectory() as d:
        art = Path(d) / "artifacts"
        _write(art, "run_r_all_1.json", _quality_run("model-r", 3.2, bench_bugs=2, records=8))
        with _patch_artifacts(Path(d))[0]:
            data = blp.build_data()
    row = data["quality"][0]
    assert row["bench_bugs"] == 2
    assert row["records"] == 8


def main() -> int:
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_") and callable(v)]
    failed = 0
    for t in tests:
        try:
            t()
        except AssertionError as e:
            print(f"FAIL {t.__name__}: {e}")
            failed += 1
        except Exception as e:
            print(f"ERROR {t.__name__}: {type(e).__name__}: {e}")
            failed += 1
        else:
            print(f"OK   {t.__name__}")
    print(f"\n{len(tests) - failed}/{len(tests)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
