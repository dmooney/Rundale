"""Benchmark of record — append-only Rundale model leaderboard (REQ 4).

Reads the promptfoo per-slice result JSONs in `output/` (same inputs as
report.py), computes per-CATEGORY quality means with 95% bootstrap confidence
intervals, the gameplay-weighted overall quality, $/game-hour, p50/p95 latency,
and a value score, then:

  * appends one row per (candidate, run) to leaderboard/leaderboard.jsonl (history)
  * rewrites leaderboard/leaderboard.md — ranked, latest-row-per-candidate

Re-running a candidate updates its leaderboard.md row (latest wins); the jsonl
keeps every run. Every row records the pinned judge model + dataset/rubric merkle
so the table is comparable across re-runs.

    python3 promptfoo/scripts/leaderboard.py [output_dir] [tier_label]

`tier_label` (free|budget|mid|premium|"") is stamped on the rows for funnel
bookkeeping. The judge model + manifest merkle are read from config + MANIFEST.
"""

from __future__ import annotations

import json
import random
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402
import report as rpt  # noqa: E402  (reuse _results/_named/_meta/_candidate + aggregates)

LEADERBOARD_DIR = rb.PROMPTFOO_DIR / "leaderboard"
CATALOG = rb.PROMPTFOO_DIR / "catalog" / "candidates.jsonl"


def _catalog_prices() -> dict[str, tuple[float, float]]:
    """Map candidate spec + model_id → (price_in, price_out) per Mtok from the
    enumerated catalog, so $/game-hour and value are correct for every candidate
    (the static pricing.COSTS snapshot only covers a hand-picked few)."""
    prices: dict[str, tuple[float, float]] = {}
    if not CATALOG.exists():
        return prices
    for line in CATALOG.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        c = json.loads(line)
        pr = (c.get("price_in", 0.0), c.get("price_out", 0.0))
        prices[c["spec"]] = pr
        prices[c["model_id"]] = pr
    return prices


# Quality categories and their gameplay weight, derived from the real per-minute
# token volume the engine spends (config/pricing.GAME_TIME_PROFILE). gaeilge has
# no standalone profile entry — Irish is a sprinkle *within* dialogue — so it
# carries a documented modifier weight (15% of dialogue). multiturn folds into
# the dialogue category (it is the multi-turn aspect of dialogue quality).
def _category_weights() -> dict[str, float]:
    prof = rb.pricing.GAME_TIME_PROFILE
    tok = {k: prof[k][0] + prof[k][1] for k in prof}
    raw = {
        "intent": tok["intent"],
        "dialogue": tok["dialogue"],
        "reaction": tok["reaction"],
        "simulation": tok["simulation"],
        "gaeilge": 0.15 * tok["dialogue"],
    }
    total = sum(raw.values())
    return {k: v / total for k, v in raw.items()}


CATEGORY_WEIGHTS = _category_weights()


def _bootstrap_ci(
    values: list[float], *, iters: int = 2000, seed: int = 1234
) -> tuple[float, float, float]:
    """Return (mean, lo95, hi95) via a percentile bootstrap. Deterministic seed
    so a row is reproducible; needs the raw per-item scores, not just the mean."""
    if not values:
        return (0.0, 0.0, 0.0)
    n = len(values)
    mean = sum(values) / n
    if n == 1:
        return (mean, mean, mean)
    rng = random.Random(seed)
    means = []
    for _ in range(iters):
        s = sum(values[rng.randrange(n)] for _ in range(n))
        means.append(s / n)
    means.sort()
    lo = means[int(0.025 * iters)]
    hi = means[int(0.975 * iters)]
    return (mean, lo, hi)


def _slice_item_scores(slice_name: str, rows: list[dict]) -> list[float]:
    """Per-item overall scores (1-5) for a quality slice, excluding bench_bug /
    judge_failure / errored rows — mirrors report.aggregate_quality's filter."""
    axes = rpt.AXES.get(slice_name, [])
    out = []
    for res in rows:
        named = rpt._named(res)
        meta = rpt._meta(res)
        if meta.get("error") or named.get("judge_failure") or named.get("bench_bug"):
            continue
        if not any(a in named for a in axes):
            continue
        out.append(float(named.get("overall", 0.0)))
    return out


def _intent_item_scores(rows: list[dict]) -> list[float]:
    out = []
    for res in rows:
        if rpt._meta(res).get("error"):
            out.append(0.0)
            continue
        out.append(float(rpt._named(res).get("intent_score", 0.0)))
    return out


def build_candidate_rows(out_dir: Path) -> dict[str, dict]:
    files = sorted(p for p in out_dir.glob("*.json") if p.stem not in ("report",))
    cands: dict[str, dict] = {}
    for f in files:
        slice_name = f.stem
        data = json.loads(f.read_text(encoding="utf-8"))
        rows = rpt._results(data)
        if not rows:
            continue
        by_cand: dict[str, list[dict]] = {}
        for res in rows:
            by_cand.setdefault(rpt._candidate(res), []).append(res)
        for cand, crows in by_cand.items():
            c = cands.setdefault(cand, {"slices": {}})
            if slice_name == "perf":
                agg = rpt.aggregate_perf(crows)
                model = rpt._meta(crows[0]).get("model") if crows else None
                pin, pout = rb.pricing.COSTS.get(model or "", (0.0, 0.0))
                agg.update(rb.pricing.gameplay_cost(pin, pout))
                agg["model"] = model
                c["slices"]["perf"] = agg
            elif slice_name == "intent":
                scores = _intent_item_scores(crows)
                mean, lo, hi = _bootstrap_ci(scores)
                c["slices"]["intent"] = {"mean": mean, "lo": lo, "hi": hi, "n": len(scores)}
            elif slice_name in rpt.AXES:
                scores = _slice_item_scores(slice_name, crows)
                mean, lo, hi = _bootstrap_ci(scores)
                c["slices"][slice_name] = {"mean": mean, "lo": lo, "hi": hi, "n": len(scores)}
    return cands


def _category_scores(slices: dict) -> dict[str, dict]:
    """Collapse slices → the 5 leaderboard categories (1-5 scale)."""

    def g(name):
        s = slices.get(name)
        return s if s and s.get("n") else None

    cats: dict[str, dict] = {}
    if g("intent"):
        cats["intent"] = g("intent")
    # dialogue category = dialogue + multiturn (multi-turn aspect of dialogue)
    dlg = [g("dialogue"), g("multiturn")]
    dlg = [d for d in dlg if d]
    if dlg:
        n = sum(d["n"] for d in dlg)
        mean = sum(d["mean"] * d["n"] for d in dlg) / n
        lo = sum(d["lo"] * d["n"] for d in dlg) / n
        hi = sum(d["hi"] * d["n"] for d in dlg) / n
        cats["dialogue"] = {"mean": mean, "lo": lo, "hi": hi, "n": n}
    if g("reaction"):
        cats["reaction"] = g("reaction")
    sim = [g("tier2-sim"), g("tier3-sim")]
    sim = [s for s in sim if s]
    if sim:
        n = sum(s["n"] for s in sim)
        mean = sum(s["mean"] * s["n"] for s in sim) / n
        lo = sum(s["lo"] * s["n"] for s in sim) / n
        hi = sum(s["hi"] * s["n"] for s in sim) / n
        cats["simulation"] = {"mean": mean, "lo": lo, "hi": hi, "n": n}
    if g("gaeilge"):
        cats["gaeilge"] = g("gaeilge")
    return cats


def _overall(cats: dict[str, dict]) -> tuple[float, float, float]:
    """Gameplay-weighted overall quality (1-5) over present categories, with the
    weights renormalised to whichever categories were actually evaluated."""
    present = {k: CATEGORY_WEIGHTS[k] for k in cats if k in CATEGORY_WEIGHTS}
    if not present:
        return (0.0, 0.0, 0.0)
    z = sum(present.values())
    mean = sum(cats[k]["mean"] * w for k, w in present.items()) / z
    lo = sum(cats[k]["lo"] * w for k, w in present.items()) / z
    hi = sum(cats[k]["hi"] * w for k, w in present.items()) / z
    return (mean, lo, hi)


def main(argv: list[str]) -> int:
    out_dir = Path(argv[1]) if len(argv) > 1 else rb.PROMPTFOO_DIR / "output"
    tier = argv[2] if len(argv) > 2 else ""
    ts = argv[3] if len(argv) > 3 else None  # injectable timestamp (scripts can't Date.now)

    judge = rb.load_judge_config()
    manifest = json.loads((rb.V2_DIR / "MANIFEST.json").read_text(encoding="utf-8"))
    merkle = manifest.get("merkle_root_sha256") or manifest.get("merkle") or ""

    cands = build_candidate_rows(out_dir)
    if not cands:
        print(f"no result JSONs in {out_dir}", file=sys.stderr)
        return 1
    prices = _catalog_prices()

    LEADERBOARD_DIR.mkdir(parents=True, exist_ok=True)
    jsonl_path = LEADERBOARD_DIR / "leaderboard.jsonl"
    rows_out = []
    stamp = ts or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()) if ts is None else ts
    for cand, c in cands.items():
        cats = _category_scores(c["slices"])
        omean, olo, ohi = _overall(cats)
        perf = c["slices"].get("perf", {})
        # $/game-hour from the catalog price (authoritative), falling back to the
        # perf model's static COSTS, then 0. Decoupled from whether perf ran.
        model = perf.get("model")
        pin, pout = (
            prices.get(cand)
            or prices.get(model or "")
            or rb.pricing.COSTS.get(model or "", (0.0, 0.0))
        )
        cost_hr = rb.pricing.gameplay_cost(pin, pout)["gameplay_cost_usd_per_hour"]
        value = (omean / cost_hr) if cost_hr and cost_hr > 0 else None
        row = {
            "candidate": cand,
            "model": perf.get("model"),
            "tier": tier,
            "overall": round(omean, 3),
            "overall_ci95": [round(olo, 3), round(ohi, 3)],
            "categories": {
                k: {
                    "score": round(v["mean"], 3),
                    "ci95": [round(v["lo"], 3), round(v["hi"], 3)],
                    "n": v["n"],
                }
                for k, v in cats.items()
            },
            "usd_per_game_hour": round(cost_hr, 5),
            "value_score": round(value, 3) if value is not None else None,
            "latency_p50_ms": perf.get("latency_p50_ms"),
            "latency_p95_ms": perf.get("latency_p95_ms"),
            "tokens_per_sec": round(perf.get("tokens_per_sec_mean", 0.0), 1),
            "judge_model": judge["model"],
            "dataset_merkle": merkle,
            "timestamp": stamp,
        }
        rows_out.append(row)

    with jsonl_path.open("a", encoding="utf-8") as fh:
        for r in rows_out:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")

    _render_md()
    print(f"[leaderboard] appended {len(rows_out)} row(s) → {jsonl_path}")
    print((LEADERBOARD_DIR / "leaderboard.md").read_text(encoding="utf-8"))
    return 0


def _render_md() -> None:
    """Rewrite leaderboard.md from the full jsonl history — latest row per
    candidate, ranked by overall quality."""
    jsonl_path = LEADERBOARD_DIR / "leaderboard.jsonl"
    history = [
        json.loads(line)
        for line in jsonl_path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    latest: dict[str, dict] = {}
    for r in history:
        latest[r["candidate"]] = r  # last wins
    rows = sorted(latest.values(), key=lambda r: r["overall"], reverse=True)

    cats = ["intent", "dialogue", "reaction", "simulation", "gaeilge"]
    lines = [
        "# Rundale model leaderboard (benchmark of record)\n",
        f"Judge (pinned): **{rows[0]['judge_model'] if rows else 'claude-sonnet-4-6'}** · "
        f"dataset merkle `{(rows[0]['dataset_merkle'] if rows else '')[:12]}` · "
        f"{len(rows)} candidates · scores 1-5, ±95% bootstrap CI.\n",
        "Overall = gameplay-token-weighted mean over categories "
        "(weights: " + ", ".join(f"{k} {CATEGORY_WEIGHTS[k]:.0%}" for k in cats) + "). "
        "Value = overall ÷ $/game-hour. A ranking gap is **real** only when the two "
        "rows' overall 95% CIs do not overlap.\n",
        "| # | Model | Tier | Overall (CI) | Intent | Dialogue | Reaction | Sim | Gaeilge | $/game-hr | Value | p50 ms | p95 ms |",
        "| - | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]

    def cell(r, cat):
        c = r["categories"].get(cat)
        return f"{c['score']:.2f}" if c else "—"

    for i, r in enumerate(rows, 1):
        ov = f"{r['overall']:.2f} ({r['overall_ci95'][0]:.2f}-{r['overall_ci95'][1]:.2f})"
        val = f"{r['value_score']:.1f}" if r.get("value_score") is not None else "free"
        p50 = r.get("latency_p50_ms")
        p95 = r.get("latency_p95_ms")
        lines.append(
            f"| {i} | `{r['candidate'].split('@')[0]}` | {r.get('tier') or '—'} | {ov} | "
            f"{cell(r, 'intent')} | {cell(r, 'dialogue')} | {cell(r, 'reaction')} | "
            f"{cell(r, 'simulation')} | {cell(r, 'gaeilge')} | "
            f"{r['usd_per_game_hour']:.4f} | {val} | "
            f"{int(p50) if p50 else '—'} | {int(p95) if p95 else '—'} |"
        )
    lines.append(
        "\n_Each row is the candidate's most recent run; full history in "
        "`leaderboard.jsonl`. Re-running a candidate updates its row._\n"
    )
    (LEADERBOARD_DIR / "leaderboard.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
