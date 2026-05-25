#!/usr/bin/env python3
"""Summarize local MLX sweep results — one row per (hf_repo, slice).

For each `run_<hf>_<slice>_<UTC>.json` under rundale-bench/artifacts/ that
points at a `mlx-community/*` target, take the latest run and print a compact
comparison table. Also walks `local_<UTC>.json` aggregates and the
corresponding local_leaderboard.md rows for RAM/cost.

Output is plain Markdown — pipe into a file to attach to evidence.

    python3 rundale-bench/summarize_local.py
"""
from __future__ import annotations

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parent
_ARTIFACTS = _HERE / "artifacts"
_LEADERBOARD = _ARTIFACTS / "local_leaderboard.md"


def _is_local(model: str) -> bool:
    return model.startswith("mlx-community/") or "@http://127.0.0.1" in model or "@http://localhost" in model


def _fnum(value, digits: int = 2) -> str:
    """Format a numeric metric, rendering `—` for missing values.

    Summaries returned by `_dialogue_aggregate` set axes to `None` (not
    `0`) when every record was excluded as a bench-bug or judge failure —
    `0` would be a real score and misleading. Format-spec on `None`
    raises `TypeError`, so render explicitly.
    """
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return f"{value:.{digits}f}"
    return "—"


def _format_metric(slice_name: str, summary: dict) -> str:
    if slice_name == "intent":
        return f"label={_fnum(summary.get('label_match_rate'), 3)} score={_fnum(summary.get('mean_score'), 3)}"
    if slice_name == "dialogue":
        ax = " ".join(
            f"{k[:2]}={_fnum(summary.get(k), 1)}"
            for k in ("character", "authenticity", "language", "responsiveness", "craft")
        )
        return f"overall={_fnum(summary.get('overall'))} ({ax})"
    if slice_name == "reaction":
        return f"in_char={_fnum(summary.get('mean_score'))}"
    if slice_name in ("tier2-sim", "tier3-sim"):
        return f"schema={_fnum(summary.get('schema_valid_rate'))} plaus={_fnum(summary.get('mean_score'))}"
    return "?"


def _scan_runs() -> dict[tuple[str, str], dict]:
    """{(hf_repo, slice): latest_run_dict} from per-slice run JSONs."""
    out: dict[tuple[str, str], dict] = {}
    for p in sorted(_ARTIFACTS.glob("run_*.json")):
        try:
            d = json.loads(p.read_text(encoding="utf-8"))
        except Exception:
            continue
        target = (d.get("target") or {}).get("model", "")
        if not _is_local(target):
            continue
        for slice_name, slc in (d.get("slices") or {}).items():
            summary = slc.get("summary") or {}
            key = (target, slice_name)
            existing = out.get(key)
            if not existing or p.stat().st_mtime > existing["mtime"]:
                out[key] = {
                    "mtime": p.stat().st_mtime,
                    "file": p,
                    "metric": _format_metric(slice_name, summary),
                    "elapsed_s": d.get("elapsed_seconds", 0.0),
                    "cost_usd": (d.get("cost") or {}).get("usd", 0.0),
                }
    return out


_LB_ROW = re.compile(
    r"^\|\s*(?P<date>\d{8}T\d{6}Z)\s*\|\s*(?P<repo>mlx-community/[^\s|]+)\s*\|"
    r"\s*(?P<slot>tiny|large)\s*\|\s*(?P<quant>[^\s|]+)\s*\|"
    r"\s*(?P<params>[\d.()A-Za-z ]+?)\s*\|\s*(?P<ram>[\d.]+)\s*\|"
    r"\s*(?P<slice>[a-z\d-]+)\s*\|"
)


def _scan_leaderboard() -> dict[tuple[str, str], dict]:
    """{(hf_repo, slice): latest_row} from leaderboard markdown."""
    out: dict[tuple[str, str], dict] = {}
    if not _LEADERBOARD.exists():
        return out
    for line in _LEADERBOARD.read_text(encoding="utf-8").splitlines():
        m = _LB_ROW.match(line.strip())
        if not m:
            continue
        key = (m.group("repo"), m.group("slice"))
        existing = out.get(key)
        if not existing or m.group("date") > existing["date"]:
            out[key] = {
                "date": m.group("date"),
                "slot": m.group("slot"),
                "quant": m.group("quant"),
                "params": m.group("params"),
                "ram_gb": float(m.group("ram")),
            }
    return out


def main() -> int:
    runs = _scan_runs()
    rows = _scan_leaderboard()

    # Group rows by slice, then sort by overall metric for dialogue / score for others
    by_slice: dict[str, list[dict]] = defaultdict(list)
    for (repo, slc), info in runs.items():
        lb = rows.get((repo, slc), {})
        by_slice[slc].append({
            "repo": repo,
            "slice": slc,
            "metric": info["metric"],
            "elapsed_s": info["elapsed_s"],
            "cost_usd": info["cost_usd"],
            "slot": lb.get("slot", "?"),
            "quant": lb.get("quant", "?"),
            "params": lb.get("params", "?"),
            "ram_gb": lb.get("ram_gb", 0.0),
        })

    def _sort_key(r: dict) -> float:
        m = r["metric"]
        # Extract first numeric we see for ordering.
        match = re.search(r"=([\d.]+)", m)
        return -float(match.group(1)) if match else 0.0

    print("# Local MLX sweep summary (latest per (model, slice))\n")
    for slc in ("intent", "dialogue", "reaction", "tier2-sim", "tier3-sim"):
        if slc not in by_slice:
            continue
        items = sorted(by_slice[slc], key=_sort_key)
        print(f"## {slc}\n")
        print("| Model | slot | quant | params | peak_RAM_GB | metric | elapsed_s | $/run |")
        print("|---|---|---|---|---|---|---|---|")
        for r in items:
            print(
                f"| {r['repo']} | {r['slot']} | {r['quant']} | {r['params']} "
                f"| {r['ram_gb']:.2f} | {r['metric']} | {r['elapsed_s']:.1f} | ${r['cost_usd']:.4f} |"
            )
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
