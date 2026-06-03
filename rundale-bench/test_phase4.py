#!/usr/bin/env python3
"""Hermetic tests for Phase 4 data bridge. Run: python3 rundale-bench/test_phase4.py"""

from __future__ import annotations

import json
import sys
import tempfile
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import build_site_data as bsd  # noqa: E402


def _write(d: Path, name: str, obj: dict):
    d.mkdir(parents=True, exist_ok=True)
    (d / name).write_text(json.dumps(obj), encoding="utf-8")


def _tmp() -> Path:
    tmp = Path(tempfile.mkdtemp())
    bsd.PROOFS_RUNS_DIR = tmp / "empty-proofs-mirror"
    return tmp


def _dialogue_run(model_id, overall, ts, tier="screen", axes=4):
    return {
        "tier": tier,
        "run_started_utc": ts,
        "candidate": {"model_id": model_id},
        "slices": {
            "dialogue": {
                "summary": {
                    "overall": overall,
                    "judged": 5,
                    "records": 5,
                    "non_latin_rate": 0.0,
                    "character": axes,
                    "authenticity": axes,
                    "language": axes,
                    "responsiveness": axes,
                    "craft": axes,
                },
                "results": [],
            }
        },
    }


def _profile_summary(seconds=300.0):
    return {
        "observed_seconds": seconds,
        "categories": {
            "intent": {
                "category": "intent",
                "requests": 10,
                "requests_per_minute": 2.0,
                "input_tokens_estimated": 1000,
                "output_tokens_estimated": 100,
            },
            "dialogue": {
                "category": "dialogue",
                "requests": 5,
                "requests_per_minute": 1.0,
                "input_tokens_estimated": 500,
                "output_tokens_estimated": 250,
            },
            "demo-player": {
                "category": "demo-player",
                "requests": 3,
                "requests_per_minute": 0.6,
                "input_tokens_estimated": 900,
                "output_tokens_estimated": 90,
            },
        },
        "total_gameplay": {
            "category": "total_gameplay",
            "requests": 15,
            "requests_per_minute": 3.0,
            "input_tokens_estimated": 1500,
            "output_tokens_estimated": 350,
        },
        "total_observed": {
            "category": "total_observed",
            "requests": 18,
            "requests_per_minute": 3.6,
            "input_tokens_estimated": 2400,
            "output_tokens_estimated": 440,
        },
    }


# ── criterion 1: leaderboard rows, latest wins ───────────────────────────────
def test_leaderboard_latest_wins():
    tmp = _tmp()
    art = tmp / "artifacts"
    _write(
        art, "run_a_dialogue_screen_1.json", _dialogue_run("model-a", 3.9, "2026-05-20T00:00:00Z")
    )
    _write(
        art, "run_a_dialogue_screen_2.json", _dialogue_run("model-a", 4.4, "2026-05-23T00:00:00Z")
    )
    _write(
        art, "run_b_dialogue_screen_1.json", _dialogue_run("model-b", 4.1, "2026-05-21T00:00:00Z")
    )
    rows = bsd.build_leaderboard(art)
    assert len(rows) == 2  # two distinct models
    a = next(r for r in rows if r["model_id"] == "model-a")
    assert a["overall"] == 4.4  # latest wins
    assert a["slug"] == "model-a"
    assert rows[0]["overall"] >= rows[1]["overall"]  # sorted desc


def test_non_dialogue_runs_ignored():
    tmp = _tmp()
    art = tmp / "artifacts"
    _write(art, "run_x_intent_1.json", {"slices": {"intent": {"summary": {"mean_score": 0.9}}}})
    assert bsd.build_leaderboard(art) == []


# ── criterion 2: perf matrix, latest per pair ────────────────────────────────
def test_perf_latest_per_pair():
    tmp = _tmp()
    pdir = tmp / "perf"
    base = {
        "latency_p50_ms": 100,
        "latency_p95_ms": 200,
        "tokens_per_sec_mean": 50.0,
        "usd_per_mtok_observed": 0.5,
        "error_rate": 0.0,
    }
    _write(
        pdir,
        "perf_m_groq_1.json",
        {**base, "model_id": "m", "provider_id": "groq", "measured_utc": "2026-05-20T00:00:00Z"},
    )
    _write(
        pdir,
        "perf_m_groq_2.json",
        {
            **base,
            "model_id": "m",
            "provider_id": "groq",
            "measured_utc": "2026-05-23T00:00:00Z",
            "latency_p50_ms": 80,
        },
    )
    _write(
        pdir,
        "perf_m_together_1.json",
        {
            **base,
            "model_id": "m",
            "provider_id": "together",
            "measured_utc": "2026-05-21T00:00:00Z",
        },
    )
    rows = bsd.build_perf(pdir)
    assert len(rows) == 2  # groq (deduped) + together
    groq = next(r for r in rows if r["provider_id"] == "groq")
    assert groq["latency_p50_ms"] == 80  # newest


def test_normal_play_profile_latest_summary_and_categories():
    tmp = _tmp()
    old = tmp / "20260501T000000Z"
    new = tmp / "20260502T000000Z"
    _write(old, "summary.json", _profile_summary(seconds=60.0))
    _write(new, "summary.json", _profile_summary(seconds=300.0))

    profile = bsd.build_normal_play_profile(tmp)

    assert profile["source"].endswith("20260502T000000Z/summary.json")
    assert profile["scope"] == "gameplay_excluding_demo_player"
    assert profile["total_gameplay"]["input_tokens_per_minute_estimated"] == 300.0
    assert profile["total_gameplay"]["output_tokens_per_minute_estimated"] == 70.0
    assert profile["categories"]["intent"]["input_tokens_per_request_estimated"] == 100.0
    assert profile["categories"]["intent"]["included_in_gameplay_cost"] is True
    assert profile["categories"]["demo-player"]["included_in_gameplay_cost"] is False


def test_attach_gameplay_costs_uses_profile_and_missing_prices_are_nonfatal():
    profile = bsd.build_normal_play_profile(_tmp())
    assert profile is None

    profile_dir = _tmp()
    _write(profile_dir / "20260502T000000Z", "summary.json", _profile_summary(seconds=300.0))
    profile = bsd.build_normal_play_profile(profile_dir)
    rows = [
        {"model_id": "m", "provider_id": "provider", "model_name_at_provider": "provider-model"},
        {"model_id": "missing", "provider_id": "provider", "model_name_at_provider": "missing"},
    ]
    bsd.attach_gameplay_costs(
        rows,
        profile,
        {
            ("provider-model", "provider"): {
                "price_input_usd_per_mtok": 1.0,
                "price_output_usd_per_mtok": 2.0,
                "price_source": "test",
            }
        },
    )

    # 300 input tok/min at $1/M + 70 output tok/min at $2/M.
    assert round(rows[0]["gameplay_cost_usd_per_minute"], 8) == 0.00044
    assert round(rows[0]["gameplay_cost_usd_per_hour"], 6) == 0.0264
    assert rows[0]["gameplay_cost_by_category_usd_per_minute"]["intent"] == 0.00024
    assert rows[1]["gameplay_cost_usd_per_minute"] is None
    assert rows[1]["price_input_usd_per_mtok"] is None


# ── criterion 3: bench.json schema, empty inputs ─────────────────────────────
def test_build_data_schema_populated():
    tmp = _tmp()
    art, pdir = tmp / "artifacts", tmp / "perf"
    _write(
        art, "run_a_dialogue_screen_1.json", _dialogue_run("model-a", 4.4, "2026-05-23T00:00:00Z")
    )
    profile_dir = tmp / "profiles"
    _write(profile_dir / "20260502T000000Z", "summary.json", _profile_summary())
    data = bsd.build_data(art, pdir, profile_dir)
    assert set(data) >= {"generated_utc", "judge_model", "suite", "leaderboard", "perf"}
    assert data["judge_model"] == "claude-sonnet-4-6"
    assert len(data["leaderboard"]) == 1 and data["perf"] == []
    assert data["normal_play_profile"]["total_gameplay"]["requests_per_minute"] == 3.0
    assert isinstance(data["cloud_cost_examples"], list)


def test_build_data_empty_inputs():
    tmp = _tmp()
    data = bsd.build_data(tmp / "none", tmp / "none2", tmp / "missing-profile")
    assert data["leaderboard"] == [] and data["perf"] == []  # no crash
    assert data["normal_play_profile"] is None


def test_slugify():
    assert bsd.slugify("openai/gpt-oss-120b:free") == "openai-gpt-oss-120b-free"
    assert bsd.slugify("Qwen2.5-7B") == "qwen2-5-7b"


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
