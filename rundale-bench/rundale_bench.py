#!/usr/bin/env python3
"""rundale-bench orchestrator.

Single entrypoint for running any slice against any OpenAI-compatible target.
Outputs per-record JSON results + a summary table; supports `--slice all` to
sweep every available slice + emit an aggregate row suitable for the
leaderboard.

Usage::

    python3 rundale-bench/rundale_bench.py \\
        --target 'model@base_url[#env:VAR]' --suite v1 --slice <name|all> \\
        [--judge <id>] [--limit N] [--split dev|holdout]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent
# Make the eval_lib loader available; lives alongside the local-eval scripts.
sys.path.insert(0, str(_REPO_ROOT / "parish" / "scripts" / "local-eval"))
sys.path.insert(0, str(_BENCH_DIR))

from eval_lib import (  # noqa: E402
    CostTracker,
    Target,
    build_dialogue_system_prompt,
    call_chat,
    load_slice,
    parse_target,
)
from grade import (  # noqa: E402
    grade_dialogue,
    grade_gaeilge,
    grade_intent,
    grade_pairwise,
    grade_reaction,
    grade_schema,
    grade_simulation,
    verify_judge_rubric,
)

import itertools
import random

import glob  # noqa: E402
import subprocess  # noqa: E402

import cache as judgment_cache  # noqa: E402
import judge_bundle as jb  # noqa: E402
import promote as promo  # noqa: E402
from catalog import load_catalog  # noqa: E402

_ARTIFACTS_DIR = _BENCH_DIR / "artifacts"

INTENT_SYS = (
    "You are a text adventure input parser. Given the player's natural language input, "
    "determine their intent. Respond with valid JSON containing:\n"
    '- "intent": one of "move", "talk", "look", "interact", "examine", "unknown"\n'
    '- "target": what the action is directed at (string or null)\n'
    '- "dialogue": what the player is saying, if talking (string or null)\n\n'
    'IMPORTANT: "move" is ONLY for when the player expresses a present desire to '
    "navigate somewhere (imperative or future intent). Narrative, past-tense, or "
    'reflective statements that merely mention a place name are "talk", not "move".\n\n'
    "Examples:\n"
    'Input: "go to the pub" → {"intent": "move", "target": "the pub", "dialogue": null}\n'
    'Input: "talk to Mary" → {"intent": "talk", "target": "Mary", "dialogue": null}\n'
    'Input: "tell Padraig I saw his cow" → {"intent": "talk", "target": "Padraig", "dialogue": "I saw his cow"}\n'
    'Input: "look around" → {"intent": "look", "target": null, "dialogue": null}\n'
    'Input: "pick up the stone" → {"intent": "interact", "target": "the stone", "dialogue": null}\n'
    'Input: "I came from the coast" → {"intent": "talk", "target": null, "dialogue": "I came from the coast"}\n'
    'Input: "I was at the shore yesterday" → {"intent": "talk", "target": null, "dialogue": "I was at the shore yesterday"}\n\n'
    "Respond ONLY with valid JSON. No explanation."
)

INTENT_SCHEMA = {
    "name": "intent",
    "strict": True,
    "schema": {
        "type": "object",
        "additionalProperties": False,
        "properties": {
            "intent": {"type": "string", "enum": ["move", "talk", "look", "interact", "examine", "unknown"]},
            "target": {"type": ["string", "null"]},
            "dialogue": {"type": ["string", "null"]},
        },
        "required": ["intent", "target", "dialogue"],
    },
}

# Mirrors `parish_npc::build_tier1_system_prompt` for the Brigid persona so
# bench scores track the runtime tier-1 grounding (issue #994).
DIALOGUE_SYS = build_dialogue_system_prompt()

GAEILGE_SYS = (
    "You are being evaluated for fluency in Irish Gaeilge.\n\n"
    "Follow the task exactly. Unless the task explicitly says otherwise, answer only "
    "in Irish Gaeilge, not English. Do not explain your choices. Prefer natural Irish "
    "syntax and idiom over word-for-word translation from English."
)


def slug(s: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "_", s).strip("_")[:80]


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def load_judge(judge_id: str, suite: str) -> dict:
    path = _BENCH_DIR / suite / f"{judge_id}.json"
    if not path.exists():
        raise FileNotFoundError(f"judge config not found: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def judge_invoker(judge: dict, tracker: CostTracker):
    """Build the `invoke(system, user, schema) -> parsed-dict` callable.

    The judge `Target` is constructed once per call to keep `call_chat`
    happy and to surface missing API keys early.
    """
    target = Target(model=judge["model"], base_url=judge["base_url"], api_key_env=judge.get("api_key_env"))
    temperature = float(judge.get("temperature", 0.0))

    def _invoke(system: str, user: str, schema: dict) -> dict:
        text, usage = call_chat(target, system, user, schema=schema, temperature=temperature)
        tracker.record(target, usage)
        return json.loads(text)

    return _invoke


# ---------------------------------------------------------------------------
# per-slice runners
# ---------------------------------------------------------------------------

def run_intent(target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    results = []
    label_matches = 0
    score_sum = 0.0
    for rec in records:
        try:
            text, usage = call_chat(target, INTENT_SYS, rec["prompt"], schema=INTENT_SCHEMA)
            tracker.record(target, usage)
            pred = json.loads(text)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e), "score": 0.0, "label_match": 0})
            continue
        graded = grade_intent(pred, rec["gold"])
        graded["id"] = rec["id"]
        graded["pred"] = pred
        graded["gold"] = rec["gold"]
        results.append(graded)
        label_matches += graded["label_match"]
        score_sum += graded["score"]
    summary = {
        "slice": "intent",
        "records": len(records),
        "label_match_rate": label_matches / max(1, len(records)),
        "mean_score": score_sum / max(1, len(records)),
    }
    return {"summary": summary, "results": results}


def _judge_is_subagent(judge: dict) -> bool:
    """A judge scored by a Claude Code subagent rather than an HTTP call."""
    return judge.get("judge_via") == "claude-code-subagent"


def run_dialogue_bundled(target: Target, records: list[dict], tracker: CostTracker, args, judge: dict) -> dict:
    """Generate dialogue replies and queue them for subagent judging.

    No judge call happens here: each (prompt, response) is hashed to a
    content-addressed cache key. Already-judged pairs are folded in from the
    cache; the rest are written into one pending bundle for the
    `/rundale-bench` skill to score. Aggregates are completed by `ingest`
    once the subagent results land in `.bench-queue/done/`.
    """
    verify_judge_rubric(judge)
    rubric_sha = judge["rubric_sha256"]
    judge_model = judge["model"]
    candidate = {
        "model_id": getattr(args, "model_id", None) or target.label(),
        "provider_id": getattr(args, "provider_id", None),
        "resolved_target": f"{target.model}@{target.base_url}",
    }

    results: list[dict] = []
    pending_items: list[dict] = []
    cache_hits = 0
    for rec in records:
        try:
            reply, usage = call_chat(target, DIALOGUE_SYS, rec["prompt"], max_tokens=200)
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e)})
            continue
        key = judgment_cache.cache_key(rec["id"], reply, rubric_sha, judge_model)
        entry = {
            "id": rec["id"],
            "reply": reply,
            "response_sha256": judgment_cache.response_sha256(reply),
            "cache_key": key,
        }
        cached = judgment_cache.get(key)
        if cached is not None:
            entry["judged"] = True
            entry["judgment"] = {k: cached.get(k) for k in ("axes", "overall", "flags")}
            cache_hits += 1
        else:
            entry["judged"] = False
            pending_items.append({"prompt_id": rec["id"], "prompt": rec["prompt"], "response": reply})
        results.append(entry)

    bundle_ids: list[str] = []
    if pending_items:
        bundle = jb.assemble_bundle(slice_name="dialogue", candidate=candidate, judge=judge, items=pending_items)
        path = jb.write_pending(bundle)
        bundle_ids.append(bundle["bundle_id"])
        print(f"[dialogue] wrote bundle {bundle['bundle_id']} ({len(pending_items)} item(s)) -> {path}")
    print(f"[dialogue] judge HTTP calls: 0; bundles queued: {len(bundle_ids)} ({cache_hits} cache hits)")

    summary = _dialogue_aggregate(results)
    summary["judge"] = judge["judge_id"]
    summary["judge_model"] = judge_model
    summary["rubric_sha256"] = rubric_sha
    summary["bundles_queued"] = len(bundle_ids)
    summary["cache_hits"] = cache_hits
    return {"summary": summary, "results": results, "bundles": bundle_ids, "candidate": candidate}


def _dialogue_aggregate(results: list[dict]) -> dict:
    """Aggregate dialogue axis means over results that carry a judgment.

    Shared by the bundled runner (partial: only cache hits) and `ingest`
    (complete: after subagent results are folded in). Unjudged or errored
    rows are excluded and surfaced via `judge_failures`.
    """
    axes = ("character", "authenticity", "language", "responsiveness", "craft")
    sums = {k: 0.0 for k in axes}
    overall_sum = 0.0
    judged = 0
    nl_flags = 0
    for r in results:
        j = r.get("judgment")
        if not r.get("judged") or not j or not j.get("axes"):
            continue
        judged += 1
        for k in axes:
            sums[k] += j["axes"].get(k, 0)
        overall_sum += j.get("overall") or 0.0
        if (j.get("flags") or {}).get("non_latin_detected"):
            nl_flags += 1
    n = max(1, judged)
    summary = {
        "slice": "dialogue",
        "records": len(results),
        "judged": judged,
        "judge_failures": len([r for r in results if not r.get("error") and not r.get("judged")]),
        "errors": len([r for r in results if r.get("error")]),
        "non_latin_rate": nl_flags / n,
        **{k: sums[k] / n for k in axes},
        "overall": overall_sum / n,
    }
    return summary


def run_dialogue(target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    judge = load_judge(args.judge, args.suite)
    if _judge_is_subagent(judge):
        return run_dialogue_bundled(target, records, tracker, args, judge)
    invoke = judge_invoker(judge, tracker)
    results = []
    axis_sums = {k: 0.0 for k in ("character", "authenticity", "language", "responsiveness", "craft", "overall")}
    nl_flags = 0
    for rec in records:
        try:
            reply, usage = call_chat(target, DIALOGUE_SYS, rec["prompt"], max_tokens=200)
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e)})
            continue
        graded = grade_dialogue(reply, judge, invoke)
        graded["id"] = rec["id"]
        graded["reply"] = reply
        results.append(graded)
        for k in axis_sums:
            axis_sums[k] += graded.get(k, 0)
        if graded.get("non_latin_chars"):
            nl_flags += 1
    n = max(1, len(records))
    summary = {
        "slice": "dialogue",
        "records": len(records),
        "non_latin_rate": nl_flags / n,
        **{k: v / n for k, v in axis_sums.items()},
    }
    return {"summary": summary, "results": results}


def run_reaction(target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    judge = load_judge("judge_reaction_v1", args.suite)
    invoke = judge_invoker(judge, tracker)
    results = []
    score_sum = 0.0
    for rec in records:
        try:
            reply, usage = call_chat(
                target,
                rec["system_template"],
                rec["prompt"],
                max_tokens=rec.get("max_tokens", 100),
            )
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e), "score": 0.0})
            continue
        graded = grade_reaction(reply, rec["persona"], judge, invoke)
        graded["id"] = rec["id"]
        graded["reply"] = reply
        results.append(graded)
        score_sum += graded["score"]
    summary = {
        "slice": "reaction",
        "records": len(records),
        "mean_score": score_sum / max(1, len(records)),
    }
    return {"summary": summary, "results": results}


def run_simulation(slice_name: str, target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    judge = load_judge("judge_sim_v1", args.suite)
    invoke = judge_invoker(judge, tracker)
    results = []
    valid = 0
    score_sum = 0.0
    for rec in records:
        try:
            reply, usage = call_chat(
                target, None, rec["prompt"],
                schema=rec["schema"],
                max_tokens=600 if slice_name == "tier3-sim" else 200,
            )
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e), "score": 0.0, "schema_valid": False})
            continue
        graded = grade_simulation(reply, rec["schema"], judge, invoke)
        graded["id"] = rec["id"]
        graded["reply"] = reply
        results.append(graded)
        if graded["schema_valid"]:
            valid += 1
        score_sum += graded["score"]
    summary = {
        "slice": slice_name,
        "records": len(records),
        "schema_valid_rate": valid / max(1, len(records)),
        "mean_score": score_sum / max(1, len(records)),
    }
    return {"summary": summary, "results": results}


def _gaeilge_candidate_prompt(rec: dict) -> str:
    constraints = "\n".join(f"- {c}" for c in rec.get("constraints", []))
    return (
        f"Task type: {rec['task_type']}\n\n"
        f"Prompt:\n{rec['prompt']}\n\n"
        f"Constraints:\n{constraints}\n\n"
        "Respond now."
    )


def run_gaeilge(target: Target, records: list[dict], tracker: CostTracker, args) -> dict:
    judge = load_judge("judge_gaeilge_v1", args.suite)
    invoke = judge_invoker(judge, tracker)
    results = []
    axis_sums = {
        k: 0.0
        for k in ("fluency", "grammar", "idiom", "task_fulfillment", "english_leakage", "overall")
    }
    leakage_flags = 0
    error_count = 0
    for rec in records:
        try:
            reply, usage = call_chat(
                target,
                GAEILGE_SYS,
                _gaeilge_candidate_prompt(rec),
                max_tokens=rec.get("max_tokens", 300),
                temperature=0.2,
            )
            tracker.record(target, usage)
        except Exception as e:
            results.append({"id": rec["id"], "error": str(e)})
            error_count += 1
            continue
        graded = grade_gaeilge(reply, rec, judge, invoke)
        graded["id"] = rec["id"]
        graded["reply"] = reply
        results.append(graded)
        if graded.get("error"):
            error_count += 1
        for k in axis_sums:
            axis_sums[k] += graded.get(k, 0)
        if graded.get("english_leakage", 0) < 4:
            leakage_flags += 1
    n = max(1, len(records))
    summary = {
        "slice": "gaeilge",
        "records": len(records),
        "errors": error_count,
        "english_leakage_flag_rate": leakage_flags / n,
        **{f"{k}_mean": v / n for k, v in axis_sums.items()},
    }
    return {"summary": summary, "results": results}


def load_tier_ids(tier: str, slice_name: str, suite: str) -> Optional[list[str]]:
    """Prompt ids for a tier/slice from `<suite>/<tier>.ids.json`, or None.

    The file maps slice name -> list of ids. Missing file or missing slice
    entry returns None (caller falls back to the full slice).
    """
    path = _BENCH_DIR / suite / f"{tier}.ids.json"
    if not path.exists():
        return None
    data = json.loads(path.read_text(encoding="utf-8"))
    ids = data.get(slice_name)
    return list(ids) if ids else None


def run_slice(slice_name: str, target: Target, tracker: CostTracker, args) -> dict:
    records = load_slice(slice_name, version=args.suite, split=args.split)
    tier = getattr(args, "tier", None)
    if tier:
        ids = load_tier_ids(tier, slice_name, args.suite)
        if ids is not None:
            keep = set(ids)
            records = [r for r in records if r["id"] in keep]
    if args.limit:
        records = records[: args.limit]
    if slice_name == "intent":
        return run_intent(target, records, tracker, args)
    if slice_name == "dialogue":
        return run_dialogue(target, records, tracker, args)
    if slice_name == "reaction":
        return run_reaction(target, records, tracker, args)
    if slice_name in ("tier2-sim", "tier3-sim"):
        return run_simulation(slice_name, target, records, tracker, args)
    if slice_name == "gaeilge":
        return run_gaeilge(target, records, tracker, args)
    raise SystemExit(f"unknown slice: {slice_name}")


# ---------------------------------------------------------------------------
# ELO mode (pairwise judge)
# ---------------------------------------------------------------------------

def _elo_update(rating_a: float, rating_b: float, score_a: float, k: float) -> tuple[float, float]:
    """Standard ELO update. `score_a` ∈ {0, 0.5, 1}. Returns (new_a, new_b)."""
    expected_a = 1.0 / (1.0 + 10.0 ** ((rating_b - rating_a) / 400.0))
    expected_b = 1.0 - expected_a
    return (
        rating_a + k * (score_a - expected_a),
        rating_b + k * ((1.0 - score_a) - expected_b),
    )


def _bootstrap_ci(
    matches: list[tuple[str, str, float]],
    targets: list[str],
    k_initial: float,
    k_settled: float = 16.0,
    settle_threshold: int = 50,
    iters: int = 500,
) -> dict:
    """Bootstrap 5/95 percentile ratings by resampling matches with replacement.

    Mirrors the main accumulator's dynamic K: K = k_initial until either
    candidate in a match has at least `settle_threshold` matches under its
    belt, then drops to k_settled. Using a constant K here understated CI
    width for late matches (where the actual ratings move slowly).
    """
    rng = random.Random(0xb1a40)
    all_ratings: dict[str, list[float]] = {t: [] for t in targets}
    for _ in range(iters):
        ratings = {t: 1500.0 for t in targets}
        match_count = {t: 0 for t in targets}
        sample = [matches[rng.randrange(len(matches))] for _ in range(len(matches))]
        for a, b, score_a in sample:
            k = k_initial if min(match_count[a], match_count[b]) < settle_threshold else k_settled
            new_a, new_b = _elo_update(ratings[a], ratings[b], score_a, k)
            ratings[a] = new_a
            ratings[b] = new_b
            match_count[a] += 1
            match_count[b] += 1
        for t in targets:
            all_ratings[t].append(ratings[t])
    ci = {}
    for t in targets:
        rs = sorted(all_ratings[t])
        ci[t] = (rs[int(0.05 * iters)], rs[int(0.95 * iters)])
    return ci


def run_elo(targets: list[Target], tracker: CostTracker, args) -> dict:
    """Pairwise ELO ranking over the dialogue slice.

    For each prompt, every (a, b) pair plays one match. A and B labels are
    randomized per match to absorb judge first-position bias. Replies are
    generated once per (target, prompt) and cached.
    """
    if len(targets) < 2:
        raise SystemExit("--mode elo requires at least 2 --target flags")

    records = load_slice("dialogue", version=args.suite, split=args.split)
    if args.limit:
        records = records[: args.limit]

    judge = load_judge(args.judge, args.suite)
    invoke = judge_invoker(judge, tracker)

    rng = random.Random(0xe10)
    # Use `model@base_url` as the canonical id so two `--target` flags with
    # the same model name but different providers / urls don't collide.
    def _target_id(t: Target) -> str:
        return f"{t.model}@{t.base_url}"
    target_ids = [_target_id(t) for t in targets]
    if len(set(target_ids)) != len(target_ids):
        raise SystemExit(f"--target flags must be unique on model+base_url; got {target_ids}")

    # Reply cache: (target_id, prompt_id) -> (reply, error)
    replies: dict[tuple[str, str], tuple[str, Optional[str]]] = {}
    for t in targets:
        tid = _target_id(t)
        for rec in records:
            try:
                reply, usage = call_chat(t, DIALOGUE_SYS, rec["prompt"], max_tokens=200)
                tracker.record(t, usage)
                replies[(tid, rec["id"])] = (reply, None)
            except Exception as e:
                replies[(tid, rec["id"])] = ("", str(e))
        print(f"[elo] candidate replies ready: {tid}")

    matches: list[tuple[str, str, float]] = []  # (winner_id, loser_id, score_a-as-listed-first)
    match_log: list[dict] = []
    pairs = list(itertools.combinations(target_ids, 2))
    for rec in records:
        prompt_id = rec["id"]
        for a, b in pairs:
            reply_a, err_a = replies[(a, prompt_id)]
            reply_b, err_b = replies[(b, prompt_id)]
            if err_a and err_b:
                continue  # both failed; no signal
            if err_a:
                # Canonical (a, b, score_a) ordering. A errored → score_a=0
                # (A loses, B gains). Earlier (b, a, 0.0) recorded B losing.
                matches.append((a, b, 0.0))
                match_log.append({"prompt": prompt_id, "a": a, "b": b, "winner": "B", "reason": f"A error: {err_a[:60]}"})
                continue
            if err_b:
                matches.append((a, b, 1.0))
                match_log.append({"prompt": prompt_id, "a": a, "b": b, "winner": "A", "reason": f"B error: {err_b[:60]}"})
                continue
            # Position randomization: 50% of the time swap A/B
            swap = rng.random() < 0.5
            shown_a, shown_b = (reply_b, reply_a) if swap else (reply_a, reply_b)
            judgment = grade_pairwise(shown_a, shown_b, rec["prompt"], judge, invoke)
            w = judgment["winner"]
            # Map judge's verdict back to canonical (a, b) labels
            if w == "tie":
                score_a = 0.5
            elif (w == "A" and not swap) or (w == "B" and swap):
                score_a = 1.0
            else:
                score_a = 0.0
            matches.append((a, b, score_a))
            match_log.append({
                "prompt": prompt_id, "a": a, "b": b, "swap": swap,
                "winner": w, "score_a": score_a, "reason": judgment.get("reason", "")[:120],
            })

    # ELO accumulation
    k_initial = 32.0
    ratings = {t: 1500.0 for t in target_ids}
    match_count = {t: 0 for t in target_ids}
    for a, b, score_a in matches:
        k = k_initial if min(match_count[a], match_count[b]) < 50 else 16.0
        new_a, new_b = _elo_update(ratings[a], ratings[b], score_a, k)
        ratings[a] = new_a
        ratings[b] = new_b
        match_count[a] += 1
        match_count[b] += 1

    ci = _bootstrap_ci(matches, target_ids, k_initial)

    standings = sorted(
        [(t, ratings[t], match_count[t], ci[t]) for t in target_ids],
        key=lambda r: -r[1],
    )
    print("\n[elo] standings:")
    for tid, rating, n, (lo, hi) in standings:
        print(f"  {rating:7.1f}  [CI {lo:6.1f}–{hi:6.1f}]  n={n:3d}  {tid}")

    return {
        "mode": "elo",
        "judge": judge["judge_id"],
        "rubric_sha256": judge["rubric_sha256"],
        "targets": target_ids,
        "prompts": len(records),
        "matches": len(matches),
        "ratings": ratings,
        "match_counts": match_count,
        "ci": {t: list(ci[t]) for t in target_ids},
        "standings": [
            {"target": t, "rating": r, "matches": n, "ci_lo": lo, "ci_hi": hi}
            for t, r, n, (lo, hi) in standings
        ],
        "match_log": match_log,
    }


_JUDGE_ALIASES = {"sonnet": "judge_sonnet_v1", "qwen": "judge_v1"}


def cmd_catalog(argv: list[str]) -> None:
    ap = argparse.ArgumentParser(prog="rundale_bench.py catalog")
    ap.add_argument("--show", action="store_true", help="print catalog with resolved quality provider")
    ap.add_argument("--suite", default="v1")
    args = ap.parse_args(argv)
    cat = load_catalog(version=args.suite)
    print(f"catalog: {len(cat.models)} model(s), sha256={cat.sha256[:12]}…")
    for m in cat.models:
        cp = m.cheapest_provider()
        local = " [local]" if m.local_only else ""
        print(f"  {m.id:<20} {m.display_name}{local}")
        print(f"      quality_provider={cp.provider_id}  (${cp.price_in_per_mtok}/{cp.price_out_per_mtok} per Mtok)  providers={[p.provider_id for p in m.providers]}")


def cmd_judge(argv: list[str]) -> None:
    ap = argparse.ArgumentParser(prog="rundale_bench.py judge")
    ap.add_argument("--verify", metavar="JUDGE_ID", help="verify a judge config's rubric_sha256")
    ap.add_argument("--suite", default="v1")
    args = ap.parse_args(argv)
    if not args.verify:
        ap.error("nothing to do; pass --verify <judge_id>")
    judge_id = _JUDGE_ALIASES.get(args.verify, args.verify)
    judge = load_judge(judge_id, args.suite)
    verify_judge_rubric(judge)  # raises on drift
    print(f"rubric_sha256 OK  ({judge_id}: model={judge['model']}, sha={judge['rubric_sha256'][:12]}…)")


def cmd_ingest(argv: list[str]) -> None:
    ap = argparse.ArgumentParser(prog="rundale_bench.py ingest")
    ap.add_argument("--finalize", action="store_true",
                    help="error if any pending bundle has no done counterpart")
    args = ap.parse_args(argv)

    done = jb.list_done()
    judgments_written = 0
    failures = 0
    for done_path in done:
        result = jb.read_result(done_path)
        pending_path = jb.PENDING_DIR / done_path.name
        if not pending_path.exists():
            print(f"[ingest] WARN: done/{done_path.name} has no pending bundle; skipping")
            continue
        bundle = jb.read_json(pending_path)
        responses = {it["prompt_id"]: it["response"] for it in bundle["items"]}
        valid, failed = jb.validate_result(result, bundle)
        for item in valid:
            response = responses.get(item["prompt_id"], "")
            key = judgment_cache.cache_key(item["prompt_id"], response, bundle["rubric_sha256"], bundle["judge_model"])
            judgment_cache.put(key, {
                "slice": bundle["slice"],
                "prompt_id": item["prompt_id"],
                "response_sha256": judgment_cache.response_sha256(response),
                "rubric_sha256": bundle["rubric_sha256"],
                "judge_model": bundle["judge_model"],
                "judge_id": bundle["judge_id"],
                "axes": item["axes"],
                "overall": item["overall"],
                "rationales": item.get("rationales", {}),
                "flags": item["flags"],
            })
            judgments_written += 1
        for item in failed:
            failures += 1
            print(f"[ingest] judge failure: {item['prompt_id']} — {item.get('error', 'invalid')}")

    unjudged = jb.unjudged_bundles()
    if unjudged and args.finalize:
        for p in unjudged:
            print(f"[ingest] pending with no result: {p.name}", file=sys.stderr)
        raise SystemExit(f"ingest --finalize: {len(unjudged)} pending bundle(s) not yet judged")

    refreshed = _refresh_run_aggregates()
    print(f"[ingest] judgments written: {judgments_written}; judge failures: {failures}; "
          f"pending unjudged: {len(unjudged)}; run files refreshed: {refreshed}")


def _refresh_run_aggregates() -> int:
    """Re-read every artifacts/run_*.json dialogue slice and fold in any
    judgments now present in the cache, recomputing the aggregate."""
    refreshed = 0
    for run_path in sorted(_ARTIFACTS_DIR.glob("run_*.json")):
        out = json.loads(run_path.read_text(encoding="utf-8"))
        dia = out.get("slices", {}).get("dialogue")
        if not dia or "results" not in dia:
            continue
        changed = False
        for r in dia["results"]:
            key = r.get("cache_key")
            if not key or r.get("judged"):
                continue
            cached = judgment_cache.get(key)
            if cached is not None:
                r["judged"] = True
                r["judgment"] = {k: cached.get(k) for k in ("axes", "overall", "flags")}
                changed = True
        if changed:
            new_summary = _dialogue_aggregate(dia["results"])
            new_summary.update({k: dia["summary"][k] for k in ("judge", "judge_model", "rubric_sha256")
                                if k in dia.get("summary", {})})
            dia["summary"] = new_summary
            run_path.write_text(json.dumps(out, indent=2, default=str) + "\n", encoding="utf-8")
            refreshed += 1
    return refreshed


# ---------------------------------------------------------------------------
# Phase 2: tier funnel + differential re-judge
# ---------------------------------------------------------------------------

def tier_prompt_ids(tier: str, slice_name: str, suite: str) -> Optional[list[str]]:
    """Prompt ids for a tier. `finalist` returns None (= full dev slice);
    `screen`/`contender` read their id files; anything else falls through."""
    if tier == "finalist":
        return None
    return load_tier_ids(tier, slice_name, suite)


def resolve_models(catalog, models_arg: str, exclude: set[str]) -> list:
    """Resolve a --models spec to catalog Model objects.

    Forms: `all`; comma list `a,b`; additive `+id` (single new model). Unknown
    ids are a hard error. `exclude` drops ids from the result.
    """
    if models_arg == "all":
        ids = catalog.ids()
    elif models_arg.startswith("+"):
        ids = [models_arg[1:]]
    else:
        ids = [m.strip() for m in models_arg.split(",") if m.strip()]
    unknown = [i for i in ids if i not in catalog.ids()]
    if unknown:
        raise SystemExit(f"unknown catalog model id(s): {unknown}")
    return [catalog.by_id(i) for i in ids if i not in exclude]


def cmd_tiers(argv: list[str]) -> None:
    ap = argparse.ArgumentParser(prog="rundale_bench.py tiers")
    ap.add_argument("--show", action="store_true")
    ap.add_argument("--slice", default="dialogue")
    ap.add_argument("--suite", default="v1")
    args = ap.parse_args(argv)
    full = load_slice(args.slice, version=args.suite, split="dev")
    full_ids = {r["id"] for r in full}
    print(f"tiers for slice={args.slice} (dev):")
    for tier in ("screen", "contender", "finalist"):
        ids = tier_prompt_ids(tier, args.slice, args.suite)
        if ids is None:
            print(f"  {tier:<10} {len(full)}  (full dev slice)")
        else:
            missing = [i for i in ids if i not in full_ids]
            tag = f"  WARNING {len(missing)} id(s) not in slice" if missing else ""
            print(f"  {tier:<10} {len(ids)}{tag}")


def _run_one_model(model, provider, args) -> dict:
    target = parse_target(provider.resolved_target())
    sub = argparse.Namespace(suite=args.suite, split=args.split, limit=args.limit,
                             tier=args.tier, judge=args.judge,
                             model_id=model.id, provider_id=provider.provider_id)
    tracker = CostTracker()
    data = run_slice(args.slice, target, tracker, sub)
    out = {
        "suite": args.suite, "split": args.split, "tier": args.tier,
        "slice": args.slice, "skip_promotion": getattr(args, "skip_promotion", False),
        "target": {"model": target.model, "base_url": target.base_url},
        "candidate": {"model_id": model.id, "provider_id": provider.provider_id,
                      "resolved_target": provider.resolved_target()},
        "promoted_from": None,
        "run_started_utc": datetime.now(timezone.utc).isoformat(),
        "slices": {args.slice: data},
        "cost": {"calls": tracker.calls, "prompt_tokens": tracker.prompt_tokens,
                 "completion_tokens": tracker.completion_tokens, "usd": tracker.usd},
    }
    _ARTIFACTS_DIR.mkdir(parents=True, exist_ok=True)
    out_path = _ARTIFACTS_DIR / f"run_{slug(model.id)}_{args.slice}_{args.tier}_{utc_stamp()}.json"
    out_path.write_text(json.dumps(out, indent=2, default=str) + "\n", encoding="utf-8")
    print(f"[run] {model.id} via {provider.provider_id}: {data['summary'].get('bundles_queued', 0)} bundle(s) queued -> {out_path.name}")
    return out


def cmd_run(argv: list[str]) -> None:
    ap = argparse.ArgumentParser(prog="rundale_bench.py run")
    ap.add_argument("--tier", required=True, choices=["screen", "contender", "finalist"])
    ap.add_argument("--models", required=True, help="all | id,id | +id (additive)")
    ap.add_argument("--exclude-models", default="", help="comma list to drop")
    ap.add_argument("--slice", default="dialogue")
    ap.add_argument("--judge", default="sonnet")
    ap.add_argument("--suite", default="v1")
    ap.add_argument("--split", default="dev", choices=["dev", "holdout"])
    ap.add_argument("--limit", type=int, default=None)
    ap.add_argument("--skip-promotion", action="store_true",
                    help="documentary flag recorded on the run; promotion is a separate step")
    args = ap.parse_args(argv)
    if args.slice != "dialogue":
        raise SystemExit("Phase 2 wires the dialogue slice only; other slices land in Phase 5")
    if args.judge in _JUDGE_ALIASES:
        args.judge = _JUDGE_ALIASES[args.judge]
    catalog = load_catalog(version=args.suite)
    exclude = {m.strip() for m in args.exclude_models.split(",") if m.strip()}
    models = resolve_models(catalog, args.models, exclude)
    if not models:
        raise SystemExit("no models selected after applying --exclude-models")
    print(f"[run] tier={args.tier} slice={args.slice} models={[m.id for m in models]}")
    for model in models:
        _run_one_model(model, model.cheapest_provider(), args)
    print(f"[run] done. Next: /rundale-bench drain-queue, then 'ingest --finalize', "
          f"then 'promote --from {args.tier}'.")


def _tier_aggregates(tier: str, slice_name: str) -> dict[str, dict]:
    """model_id -> slice summary, from this tier's run JSONs (latest wins)."""
    aggs: dict[str, dict] = {}
    for path in sorted(glob.glob(str(_ARTIFACTS_DIR / "run_*.json"))):
        out = json.loads(Path(path).read_text(encoding="utf-8"))
        if out.get("tier") != tier:
            continue
        s = out.get("slices", {}).get(slice_name)
        mid = out.get("candidate", {}).get("model_id")
        if s and mid:
            aggs[mid] = s["summary"]
    return aggs


def cmd_promote(argv: list[str]) -> None:
    ap = argparse.ArgumentParser(prog="rundale_bench.py promote")
    ap.add_argument("--from", dest="from_tier", required=True, choices=["screen", "contender"])
    ap.add_argument("--slice", default="dialogue")
    ap.add_argument("--include-models", default="")
    ap.add_argument("--exclude-models", default="")
    args = ap.parse_args(argv)
    include = {m.strip() for m in args.include_models.split(",") if m.strip()}
    exclude = {m.strip() for m in args.exclude_models.split(",") if m.strip()}
    aggs = _tier_aggregates(args.from_tier, args.slice)
    decisions = promo.promote(aggs, args.from_tier, include=include, exclude=exclude)
    nxt = promo.TIER_RULES[args.from_tier]["next"]
    print(f"[promote] {args.from_tier} -> {nxt}:")
    for d in decisions:
        mark = "PROMOTE" if d.promoted else "hold   "
        print(f"  {mark}  {d.model_id:<20} {d.reason}")
    ids = promo.promoted_ids(decisions)
    print(f"[promote] promoted: {','.join(ids) if ids else '(none)'}")


# ── differential re-judge ────────────────────────────────────────────────────
def collect_samples(slice_name: str = "dialogue") -> list[dict]:
    """Every committed sample (model_id, prompt_id, response) from run JSONs."""
    samples: list[dict] = []
    for path in sorted(glob.glob(str(_ARTIFACTS_DIR / "run_*.json"))):
        out = json.loads(Path(path).read_text(encoding="utf-8"))
        s = out.get("slices", {}).get(slice_name)
        mid = out.get("candidate", {}).get("model_id") or out.get("target", {}).get("model", "?")
        if not s:
            continue
        for r in s.get("results", []):
            if r.get("reply") is None or r.get("error"):
                continue
            samples.append({"model_id": mid, "prompt_id": r["id"], "response": r["reply"]})
    return samples


def stale_samples(samples: list[dict], judge: dict) -> list[dict]:
    """Samples whose cache_key under the CURRENT judge is absent from cache."""
    out = []
    for s in samples:
        key = judgment_cache.cache_key(s["prompt_id"], s["response"],
                                       judge["rubric_sha256"], judge["model"])
        if not judgment_cache.has(key):
            out.append({**s, "cache_key": key})
    return out


def _git_show(ref: str, path: str) -> Optional[str]:
    try:
        return subprocess.check_output(["git", "show", f"{ref}:{path}"],
                                       cwd=_REPO_ROOT, stderr=subprocess.DEVNULL).decode("utf-8")
    except subprocess.CalledProcessError:
        return None


def classify_since(ref: str, judge_id: str, suite: str, *, show=_git_show) -> str:
    """Why are judgments stale relative to `ref`? Compares the judge config
    between ref and HEAD. `show` is injectable for tests."""
    rel = f"rundale-bench/{suite}/{judge_id}.json"
    old = show(ref, rel)
    if old is None:
        return "new judge config"
    try:
        old_cfg = json.loads(old)
    except json.JSONDecodeError:
        return "judge config unparseable at ref"
    cur = load_judge(judge_id, suite)
    if old_cfg.get("model") != cur.get("model"):
        return "judge model swap"
    if old_cfg.get("rubric_sha256") != cur.get("rubric_sha256"):
        return "rubric drift"
    return "new responses"


def cmd_rejudge(argv: list[str]) -> None:
    ap = argparse.ArgumentParser(prog="rundale_bench.py rejudge")
    ap.add_argument("--dry-run", action="store_true", help="report stale count; make no changes")
    ap.add_argument("--since", default=None, help="git ref to diff the judge config against")
    ap.add_argument("--judge", default="sonnet")
    ap.add_argument("--slice", default="dialogue")
    ap.add_argument("--suite", default="v1")
    args = ap.parse_args(argv)
    judge_id = _JUDGE_ALIASES.get(args.judge, args.judge)
    judge = load_judge(judge_id, args.suite)
    samples = collect_samples(args.slice)
    stale = stale_samples(samples, judge)
    reason = classify_since(args.since, judge_id, args.suite) if args.since else "stale (rubric/judge/new)"
    print(f"[rejudge] samples={len(samples)} stale={len(stale)} reason={reason}")
    if args.dry_run:
        return
    if not stale:
        print("[rejudge] nothing to re-queue.")
        return
    # Group stale samples into one bundle per model_id and queue them.
    by_model: dict[str, list[dict]] = {}
    for s in stale:
        by_model.setdefault(s["model_id"], []).append(s)
    for mid, items in by_model.items():
        bundle = jb.assemble_bundle(
            slice_name=args.slice,
            candidate={"model_id": mid, "provider_id": None, "resolved_target": "rejudge"},
            judge=judge,
            items=[{"prompt_id": s["prompt_id"], "prompt": "", "response": s["response"]} for s in items],
        )
        jb.write_pending(bundle)
        print(f"[rejudge] queued {len(items)} item(s) for {mid}")
    print("[rejudge] run /rundale-bench drain-queue, then 'ingest --finalize'.")


def main() -> None:
    argv = sys.argv[1:]
    dispatch = {
        "catalog": cmd_catalog, "judge": cmd_judge, "ingest": cmd_ingest,
        "tiers": cmd_tiers, "run": cmd_run, "promote": cmd_promote, "rejudge": cmd_rejudge,
    }
    if argv and argv[0] in dispatch:
        dispatch[argv[0]](argv[1:])
        return
    legacy_main(argv)


def legacy_main(argv: list[str]) -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--target", action="append", required=True,
                    help="model@base_url[#env:VAR]; pass multiple times in --mode elo")
    ap.add_argument("--suite", default="v1")
    ap.add_argument("--slice", default=None,
                    choices=["intent", "dialogue", "reaction", "tier2-sim", "tier3-sim", "gaeilge", "all"],
                    help="absolute-score mode: one slice to run (omit when --mode elo)")
    ap.add_argument("--mode", default="absolute", choices=["absolute", "elo"],
                    help="absolute: per-slice graders; elo: pairwise ELO over dialogue slice")
    ap.add_argument("--judge", default=None,
                    help="judge config id or alias (sonnet|qwen); default judge_v1 absolute, judge_pairwise_v1 elo")
    ap.add_argument("--tier", default=None, choices=["screen", "contender", "finalist"],
                    help="filter prompts to a tier id set (<suite>/<tier>.ids.json)")
    ap.add_argument("--model-id", dest="model_id", default=None,
                    help="catalog model id recorded on the candidate (subagent judging)")
    ap.add_argument("--provider-id", dest="provider_id", default=None,
                    help="provider id recorded on the candidate (subagent judging)")
    ap.add_argument("--limit", type=int, default=None)
    ap.add_argument("--split", default="dev", choices=["dev", "holdout"])
    args = ap.parse_args(argv)

    if args.judge in _JUDGE_ALIASES:
        args.judge = _JUDGE_ALIASES[args.judge]

    if args.mode == "elo":
        if args.slice is not None and args.slice != "dialogue":
            raise SystemExit("--mode elo currently only supports the dialogue slice")
        if args.judge is None:
            args.judge = "judge_pairwise_v1"
        targets = [parse_target(t) for t in args.target]
        tracker = CostTracker()
        started = time.time()
        out = {
            "suite": args.suite, "split": args.split, "mode": "elo",
            "run_started_utc": datetime.now(timezone.utc).isoformat(),
            "elo": run_elo(targets, tracker, args),
        }
        elapsed = time.time() - started
        out["elapsed_seconds"] = elapsed
        out["cost"] = {"calls": tracker.calls, "prompt_tokens": tracker.prompt_tokens,
                       "completion_tokens": tracker.completion_tokens, "usd": tracker.usd}
        print(f"\ntotal: {tracker.summary()} in {elapsed:.1f}s")
        _ARTIFACTS_DIR.mkdir(parents=True, exist_ok=True)
        out_path = _ARTIFACTS_DIR / f"elo_{utc_stamp()}.json"
        out_path.write_text(json.dumps(out, indent=2, default=str) + "\n", encoding="utf-8")
        print(f"wrote {out_path}")
        return

    # absolute mode
    if args.slice is None:
        raise SystemExit("--slice is required in absolute mode")
    if args.judge is None:
        args.judge = "judge_v1"
    if len(args.target) > 1:
        raise SystemExit("absolute mode takes one --target; pass --mode elo for multi-target sweeps")
    target = parse_target(args.target[0])
    tracker = CostTracker()
    started = time.time()

    slices = (
        ["intent", "dialogue", "reaction", "tier2-sim", "tier3-sim", "gaeilge"]
        if args.slice == "all"
        else [args.slice]
    )

    out = {
        "suite": args.suite,
        "split": args.split,
        "target": {"model": target.model, "base_url": target.base_url},
        "run_started_utc": datetime.now(timezone.utc).isoformat(),
        "slices": {},
    }

    for s in slices:
        try:
            data = run_slice(s, target, tracker, args)
        except FileNotFoundError as e:
            print(f"[{s}] skipped: {e}")
            continue
        out["slices"][s] = data
        summary = data["summary"]
        line = " ".join(f"{k}={v:.3f}" if isinstance(v, float) else f"{k}={v}"
                        for k, v in summary.items() if k != "slice")
        print(f"[{s}] {line}")

    elapsed = time.time() - started
    out["elapsed_seconds"] = elapsed
    out["cost"] = {
        "calls": tracker.calls,
        "prompt_tokens": tracker.prompt_tokens,
        "completion_tokens": tracker.completion_tokens,
        "usd": tracker.usd,
    }
    print(f"\ntotal: {tracker.summary()} in {elapsed:.1f}s")

    _ARTIFACTS_DIR.mkdir(parents=True, exist_ok=True)
    out_path = _ARTIFACTS_DIR / f"run_{slug(target.model)}_{args.slice}_{utc_stamp()}.json"
    out_path.write_text(json.dumps(out, indent=2, default=str) + "\n", encoding="utf-8")
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
