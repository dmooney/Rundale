#!/usr/bin/env python3
"""Per-provider performance measurement for rundale-bench.

Quality is judged once per model on the cheapest provider (Phase 1). Perf is
orthogonal: the same model on Groq vs Together vs OpenRouter has different
latency, throughput, price, and reliability. This module measures each
(model, provider) pair independently and writes one perf JSON per pair; the
leaderboard renders the matrix (Phase 4).

`summarize_perf` is a pure function (no I/O) so the statistics are unit-tested
without a network. The measurement loop calls the shared `call_chat`, so it
inherits the same retry/timeout behavior the engine uses.
"""
from __future__ import annotations

import json
import math
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Optional

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent
PERF_DIR = _REPO_ROOT / "docs" / "proofs" / "rundale-bench" / "perf"


def _nearest_rank(sorted_vals: list[float], pct: float) -> float:
    """Nearest-rank percentile. pct in [0,1]. Empty -> 0.0."""
    if not sorted_vals:
        return 0.0
    rank = max(1, math.ceil(pct * len(sorted_vals)))
    return sorted_vals[rank - 1]


def summarize_perf(samples: list[dict], price_in_per_mtok: float, price_out_per_mtok: float) -> dict:
    """Aggregate per-call measurements into the perf row.

    `samples` items: {latency_ms, tokens_in, tokens_out, error}. Errored calls
    are excluded from latency/throughput but counted in error_rate. Observed
    $/Mtok is blended from real token usage × the provider's catalog price.
    """
    ok = [s for s in samples if not s.get("error")]
    n_ok = len(ok)
    n_error = len(samples) - n_ok
    total = len(samples) or 1

    latencies = sorted(float(s["latency_ms"]) for s in ok if s.get("latency_ms") is not None)
    tok_per_sec = [
        (s["tokens_out"] / (s["latency_ms"] / 1000.0))
        for s in ok
        if s.get("tokens_out") and s.get("latency_ms")
    ]

    total_in = sum(s.get("tokens_in", 0) or 0 for s in ok)
    total_out = sum(s.get("tokens_out", 0) or 0 for s in ok)
    total_tokens = total_in + total_out
    total_cost = total_in / 1_000_000 * price_in_per_mtok + total_out / 1_000_000 * price_out_per_mtok
    usd_per_mtok = (total_cost * 1_000_000 / total_tokens) if total_tokens else 0.0

    return {
        "n_ok": n_ok,
        "n_error": n_error,
        "error_rate": n_error / total,
        "latency_p50_ms": _nearest_rank(latencies, 0.50),
        "latency_p95_ms": _nearest_rank(latencies, 0.95),
        "tokens_per_sec_mean": (sum(tok_per_sec) / len(tok_per_sec)) if tok_per_sec else 0.0,
        "usd_per_mtok_observed": round(usd_per_mtok, 4),
    }


def measure_provider(
    model,
    provider,
    prompts: list[str],
    warmup_prompt: str,
    *,
    warmup: int = 5,
    measure: int = 20,
    system: Optional[str] = None,
    max_tokens: int = 200,
    call: Callable = None,
) -> dict:
    """Run warm-up (discarded) + measurement calls against one provider.

    `call` defaults to eval_lib.call_chat; injectable for tests. Returns the
    perf row dict (not yet written to disk).
    """
    if call is None:
        from eval_lib import call_chat as call  # lazy: keeps import light for tests
    from eval_lib import Target

    target = Target(model=provider.model_name_at_provider,
                    base_url=provider.base_url,
                    api_key_env=provider.api_key_env)

    # Warm-up: prime connection / cold-start; results discarded.
    for _ in range(warmup):
        try:
            call(target, system, warmup_prompt, max_tokens=max_tokens)
        except Exception:
            pass

    samples: list[dict] = []
    for i in range(measure):
        prompt = prompts[i % len(prompts)]
        t0 = time.perf_counter()
        try:
            _text, usage = call(target, system, prompt, max_tokens=max_tokens)
            latency_ms = (time.perf_counter() - t0) * 1000.0
            samples.append({
                "latency_ms": latency_ms,
                "tokens_in": usage.get("prompt_tokens", 0),
                "tokens_out": usage.get("completion_tokens", 0),
                "error": None,
            })
        except Exception as e:
            samples.append({"latency_ms": None, "tokens_in": 0, "tokens_out": 0, "error": str(e)})

    row = summarize_perf(samples, provider.price_in_per_mtok, provider.price_out_per_mtok)
    row.update({
        "model_id": model.id,
        "provider_id": provider.provider_id,
        "model_name_at_provider": provider.model_name_at_provider,
        "measured_utc": datetime.now(timezone.utc).isoformat(),
    })
    return row


def write_perf(row: dict) -> Path:
    PERF_DIR.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    path = PERF_DIR / f"perf_{row['model_id']}_{row['provider_id']}_{stamp}.json"
    path.write_text(json.dumps(row, indent=2) + "\n", encoding="utf-8")
    return path
