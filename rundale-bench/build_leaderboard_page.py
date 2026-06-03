#!/usr/bin/env python3
"""Regenerate the static leaderboard pages.

Walks every `multiaxis_*.json`, `perf_*.json`, `run_*_gaeilge_*.json`, and
`dialogue_samples_*.json` under `rundale-bench/artifacts/`, aggregates
them into a single payload, inlines the result into `leaderboard.html`
(replacing the `__DATA_PLACEHOLDER__` marker that the template ships with),
and writes a GitHub-renderable `leaderboard.md` snapshot from the same data.

Run after any new judging / perf / cache run::

    python3 rundale-bench/build_leaderboard_page.py

The HTML page is fully static — no server required. Open it with::

    open rundale-bench/artifacts/leaderboard.html
"""

from __future__ import annotations

import glob
import json
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent
_ARTIFACTS_DIR = _BENCH_DIR / "artifacts"
_TEMPLATE = _ARTIFACTS_DIR / "leaderboard.html"
_MARKDOWN_PAGE = _ARTIFACTS_DIR / "leaderboard.md"
_MARKER = "__DATA_PLACEHOLDER__"


def _round(v: float | None, n: int = 2) -> float | None:
    return None if v is None else round(v, n)


def _fmt(v, digits: int = 2) -> str:
    if v is None:
        return "-"
    if isinstance(v, float):
        return f"{v:.{digits}f}"
    return str(v)


def _md_escape(v) -> str:
    return _fmt(v).replace("|", "\\|").replace("\n", " ")


def _markdown_table(headers: list[str], rows: list[list]) -> str:
    if not rows:
        return "_No rows yet._\n"
    lines = [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
    ]
    for row in rows:
        lines.append("| " + " | ".join(_md_escape(cell) for cell in row) + " |")
    return "\n".join(lines) + "\n"


def build_data() -> dict:
    out: dict = {"quality": [], "perf": [], "gaeilge": [], "coverage": {}, "unjudged": []}

    for f in sorted(glob.glob(str(_ARTIFACTS_DIR / "multiaxis_*.json"))):
        d = json.loads(Path(f).read_text(encoding="utf-8"))
        judge = (d.get("judge") or {}).get("model", "?")
        for cand, ag in (d.get("aggregates") or {}).items():
            if ":free" in cand:
                continue
            out["quality"].append(
                {
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
                }
            )

    # Keep only the latest perf measurement per candidate (perf files
    # are timestamped; later runs supersede earlier smoke probes).
    latest_perf: dict[str, dict] = {}
    for f in sorted(glob.glob(str(_ARTIFACTS_DIR / "perf_*.json"))):
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

    # Keep only the latest Gaeilge measurement per candidate/base/split. Direct
    # Gaeilge runs and documented `--slice all` sweeps both carry
    # slices.gaeilge.summary, so ingest both filename shapes.
    latest_gaeilge: dict[tuple[str, str, str], dict] = {}
    gaeilge_run_files = sorted(
        {
            Path(f)
            for pattern in ("run_*_gaeilge_*.json", "run_*_all_*.json")
            for f in glob.glob(str(_ARTIFACTS_DIR / pattern))
        }
    )
    for path in gaeilge_run_files:
        d = json.loads(path.read_text(encoding="utf-8"))
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
            "file": path.name,
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
    for f in sorted(glob.glob(str(_ARTIFACTS_DIR / "dialogue_samples_*.json"))):
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
        def mean(key: str, rows: list[dict] = rows) -> float:
            xs = [r[key] for r in rows if r.get(key) is not None]
            return round(sum(xs) / len(xs), 2) if xs else 0.0

        n_total = sum(r["n"] for r in rows)
        averaged.append(
            {
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
            }
        )
    averaged.sort(key=lambda r: -r["total"])
    out["averaged"] = averaged
    return out


def build_markdown(data: dict) -> str:
    judges = sorted({q["judge"] for q in data["quality"]})
    cached_count = len(data["coverage"])
    judged_count = sum(1 for c in data["coverage"].values() if c)
    summary_rows = [
        ["Cached candidates", cached_count],
        ["Judged candidates", judged_count],
        ["Unjudged backlog", len(data["unjudged"])],
        ["Distinct judges", len(judges)],
        ["Quality rows", len(data["quality"])],
        ["Perf rows", len(data["perf"])],
        ["Gaeilge rows", len(data["gaeilge"])],
    ]

    gaeilge_rows = [
        [
            r["candidate"],
            r["split"],
            r["n"],
            r["errors"],
            _fmt(r["overall"]),
            _fmt(r["fluency"]),
            _fmt(r["grammar"]),
            _fmt(r["idiom"]),
            _fmt(r["task_fulfillment"]),
            _fmt(r["english_leakage"]),
            f"${_fmt(r['usd'], 4)}",
            r["file"],
        ]
        for r in sorted(
            data["gaeilge"], key=lambda r: (r["overall"] is not None, r["overall"]), reverse=True
        )
    ]

    averaged_rows = [
        [
            r["candidate"],
            r["n"],
            _fmt(r["total"]),
            _fmt(r["character"]),
            _fmt(r["authenticity"]),
            _fmt(r["language"]),
            _fmt(r["responsiveness"]),
            _fmt(r["craft"]),
            r["judge_count"],
        ]
        for r in data.get("averaged", [])
    ]

    quality_rows = [
        [
            r["candidate"],
            r["judge"],
            r["n"],
            _fmt(r["total"]),
            _fmt(r["character"]),
            _fmt(r["authenticity"]),
            _fmt(r["language"]),
            _fmt(r["responsiveness"]),
            _fmt(r["craft"]),
            r["file"],
        ]
        for r in sorted(
            data["quality"], key=lambda r: (r["total"] is not None, r["total"]), reverse=True
        )
    ]

    perf_rows = [
        [
            r["candidate"],
            r["n_ok"],
            _fmt(r["ttft_p50"], 0),
            _fmt(r["ttft_p90"], 0),
            _fmt(r["total_p50"], 0),
            _fmt(r["tps_p50"]),
            _fmt(r["tps_p90"]),
            f"{_fmt(r['json_freeform'], 1)}%",
            f"{_fmt(r['json_schema'], 1)}%",
            r["file"],
        ]
        for r in sorted(
            data["perf"], key=lambda r: (r["tps_p50"] is not None, r["tps_p50"]), reverse=True
        )
    ]

    unjudged = ", ".join(f"`{c}`" for c in data["unjudged"]) if data["unjudged"] else "_None._"

    return "\n".join(
        [
            "Evidence type: gameplay transcript",
            "",
            "# rundale-bench v1 leaderboard",
            "",
            "Generated from the same JSON artifacts as [`leaderboard.html`](leaderboard.html). "
            "GitHub Markdown strips the dashboard JavaScript/CSS, so this file is a static "
            "Markdown snapshot and the HTML file is the interactive view.",
            "",
            "## Summary",
            "",
            _markdown_table(["Metric", "Count"], summary_rows),
            "## Gaeilge fluency (1-5 rubric)",
            "",
            "Latest `--slice gaeilge` run per candidate/base/split. Higher is better; "
            "English leakage is 5 when no English leaks.",
            "",
            _markdown_table(
                [
                    "Candidate",
                    "Split",
                    "n",
                    "Err",
                    "Overall",
                    "Fluency",
                    "Grammar",
                    "Idiom",
                    "Task",
                    "No Eng",
                    "Cost",
                    "File",
                ],
                gaeilge_rows,
            ),
            "## Quality scores: cross-judge average",
            "",
            _markdown_table(
                ["Candidate", "n", "Total", "Char", "Auth", "Lang", "Resp", "Craft", "Judges"],
                averaged_rows,
            ),
            "## Quality scores: by judge",
            "",
            _markdown_table(
                [
                    "Candidate",
                    "Judge",
                    "n",
                    "Total",
                    "Char",
                    "Auth",
                    "Lang",
                    "Resp",
                    "Craft",
                    "File",
                ],
                quality_rows,
            ),
            "## Perf probe",
            "",
            _markdown_table(
                [
                    "Candidate",
                    "n_ok",
                    "TTFT p50 ms",
                    "TTFT p90 ms",
                    "Total p50 ms",
                    "Tok/s p50",
                    "Tok/s p90",
                    "JSON free",
                    "JSON schema",
                    "File",
                ],
                perf_rows,
            ),
            "## Unjudged backlog",
            "",
            unjudged,
            "",
        ]
    )


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
    _MARKDOWN_PAGE.write_text(build_markdown(data), encoding="utf-8")
    print(
        f"wrote {_TEMPLATE.relative_to(_REPO_ROOT)} "
        f"+ {_MARKDOWN_PAGE.relative_to(_REPO_ROOT)} "
        f"(quality={len(data['quality'])} perf={len(data['perf'])} "
        f"gaeilge={len(data['gaeilge'])} "
        f"cached={len(data['coverage'])} unjudged={len(data['unjudged'])})"
    )


if __name__ == "__main__":
    main()
