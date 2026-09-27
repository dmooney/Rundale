"""Post-process promptfoo result JSONs → rundale-bench v2 report.

Reads `promptfoo/output/<slice>.json` files (written by `promptfoo eval -o`),
aggregates per-slice quality (per-axis means, excluding bench_bug /
judge_failure rows — matching v1's `_dialogue_aggregate`), the perf rollup
(p50/p95 latency, tokens/sec, error_rate, observed $/Mtok — ported from
perf.py::summarize_perf), and the cost/game-time projection (provider price ×
tokens-per-game-minute → USD/min·hr — ported from build_site_data.py).

Writes `output/report.md` + `output/report.json`.

    python3 promptfoo/scripts/report.py [output_dir]
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path
from typing import TypedDict

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402


class PerfCacheBucket(TypedDict):
    latencies: list[float]
    ttfts: list[float]
    ok: int
    errors: int


AXES = {
    "dialogue": [
        "character",
        "authenticity",
        "language",
        "responsiveness",
        "craft",
        "brevity",
        "repetition",
        "mood_fidelity",
        "grounding",
    ],
    "reaction": ["in_character"],
    "tier2-sim": ["plausibility"],
    "tier3-sim": ["plausibility"],
    "gaeilge": ["fluency", "grammar", "idiom", "task_fulfillment", "english_leakage"],
    "multiturn": [
        "continuity",
        "name_fidelity",
        "no_premature_farewell",
        "persona_consistency",
        "memory_retention",
        "freshness",
    ],
}


def _nearest_rank(sorted_vals: list[float], pct: float) -> float:
    if not sorted_vals:
        return 0.0
    rank = max(1, math.ceil(pct * len(sorted_vals)))
    return sorted_vals[rank - 1]


def _results(data: dict) -> list[dict]:
    # promptfoo's `eval -o` export nests the per-test rows under
    # results.results; some versions/exporters use results.outputs. Accept both.
    r = data.get("results", data)
    if isinstance(r, dict):
        for key in ("results", "outputs"):
            if isinstance(r.get(key), list):
                return r[key]
    return r if isinstance(r, list) else []


def _named(res: dict) -> dict:
    return res.get("namedScores") or res.get("response", {}).get("namedScores") or {}


def _meta(res: dict) -> dict:
    return (res.get("response") or {}).get("metadata") or res.get("metadata") or {}


def _candidate(res: dict) -> str:
    return _meta(res).get("target") or (res.get("provider") or {}).get("label") or "candidate"


def aggregate_quality(slice_name: str, results: list[dict]) -> dict:
    axes = AXES[slice_name]
    sums = {a: 0.0 for a in axes}
    overall_sum = 0.0
    judged = bench_bugs = judge_failures = errors = non_latin = 0
    schema_valid = schema_total = 0
    is_sim = slice_name in ("tier2-sim", "tier3-sim")
    for res in results:
        named = _named(res)
        meta = _meta(res)
        if meta.get("error"):
            errors += 1
            # A failed candidate call is a schema failure too — keep it in the
            # rate denominator so a run of mostly-errors can't report 1.00
            # schema_valid_rate (matches v1's len(records) denominator).
            if is_sim:
                schema_total += 1
            continue
        if is_sim:
            schema_total += 1
            if named.get("schema_valid"):
                schema_valid += 1
        if named.get("judge_failure"):
            judge_failures += 1
            continue
        if named.get("bench_bug"):
            bench_bugs += 1
            continue
        if not any(a in named for a in axes):
            judge_failures += 1
            continue
        judged += 1
        for a in axes:
            sums[a] += float(named.get(a, 0))
        overall_sum += float(named.get("overall", 0))
        if named.get("non_latin"):
            non_latin += 1
    n = max(1, judged)
    out = {
        "slice": slice_name,
        "records": len(results),
        "judged": judged,
        "bench_bugs": bench_bugs,
        "judge_failures": judge_failures,
        "errors": errors,
        "non_latin": non_latin,
        "overall": (overall_sum / n) if judged else None,
        **{a: (sums[a] / n if judged else None) for a in axes},
    }
    if slice_name in ("tier2-sim", "tier3-sim"):
        out["schema_valid_rate"] = schema_valid / max(1, schema_total)
    return out


def aggregate_intent(results: list[dict]) -> dict:
    matches = errors = 0
    score_sum = 0.0
    n = 0
    for res in results:
        meta = _meta(res)
        # Errored rows stay in the denominator scoring 0 (v1 run_intent records
        # exceptions as label_match=0 / score=0 and divides by len(records)),
        # so transport failures can't inflate the deterministic rates.
        n += 1
        if meta.get("error"):
            errors += 1
            continue
        named = _named(res)
        matches += int(round(float(named.get("label_match", 0))))
        score_sum += float(named.get("intent_score", 0))
    return {
        "slice": "intent",
        "records": len(results),
        "errors": errors,
        "label_match_rate": matches / max(1, n),
        "mean_score": score_sum / max(1, n),
    }


def _is_warmup(res: dict) -> bool:
    return bool((res.get("vars") or {}).get("perf_warmup"))


def aggregate_perf(results: list[dict]) -> dict:
    latencies, ttfts, tps = [], [], []
    by_cache_state: dict[str, PerfCacheBucket] = {
        "cold": {"latencies": [], "ttfts": [], "ok": 0, "errors": 0},
        "warm": {"latencies": [], "ttfts": [], "ok": 0, "errors": 0},
    }
    total_in = total_out = errors = ok = 0
    observed_cost = 0.0
    observed_cost_seen = False
    model = None
    for res in results:
        if _is_warmup(res):  # discard cold-start warmup measurements
            continue
        meta = _meta(res)
        cache_state = meta.get("perf_cache_state") or (res.get("vars") or {}).get(
            "perf_cache_state"
        )
        cache_bucket = by_cache_state.get(str(cache_state)) if cache_state is not None else None
        response = res.get("response") or {}
        output = response.get("output")
        # A gateway may deliver the entire completion in the first content
        # chunk. With millisecond timing that makes total_ms == ttft_ms, so
        # throughput is not measurable even though the request and its latency
        # measurement are valid. Keep such rows in latency/error accounting and
        # conservatively omit them only from the throughput distribution.
        measurement_complete = (
            isinstance(output, str) and bool(output.strip()) and meta.get("ttft_ms") is not None
        )
        if meta.get("error") or not measurement_complete:
            errors += 1
            if cache_bucket is not None:
                cache_bucket["errors"] += 1
            continue
        ok += 1
        if cache_bucket is not None:
            cache_bucket["ok"] += 1
        model = model or meta.get("model")
        lat = res.get("latencyMs")
        if lat is not None:
            latencies.append(float(lat))
            if cache_bucket is not None:
                cache_bucket["latencies"].append(float(lat))
        if meta.get("ttft_ms") is not None:
            ttfts.append(float(meta["ttft_ms"]))
            if cache_bucket is not None:
                cache_bucket["ttfts"].append(float(meta["ttft_ms"]))
        if meta.get("tokens_per_second"):
            tps.append(float(meta["tokens_per_second"]))
        usage = response.get("tokenUsage") or {}
        total_in += int(usage.get("prompt", 0) or 0)
        total_out += int(usage.get("completion", 0) or 0)
        if response.get("cost") is not None:
            observed_cost += float(response["cost"])
            observed_cost_seen = True
    latencies.sort()
    ttfts.sort()
    total_tokens = total_in + total_out
    gameplay_cost_priced = (model or "") in rb.pricing.COSTS
    price_in, price_out = rb.pricing.COSTS.get(model or "", (0.0, 0.0))
    static_cost = total_in / 1e6 * price_in + total_out / 1e6 * price_out
    total_cost = observed_cost if observed_cost_seen else static_cost
    for values in by_cache_state.values():
        values["latencies"].sort()
        values["ttfts"].sort()
    return {
        "slice": "perf",
        "n_ok": ok,
        "n_error": errors,
        "error_rate": errors / max(1, ok + errors),
        "latency_p50_ms": _nearest_rank(latencies, 0.50),
        "latency_p95_ms": _nearest_rank(latencies, 0.95),
        "ttft_p50_ms": _nearest_rank(ttfts, 0.50),
        "ttft_p95_ms": _nearest_rank(ttfts, 0.95),
        "cold_n_ok": by_cache_state["cold"]["ok"],
        "cold_n_error": by_cache_state["cold"]["errors"],
        "cold_latency_p95_ms": _nearest_rank(by_cache_state["cold"]["latencies"], 0.95),
        "cold_ttft_p95_ms": _nearest_rank(by_cache_state["cold"]["ttfts"], 0.95),
        "warm_n_ok": by_cache_state["warm"]["ok"],
        "warm_n_error": by_cache_state["warm"]["errors"],
        "warm_latency_p95_ms": _nearest_rank(by_cache_state["warm"]["latencies"], 0.95),
        "warm_ttft_p95_ms": _nearest_rank(by_cache_state["warm"]["ttfts"], 0.95),
        "tokens_per_sec_p50": _nearest_rank(sorted(tps), 0.50),
        "tokens_per_sec_mean": (sum(tps) / len(tps)) if tps else 0.0,
        "usd_per_mtok_observed": round(
            (total_cost * 1e6 / total_tokens) if total_tokens else 0.0, 4
        ),
        "gameplay_cost_priced": gameplay_cost_priced,
    }


def run_cost_total(results: list[dict]) -> dict:
    cost = 0.0
    tin = tout = 0
    for res in results:
        resp = res.get("response") or {}
        cost += float(resp.get("cost") or 0.0)
        usage = resp.get("tokenUsage") or {}
        tin += int(usage.get("prompt", 0) or 0)
        tout += int(usage.get("completion", 0) or 0)
    return {"usd": cost, "prompt_tokens": tin, "completion_tokens": tout}


def main(argv: list[str]) -> int:
    out_dir = Path(argv[1]) if len(argv) > 1 else rb.PROMPTFOO_DIR / "output"
    files = sorted(p for p in out_dir.glob("*.json") if p.stem not in ("report",))
    if not files:
        print(f"no result JSONs in {out_dir}", file=sys.stderr)
        return 1

    report: dict = {"slices": {}, "candidates": {}}
    for f in files:
        slice_name = f.stem
        data = json.loads(f.read_text(encoding="utf-8"))
        results = _results(data)
        if not results:
            continue
        # group by candidate
        by_cand: dict[str, list[dict]] = {}
        for res in results:
            by_cand.setdefault(_candidate(res), []).append(res)
        for cand, rows in by_cand.items():
            c = report["candidates"].setdefault(cand, {"slices": {}, "run_cost": {"usd": 0.0}})
            if slice_name == "perf":
                agg = aggregate_perf(rows)
                model = _meta(rows[0]).get("model") if rows else None
                price_in, price_out = rb.pricing.COSTS.get(model or "", (0.0, 0.0))
                agg.update(rb.pricing.gameplay_cost(price_in, price_out))
                agg["model"] = model
                agg["price_in_per_mtok"] = price_in
                agg["price_out_per_mtok"] = price_out
            elif slice_name == "intent":
                agg = aggregate_intent(rows)
            elif slice_name in AXES:
                agg = aggregate_quality(slice_name, rows)
            else:
                continue
            c["slices"][slice_name] = agg
            rc = run_cost_total(rows)
            c["run_cost"]["usd"] += rc["usd"]

    _write_markdown(report, out_dir / "report.md")
    (out_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"[report] wrote {out_dir / 'report.md'} and report.json "
        f"({len(report['candidates'])} candidate(s))"
    )
    print((out_dir / "report.md").read_text(encoding="utf-8"))
    return 0


def _fmt(v, nd=2):
    if v is None:
        return "—"
    if isinstance(v, float):
        return f"{v:.{nd}f}"
    return str(v)


def _write_markdown(report: dict, path: Path) -> None:
    lines = ["# rundale-bench v2 report\n"]
    for cand, c in sorted(report["candidates"].items()):
        lines.append(f"## `{cand}`\n")
        # Quality slices
        for slice_name in (
            "dialogue",
            "reaction",
            "tier2-sim",
            "tier3-sim",
            "gaeilge",
            "multiturn",
        ):
            s = c["slices"].get(slice_name)
            if not s:
                continue
            axes = AXES[slice_name]
            axis_str = " · ".join(f"{a}={_fmt(s.get(a))}" for a in axes)
            extra = ""
            if "schema_valid_rate" in s:
                extra = f" · schema_valid={_fmt(s['schema_valid_rate'])}"
            lines.append(
                f"- **{slice_name}**: overall={_fmt(s.get('overall'))} ({axis_str}){extra} "
                f"— judged {s['judged']}/{s['records']}, bench_bugs={s['bench_bugs']}, "
                f"judge_failures={s['judge_failures']}, errors={s['errors']}"
            )
        # Intent
        si = c["slices"].get("intent")
        if si:
            lines.append(
                f"- **intent**: label_match_rate={_fmt(si['label_match_rate'])} "
                f"mean_score={_fmt(si['mean_score'])} (errors={si['errors']}, n={si['records']})"
            )
        # Perf + cost/game-time
        sp = c["slices"].get("perf")
        if sp:
            lines.append(
                f"- **perf**: p50={_fmt(sp['latency_p50_ms'], 0)}ms p95={_fmt(sp['latency_p95_ms'], 0)}ms "
                f"ttft_p50={_fmt(sp['ttft_p50_ms'], 0)}ms "
                f"ttft_p95={_fmt(sp['ttft_p95_ms'], 0)}ms "
                f"tok/s_p50={_fmt(sp['tokens_per_sec_p50'], 1)} "
                f"err={_fmt(sp['error_rate'])} $/Mtok={_fmt(sp['usd_per_mtok_observed'], 4)}"
            )
            if sp.get("gameplay_cost_priced"):
                lines.append(
                    f"- **cost/game-time** (model `{sp.get('model')}`, "
                    f"${_fmt(sp.get('price_in_per_mtok'), 2)}/{_fmt(sp.get('price_out_per_mtok'), 2)} per Mtok): "
                    f"**${_fmt(sp.get('gameplay_cost_usd_per_minute'), 5)}/min** · "
                    f"${_fmt(sp.get('gameplay_cost_usd_per_hour'), 3)}/hr"
                )
            else:
                lines.append(
                    f"- **cost/game-time**: unavailable for unpriced routed model "
                    f"`{sp.get('model')}`; use observed run spend"
                )
        lines.append(f"- run spend: ${_fmt(c['run_cost']['usd'], 4)}\n")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
