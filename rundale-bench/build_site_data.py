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


def slugify(model_id: str) -> str:
    """Route-safe slug for a model id (ids contain '/' and ':')."""
    return re.sub(r"[^A-Za-z0-9]+", "-", model_id).strip("-").lower()


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


def build_perf(perf_dir: Path) -> list[dict]:
    latest: dict[tuple[str, str], tuple[str, dict]] = {}
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
            latest[key] = (ts, row)
    return [row for _, row in sorted(latest.values(), key=lambda kv: (kv[1]["model_id"], kv[1]["provider_id"]))]


def build_data(artifacts_dir: Path = ARTIFACTS_DIR, perf_dir: Path = PERF_DIR,
               *, suite: str = "v1", judge_model: str = "claude-sonnet-4-6") -> dict:
    return {
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "judge_model": judge_model,
        "suite": suite,
        "leaderboard": build_leaderboard(artifacts_dir, _family_lookup(suite)),
        "perf": build_perf(perf_dir),
    }


def main() -> None:
    data = build_data()
    SITE_DATA.parent.mkdir(parents=True, exist_ok=True)
    SITE_DATA.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {SITE_DATA} — {len(data['leaderboard'])} leaderboard row(s), {len(data['perf'])} perf row(s)")


if __name__ == "__main__":
    main()
