#!/usr/bin/env python3
"""Pure-helper tests for ``local_runner.py`` (TD-005 + RAM-cap + cost ledger).

These cover the OOM-safety and result-shaping logic without launching MLX:

  - ``ram_cap_exceeded`` (the kill-switch predicate, AC1)
  - ``RamSampler.breached`` latch (AC1)
  - ``slice_cost_ledger`` (per-slice cost rollup, AC2)
  - ``fitness_check`` (skip vs run, TD-005)
  - ``metric_from_summary`` (pending_judge surfacing + None-coalescing, TD-005)
"""

from __future__ import annotations

import sys
import threading
import time
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import pytest  # noqa: E402

# local_runner imports psutil lazily, so the module — and every pure helper —
# is importable in the CI dev venv where psutil is absent. Only the two live
# RamSampler tests need a real process; they skip when psutil is missing.
from local_runner import (  # noqa: E402
    RamSampler,
    fitness_check,
    metric_from_summary,
    ram_cap_exceeded,
    slice_cost_ledger,
)

try:
    import psutil  # noqa: F401

    _HAVE_PSUTIL = True
except ImportError:
    _HAVE_PSUTIL = False

requires_psutil = pytest.mark.skipif(not _HAVE_PSUTIL, reason="psutil only present in .venv-mlx")

GB = 1_000_000_000  # 1e9 bytes, matching local_runner's convention


# ---------------------------------------------------------------------------
# AC1 — ram_cap_exceeded
# ---------------------------------------------------------------------------


def test_ram_cap_exceeded_none_disables() -> None:
    # No cap configured → never trips, even at absurd RSS.
    assert ram_cap_exceeded(999 * GB, None) is False


def test_ram_cap_exceeded_under_cap() -> None:
    assert ram_cap_exceeded(20 * GB, 22.0) is False


def test_ram_cap_exceeded_at_cap_is_not_exceeded() -> None:
    # Strictly greater-than: exactly at the cap is allowed.
    assert ram_cap_exceeded(22 * GB, 22.0) is False


def test_ram_cap_exceeded_over_cap() -> None:
    assert ram_cap_exceeded(23 * GB, 22.0) is True


# ---------------------------------------------------------------------------
# AC1 — RamSampler breach latch
# ---------------------------------------------------------------------------


@requires_psutil
def test_ram_sampler_breach_flag() -> None:
    """A sampler watching its own process with a 0 GB cap latches `breached`
    and fires on_breach exactly once."""
    fired: list[int] = []
    ev = threading.Event()

    def on_breach(peak: int) -> None:
        fired.append(peak)
        ev.set()

    # Cap of 0.0 GB: the very first sample of any live process exceeds it.
    sampler = RamSampler(
        pid=psutil.Process().pid,
        interval_s=0.01,
        max_ram_gb=0.0,
        on_breach=on_breach,
    )
    sampler.start()
    # Wait for the breach callback (generously bounded so CI noise can't flake).
    assert ev.wait(timeout=5.0), "breach callback never fired"
    sampler.stop()

    assert sampler.breached is True
    assert len(fired) == 1
    assert fired[0] > 0


@requires_psutil
def test_ram_sampler_no_breach_when_cap_none() -> None:
    sampler = RamSampler(pid=psutil.Process().pid, interval_s=0.01, max_ram_gb=None)
    sampler.start()
    time.sleep(0.05)
    sampler.stop()
    assert sampler.breached is False


# ---------------------------------------------------------------------------
# AC2 — slice_cost_ledger
# ---------------------------------------------------------------------------


def test_slice_cost_ledger_sums() -> None:
    rows = [
        {"slice": "dialogue", "cost_usd": 0.0},
        {"slice": "dialogue", "cost_usd": 0.0},
        {"slice": "intent", "cost_usd": 0.0123},
        {"slice": "reaction", "cost_usd": 0.0077},
    ]
    ledger = slice_cost_ledger(rows)
    assert ledger["dialogue"]["runs"] == 2
    assert ledger["dialogue"]["usd"] == 0.0
    assert ledger["intent"]["usd"] == pytest.approx(0.0123)
    assert ledger["reaction"]["usd"] == pytest.approx(0.0077)
    assert ledger["total"]["usd"] == pytest.approx(0.02)
    assert ledger["total"]["runs"] == 4


def test_slice_cost_ledger_surfaces_judge_minutes() -> None:
    rows = [
        {"slice": "dialogue", "cost_usd": 0.0, "judge_compute_minutes": 1.5},
        {"slice": "dialogue", "cost_usd": 0.0, "judge_compute_minutes": 2.5},
        {"slice": "gaeilge", "cost_usd": 0.0, "judge_compute_minutes": 0.75},
    ]
    ledger = slice_cost_ledger(rows)
    assert ledger["dialogue"]["judge_minutes"] == pytest.approx(4.0)
    assert ledger["gaeilge"]["judge_minutes"] == pytest.approx(0.75)
    assert ledger["total"]["judge_minutes"] == pytest.approx(4.75)


def test_slice_cost_ledger_handles_none_and_missing() -> None:
    rows: list[dict] = [
        {"slice": "dialogue", "cost_usd": None},
        {"slice": "dialogue"},  # cost_usd absent
    ]
    ledger = slice_cost_ledger(rows)
    assert ledger["dialogue"]["usd"] == 0.0
    assert ledger["dialogue"]["runs"] == 2


def test_slice_cost_ledger_empty() -> None:
    ledger = slice_cost_ledger([])
    assert ledger == {"total": {"usd": 0.0, "runs": 0, "judge_minutes": 0.0}}


# ---------------------------------------------------------------------------
# TD-005 — fitness_check
# ---------------------------------------------------------------------------


def test_fitness_check_passes_when_within_budget() -> None:
    cand = {"peak_ram_gb_est": 10.0}
    # 48 GB available, 26 GB headroom → 22 GB usable; 10 GB fits.
    assert fitness_check(cand, available_gb=48.0, headroom_gb=26.0) is None


def test_fitness_check_skips_when_over_budget() -> None:
    cand = {"peak_ram_gb_est": 30.0}
    msg = fitness_check(cand, available_gb=48.0, headroom_gb=26.0)
    assert msg is not None
    assert "30.0 GB" in msg


def test_fitness_check_missing_estimate_treated_as_zero() -> None:
    # No estimate → 0.0, always fits (the schema validator catches the missing
    # field separately; fitness_check must not crash on it).
    assert fitness_check({}, available_gb=48.0, headroom_gb=26.0) is None


# ---------------------------------------------------------------------------
# TD-005 — metric_from_summary
# ---------------------------------------------------------------------------


def test_metric_from_summary_intent() -> None:
    out = metric_from_summary({"slice": "intent", "label_match_rate": 0.875})
    assert out == "label_match=0.875"


def test_metric_from_summary_dialogue_coalesces_none() -> None:
    # None score fields must not TypeError the format string.
    out = metric_from_summary(
        {
            "slice": "dialogue",
            "overall": None,
            "character": None,
            "authenticity": None,
            "language": None,
            "responsiveness": None,
            "craft": None,
        }
    )
    assert out.startswith("overall=0.00")


def test_metric_from_summary_reaction_pending_judge() -> None:
    out = metric_from_summary({"slice": "reaction", "mean_score": None, "pending_judge": True})
    assert "pending_judge" in out


def test_metric_from_summary_gaeilge_scored() -> None:
    out = metric_from_summary({"slice": "gaeilge", "overall": 4.02})
    assert out == "gaeilge_overall=4.02"


def test_metric_from_summary_unknown_slice() -> None:
    assert metric_from_summary({"slice": "mystery"}) == "n/a"
