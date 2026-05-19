#!/usr/bin/env python3
"""Regenerate the static leaderboard HTML page.

Walks every `multiaxis_*.json`, `perf_*.json`, `run_*_gaeilge_*.json`, and
`dialogue_samples_*.json` under `docs/proofs/rundale-bench/`, aggregates
them into a single payload, and inlines the result into `leaderboard.html`
(replacing the
`__DATA_PLACEHOLDER__` marker that the template ships with).

The same generated HTML is mirrored into `leaderboard.md`, so the Markdown
artifact does not become a separate hand-maintained leaderboard.

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
_MARKDOWN_MIRROR = _BENCH / "leaderboard.md"
_MARKER = "__DATA_PLACEHOLDER__"


def _round(v: float | None, n: int = 2) -> float | None:
    return None if v is None else round(v, n)


def build_data() -> dict:
    out: dict = {"quality": [], "perf": [], "gaeilge": [], "coverage": {}, "unjudged": []}

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

    # Keep only the latest perf measurement per candidate (perf files
    # are timestamped; later runs supersede earlier smoke probes).
    latest_perf: dict[str, dict] = {}
    for f in sorted(glob.glob(str(_BENCH / "perf_*.json"))):
        d = json.loads(Path(f).read_text(encoding="utf-8"))
        for cand, s in (d.get("per_target") or {}).items():
            if ":free" in cand:
                continue
            latest_perf[cand] = {
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
            }
    out["perf"] = list(latest_perf.values())

    # Keep only the latest Gaeilge measurement per candidate/base/split. Bench
    # run files are timestamped; sorted iteration means later runs supersede
    # earlier smoke probes for the same target.
    latest_gaeilge: dict[tuple[str, str, str], dict] = {}
    for f in sorted(glob.glob(str(_BENCH / "run_*_gaeilge_*.json"))):
        d = json.loads(Path(f).read_text(encoding="utf-8"))
        target = d.get("target") or {}
        summary = ((d.get("slices") or {}).get("gaeilge") or {}).get("summary") or {}
        if not target or not summary:
            continue
        candidate = target.get("model", "?")
        base_url = target.get("base_url", "?")
        split = d.get("split", "?")
        latest_gaeilge[(candidate, base_url, split)] = {
            "candidate": candidate,
            "base_url": base_url,
            "split": split,
            "file": Path(f).name,
            "n": summary.get("records", 0),
            "errors": summary.get("errors", 0),
            "overall": _round(summary.get("overall_mean")),
            "fluency": _round(summary.get("fluency_mean")),
            "grammar": _round(summary.get("grammar_mean")),
            "idiom": _round(summary.get("idiom_mean")),
            "task_fulfillment": _round(summary.get("task_fulfillment_mean")),
            "english_leakage": _round(summary.get("english_leakage_mean")),
            "english_leakage_flag_rate": _round(summary.get("english_leakage_flag_rate"), 3),
            "usd": _round((d.get("cost") or {}).get("usd"), 4),
        }
    out["gaeilge"] = list(latest_gaeilge.values())

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
    _MARKDOWN_MIRROR.write_text(html, encoding="utf-8")
    print(f"wrote {_TEMPLATE.relative_to(_REPO_ROOT)} "
          f"+ {_MARKDOWN_MIRROR.relative_to(_REPO_ROOT)} "
          f"(quality={len(data['quality'])} perf={len(data['perf'])} "
          f"gaeilge={len(data['gaeilge'])} "
          f"cached={len(data['coverage'])} unjudged={len(data['unjudged'])})")


if __name__ == "__main__":
    main()
