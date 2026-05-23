#!/usr/bin/env python3
"""Bridge committed eval artifacts into one JSON the static site consumes.

Walks `rundale-bench/artifacts/run_*.json` (dialogue quality) and
`docs/proofs/rundale-bench/perf/*.json` (per-provider perf) and writes
`bench-site/src/data/bench.json`:

    { generated_utc, judge_model, suite, leaderboard[], perf[] }

Pure aggregation with injectable directories so it tests without a network.
Latest run/measurement wins per model / (model, provider).
"""
from __future__ import annotations

import glob
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent
ARTIFACTS_DIR = _BENCH_DIR / "artifacts"
PERF_DIR = _REPO_ROOT / "docs" / "proofs" / "rundale-bench" / "perf"
SITE_DATA = _REPO_ROOT / "bench-site" / "src" / "data" / "bench.json"

AXES = ("character", "authenticity", "language", "responsiveness", "craft")
GAEILGE_AXES = ("fluency", "grammar", "idiom", "task_fulfillment", "english_leakage")
DATASET_SLICES = ("dialogue", "reaction", "tier2-sim", "tier3-sim", "gaeilge", "intent")


def slugify(model_id: str) -> str:
    """Route-safe slug for a model id (ids contain '/' and ':')."""
    return re.sub(r"[^A-Za-z0-9]+", "-", model_id).strip("-").lower()


def _infer_provider(candidate_id: str) -> str:
    """Best-effort provider tag for legacy perf rows (no provider_id field).

    Legacy runs went through OpenRouter; vendor-prefixed ids (`anthropic/…`,
    `google/…`, etc.) signal OpenRouter routing. Bare local ids fall back to
    `legacy` so the row isn't silently dropped.
    """
    if "/" in candidate_id:
        return "openrouter"
    return "legacy"


def _family_lookup(suite: str) -> dict[str, str]:
    try:
        from catalog import load_catalog
        cat = load_catalog(version=suite)
        out: dict[str, str] = {}
        for m in cat.models:
            out[m.id] = m.family
            for p in m.providers:
                out[p.model_name_at_provider] = m.family  # match legacy target.model ids too
        return out
    except Exception:
        return {}


def _run_ts(out: dict, path: Path) -> str:
    return out.get("run_started_utc") or path.stem


def build_leaderboard(artifacts_dir: Path, families: Optional[dict] = None) -> list[dict]:
    families = families or {}
    latest: dict[str, tuple[str, dict]] = {}  # model_id -> (ts, row)
    for p in sorted(glob.glob(str(artifacts_dir / "run_*.json"))):
        path = Path(p)
        try:
            out = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        dia = out.get("slices", {}).get("dialogue")
        if not dia or "summary" not in dia:
            continue
        s = dia["summary"]
        model_id = (out.get("candidate", {}) or {}).get("model_id") or out.get("target", {}).get("model")
        if not model_id:
            continue
        ts = _run_ts(out, path)
        row = {
            "model_id": model_id,
            "slug": slugify(model_id),
            "family": families.get(model_id, "unknown"),
            "tier": out.get("tier"),
            "overall": s.get("overall"),
            "judged": s.get("judged", s.get("records")),
            "records": s.get("records"),
            "non_latin_rate": s.get("non_latin_rate"),
            **{a: s.get(a) for a in AXES},
            "measured_utc": ts,
        }
        if model_id not in latest or ts > latest[model_id][0]:
            latest[model_id] = (ts, row)
    return [row for _, row in sorted(latest.values(), key=lambda kv: -(kv[1].get("overall") or 0))]


def build_perf(perf_dir: Path, legacy_dir: Optional[Path] = None) -> list[dict]:
    """Per-(model, provider) perf row, latest per pair wins.

    Reads Phase 3 schema from `perf_dir` and the legacy multi-target schema
    from `legacy_dir` (per_target keyed by candidate, ttft + total_ms +
    tokens_per_second). Legacy rows are inferred to provider `openrouter` for
    vendor-prefixed candidates.
    """
    latest: dict[tuple[str, str], tuple[str, dict]] = {}

    # Phase 3 perf JSONs
    for p in sorted(glob.glob(str(perf_dir / "perf_*.json"))):
        try:
            row = json.loads(Path(p).read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        key = (row.get("model_id"), row.get("provider_id"))
        if None in key:
            continue
        ts = row.get("measured_utc", "")
        if key not in latest or ts > latest[key][0]:
            latest[key] = (ts, {**row, "source": "phase3"})

    # Legacy multi-target perf JSONs (one file holds many per_target entries)
    if legacy_dir and legacy_dir.exists():
        for p in sorted(glob.glob(str(legacy_dir / "perf_*.json"))):
            try:
                bundle = json.loads(Path(p).read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                continue
            ts = bundle.get("ran_at_utc", "")
            for cand, stats in (bundle.get("per_target") or {}).items():
                pid = _infer_provider(cand)
                key = (cand, pid)
                n_streamed = stats.get("n_streamed", 0) or 0
                n_ok = stats.get("n_ok", 0) or 0
                row = {
                    "model_id": cand,
                    "provider_id": pid,
                    "model_name_at_provider": cand,
                    "n_ok": n_ok,
                    "n_error": max(0, n_streamed - n_ok),
                    "error_rate": ((n_streamed - n_ok) / n_streamed) if n_streamed else 0.0,
                    "latency_p50_ms": stats.get("total_ms_median"),
                    "latency_p95_ms": stats.get("total_ms_p90"),  # legacy p90 — best available
                    "tokens_per_sec_mean": stats.get("tokens_per_second_median"),
                    "usd_per_mtok_observed": None,  # legacy lacks usage rollup
                    "measured_utc": ts,
                    "ttft_p50_ms": stats.get("ttft_ms_median"),
                    "ttft_p90_ms": stats.get("ttft_ms_p90"),
                    "source": "legacy",
                }
                if key not in latest or ts > latest[key][0]:
                    latest[key] = (ts, row)

    return [row for _, row in sorted(latest.values(), key=lambda kv: (kv[1]["model_id"], kv[1]["provider_id"]))]


def build_gaeilge(artifacts_dir: Path, families: Optional[dict] = None) -> list[dict]:
    """Gaeilge leaderboard: per model, axes + leakage. Latest wins."""
    families = families or {}
    latest: dict[str, tuple[str, dict]] = {}
    for p in sorted(glob.glob(str(artifacts_dir / "run_*.json"))):
        path = Path(p)
        try:
            out = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        ga = out.get("slices", {}).get("gaeilge")
        if not ga or "summary" not in ga:
            continue
        s = ga["summary"]
        model_id = (out.get("candidate", {}) or {}).get("model_id") or out.get("target", {}).get("model")
        if not model_id:
            continue
        ts = _run_ts(out, path)
        row = {
            "model_id": model_id,
            "slug": slugify(model_id),
            "family": families.get(model_id, "unknown"),
            "overall": s.get("overall_mean"),
            "records": s.get("records"),
            "errors": s.get("errors"),
            "english_leakage_flag_rate": s.get("english_leakage_flag_rate"),
            **{a: s.get(f"{a}_mean") for a in GAEILGE_AXES},
            "measured_utc": ts,
        }
        if model_id not in latest or ts > latest[model_id][0]:
            latest[model_id] = (ts, row)
    return [row for _, row in sorted(latest.values(), key=lambda kv: -(kv[1].get("overall") or 0))]


def build_datasets(suite: str = "v1") -> dict:
    """Browseable dev-split datasets — counts + every record.

    Holdout is sealed (model-pick defense); only dev records are exposed. Each
    slice contributes `{count, records: [{id, prompt, ...}]}` keyed by slice
    name. Total size on the dev set is ~120 KB across all slices — small
    enough to ship inside `bench.json` and render statically.
    """
    out: dict[str, dict] = {}
    for slice_name in DATASET_SLICES:
        path = _BENCH_DIR / suite / f"{slice_name}.jsonl"
        if not path.exists():
            continue
        records = []
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                records.append(json.loads(line))
            except json.JSONDecodeError:
                continue
        out[slice_name] = {"count": len(records), "records": records}
    return out


def build_data(artifacts_dir: Path = ARTIFACTS_DIR, perf_dir: Path = PERF_DIR,
               *, suite: str = "v1", judge_model: str = "claude-sonnet-4-6") -> dict:
    families = _family_lookup(suite)
    return {
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "judge_model": judge_model,
        "suite": suite,
        "leaderboard": build_leaderboard(artifacts_dir, families),
        "gaeilge": build_gaeilge(artifacts_dir, families),
        "perf": build_perf(perf_dir, legacy_dir=artifacts_dir),
        "datasets": build_datasets(suite),
    }


def main() -> None:
    data = build_data()
    SITE_DATA.parent.mkdir(parents=True, exist_ok=True)
    SITE_DATA.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {SITE_DATA} — {len(data['leaderboard'])} leaderboard row(s), {len(data['perf'])} perf row(s)")


if __name__ == "__main__":
    main()
