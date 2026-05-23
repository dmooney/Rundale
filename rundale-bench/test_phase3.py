#!/usr/bin/env python3
"""Hermetic tests for Phase 3: perf-by-provider. Run: python3 rundale-bench/test_phase3.py"""
from __future__ import annotations

import contextlib
import json
import sys
import tempfile
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import perf as perf_mod  # noqa: E402
import rundale_bench as rb  # noqa: E402
from catalog import load_catalog  # noqa: E402


@contextlib.contextmanager
def perf_sandbox():
    tmp = Path(tempfile.mkdtemp(prefix="bench-phase3-"))
    saved = perf_mod.PERF_DIR
    perf_mod.PERF_DIR = tmp / "perf"
    try:
        yield tmp
    finally:
        perf_mod.PERF_DIR = saved


def _assert_raises(exc, fn):
    try:
        fn()
    except exc:
        return
    raise AssertionError(f"expected {exc.__name__}")


# ── criterion 1 + 2: aggregation + percentiles ──────────────────────────────
def test_summarize_and_percentiles():
    samples = [
        {"latency_ms": 100, "tokens_in": 50, "tokens_out": 50, "error": None},
        {"latency_ms": 200, "tokens_in": 50, "tokens_out": 100, "error": None},
        {"latency_ms": 300, "tokens_in": 50, "tokens_out": 150, "error": None},
        {"latency_ms": 400, "tokens_in": 50, "tokens_out": 200, "error": None},
        {"latency_ms": 500, "tokens_in": 50, "tokens_out": 250, "error": None},
    ]
    r = perf_mod.summarize_perf(samples, 1.0, 2.0)
    assert r["n_ok"] == 5 and r["n_error"] == 0 and r["error_rate"] == 0.0
    assert r["latency_p50_ms"] == 300  # nearest-rank
    assert r["latency_p95_ms"] == 500
    assert r["tokens_per_sec_mean"] > 0


def test_percentiles_nearest_rank():
    assert perf_mod._nearest_rank([100, 200, 300, 400, 500], 0.50) == 300
    assert perf_mod._nearest_rank([100, 200, 300, 400, 500], 0.95) == 500
    assert perf_mod._nearest_rank([], 0.5) == 0.0


# ── criterion 5: observed $/Mtok from real usage ─────────────────────────────
def test_usd_from_usage():
    # 1M in @ $1, 1M out @ $2 over one call -> total cost $3 over 2M tokens = $1.5/Mtok
    samples = [{"latency_ms": 100, "tokens_in": 1_000_000, "tokens_out": 1_000_000, "error": None}]
    assert perf_mod.summarize_perf(samples, 1.0, 2.0)["usd_per_mtok_observed"] == 1.5
    # local ($0) provider
    assert perf_mod.summarize_perf(samples, 0.0, 0.0)["usd_per_mtok_observed"] == 0.0


# ── criterion 7: errors tolerated + surfaced ─────────────────────────────────
def test_error_handling():
    samples = [
        {"latency_ms": 100, "tokens_in": 10, "tokens_out": 10, "error": None},
        {"latency_ms": None, "tokens_in": 0, "tokens_out": 0, "error": "boom"},
        {"latency_ms": 300, "tokens_in": 10, "tokens_out": 10, "error": None},
    ]
    r = perf_mod.summarize_perf(samples, 1.0, 1.0)
    assert r["n_ok"] == 2 and r["n_error"] == 1
    assert abs(r["error_rate"] - 1 / 3) < 1e-9
    assert r["latency_p50_ms"] in (100, 300)  # computed over OK calls only


# ── criterion 6: warm-up excluded from stats ─────────────────────────────────
def _stub_call(latency_seq=None):
    calls = {"n": 0}

    def call(target, system, prompt, **k):
        calls["n"] += 1
        return ("reply", {"prompt_tokens": 20, "completion_tokens": 40})
    call.calls = calls
    return call


def test_warmup_excluded():
    cat = load_catalog()
    model = cat.by_id("llama-3.3-70b")
    provider = model.cheapest_provider()
    call = _stub_call()
    row = perf_mod.measure_provider(model, provider, ["p1", "p2"], "warm",
                                    warmup=2, measure=5, call=call)
    assert row["n_ok"] + row["n_error"] == 5      # warm-ups not counted
    assert call.calls["n"] == 7                    # 2 warmup + 5 measure actually issued


# ── criterion 3 + 8: sweep providers, schema, files written ──────────────────
def test_perf_subcommand_sweeps_providers():
    with perf_sandbox() as tmp:
        # stub the shared call_chat that measure_provider lazy-imports
        import eval_lib
        saved = eval_lib.call_chat
        eval_lib.call_chat = lambda target, system, prompt, **k: ("reply", {"prompt_tokens": 20, "completion_tokens": 40})
        try:
            rb.cmd_perf(["--model", "llama-3.3-70b", "--providers", "groq,together,openrouter",
                         "--warmup", "1", "--measure", "3"])
        finally:
            eval_lib.call_chat = saved
        files = sorted((tmp / "perf").glob("perf_*.json"))
        assert len(files) == 3
        keys = {"model_id", "provider_id", "model_name_at_provider", "n_ok", "n_error",
                "error_rate", "latency_p50_ms", "latency_p95_ms", "tokens_per_sec_mean",
                "usd_per_mtok_observed", "measured_utc"}
        for f in files:
            row = json.loads(f.read_text())
            assert keys <= set(row), f"missing keys in {f.name}: {keys - set(row)}"
            assert row["n_ok"] + row["n_error"] == 3


# ── criterion 4: --providers all / unknown ───────────────────────────────────
def test_providers_all_and_unknown():
    with perf_sandbox():
        import eval_lib
        saved = eval_lib.call_chat
        eval_lib.call_chat = lambda target, system, prompt, **k: ("r", {"prompt_tokens": 1, "completion_tokens": 1})
        try:
            cat = load_catalog()
            n_providers = len(cat.by_id("deepseek-v3").providers)
            rb.cmd_perf(["--model", "deepseek-v3", "--providers", "all", "--warmup", "0", "--measure", "1"])
            files = list(perf_mod.PERF_DIR.glob("perf_deepseek-v3_*.json"))
            assert len(files) == n_providers
            _assert_raises(SystemExit, lambda: rb.cmd_perf(
                ["--model", "deepseek-v3", "--providers", "bogus", "--measure", "1"]))
        finally:
            eval_lib.call_chat = saved


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
