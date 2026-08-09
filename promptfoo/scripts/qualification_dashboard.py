#!/usr/bin/env python3
"""Build the cloud-dialogue qualification dashboard from immutable receipts.

This is a derived view: raw production-soak and promptfoo performance artifacts
remain the source of truth.  Re-running this script never mutates those paid
receipts and never invents a quality score for a deterministically rejected
candidate.
"""

from __future__ import annotations

import hashlib
import json
import re
import statistics
import sys
from pathlib import Path
from typing import Any

PF = Path(__file__).resolve().parents[1]
REPO = PF.parent
sys.path.insert(0, str(PF / "scripts"))
import report as rpt  # noqa: E402
from soak_dialogue import QUESTIONS  # noqa: E402

DEFAULT_RUNS = REPO / "docs" / "proofs" / "cloud-dialogue-qualification" / "runs"
DEFAULT_OUTPUT = PF / "leaderboard" / "dialogue-qualification.json"
DEFAULT_CALLS_OUTPUT = PF / "bench-site" / "public" / "qualification-calls"
POLICY = json.loads((PF / "config" / "cloud_dialogue_screening.json").read_text())


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _artifact(path: Path) -> dict[str, str]:
    return {
        "path": str(path.relative_to(REPO)),
        "sha256": _sha256(path),
    }


def _model(candidate: str) -> str:
    return candidate.split("@", 1)[0]


def _model_family(model: str) -> str:
    lowered = model.lower().lstrip("~")
    if "/" in lowered:
        vendor = lowered.split("/", 1)[0]
        return {"moonshotai": "moonshot"}.get(vendor, vendor)
    for prefix, family in (
        ("gpt-", "openai"), ("o1", "openai"), ("o3", "openai"),
        ("claude-", "anthropic"), ("gemini-", "google"),
        ("deepseek-", "deepseek"), ("kimi-", "moonshot"),
    ):
        if lowered.startswith(prefix):
            return family
    return lowered.split("-", 1)[0]


def _jsonl(path: Path) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def _question(row: dict[str, Any]) -> str:
    if isinstance(row.get("question"), str):
        return row["question"]
    question_id = int(row.get("question_id", -1))
    return QUESTIONS[question_id] if 0 <= question_id < len(QUESTIONS) else ""


def _preflight_calls(path: Path, data: dict[str, Any]) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    turns_meta = data.get("turns_artifact") or {}
    turns_path = path.parent / str(turns_meta.get("path", ""))
    conventional_turns = path.with_suffix(".turns.jsonl")
    if not turns_path.is_file() and conventional_turns.is_file():
        expected_sha = turns_meta.get("sha256")
        if expected_sha and _sha256(conventional_turns) == expected_sha:
            turns_path = conventional_turns
    turns = _jsonl(turns_path) if turns_path.is_file() else []
    calls_path = path.with_name(f"{path.stem}.calls.jsonl")
    raw_calls = _jsonl(calls_path) if calls_path.is_file() else []
    sources = [_artifact(turns_path)] if turns_path.is_file() else []
    if calls_path.is_file():
        sources.append(_artifact(calls_path))

    calls: list[dict[str, Any]] = []
    for index, turn in enumerate(turns):
        raw = raw_calls[index] if index < len(raw_calls) else {}
        profile = (turn.get("request_profiles") or [data.get("request_profile") or {}])[0]
        calls.append({
            "id": f"preflight-{int(turn.get('attempt', index)) + 1}",
            "kind": "preflight",
            "label": f"Preflight call {int(turn.get('attempt', index)) + 1}",
            "phase": "preflight",
            "question": _question(turn),
            "npc": turn.get("npc"),
            "request": {
                "system": raw.get("gen_ai.prompt.system"),
                "user": raw.get("gen_ai.prompt"),
                "model": raw.get("gen_ai.request.model") or profile.get("model"),
                "max_tokens": raw.get("gen_ai.request.max_tokens") or profile.get("max_tokens"),
                "temperature": raw.get("gen_ai.request.temperature") or profile.get("temperature"),
                "frequency_penalty": profile.get("frequency_penalty"),
                "reasoning_effort": profile.get("reasoning_effort"),
                "json_mode": profile.get("json_mode"),
            },
            "response": raw.get("gen_ai.completion") or "\n".join(
                str(line.get("text", "")) for line in turn.get("response_lines", []) if isinstance(line, dict)
            ) or None,
            "metrics": {
                "elapsed_ms": raw.get("gen_ai.response.duration_ms") or turn.get("elapsed_ms"),
                "ttft_ms": raw.get("parish.ttft_ms"),
                "stream_chunks": raw.get("gen_ai.usage.output_tokens"),
            },
            "outcome": {
                "contract_valid": bool(turn.get("contract_valid")),
                "parse_dispositions": turn.get("parse_dispositions", []),
                "guard_reasons": turn.get("guard_reasons", []),
                "transport_error": turn.get("transport_error"),
            },
        })
    return calls, sources


def _parse_sse(path: Path) -> dict[str, Any]:
    content: list[str] = []
    finish_reason = None
    native_finish_reason = None
    usage: dict[str, Any] = {}
    provider = None
    model = None
    error = None
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("data: ") or line == "data: [DONE]":
            continue
        payload = json.loads(line[6:])
        if payload.get("error"):
            error = payload["error"]
            continue
        provider = payload.get("provider") or provider
        model = payload.get("model") or model
        choice = (payload.get("choices") or [{}])[0]
        content.append(str((choice.get("delta") or {}).get("content") or ""))
        finish_reason = choice.get("finish_reason") or finish_reason
        native_finish_reason = choice.get("native_finish_reason") or native_finish_reason
        usage = payload.get("usage") or usage
    return {
        "response": "".join(content) or None,
        "finish_reason": finish_reason,
        "native_finish_reason": native_finish_reason,
        "usage": usage,
        "provider": provider,
        "model": model,
        "error": error,
    }


def _diagnostic_calls(path: Path) -> tuple[dict[str, Any] | None, list[dict[str, Any]], list[dict[str, str]]]:
    diagnosis_path = path.with_suffix(".diagnosis.json")
    if not diagnosis_path.is_file():
        return None, [], []
    diagnosis = json.loads(diagnosis_path.read_text(encoding="utf-8"))
    source_meta = diagnosis.get("source_request") or {}
    source_path = path.parent / str(source_meta.get("artifact", ""))
    source_rows = _jsonl(source_path) if source_path.is_file() else []
    request_id = source_meta.get("request_id")
    source = next((row for row in source_rows if row.get("parish.request_id") == request_id), {})
    sources = [_artifact(diagnosis_path)]
    if source_path.is_file():
        sources.append(_artifact(source_path))
    calls = []
    for index, replay in enumerate(diagnosis.get("replays", []), 1):
        replay_path = path.parent / str(replay["artifact"])
        parsed = _parse_sse(replay_path)
        sources.append(_artifact(replay_path))
        usage = parsed["usage"]
        calls.append({
            "id": f"diagnostic-{index}",
            "kind": "diagnostic",
            "label": f"Exact diagnostic replay {index}",
            "phase": "profile diagnosis",
            "question": "Exact replay of production request",
            "request": {
                "system": source.get("gen_ai.prompt.system"),
                "user": source.get("gen_ai.prompt"),
                "model": parsed["model"] or source.get("gen_ai.request.model"),
                "max_tokens": diagnosis.get("request_profile", {}).get("max_tokens"),
                "temperature": source.get("gen_ai.request.temperature"),
                "reasoning_effort": diagnosis.get("request_profile", {}).get("reasoning_effort"),
                "json_mode": True,
            },
            "response": parsed["response"],
            "metrics": {
                "finish_reason": parsed["finish_reason"],
                "native_finish_reason": parsed["native_finish_reason"],
                "prompt_tokens": usage.get("prompt_tokens"),
                "completion_tokens": usage.get("completion_tokens"),
                "reasoning_tokens": (usage.get("completion_tokens_details") or {}).get("reasoning_tokens"),
                "cost_usd": usage.get("cost"),
            },
            "outcome": {"error": parsed["error"]},
            "artifact": _artifact(replay_path),
        })
    return diagnosis, calls, sources


def _judgment_row(
    date_dir: Path,
    slug: str,
    candidate_model: str,
) -> tuple[dict[str, Any] | None, list[dict[str, Any]], list[dict[str, str]]]:
    policy = POLICY["judgment"]
    configured = {judge["id"]: judge for judge in policy["judges"]}
    configured_by_profile = {
        (judge["model"], judge["provider"], judge["reasoning_effort"]): judge
        for judge in policy["judges"]
    }
    receipts: dict[str, tuple[Path, dict[str, Any], Path, Path]] = {}
    for path in sorted(date_dir.glob(f"{slug}-judgment-*.json"), reverse=True):
        if path.name.endswith((".raw.json", ".bundle.json", ".diagnosis.json")):
            continue
        data = json.loads(path.read_text(encoding="utf-8"))
        judge = data.get("judge") or {}
        profile = configured.get(judge.get("id")) or configured_by_profile.get((
            judge.get("model"), judge.get("provider"), judge.get("reasoning_effort")
        ))
        if not profile or profile["id"] in receipts:
            continue
        source = data.get("source") or {}
        bundle_path = REPO / str(source.get("bundle", ""))
        raw_path = REPO / str(source.get("raw", ""))
        if not bundle_path.is_file() or not raw_path.is_file():
            continue
        bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
        if bundle.get("version") != 2:
            continue
        receipts[profile["id"]] = (path, data, bundle_path, raw_path)

    if not receipts:
        return None, [], []

    candidate_family = _model_family(candidate_model)
    judgments: list[dict[str, Any]] = []
    calls: list[dict[str, Any]] = []
    sources: list[dict[str, str]] = []
    for judge_id in configured:
        if judge_id not in receipts:
            continue
        path, data, bundle_path, raw_path = receipts[judge_id]
        profile = configured[judge_id]
        bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
        judge = data["judge"] | {
            "id": judge_id,
            "family": profile["family"],
        }
        bundle_items = {item["prompt_id"]: item for item in bundle.get("items", [])}
        for index, item in enumerate(data.get("items", []), 1):
            evidence = bundle_items.get(item.get("prompt_id"), {})
            calls.append({
                "id": f"judgment-{judge_id}-{index}",
                "kind": "judgment",
                "label": f"{judge_id} judgment {index}",
                "phase": "quality judgment",
                "question": item.get("prompt_id"),
                "request": {
                    "model": judge.get("model"),
                    "reasoning_effort": judge.get("reasoning_effort"),
                    "max_tokens": bundle.get("judge_profile", {}).get("max_tokens"),
                    "system": evidence.get("prompt"),
                    "user": evidence.get("response"),
                },
                "response": json.dumps({
                    "axes": item.get("axes"),
                    "overall": item.get("overall"),
                    "rationales": item.get("rationales"),
                    "flags": item.get("flags"),
                }, ensure_ascii=False, indent=2),
                "metrics": {},
                "outcome": {
                    "contract_valid": not bool((item.get("flags") or {}).get("bench_bug")),
                    "guard_reasons": [],
                },
            })
        quality = data["quality"]
        sample = dict(data["sample"])
        sample.setdefault("judged_items", sample["items"])
        sample.setdefault("unusable_outputs", sample["items"] - sample["judged_items"])
        eligible = not policy.get("exclude_same_family", True) or profile["family"] != candidate_family
        judgments.append({
            "id": judge_id,
            "family": profile["family"],
            "eligible": eligible,
            "exclusion_reason": None if eligible else "same-family judge",
            "overall": quality["overall"],
            "axes": quality["axes"],
            "hard_failures": quality["hard_failures"],
            "pass": quality["pass"],
            "sample": sample,
            "judge": judge,
            "artifact": _artifact(path),
        })
        sources.extend((_artifact(path), _artifact(bundle_path), _artifact(raw_path)))

    eligible = [item for item in judgments if item["eligible"]]
    required = int(policy["minimum_independent_judges"])
    complete = len(eligible) >= required
    axes = {
        axis: round(statistics.median(item["axes"][axis] for item in eligible), 4)
        for axis in eligible[0]["axes"]
    } if eligible else {}
    overall = round(statistics.median(item["overall"] for item in eligible), 4) if eligible else None
    pass_votes = sum(bool(item["pass"]) for item in eligible)
    fail_votes = len(eligible) - pass_votes
    spread = (
        round(max(item["overall"] for item in eligible) - min(item["overall"] for item in eligible), 4)
        if len(eligible) > 1 else None
    )
    needs_adjudication = complete and (
        (pass_votes > 0 and fail_votes > 0)
        or (spread is not None and spread > policy["maximum_overall_spread_without_adjudication"])
    )
    consensus_pass = complete and not needs_adjudication and pass_votes >= required
    hard_failure_keys = sorted({
        key for item in eligible for key in item["hard_failures"]
    })
    hard_failures = {
        key: max((item["hard_failures"].get(key, 0) for item in eligible), default=0)
        for key in hard_failure_keys
    }
    sample = {
        "items": max((item["sample"]["items"] for item in eligible), default=0),
        "judged_items": min((item["sample"]["judged_items"] for item in eligible), default=0),
        "unusable_outputs": max((item["sample"]["unusable_outputs"] for item in eligible), default=0),
    }
    summary = {
        "method": policy["consensus_method"],
        "candidate_family": candidate_family,
        "overall": overall,
        "axes": axes,
        "hard_failures": hard_failures,
        "pass": consensus_pass,
        "complete": complete,
        "needs_adjudication": needs_adjudication,
        "overall_spread": spread,
        "votes": {
            "eligible": len(eligible),
            "required": required,
            "pass": pass_votes,
            "fail": fail_votes,
            "self_excluded": len(judgments) - len(eligible),
        },
        "sample": sample,
        "judges": judgments,
        "cost_usd": round(sum(
            float((item["judge"].get("cost_usd") or 0.0)) for item in judgments
        ), 8),
    }
    return summary, calls, sources


def _preflight_row(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]], list[dict[str, str]]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    soak = data["reliability_soak"]
    guards = data["guard_observation"]
    calls = int(soak["calls"])
    turns = int(guards["turns"])
    calls_payload, sources = _preflight_calls(path, data)
    return {
        "candidate": data["candidate"],
        "calls": calls,
        "valid": int(soak["valid_responses"]),
        "guard_interventions": int(guards["interventions"]),
        "guard_rate": int(guards["interventions"]) / turns if turns else None,
        "request_profile": data.get("request_profile"),
        "artifact": _artifact(path),
    }, calls_payload, sources


def _partial_row(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]], list[dict[str, str]]] | None:
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    if not rows:
        return None
    turns = sum(int(row.get("turns", 0)) for row in rows)
    elapsed = [int(row["elapsed_ms"]) for row in rows if row.get("elapsed_ms") is not None]
    calls = [{
        "id": f"preflight-{int(row.get('attempt', index)) + 1}",
        "kind": "preflight",
        "label": f"Preflight call {int(row.get('attempt', index)) + 1}",
        "phase": "preflight",
        "question": _question(row),
        "npc": row.get("npc"),
        "request": ((row.get("request_profiles") or [{}])[0]),
        "response": "\n".join(str(line.get("text", "")) for line in row.get("response_lines", []) if isinstance(line, dict)) or None,
        "metrics": {"elapsed_ms": row.get("elapsed_ms")},
        "outcome": {
            "contract_valid": bool(row.get("contract_valid")),
            "parse_dispositions": row.get("parse_dispositions", []),
            "guard_reasons": row.get("guard_reasons", []),
            "transport_error": row.get("transport_error"),
        },
    } for index, row in enumerate(rows)]
    return {
        "candidate": rows[0]["candidate"],
        "calls": len(rows),
        "valid": sum(
            int(row.get("turns", 0)) == 1 and int(row.get("contract_valid", 0)) == 1
            for row in rows
        ),
        "guard_interventions": sum(int(row.get("guard_interventions", 0)) for row in rows),
        "guard_rate": (
            sum(int(row.get("guard_interventions", 0)) for row in rows) / turns
            if turns
            else None
        ),
        "elapsed_min_ms": min(elapsed) if elapsed else None,
        "elapsed_max_ms": max(elapsed) if elapsed else None,
        "artifact": _artifact(path),
    }, calls, [_artifact(path)]


def _perf_row(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]], list[dict[str, str]]] | None:
    rows = rpt._results(json.loads(path.read_text(encoding="utf-8")))
    if not rows:
        return None
    agg = rpt.aggregate_perf(rows)
    calls = []
    for index, row in enumerate(rows, 1):
        response = row.get("response") or {}
        metadata = response.get("metadata") or {}
        record = metadata.get("record") or {}
        profile = metadata.get("request_profile") or {}
        token_usage = response.get("tokenUsage") or row.get("tokenUsage") or {}
        calls.append({
            "id": f"performance-{index}",
            "kind": "performance",
            "label": f"Performance call {index}",
            "phase": metadata.get("perf_cache_state"),
            "question": record.get("id"),
            "request": {
                "system": record.get("system"),
                "user": record.get("user"),
                "model": profile.get("model") or metadata.get("model"),
                "max_tokens": profile.get("max_tokens"),
                "temperature": profile.get("temperature"),
                "frequency_penalty": profile.get("frequency_penalty"),
                "reasoning_effort": profile.get("reasoning_effort"),
                "json_mode": profile.get("json_mode"),
            },
            "response": response.get("output"),
            "metrics": {
                "elapsed_ms": row.get("latencyMs"),
                "ttft_ms": metadata.get("ttft_ms"),
                "tokens_per_second": metadata.get("tokens_per_second"),
                "prompt_tokens": token_usage.get("prompt"),
                "completion_tokens": token_usage.get("completion"),
                "reasoning_tokens": (token_usage.get("completionDetails") or {}).get("reasoning"),
                "cost_usd": response.get("cost") if response.get("cost") is not None else row.get("cost"),
            },
            "outcome": {"error": response.get("error") or row.get("failureReason")},
        })
    return {
        "candidate": rpt._candidate(rows[0]),
        "measurements": int(agg["n_ok"]) + int(agg["n_error"]),
        "cold_measurements": int(agg["cold_n_ok"]) + int(agg["cold_n_error"]),
        "warm_measurements": int(agg["warm_n_ok"]) + int(agg["warm_n_error"]),
        "error_rate": agg["error_rate"],
        "cold_ttft_p95_ms": agg["cold_ttft_p95_ms"],
        "warm_ttft_p95_ms": agg["warm_ttft_p95_ms"],
        "cold_completion_p95_ms": agg["cold_latency_p95_ms"],
        "warm_completion_p95_ms": agg["warm_latency_p95_ms"],
        "tokens_per_second_p50": agg["tokens_per_sec_p50"],
        "artifact": _artifact(path),
    }, calls, [_artifact(path)]


def _performance_failures(perf: dict[str, Any]) -> list[str]:
    cfg = POLICY["performance"]
    failed: list[str] = []
    if perf["measurements"] < cfg["minimum_measurements"]:
        failed.append("insufficient performance measurements")
    if perf["cold_measurements"] < cfg["minimum_cold_measurements"]:
        failed.append("insufficient cold measurements")
    if perf["warm_measurements"] < cfg["minimum_warm_measurements"]:
        failed.append("insufficient warm measurements")
    if perf["error_rate"] is None or perf["error_rate"] > cfg["maximum_error_rate"]:
        failed.append("request error rate")
    if perf["warm_ttft_p95_ms"] is None or perf["warm_completion_p95_ms"] is None:
        failed.append("missing latency evidence")
    return failed


def _rank_survivors(runs: list[dict[str, Any]]) -> None:
    ranking = POLICY["performance"]["ranking"]
    ttft_weight = float(ranking["warm_ttft_weight"])
    completion_weight = float(ranking["warm_completion_weight"])
    eligible = []
    for run in runs:
        perf = run.get("performance")
        if run["status"] not in {
            "needs_judgment", "needs_adjudication", "qualified", "quality_rejected"
        } or not perf:
            continue
        ttft = perf.get("warm_ttft_p95_ms")
        completion = perf.get("warm_completion_p95_ms")
        if ttft is None or completion is None:
            continue
        perf["speed_index_ms"] = ttft_weight * ttft + completion_weight * completion
        eligible.append(run)

    eligible.sort(key=lambda run: (
        run["performance"]["speed_index_ms"],
        run["performance"]["warm_completion_p95_ms"],
        run["run_id"],
    ))
    cohort_size = len(eligible)
    for rank, run in enumerate(eligible, 1):
        run["performance"]["speed_rank"] = rank
        run["performance"]["speed_cohort_size"] = cohort_size
    judged = [
        run for run in eligible
        if run.get("judgment") and run["judgment"].get("complete")
        and not run["judgment"].get("needs_adjudication")
    ]
    judged.sort(key=lambda run: (-run["judgment"]["overall"], run["run_id"]))
    for rank, run in enumerate(judged, 1):
        run["judgment"]["quality_rank"] = rank
        run["judgment"]["quality_cohort_size"] = len(judged)
    for run in eligible:
        speed = run["performance"]
        judgment = run.get("judgment")
        if judgment and judgment.get("needs_adjudication"):
            run["reason"] = (
                f"judge disagreement requires adjudication; "
                f"speed rank #{speed['speed_rank']} of {cohort_size}"
            )
        elif judgment and judgment.get("complete"):
            verdict = "quality passed" if judgment["pass"] else "quality screen failed"
            run["reason"] = (
                f"{verdict}; quality rank #{judgment['quality_rank']} of {len(judged)}; "
                f"speed rank #{speed['speed_rank']} of {cohort_size}"
            )
        else:
            completed = (judgment or {}).get("votes", {}).get("eligible", 0)
            required = POLICY["judgment"]["minimum_independent_judges"]
            run["reason"] = (
                f"awaiting independent judges ({completed}/{required}); "
                f"cloud speed rank #{speed['speed_rank']} of {cohort_size}"
            )


def _decision(
    preflight: dict[str, Any],
    perf: dict[str, Any] | None,
    diagnosis: dict[str, Any] | None = None,
    judgment: dict[str, Any] | None = None,
) -> tuple[str, str, str]:
    if diagnosis and diagnosis.get("classification") == "invalid_profile":
        return "invalid_profile", "configuration", "superseded: insufficient completion budget"
    if preflight.get("partial"):
        # A partial run may still be a conclusive reject. Preflight requires
        # 100% validity, so one invalid response can never recover; likewise,
        # interventions already above 10% of the fixed 12-call denominator
        # cannot be diluted back under the gate. This is the early-stop rule
        # used to avoid unnecessary paid calls.
        if preflight["valid"] != preflight["calls"]:
            return "rejected", "preflight", "structural reliability (early stop)"
        if preflight["guard_interventions"] / 12 > POLICY["guards"]["maximum_intervention_rate"]:
            return "rejected", "preflight", "guard intervention rate (early stop)"
        required_calls = POLICY["preflight"]["calls"]
        return "stopped", "preflight", f"stopped after {preflight['calls']}/{required_calls} calls"
    if preflight["valid"] != preflight["calls"]:
        return "rejected", "preflight", "structural reliability"
    if (preflight["guard_rate"] or 0.0) > POLICY["guards"]["maximum_intervention_rate"]:
        return "rejected", "preflight", "guard intervention rate"
    if perf is None:
        return "needs_performance", "performance", "preflight passed"
    failures = _performance_failures(perf)
    if failures:
        return "rejected", "performance", ", ".join(failures)
    if judgment is not None:
        if not judgment.get("complete"):
            return "needs_judgment", "judgment", "independent judge panel incomplete"
        if judgment.get("needs_adjudication"):
            return "needs_adjudication", "judgment", "judge disagreement requires adjudication"
        if judgment["pass"]:
            return "qualified", "judgment", "quality screen passed"
        return "quality_rejected", "judgment", "quality screen failed"
    return "needs_judgment", "judgment", "qualified for quality judgment"


def _build(runs_root: Path) -> tuple[dict[str, Any], dict[str, bytes]]:
    runs: list[dict[str, Any]] = []
    call_feeds: dict[str, bytes] = {}
    for date_dir in sorted(path for path in runs_root.glob("*") if path.is_dir()):
        preflights: list[tuple[str, dict[str, Any], list[dict[str, Any]], list[dict[str, str]]]] = []
        for path in sorted(date_dir.glob("*-preflight.json")):
            slug = path.name.removesuffix("-preflight.json")
            preflight, calls, sources = _preflight_row(path)
            preflights.append((slug, preflight, calls, sources))
        for path in sorted(date_dir.glob("*-preflight.partial.jsonl")):
            slug = path.name.removesuffix("-preflight.partial.jsonl")
            partial = _partial_row(path)
            if partial:
                row, calls, sources = partial
                row["partial"] = True
                preflights.append((slug, row, calls, sources))

        for slug, preflight, calls, sources in preflights:
            perf_paths = sorted(date_dir.glob(f"{slug}-perf*.json"))
            # Expanded panels supersede shorter same-run panels for the derived view.
            perf_path = next((p for p in perf_paths if "expanded" in p.stem), None)
            if perf_path is None and perf_paths:
                perf_path = perf_paths[-1]
            perf_result = _perf_row(perf_path) if perf_path else None
            perf, perf_calls, perf_sources = perf_result if perf_result else (None, [], [])
            diagnosis_path = date_dir / f"{slug}-preflight.json"
            diagnosis, diagnostic_calls, diagnostic_sources = _diagnostic_calls(diagnosis_path)
            judgment, judgment_calls, judgment_sources = _judgment_row(
                date_dir, slug, _model(preflight["candidate"])
            )
            calls.extend(diagnostic_calls)
            calls.extend(perf_calls)
            calls.extend(judgment_calls)
            sources.extend(diagnostic_sources)
            sources.extend(perf_sources)
            sources.extend(judgment_sources)
            status, stage, reason = _decision(preflight, perf, diagnosis, judgment)
            candidate = preflight["candidate"]
            run_id = f"{date_dir.name}/{slug}"
            call_filename = re.sub(r"[^a-zA-Z0-9._-]+", "__", run_id) + ".json"
            call_payload = {
                "version": 1,
                "run_id": run_id,
                "candidate": candidate,
                "diagnosis": diagnosis,
                "sources": sources,
                "calls": calls,
            }
            call_bytes = (json.dumps(call_payload, indent=2, sort_keys=True) + "\n").encode("utf-8")
            call_feeds[call_filename] = call_bytes
            runs.append(
                {
                    "run_id": run_id,
                    "tested_on": date_dir.name,
                    "candidate": candidate,
                    "model": _model(candidate),
                    "status": status,
                    "stage": stage,
                    "reason": reason,
                    "preflight": {k: v for k, v in preflight.items() if k not in {"candidate", "partial"}},
                    "performance": perf,
                    "judgment": judgment,
                    "calls": {
                        "count": len(calls),
                        "preflight": sum(call["kind"] == "preflight" for call in calls),
                        "diagnostic": sum(call["kind"] == "diagnostic" for call in calls),
                        "performance": sum(call["kind"] == "performance" for call in calls),
                        "judgment": sum(call["kind"] == "judgment" for call in calls),
                        "path": f"qualification-calls/{call_filename}",
                        "sha256": hashlib.sha256(call_bytes).hexdigest(),
                    },
                }
            )
    _rank_survivors(runs)
    counts = {status: sum(run["status"] == status for run in runs) for status in (
        "invalid_profile", "rejected", "stopped", "needs_performance", "needs_judgment",
        "needs_adjudication", "quality_rejected", "qualified"
    )}
    payload = {
        "version": 1,
        "source": "immutable cloud-dialogue qualification receipts",
        "policy": POLICY,
        "counts": counts,
        "runs": runs,
    }
    return payload, call_feeds


def build(runs_root: Path) -> dict[str, Any]:
    return _build(runs_root)[0]


def main(argv: list[str]) -> int:
    runs_root = Path(argv[1]).resolve() if len(argv) > 1 else DEFAULT_RUNS
    output = Path(argv[2]).resolve() if len(argv) > 2 else DEFAULT_OUTPUT
    payload, call_feeds = _build(runs_root)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    DEFAULT_CALLS_OUTPUT.mkdir(parents=True, exist_ok=True)
    for filename, content in call_feeds.items():
        (DEFAULT_CALLS_OUTPUT / filename).write_bytes(content)
    print(f"[qualification-dashboard] {len(payload['runs'])} runs -> {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
