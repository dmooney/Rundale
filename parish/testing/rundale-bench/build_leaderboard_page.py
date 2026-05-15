#!/usr/bin/env python3
"""Regenerate the static leaderboard HTML page.

Walks every `multiaxis_*.json`, `perf_*.json`, and `dialogue_samples_*.json`
under `docs/proofs/rundale-bench/`, aggregates them into a single payload,
and inlines the result into `leaderboard.html` (replacing the
`__DATA_PLACEHOLDER__` marker that the template ships with).

Run after any new judging / perf / cache run::

    python3 parish/testing/rundale-bench/build_leaderboard_page.py

The page is fully static — no server required. Open it with::

    open docs/proofs/rundale-bench/leaderboard.html
"""
from __future__ import annotations

import glob
import json
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[3]
_BENCH = _REPO_ROOT / "docs" / "proofs" / "rundale-bench"
_TEMPLATE = _BENCH / "leaderboard.html"
_MARKER = "__DATA_PLACEHOLDER__"


def _round(v: float | None, n: int = 2) -> float | None:
    return None if v is None else round(v, n)


def build_data() -> dict:
    out: dict = {"quality": [], "perf": [], "coverage": {}, "unjudged": []}

    for f in sorted(glob.glob(str(_BENCH / "multiaxis_*.json"))):
        d = json.loads(Path(f).read_text(encoding="utf-8"))
        judge = (d.get("judge") or {}).get("model", "?")
        for cand, ag in (d.get("aggregates") or {}).items():
            if ":free" in cand:
                continue
            out["quality"].append({
                "candidate": cand,
                "judge": judge,
                "file": Path(f).name,
                "n": ag.get("total_n", 0),
                "total": _round(ag.get("total_mean", 0.0)),
                "character": _round(ag.get("character_mean", 0.0)),
                "authenticity": _round(ag.get("authenticity_mean", 0.0)),
                "language": _round(ag.get("language_mean", 0.0)),
                "responsiveness": _round(ag.get("responsiveness_mean", 0.0)),
                "craft": _round(ag.get("craft_mean", 0.0)),
            })

    for f in sorted(glob.glob(str(_BENCH / "perf_*.json"))):
        d = json.loads(Path(f).read_text(encoding="utf-8"))
        for cand, s in (d.get("per_target") or {}).items():
            if ":free" in cand:
                continue
            out["perf"].append({
                "candidate": cand,
                "file": Path(f).name,
                "n_ok": s.get("n_ok", 0),
                "ttft_p50": s.get("ttft_ms_median"),
                "ttft_p90": s.get("ttft_ms_p90"),
                "total_p50": s.get("total_ms_median"),
                "tps_p50": s.get("tokens_per_second_median"),
                "tps_p90": s.get("tokens_per_second_p90"),
                "json_freeform": round((s["json_freeform"]["rate"] or 0) * 100, 1),
                "json_schema": round((s["json_schema"]["rate"] or 0) * 100, 1),
            })

    judged: dict[str, set[str]] = {}
    for q in out["quality"]:
        judged.setdefault(q["candidate"], set()).add(q["judge"])

    cached: set[str] = set()
    for f in sorted(glob.glob(str(_BENCH / "dialogue_samples_*.json"))):
        d = json.loads(Path(f).read_text(encoding="utf-8"))
        for c in d.get("candidates", []):
            if ":free" in c:
                continue
            cached.add(c)

    out["coverage"] = {c: sorted(judged.get(c, set())) for c in sorted(cached)}
    out["unjudged"] = sorted(cached - set(judged.keys()))

    # Synthetic "average" judge: per-candidate mean across distinct judges,
    # weighted equally. Only emit when a candidate has been judged by 2+ judges.
    by_cand: dict[str, list[dict]] = {}
    for q in out["quality"]:
        by_cand.setdefault(q["candidate"], []).append(q)
    averaged: list[dict] = []
    for cand, rows in by_cand.items():
        if len({r["judge"] for r in rows}) < 2:
            continue
        # Mean per axis across rows (one row per judge already).
        def mean(key: str) -> float:
            xs = [r[key] for r in rows if r.get(key) is not None]
            return round(sum(xs) / len(xs), 2) if xs else 0.0
        n_total = sum(r["n"] for r in rows)
        averaged.append({
            "candidate": cand,
            "judge": "average",
            "file": "(synthetic)",
            "n": n_total,
            "total": mean("total"),
            "character": mean("character"),
            "authenticity": mean("authenticity"),
            "language": mean("language"),
            "responsiveness": mean("responsiveness"),
            "craft": mean("craft"),
            "judge_count": len({r["judge"] for r in rows}),
        })
    averaged.sort(key=lambda r: -r["total"])
    out["averaged"] = averaged
    return out


def main() -> None:
    data = build_data()
    if not _TEMPLATE.exists():
        raise SystemExit(f"missing template: {_TEMPLATE}")
    html = _TEMPLATE.read_text(encoding="utf-8")
    payload = json.dumps(data, ensure_ascii=False)
    # First insert: replace placeholder. Subsequent runs: replace whatever is
    # currently between the script tag bounds with the freshly-built payload.
    if _MARKER in html:
        html = html.replace(_MARKER, payload)
    else:
        import re
        html = re.sub(
            r'(<script type="application/json" id="bench-data">)[\s\S]*?(</script>)',
            lambda m: m.group(1) + "\n" + payload + "\n" + m.group(2),
            html,
            count=1,
        )
    _TEMPLATE.write_text(html, encoding="utf-8")
    print(f"wrote {_TEMPLATE.relative_to(_REPO_ROOT)} "
          f"(quality={len(data['quality'])} perf={len(data['perf'])} "
          f"cached={len(data['coverage'])} unjudged={len(data['unjudged'])})")


if __name__ == "__main__":
    main()
