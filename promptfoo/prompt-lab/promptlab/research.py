"""The automated research loop (§6.4) and the held-out review (§6.5).

Layout of a run (under output/, kept out of git):

  output/<run>/run.json              state: spend, rounds, best, stop reason
  output/<run>/round-00-baseline/    v1 baseline, scored once for reference
  output/<run>/round-NN/variant.json the prompt scored this round
  output/<run>/round-NN/rows.jsonl   every request size, reply and verdict
  output/<run>/round-NN/summary.json score, per model, per requirement, flags
  output/<run>/round-NN/rewrite.json the rewriter's reasoning for this variant
  output/<run>/heldout/              best and baseline on the held-out set
"""

from __future__ import annotations

import copy
import datetime as dt
import json
import statistics
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from . import content, judges, models, render
from . import variant as variant_mod
from .content import Case

OUTPUT_DIR = content.LAB_DIR / "output"
CONFIG_PATH = content.LAB_DIR / "config.json"


class SpendCapReached(RuntimeError):
    pass


class Outage(RuntimeError):
    """Too many rows failed to complete (a request or a judge): the round is not
    comparable with others, so it is discarded rather than scored."""


def load_config() -> dict:
    return json.loads(CONFIG_PATH.read_text(encoding="utf-8"))


class Ledger:
    """Model spend across a run. Refuses work once the cap is reached."""

    def __init__(self, cap: float, spent: float = 0.0):
        self.cap, self.spent = cap, spent
        self._lock = threading.Lock()

    def check(self) -> None:
        if self.spent >= self.cap:
            raise SpendCapReached(f"spend ${self.spent:.2f} reached the ${self.cap:.2f} cap")

    def add(self, usd: float) -> None:
        with self._lock:
            self.spent += usd


def model_configs(cfg: dict, stand_in: bool) -> dict[str, dict]:
    if stand_in:
        return {"stand_in": cfg["stand_in"]}
    return dict(cfg["models"])


def judge_configs(cfg: dict) -> dict[str, dict]:
    return {name: judges.resolve(j) for name, j in cfg["judges"].items()}


# --- one round ------------------------------------------------------------------


def score_round(
    variant: dict | str,
    cases: list[Case],
    cfg: dict,
    ledger: Ledger,
    out_dir: Path,
    *,
    stand_in: bool = False,
    log=print,
) -> dict:
    out_dir.mkdir(parents=True, exist_ok=True)
    if isinstance(variant, dict):
        (out_dir / "variant.json").write_text(
            json.dumps(variant, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        (out_dir / "instructions.md").write_text(variant["instructions"] + "\n", encoding="utf-8")
    research = cfg["research"]
    mcfgs, jcfgs = model_configs(cfg, stand_in), judge_configs(cfg)
    jobs = [
        (case, mname, s)
        for case in cases
        for mname in mcfgs
        for s in range(research["samples_per_case"])
    ]

    def generate(job):
        case, mname, sample = job
        ledger.check()
        request = render.render(variant, case)
        reply = models.generate(mcfgs[mname], request)
        ledger.add(reply.cost_usd)
        return {
            "case": case.id,
            "tags": list(case.tags),
            "model": mname,
            "model_id": mcfgs[mname]["model"],
            "sample": sample,
            "request_chars": request.size_chars,
            "reply": reply.to_json(),
        }

    with ThreadPoolExecutor(research["concurrency"]["generate"]) as pool:
        rows = list(pool.map(generate, jobs))
    log(f"  generated {len(rows)} replies, spend now ${ledger.spent:.4f}")

    by_id = {c.id: c for c in cases}
    verdict_jobs = [
        (i, jname) for i, row in enumerate(rows) if row["reply"]["ok"] for jname in jcfgs
    ]

    def run_judge(job):
        i, jname = job
        row = rows[i]
        if jcfgs[jname].get("billed"):
            ledger.check()
        verdict = judges.judge(jname, jcfgs[jname], by_id[row["case"]], row["reply"]["dialogue"])
        ledger.add(verdict.cost_usd)
        return i, verdict

    with ThreadPoolExecutor(research["concurrency"]["judge"]) as pool:
        verdicts = list(pool.map(run_judge, verdict_jobs))
    for row in rows:
        row["verdicts"] = {}
    for i, verdict in verdicts:
        rows[i]["verdicts"][verdict.judge] = verdict.to_json()
    for row in rows:
        finish_row(row, list(jcfgs), research["judge_gap_flag"])

    summary = summarise(rows, ledger, stand_in=stand_in)
    summary["variant"] = variant if isinstance(variant, str) else variant["name"]
    missing = summary["unjudged"]
    summary["outage"] = bool(rows) and missing / len(rows) > research["max_unjudged_fraction"]
    with (out_dir / "rows.jsonl").open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False) + "\n")
    (out_dir / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    if summary["outage"]:
        raise Outage(
            f"{missing} of {len(rows)} rows in {out_dir.name} lack a completed request or a"
            " judge; see its rows.jsonl"
        )
    return summary


def finish_row(row: dict, judge_names: list[str], gap_flag: float) -> None:
    """Mean of every judge, or unjudged; a failed reply scores 0; a wide split is flagged.

    A row one judge missed is left out rather than scored by the other alone,
    so every score in every round is the same two-judge mean (§6.4).
    """
    if row["reply"].get("transport_failed"):
        row.update(score=None, judged=False, gap=None, flagged=True, note="request failed")
        return
    if not row["reply"]["ok"]:
        # A completed response that is not output-schema JSON is the model's miss.
        row.update(score=0.0, judged=True, gap=None, flagged=False, note="no valid reply")
        return
    missing = [j for j in judge_names if not row["verdicts"].get(j, {}).get("ok")]
    if missing:
        row.update(
            score=None,
            judged=False,
            gap=None,
            flagged=True,
            note=f"missing judge: {', '.join(missing)}",
        )
        return
    scores = [row["verdicts"][j]["score"] for j in judge_names]
    gap = max(scores) - min(scores)
    row.update(
        score=statistics.fmean(scores), judged=True, gap=gap, flagged=gap > gap_flag, note=None
    )


def summarise(rows: list[dict], ledger: Ledger, *, stand_in: bool) -> dict:
    judged = [r for r in rows if r["judged"]]
    by_model: dict[str, list[float]] = {}
    by_req: dict[str, list[float]] = {}
    for row in judged:
        by_model.setdefault(row["model"], []).append(row["score"])
        for tag in row["tags"]:
            by_req.setdefault(tag, []).append(row["score"])
    return {
        "score": statistics.fmean(r["score"] for r in judged) if judged else 0.0,
        "replies": len(rows),
        "judged": len(judged),
        "failed_replies": sum(
            1 for r in rows if not r["reply"]["ok"] and not r["reply"].get("transport_failed")
        ),
        "failed_requests": sum(1 for r in rows if r["reply"].get("transport_failed")),
        "unjudged": sum(1 for r in rows if not r["judged"]),
        "flagged": [
            {"case": r["case"], "model": r["model"], "gap": r["gap"], "note": r.get("note")}
            for r in rows
            if r["flagged"]
        ],
        "by_model": {k: statistics.fmean(v) for k, v in by_model.items()},
        "by_requirement": {
            k: statistics.fmean(v) for k, v in sorted(by_req.items(), key=lambda kv: int(kv[0][1:]))
        },
        "round_spend_usd": sum(r["reply"]["cost_usd"] for r in rows)
        + sum(v.get("cost_usd", 0.0) for r in rows for v in r["verdicts"].values()),
        "run_spend_usd": ledger.spent,
        "mean_request_chars": statistics.fmean(r["request_chars"] for r in rows) if rows else 0,
        "plumbing_only": stand_in,
    }


# --- the loop ---------------------------------------------------------------------


def plateaued(history: list[dict], points: float, rounds: int) -> bool:
    """True after `rounds` research rounds in a row each gaining < `points` on the best before it."""
    gains = [h["gain"] for h in history if h.get("gain") is not None]
    return len(gains) >= rounds and all(g < points for g in gains[-rounds:])


def _state_path(run_dir: Path) -> Path:
    return run_dir / "run.json"


def _save(run_dir: Path, state: dict) -> None:
    _state_path(run_dir).write_text(
        json.dumps(state, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )


def _weakest(rows: list[dict], limit: int = 12) -> list[dict]:
    scored = sorted((r for r in rows if r["judged"]), key=lambda r: r["score"])
    out = []
    for row in scored[:limit]:
        out.append(
            {
                "case": row["case"],
                "model": row["model"],
                "tests": row["tags"],
                "reply": row["reply"]["dialogue"],
                "score": round(row["score"], 1),
                "judges": {
                    j: {
                        "score": v.get("score"),
                        "misses": v.get("misses"),
                        "reason": v.get("reason"),
                    }
                    for j, v in row["verdicts"].items()
                },
            }
        )
    failed = [r for r in rows if not r["reply"]["ok"]]
    for row in failed[:4]:
        out.append(
            {
                "case": row["case"],
                "model": row["model"],
                "failed": row["reply"]["error"],
                "raw": (row["reply"]["raw"] or "")[:300],
            }
        )
    return out


def _rows(round_dir: Path) -> list[dict]:
    with (round_dir / "rows.jsonl").open(encoding="utf-8") as handle:
        return [json.loads(line) for line in handle]


def propose(
    state: dict, run_dir: Path, cfg: dict, start: dict, train: list[Case], log=print
) -> tuple[dict, dict]:
    best = state["best"]
    best_variant = variant_mod.load(run_dir / best["dir"] / "variant.json")
    best_rows = _rows(run_dir / best["dir"])
    last_dir = run_dir / state["rounds"][-1]["dir"]
    last_rows = _rows(last_dir) if last_dir != run_dir / best["dir"] else best_rows
    summary = json.loads((run_dir / best["dir"] / "summary.json").read_text(encoding="utf-8"))
    example = render.render(best_variant, train[0]).user
    history = [
        {
            "round": h["round"],
            "score": round(h["score"], 2),
            "gain": None if h.get("gain") is None else round(h["gain"], 2),
            "change": h.get("reasoning", ""),
        }
        for h in state["rounds"]
        if h["round"] > 0
    ]
    feedback = None
    name = f"{start['name']}-r{len(state['rounds']):02d}"
    for _ in range(3):
        prompt = judges.rewrite_prompt(
            variant=best_variant,
            best_score=best["score"],
            example=example,
            by_requirement=summary["by_requirement"],
            misses=_weakest(last_rows) + ([] if last_rows is best_rows else _weakest(best_rows, 4)),
            history=history,
            feedback=feedback,
        )
        try:
            text, _ = judges.ask(cfg["rewriter"], prompt)
            parsed = judges.extract_json(text)
            proposal = parsed["variant"]
            candidate = variant_mod.apply_rewrite(start, best_variant, proposal, name)
            return candidate, {"reasoning": str(parsed.get("reasoning", "")), "from": best["dir"]}
        except Exception as err:
            feedback = str(err)[:600]
            log(f"  rewrite refused: {feedback}")
    raise RuntimeError(f"rewriter failed three times: {feedback}")


def run_loop(
    run_id: str,
    start_name: str = "brief",
    *,
    stand_in: bool = False,
    case_ids: set[str] | None = None,
    max_rounds: int | None = None,
    log=print,
) -> dict:
    cfg = load_config()
    research = cfg["research"]
    run_dir = OUTPUT_DIR / run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    start = variant_mod.load_named(start_name)
    train = content.cases("train", ids=case_ids)  # the loop never loads held-out cases
    if not train:
        raise ValueError("no training cases selected")

    state = (
        json.loads(_state_path(run_dir).read_text(encoding="utf-8"))
        if _state_path(run_dir).exists()
        else None
    )
    if state is None:
        state = {
            "run": run_id,
            "created": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
            "start": start_name,
            "plumbing_only": stand_in,
            "cases": [c.id for c in train],
            "config": cfg,
            "spend_usd": 0.0,
            "rounds": [],
            "baseline": None,
            "best": None,
            "stopped": None,
        }
    elif state["plumbing_only"] != stand_in:
        raise ValueError("cannot mix stand-in and production rounds in one run")
    if state.get("stopped"):
        log(f"run already stopped: {state['stopped']}")
        return state
    state.pop("paused", None)
    ledger = Ledger(research["spend_cap_usd"], state["spend_usd"])
    limit = max_rounds if max_rounds is not None else research["max_rounds"]

    try:
        if state["baseline"] is None:
            log("round 00: v1 baseline (reference)")
            summary = score_round(
                render.BASELINE,
                train,
                cfg,
                ledger,
                run_dir / "round-00-baseline",
                stand_in=stand_in,
                log=log,
            )
            state["baseline"] = {
                "dir": "round-00-baseline",
                "score": summary["score"],
                "by_model": summary["by_model"],
            }
            state["spend_usd"] = ledger.spent
            _save(run_dir, state)
            log(f"  baseline {summary['score']:.1f}")
        while True:
            n = len(state["rounds"])
            if n > limit:
                state["stopped"] = f"reached max_rounds {limit}"
                break
            if n and plateaued(
                state["rounds"], research["plateau_points"], research["plateau_rounds"]
            ):
                state["stopped"] = (
                    f"plateau: {research['plateau_rounds']} rounds in a row each gaining under {research['plateau_points']} points"
                )
                break
            reserve = research["heldout_reserve_usd"]
            if n and ledger.spent + state["rounds"][-1]["spend_usd"] + reserve > ledger.cap:
                state["stopped"] = (
                    f"next round would pass the ${ledger.cap:.2f} cap less the ${reserve:.2f}"
                    f" held-out reserve (spent ${ledger.spent:.2f})"
                )
                break
            if n == 0:
                candidate, meta = (
                    copy.deepcopy(start),
                    {"reasoning": "starting prompt", "from": None},
                )
            else:
                log(f"round {n:02d}: rewriting from {state['best']['dir']}")
                candidate, meta = propose(state, run_dir, cfg, start, train, log=log)
            round_dir = run_dir / f"round-{n:02d}"
            round_dir.mkdir(parents=True, exist_ok=True)
            (round_dir / "rewrite.json").write_text(
                json.dumps(meta, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
            )
            log(f"round {n:02d}: scoring {candidate['name']}")
            summary = score_round(
                candidate, train, cfg, ledger, round_dir, stand_in=stand_in, log=log
            )
            best_before = state["best"]["score"] if state["best"] else None
            gain = None if best_before is None else summary["score"] - best_before
            entry = {
                "round": n,
                "dir": round_dir.name,
                "variant": candidate["name"],
                "score": summary["score"],
                "gain": gain,
                "by_model": summary["by_model"],
                "spend_usd": summary["round_spend_usd"],
                "reasoning": meta["reasoning"],
                "flagged": len(summary["flagged"]),
            }
            state["rounds"].append(entry)
            if best_before is None or summary["score"] > best_before:
                state["best"] = {
                    "dir": round_dir.name,
                    "round": n,
                    "score": summary["score"],
                    "variant": candidate["name"],
                }
            state["spend_usd"] = ledger.spent
            _save(run_dir, state)
            log(
                f"  score {summary['score']:.1f} (gain {'n/a' if gain is None else f'{gain:+.1f}'}), best {state['best']['score']:.1f} at round {state['best']['round']}, spend ${ledger.spent:.4f}"
            )
    except SpendCapReached as err:
        state["stopped"] = str(err)
    except Outage as err:
        # Not a stop: the round is discarded and resuming the run reruns it.
        state["paused"] = f"outage: {err}; resume when the model and both judges answer"
        log(state["paused"])
    finally:
        # Persist metered spend even on an interrupt or a rewriter failure.
        state["spend_usd"] = ledger.spent
        _save(run_dir, state)
    best = state["best"]
    if best:
        (run_dir / "best-variant.json").write_text(
            (run_dir / best["dir"] / "variant.json").read_text(encoding="utf-8"), encoding="utf-8"
        )
    if state.get("stopped"):
        state.pop("paused", None)
        _save(run_dir, state)
        log(f"stopped: {state['stopped']}")
    return state


# --- held-out review (§6.5) ---------------------------------------------------------


def run_heldout(run_id: str, *, log=print) -> dict:
    cfg = load_config()
    run_dir = OUTPUT_DIR / run_id
    state = json.loads(_state_path(run_dir).read_text(encoding="utf-8"))
    if not state.get("best"):
        raise ValueError("run has no best prompt yet")
    stand_in = state["plumbing_only"]
    ledger = Ledger(cfg["research"]["spend_cap_usd"], state["spend_usd"])
    held = content.cases("heldout")
    best_variant = variant_mod.load(run_dir / state["best"]["dir"] / "variant.json")
    out = run_dir / "heldout"
    # The loop stops early enough to leave `heldout_reserve_usd` for this.
    try:
        log("held-out: best prompt")
        best = score_round(
            best_variant, held, cfg, ledger, out / "best", stand_in=stand_in, log=log
        )
        log("held-out: v1 baseline")
        base = score_round(
            render.BASELINE, held, cfg, ledger, out / "baseline", stand_in=stand_in, log=log
        )
    finally:
        state["spend_usd"] = ledger.spent
        _save(run_dir, state)
    state["heldout"] = {
        "best": best["score"],
        "baseline": base["score"],
        "best_by_model": best["by_model"],
        "baseline_by_model": base["by_model"],
    }
    state["spend_usd"] = ledger.spent
    _save(run_dir, state)
    (out / "side-by-side.md").write_text(side_by_side(out, held), encoding="utf-8")
    log(
        f"held-out: best {best['score']:.1f}, baseline {base['score']:.1f}; read {out / 'side-by-side.md'}"
    )
    return state


def side_by_side(out: Path, held: list[Case]) -> str:
    best = {(r["case"], r["model"], r["sample"]): r for r in _rows(out / "best")}
    base = {(r["case"], r["model"], r["sample"]): r for r in _rows(out / "baseline")}

    def cell(row: dict | None) -> str:
        if row is None:
            return "(missing)"
        text = (
            row["reply"]["dialogue"] if row["reply"]["ok"] else f"(failed: {row['reply']['error']})"
        )
        score = "unjudged" if row["score"] is None else f"{row['score']:.0f}"
        return f"{text} [{score}]"

    lines = ["# Held-out replies: best prompt beside the v1 baseline", ""]
    for case in held:
        lines += [f"## {case.id} ({', '.join(case.tags)})", "", f"Player: {case.player_line}", ""]
        for key in sorted(k for k in best if k[0] == case.id):
            lines += [
                f"- **{key[1]}**, best: {cell(best.get(key))}",
                f"- **{key[1]}**, baseline: {cell(base.get(key))}",
            ]
        lines.append("")
    return "\n".join(lines)


# --- write-up ------------------------------------------------------------------------


def report(run_id: str) -> str:
    run_dir = OUTPUT_DIR / run_id
    state = json.loads(_state_path(run_dir).read_text(encoding="utf-8"))
    lines = [f"# Prompt lab run {run_id}", ""]
    if state["plumbing_only"]:
        lines += [
            "**Plumbing check only.** Replies came from the OpenAI stand-in, not Gemini; these scores are not research results.",
            "",
        ]
    lines += [
        f"- Started: {state['created']} from `{state['start']}`, {len(state['cases'])} training cases",
        f"- Stopped: {state.get('stopped') or state.get('paused') or 'still running'}",
        f"- Model spend: ${state['spend_usd']:.2f} of ${state['config']['research']['spend_cap_usd']:.2f}",
        f"- Baseline (v1) training score: {state['baseline']['score']:.1f}"
        if state.get("baseline")
        else "- Baseline: not scored",
        f"- Best: round {state['best']['round']} (`{state['best']['variant']}`), {state['best']['score']:.1f}"
        if state.get("best")
        else "- Best: none",
    ]
    if state.get("heldout"):
        held = state["heldout"]
        lines.append(f"- Held-out: best {held['best']:.1f}, baseline {held['baseline']:.1f}")
    lines += [
        "",
        "| Round | Variant | Score | Gain | Flagged | Spend | Change |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for r in state["rounds"]:
        gain = "" if r["gain"] is None else f"{r['gain']:+.1f}"
        change = r["reasoning"].replace("|", "/").replace("\n", " ")
        lines.append(
            f"| {r['round']} | `{r['variant']}` | {r['score']:.1f} | {gain} | {r['flagged']} | ${r['spend_usd']:.2f} | {change} |"
        )
    for r in state["rounds"]:
        summary = json.loads((run_dir / r["dir"] / "summary.json").read_text(encoding="utf-8"))
        lines += [
            "",
            f"## Round {r['round']}",
            "",
            "By requirement: "
            + ", ".join(f"{k} {v:.0f}" for k, v in summary["by_requirement"].items()),
            "",
            "By model: " + ", ".join(f"{k} {v:.1f}" for k, v in summary["by_model"].items()),
        ]
        lines += [
            "",
            "<details><summary>Instructions</summary>",
            "",
            "```text",
            (run_dir / r["dir"] / "instructions.md").read_text(encoding="utf-8").rstrip(),
            "```",
            "",
            "</details>",
        ]
    return "\n".join(lines) + "\n"
