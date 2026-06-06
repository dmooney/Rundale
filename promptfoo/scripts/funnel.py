"""Phased funnel runner with a budget guard (REQ: phased funnel).

Funnels the enumerated candidates down through cheap → full evals:

  screen  — ALL viable candidates, small limit, JUDGE-LIGHT (intent exact-match +
            sim schema-valid are free; only a tiny dialogue sample is judged).
            Ranks per cost tier, keeps the top-K per tier.
  medium  — survivors, every slice incl. multiturn, medium limit, full judging.
  full    — short-list per tier, full datasets + perf + multiturn, pinned judge.
            Produces the leaderboard of record.

Runs IN-PROCESS through rb_common (the exact request + judge code the promptfoo
provider/assertion use), so scoring is identical to a promptfoo run but fast
enough for hundreds of candidates. Emits promptfoo-shaped output/<slice>.json so
report.py + leaderboard.py consume the results unchanged. (For a deep single-
candidate inspection with promptfoo's web grid, use `just -f promptfoo/justfile
bench '<spec>'`.)

A pre-flight cost estimate gates every phase against --cap; the run aborts before
spending if a phase would breach it.

    python3 promptfoo/scripts/funnel.py screen --cap 15 --limit 6 --keep 8
    python3 promptfoo/scripts/funnel.py medium --cap 30 --limit 15 --keep 4
    python3 promptfoo/scripts/funnel.py full   --cap 60
"""

from __future__ import annotations

import argparse
import json
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import leaderboard as lb  # noqa: E402
import rb_common as rb  # noqa: E402
import report as rpt  # noqa: E402

CATALOG = rb.PROMPTFOO_DIR / "catalog" / "candidates.jsonl"
OUT = rb.PROMPTFOO_DIR / "output"
STATE = rb.PROMPTFOO_DIR / "leaderboard" / "funnel_survivors.json"
# Resumable run-state checkpoint: the run key + every completed (candidate, slice)
# result. Re-running the same command reuses these and re-runs only what's missing
# or errored, so a top-up-then-rerun is approximately seamless.
RUN_STATE = rb.PROMPTFOO_DIR / "leaderboard" / "funnel_state.json"


def _state_key(phase_name: str, args, judge: dict, merkle: str, slices: list[str]) -> dict:
    """Identifies a resumable run. If any of these change, cached results are not
    comparable and the run starts fresh."""
    return {
        "phase": phase_name,
        "tier": args.tier or "all",
        "limit": args.limit,
        "judge_model": judge["model"],
        "merkle": merkle,
        "slices": list(slices),
    }


def load_run_state(key: dict, *, fresh: bool) -> dict:
    """Return {key, completed:{<spec>\\x00<slice>: [rows]}}. Reuses the on-disk
    state only when its key matches exactly and --fresh wasn't passed."""
    if fresh or not RUN_STATE.exists():
        return {"key": key, "completed": {}}
    try:
        st = json.loads(RUN_STATE.read_text(encoding="utf-8"))
    except (ValueError, OSError):
        return {"key": key, "completed": {}}
    if st.get("key") != key:
        RUN_STATE.replace(RUN_STATE.with_suffix(".json.bak"))  # archive the mismatched run
        return {"key": key, "completed": {}}
    return {"key": key, "completed": st.get("completed", {})}


def save_run_state(state: dict) -> None:
    """Atomic write so a crash mid-write can't corrupt the checkpoint."""
    tmp = RUN_STATE.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(state, ensure_ascii=False), encoding="utf-8")
    tmp.replace(RUN_STATE)


def slice_clean(rows: list[dict]) -> bool:
    """A (candidate, slice) is 'done' only if no row had a candidate error and no
    judged row failed judging — so 402/429/judge failures are retried on resume."""
    for r in rows:
        if (r.get("response", {}).get("metadata") or {}).get("error"):
            return False
        if (r.get("namedScores") or {}).get("judge_failure"):
            return False
    return True


# Calibrated judge cost per judged item (Sonnet 4.6: ~2.5k in @ $3/Mtok +
# ~150 out @ $15/Mtok ≈ $0.0098). Candidate cost is computed exactly from usage.
JUDGE_USD_PER_ITEM = 0.01
JUDGED = {"dialogue", "reaction", "tier2-sim", "tier3-sim", "gaeilge", "multiturn"}

PHASES = {
    "screen": {
        "slices": ["intent", "tier2-sim", "dialogue"],
        "judged": {"dialogue"},
        "perf": False,
    },
    "medium": {
        "slices": [
            "intent",
            "dialogue",
            "reaction",
            "tier2-sim",
            "tier3-sim",
            "gaeilge",
            "multiturn",
        ],
        "judged": JUDGED,
        "perf": False,
    },
    "full": {
        "slices": [
            "intent",
            "dialogue",
            "reaction",
            "tier2-sim",
            "tier3-sim",
            "gaeilge",
            "multiturn",
        ],
        "judged": JUDGED,
        "perf": True,
    },
}


def load_candidates(path: Path | None = None) -> list[dict]:
    p = path or CATALOG
    return [json.loads(ln) for ln in p.read_text(encoding="utf-8").splitlines() if ln.strip()]


def _dataset(slice_name: str, limit: int | None) -> list[dict]:
    recs = [
        json.loads(ln)
        for ln in (rb.DATASETS_DIR / f"{slice_name}.jsonl").read_text(encoding="utf-8").splitlines()
        if ln.strip()
    ]
    return recs[:limit] if limit else recs


def estimate_phase_usd(cands: list[dict], phase: dict, limit: int | None) -> dict:
    judged_items = 0
    for s in phase["slices"]:
        if s in phase["judged"]:
            n = len(_dataset(s, limit))
            judged_items += n
    judge_usd = len(cands) * judged_items * JUDGE_USD_PER_ITEM
    # candidate spend: rough — input ~3k + output ~200 per item across all slices.
    cand_items = sum(len(_dataset(s, limit)) for s in phase["slices"])
    cand_usd = 0.0
    for c in cands:
        cand_usd += (
            cand_items * (3000 * c.get("price_in", 0.0) + 200 * c.get("price_out", 0.0)) / 1e6
        )
    return {
        "judge_usd": judge_usd,
        "candidate_usd": cand_usd,
        "total_usd": judge_usd + cand_usd,
        "judged_items_per_candidate": judged_items,
        "n_candidates": len(cands),
    }


def _result_row(
    spec: str,
    model: str,
    slice_name: str,
    named: dict,
    *,
    usage: dict,
    cost: float,
    meta_extra: dict,
    latency_ms: float | None = None,
) -> dict:
    meta = {"target": spec, "model": model, "slice": slice_name, **meta_extra}
    return {
        "namedScores": named,
        "latencyMs": latency_ms,
        "response": {
            "metadata": meta,
            "tokenUsage": {
                "prompt": usage.get("prompt_tokens", 0),
                "completion": usage.get("completion_tokens", 0),
                "total": usage.get("prompt_tokens", 0) + usage.get("completion_tokens", 0),
            },
            "cost": cost,
        },
    }


def run_slice(
    spec: str, cand: dict, slice_name: str, limit: int | None, judged: bool
) -> list[dict]:
    target = rb.parse_target(spec)
    rows = []
    for rec in _dataset(slice_name, limit):
        t0 = time.perf_counter()
        gen = rb.generate_candidate(slice_name, target, rec)
        latency = (time.perf_counter() - t0) * 1000.0
        usage = {
            "prompt_tokens": gen["prompt_tokens"],
            "completion_tokens": gen["completion_tokens"],
        }
        if gen["error"]:
            rows.append(
                _result_row(
                    spec,
                    target.model,
                    slice_name,
                    {},
                    usage=usage,
                    cost=0.0,
                    meta_extra={"error": gen["error"]},
                    latency_ms=latency,
                )
            )
            continue
        named: dict = {}
        if slice_name == "intent":
            graded = rb.rb_grade.grade_intent(_parse(gen["output"]), rec.get("gold", {}))
            named = {"label_match": float(graded["label_match"]), "intent_score": graded["score"]}
        else:
            if slice_name in ("tier2-sim", "tier3-sim"):
                named["schema_valid"] = 1.0 if gen.get("schema_valid") else 0.0
            if judged and (gen["output"] or "").strip():
                res = rb.judge_item(slice_name, rec["id"], rec.get("user", ""), gen["output"], rec)
                if res.get("judge_failure"):
                    named["judge_failure"] = 1.0
                elif res.get("flags", {}).get("bench_bug"):
                    named["bench_bug"] = 1.0
                else:
                    for k, v in res.get("axes", {}).items():
                        named[k] = float(v)
                    named["overall"] = float(res.get("overall", 0.0))
        rows.append(
            _result_row(
                spec,
                target.model,
                slice_name,
                named,
                usage=usage,
                cost=gen["cost"],
                meta_extra={"schema_valid": gen.get("schema_valid")},
                latency_ms=latency,
            )
        )
    return rows


def _parse(text) -> dict:
    try:
        v = json.loads(text) if isinstance(text, str) else text
    except Exception:  # noqa: BLE001
        return {}
    # grade_intent expects a dict; valid-JSON-but-not-an-object (list/null/str)
    # must not leak downstream.
    return v if isinstance(v, dict) else {}


def survivors_from_leaderboard(keep_per_tier: int) -> list[str]:
    path = lb.LEADERBOARD_DIR / "leaderboard.jsonl"
    history = [json.loads(ln) for ln in path.read_text(encoding="utf-8").splitlines() if ln.strip()]
    latest = {r["candidate"]: r for r in history}
    by_tier: dict[str, list[dict]] = {}
    for r in latest.values():
        by_tier.setdefault(r.get("tier") or "?", []).append(r)
    keep = []
    for _tier, rows in by_tier.items():
        # Drop candidates with no usable data (overall 0 → fully errored / 429'd).
        rows = [r for r in rows if r["overall"] > 0]
        rows.sort(key=lambda r: r["overall"], reverse=True)
        keep += [r["candidate"] for r in rows[:keep_per_tier]]
    return keep


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("phase", choices=list(PHASES))
    ap.add_argument("--cap", type=float, required=True, help="USD ceiling for this phase")
    ap.add_argument("--limit", type=int, default=None)
    ap.add_argument("--keep", type=int, default=8, help="survivors kept per tier after this phase")
    ap.add_argument(
        "--from-survivors", action="store_true", help="run only previous phase's survivors"
    )
    ap.add_argument("--max-candidates", type=int, default=None, help="hard cap on candidate count")
    ap.add_argument(
        "--tier", default=None, help="comma list of cost tiers to include (free,budget,mid,premium)"
    )
    ap.add_argument(
        "--include-local", action="store_true", help="include local MLX (needs a running server)"
    )
    ap.add_argument("--concurrency", type=int, default=6, help="candidates evaluated in parallel")
    ap.add_argument("--fresh", action="store_true", help="ignore the saved checkpoint and restart")
    ap.add_argument("--catalog", default=None, help="override candidate catalog path (testing)")
    ap.add_argument("--yes", action="store_true", help="skip the estimate confirmation prompt")
    args = ap.parse_args(argv[1:])

    phase = PHASES[args.phase]
    cands = load_candidates(Path(args.catalog) if args.catalog else None)
    if args.tier:
        keep_tiers = {t.strip() for t in args.tier.split(",")}
        cands = [c for c in cands if c.get("tier") in keep_tiers]
    if not args.include_local:
        cands = [c for c in cands if not c.get("local")]
    if args.from_survivors:
        keep = set(json.loads(STATE.read_text()) if STATE.exists() else [])
        cands = [c for c in cands if c["spec"] in keep]
    if args.max_candidates:
        cands = cands[: args.max_candidates]

    # --- resume: load the checkpoint for this exact run key -------------------
    judge = rb.load_judge_config()
    manifest = json.loads((rb.V2_DIR / "MANIFEST.json").read_text(encoding="utf-8"))
    merkle = manifest.get("merkle_root_sha256") or ""
    key = _state_key(args.phase, args, judge, merkle, phase["slices"])
    state = load_run_state(key, fresh=args.fresh)
    completed: dict[str, list[dict]] = state["completed"]

    def _ck(spec: str, s: str) -> str:
        return f"{spec}\x00{s}"

    # estimate only the work NOT already complete (so a resume reflects what's left)
    todo = [c for c in cands if any(_ck(c["spec"], s) not in completed for s in phase["slices"])]
    est = estimate_phase_usd(todo, phase, args.limit)
    reused = len(cands) - len(todo)
    print(
        f"[funnel:{args.phase}] {len(cands)} candidates ({reused} already complete, "
        f"{len(todo)} to run) · est judge ${est['judge_usd']:.2f} + candidate "
        f"${est['candidate_usd']:.2f} = ${est['total_usd']:.2f} (cap ${args.cap:.2f})"
    )
    if est["total_usd"] > args.cap:
        print(
            f"[funnel] ABORT: estimate ${est['total_usd']:.2f} exceeds cap ${args.cap:.2f}. "
            f"Lower --limit / --max-candidates or raise --cap.",
            file=sys.stderr,
        )
        return 2
    if not args.yes:
        print("[funnel] re-run with --yes to execute.")
        return 0

    OUT.mkdir(parents=True, exist_ok=True)
    per_slice: dict[str, list[dict]] = {s: [] for s in phase["slices"]}
    lock = threading.Lock()
    spent = 0.0
    done = 0

    def _flush():
        # output/ is a render of the checkpoint; rebuilt fully each flush.
        for s in phase["slices"]:
            (OUT / f"{s}.json").write_text(
                json.dumps({"results": {"results": per_slice[s]}}, ensure_ascii=False),
                encoding="utf-8",
            )

    def run_candidate(c: dict) -> dict[str, list[dict]]:
        # Per-candidate fault isolation + resume: reuse a completed (cand, slice)
        # verbatim (zero calls); otherwise run it. A single hang/error can't kill
        # the sweep.
        out: dict[str, list[dict]] = {}
        for s in phase["slices"]:
            cached = completed.get(_ck(c["spec"], s))
            if cached is not None:
                out[s] = cached
                continue
            try:
                out[s] = run_slice(c["spec"], c, s, args.limit, s in phase["judged"])
            except Exception as e:  # noqa: BLE001
                out[s] = [
                    _result_row(
                        c["spec"],
                        c["model_id"],
                        s,
                        {},
                        usage={},
                        cost=0.0,
                        meta_extra={"error": f"{type(e).__name__}: {e}"},
                    )
                ]
        return out

    with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futs = {pool.submit(run_candidate, c): c for c in cands}
        for fut in as_completed(futs):
            c = futs[fut]
            res = fut.result()
            with lock:
                for s, rows in res.items():
                    per_slice[s].extend(rows)
                    if _ck(c["spec"], s) not in completed:
                        spent += sum((r["response"]["cost"] or 0.0) for r in rows)
                    # mark complete only if the slice is clean (errors/402 retried next run)
                    if slice_clean(rows):
                        completed[_ck(c["spec"], s)] = rows
                done += 1
                print(f"[funnel] ({done}/{len(cands)}) {c['spec'].split('@')[0]} done", flush=True)
                save_run_state({"key": key, "completed": completed, "updated": None})
                if done % 10 == 0 or done == len(cands):
                    _flush()
    _flush()

    rpt.main(["report", str(OUT)])
    lb.main(["leaderboard", str(OUT), args.phase])
    keep = survivors_from_leaderboard(args.keep)
    STATE.write_text(json.dumps(keep, indent=2), encoding="utf-8")
    print(
        f"[funnel:{args.phase}] candidate spend ${spent:.4f} (this run); survivors kept "
        f"({len(keep)}): " + ", ".join(k.split("@")[0] for k in keep)
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
