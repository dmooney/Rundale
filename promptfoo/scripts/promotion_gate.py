"""Production promotion gate for fully-local Rundale dialogue profiles.

The leaderboard answers "which candidate scored best?" This module answers the
stricter product question: "may this exact candidate/backend/hardware profile
become a shipped local-dialogue preset?"

It consumes promptfoo result JSON plus an immutable measurement receipt for the
parts promptfoo cannot observe itself (memory, production-parser soak, and
post-generation guard interventions). Missing evidence is a failure, never an
implicit pass.

Usage:
    python3 promptfoo/scripts/promotion_gate.py OUTPUT_DIR \
      --candidate 'model@http://127.0.0.1:8000/v1' \
      --evidence path/to/profile-evidence.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402
import leaderboard as lb  # noqa: E402
import report as rpt  # noqa: E402

DEFAULT_CONFIG = rb.CONFIG_DIR / "dialogue_promotion.json"
DEFAULT_PROFILES = rb.CONFIG_DIR / "local_hardware_profiles.json"


class EvidenceError(ValueError):
    """The promotion evidence is incomplete or internally inconsistent."""


def _wilson_lower(successes: int, total: int, z: float = 1.959963984540054) -> float:
    if total <= 0:
        return 0.0
    p = successes / total
    z2 = z * z
    denominator = 1.0 + z2 / total
    centre = p + z2 / (2.0 * total)
    spread = z * math.sqrt((p * (1.0 - p) + z2 / (4.0 * total)) / total)
    return max(0.0, (centre - spread) / denominator)


def _read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise EvidenceError(f"cannot read JSON {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise EvidenceError(f"{path} must contain a JSON object")
    return value


def _candidate_rows(out_dir: Path, slice_name: str, candidate: str) -> list[dict]:
    path = out_dir / f"{slice_name}.json"
    if not path.exists():
        return []
    return [
        row
        for row in rpt._results(_read_json(path))
        if rpt._candidate(row) == candidate
    ]


def _scoreable(row: dict, axes: list[str]) -> bool:
    named = rpt._named(row)
    return (
        not rpt._meta(row).get("error")
        and not named.get("judge_failure")
        and not named.get("bench_bug")
        and all(axis in named for axis in axes)
        and "overall" in named
    )


def _split_is(row: dict, required: str) -> bool:
    return rpt._meta(row).get("dataset_split") == required


def _hard_failure_counts(rows: list[dict], signals: list[str]) -> dict[str, int]:
    return {
        signal: sum(1 for row in rows if rpt._named(row).get(signal))
        for signal in signals
    }


def _mean(values: list[float]) -> float:
    return sum(values) / len(values) if values else 0.0


def _canonical_digest(value: Any) -> str:
    raw = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def _validate_provenance(evidence: dict[str, Any], evidence_path: Path) -> None:
    provenance = evidence.get("provenance")
    if not isinstance(provenance, dict):
        raise EvidenceError("evidence.provenance is required")
    resolved: dict[str, Path] = {}
    for source_name in ("soak_receipt", "soak_turns", "local_runner_artifact"):
        source = provenance.get(source_name)
        if not isinstance(source, dict) or not source.get("path") or not source.get(
            "sha256"
        ):
            raise EvidenceError(f"evidence.provenance.{source_name} is incomplete")
        path = (evidence_path.parent / source["path"]).resolve()
        try:
            actual_sha = hashlib.sha256(path.read_bytes()).hexdigest()
        except OSError as exc:
            raise EvidenceError(f"cannot read provenance source {path}: {exc}") from exc
        if actual_sha != source["sha256"]:
            raise EvidenceError(f"provenance hash mismatch for {source_name}")
        resolved[source_name] = path

    rows = [
        json.loads(line)
        for line in resolved["soak_turns"].read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    if any(
        row.get("candidate") != evidence.get("candidate")
        or row.get("dataset_merkle") != evidence.get("dataset_merkle")
        for row in rows
    ):
        raise EvidenceError("soak turns do not match the evidence candidate/dataset")
    recomputed_soak = {
        "calls": len(rows),
        "valid_responses": sum(
            1
            for row in rows
            if int(row.get("turns", 0)) == 1
            and int(row.get("contract_valid", 0)) == 1
        ),
    }
    recomputed_guards = {
        "turns": sum(int(row.get("turns", 0)) for row in rows),
        "interventions": sum(int(row.get("guard_interventions", 0)) for row in rows),
    }
    if evidence.get("reliability_soak") != recomputed_soak:
        raise EvidenceError("reliability_soak does not match raw soak turns")
    if evidence.get("guard_observation") != recomputed_guards:
        raise EvidenceError("guard_observation does not match raw soak turns")
    request_profiles = {
        json.dumps(profile, sort_keys=True)
        for row in rows
        for profile in row.get("request_profiles", [])
    }
    if len(request_profiles) != 1 or json.loads(
        next(iter(request_profiles), "{}")
    ) != evidence.get("request_profile"):
        raise EvidenceError("request_profile does not match raw soak turns")

    runner = _read_json(resolved["local_runner_artifact"])
    target = rb.parse_target(str(evidence.get("candidate")))
    runner_rows = [
        row
        for row in runner.get("rows", [])
        if isinstance(row, dict) and row.get("hf_repo") == target.model
    ]
    if not runner_rows:
        raise EvidenceError("local runner artifact has no rows for the candidate model")
    host = runner.get("host") or {}
    hardware = evidence.get("hardware") or {}
    peak = max(float(row.get("peak_ram_gb", 0.0)) for row in runner_rows)
    if not math.isclose(float(hardware.get("peak_memory_gb", 0.0)), peak):
        raise EvidenceError("hardware peak memory does not match local runner artifact")
    if not math.isclose(
        float(hardware.get("total_memory_gb", 0.0)),
        float(host.get("memory_gb", 0.0)),
    ):
        raise EvidenceError("hardware total memory does not match local runner artifact")


def _check(checks: list[dict], check_id: str, actual: Any, requirement: str, passed: bool) -> None:
    checks.append(
        {
            "id": check_id,
            "actual": actual,
            "requirement": requirement,
            "passed": bool(passed),
        }
    )


def _validate_evidence(
    evidence: dict[str, Any],
    *,
    candidate: str,
    profiles: dict[str, Any],
    manifest_merkle: str,
) -> tuple[dict[str, Any], dict[str, Any]]:
    if evidence.get("version") != 1:
        raise EvidenceError("evidence.version must be 1")
    if evidence.get("candidate") != candidate:
        raise EvidenceError("evidence candidate does not match --candidate")
    if evidence.get("dataset_merkle") != manifest_merkle:
        raise EvidenceError("evidence dataset_merkle does not match the frozen manifest")

    profile_id = evidence.get("hardware_profile_id")
    profile = (profiles.get("profiles") or {}).get(profile_id)
    if not profile:
        raise EvidenceError(f"unknown hardware_profile_id: {profile_id!r}")
    hardware = evidence.get("hardware")
    soak = evidence.get("reliability_soak")
    guards = evidence.get("guard_observation")
    if not isinstance(hardware, dict):
        raise EvidenceError("evidence.hardware is required")
    if not isinstance(soak, dict):
        raise EvidenceError("evidence.reliability_soak is required")
    if not isinstance(guards, dict):
        raise EvidenceError("evidence.guard_observation is required")
    if not isinstance(evidence.get("request_profile"), dict):
        raise EvidenceError("evidence.request_profile is required")
    if hardware.get("platform") != profile.get("platform"):
        raise EvidenceError("hardware platform does not match the registered profile")
    if hardware.get("memory_kind") != profile.get("memory_kind"):
        raise EvidenceError("hardware memory_kind does not match the registered profile")
    return profile, {"hardware": hardware, "soak": soak, "guards": guards}


def evaluate(
    out_dir: Path,
    *,
    candidate: str,
    evidence: dict[str, Any],
    config: dict[str, Any],
    profiles: dict[str, Any],
    manifest_merkle: str,
) -> dict[str, Any]:
    profile, measurements = _validate_evidence(
        evidence,
        candidate=candidate,
        profiles=profiles,
        manifest_merkle=manifest_merkle,
    )
    checks: list[dict] = []
    required_split = config["required_dataset_split"]

    dialogue_axes = list(rb.SLICE_META["dialogue"]["axes"])
    dialogue_rows = _candidate_rows(out_dir, "dialogue", candidate)
    request_profile = evidence["request_profile"]
    ready_cfg = config["player_ready"]
    critical_axes = ready_cfg["critical_axes"]
    ready = 0
    for row in dialogue_rows:
        named = rpt._named(row)
        if (
            _split_is(row, required_split)
            and _scoreable(row, dialogue_axes)
            and float(named["overall"]) >= ready_cfg["minimum_overall"]
            and all(float(named[axis]) >= ready_cfg["minimum_critical_axis"] for axis in critical_axes)
            and not any(named.get(signal) for signal in config["hard_failures"]["signals"])
        ):
            ready += 1
    ready_total = len(dialogue_rows)
    ready_rate = ready / ready_total if ready_total else 0.0
    ready_lower = _wilson_lower(ready, ready_total)
    _check(
        checks,
        "dialogue.minimum_records",
        ready_total,
        f">={ready_cfg['minimum_records']}",
        ready_total >= ready_cfg["minimum_records"],
    )
    _check(
        checks,
        "dialogue.holdout_only",
        sum(_split_is(row, required_split) for row in dialogue_rows),
        f"all {ready_total} rows stamped {required_split!r}",
        ready_total > 0 and all(_split_is(row, required_split) for row in dialogue_rows),
    )
    _check(
        checks,
        "dialogue.request_profile",
        sum(rpt._meta(row).get("request_profile") == request_profile for row in dialogue_rows),
        f"all {ready_total} rows match the live soak request profile",
        ready_total > 0
        and all(
            rpt._meta(row).get("request_profile") == request_profile
            for row in dialogue_rows
        ),
    )
    _check(
        checks,
        "dialogue.player_ready_rate",
        ready_rate,
        f">={ready_cfg['minimum_rate']}",
        ready_rate >= ready_cfg["minimum_rate"],
    )
    _check(
        checks,
        "dialogue.player_ready_wilson_lower_95",
        ready_lower,
        f">={ready_cfg['minimum_wilson_lower_95']}",
        ready_lower >= ready_cfg["minimum_wilson_lower_95"],
    )

    quality_scores = [
        float(rpt._named(row)["overall"])
        for row in dialogue_rows
        if _split_is(row, required_split) and _scoreable(row, dialogue_axes)
    ]
    quality_mean, quality_lo, quality_hi = lb._bootstrap_ci(quality_scores)
    quality_cfg = config["quality"]
    _check(
        checks,
        "dialogue.mean_quality",
        quality_mean,
        f">={quality_cfg['minimum_mean']}",
        quality_mean >= quality_cfg["minimum_mean"],
    )
    _check(
        checks,
        "dialogue.quality_bootstrap_lower_95",
        quality_lo,
        f">={quality_cfg['minimum_bootstrap_lower_95']}",
        quality_lo >= quality_cfg["minimum_bootstrap_lower_95"],
    )

    hard_counts = _hard_failure_counts(dialogue_rows, config["hard_failures"]["signals"])
    hard_total = sum(hard_counts.values())
    _check(
        checks,
        "dialogue.hard_failures",
        hard_counts,
        f"total<={config['hard_failures']['maximum_count']}",
        hard_total <= config["hard_failures"]["maximum_count"],
    )

    multiturn_axes = list(rb.SLICE_META["multiturn"]["axes"])
    multiturn_rows = _candidate_rows(out_dir, "multiturn", candidate)
    scoreable_mt = [
        row
        for row in multiturn_rows
        if _split_is(row, required_split) and _scoreable(row, multiturn_axes)
    ]
    mt_scores = [float(rpt._named(row)["overall"]) for row in scoreable_mt]
    mt_axis_means = {
        axis: _mean([float(rpt._named(row)[axis]) for row in scoreable_mt])
        for axis in config["multiturn"]["critical_axes"]
    }
    mt_cfg = config["multiturn"]
    _check(
        checks,
        "multiturn.minimum_records",
        len(multiturn_rows),
        f">={mt_cfg['minimum_records']}",
        len(multiturn_rows) >= mt_cfg["minimum_records"],
    )
    _check(
        checks,
        "multiturn.holdout_only",
        sum(_split_is(row, required_split) for row in multiturn_rows),
        f"all {len(multiturn_rows)} rows stamped {required_split!r}",
        bool(multiturn_rows) and all(_split_is(row, required_split) for row in multiturn_rows),
    )
    _check(
        checks,
        "multiturn.mean_quality",
        _mean(mt_scores),
        f">={mt_cfg['minimum_mean']}",
        bool(mt_scores) and _mean(mt_scores) >= mt_cfg["minimum_mean"],
    )
    _check(
        checks,
        "multiturn.critical_axis_means",
        mt_axis_means,
        f"every axis>={mt_cfg['minimum_axis_mean']}",
        bool(mt_axis_means)
        and all(value >= mt_cfg["minimum_axis_mean"] for value in mt_axis_means.values()),
    )

    perf_rows = _candidate_rows(out_dir, "perf", candidate)
    perf = rpt.aggregate_perf(perf_rows)
    perf_cfg = config["performance"]
    measured_perf_rows = [
        row
        for row in perf_rows
        if not bool((row.get("vars") or {}).get("perf_warmup"))
    ]
    _check(
        checks,
        "performance.request_profile",
        sum(
            rpt._meta(row).get("request_profile") == request_profile
            for row in measured_perf_rows
        ),
        f"all {len(measured_perf_rows)} measured rows match the live soak request profile",
        bool(measured_perf_rows)
        and all(
            rpt._meta(row).get("request_profile") == request_profile
            for row in measured_perf_rows
        ),
    )
    _check(
        checks,
        "performance.holdout_only",
        sum(_split_is(row, required_split) for row in measured_perf_rows),
        f"all {len(measured_perf_rows)} measured rows stamped {required_split!r}",
        bool(measured_perf_rows)
        and all(_split_is(row, required_split) for row in measured_perf_rows),
    )
    _check(
        checks,
        "performance.minimum_measurements",
        perf["n_ok"],
        f">={perf_cfg['minimum_measurements']}",
        perf["n_ok"] >= perf_cfg["minimum_measurements"],
    )
    _check(
        checks,
        "performance.minimum_cold_measurements",
        perf["cold_n_ok"],
        f">={perf_cfg['minimum_cold_measurements']}",
        perf["cold_n_ok"] >= perf_cfg["minimum_cold_measurements"],
    )
    _check(
        checks,
        "performance.minimum_warm_measurements",
        perf["warm_n_ok"],
        f">={perf_cfg['minimum_warm_measurements']}",
        perf["warm_n_ok"] >= perf_cfg["minimum_warm_measurements"],
    )
    for check_id, key, comparator, threshold in (
        (
            "performance.cold_ttft_p95_ms",
            "cold_ttft_p95_ms",
            "<=",
            perf_cfg["maximum_cold_ttft_p95_ms"],
        ),
        (
            "performance.warm_ttft_p95_ms",
            "warm_ttft_p95_ms",
            "<=",
            perf_cfg["maximum_warm_ttft_p95_ms"],
        ),
        (
            "performance.tokens_per_second_p50",
            "tokens_per_sec_p50",
            ">=",
            perf_cfg["minimum_tokens_per_second_p50"],
        ),
        (
            "performance.cold_completion_p95_ms",
            "cold_latency_p95_ms",
            "<=",
            perf_cfg["maximum_cold_completion_p95_ms"],
        ),
        (
            "performance.warm_completion_p95_ms",
            "warm_latency_p95_ms",
            "<=",
            perf_cfg["maximum_warm_completion_p95_ms"],
        ),
        ("performance.error_rate", "error_rate", "<=", perf_cfg["maximum_error_rate"]),
    ):
        actual = perf.get(key)
        passed = (
            perf["n_ok"] >= perf_cfg["minimum_measurements"]
            and actual is not None
            and (key == "error_rate" or actual > 0)
            and (actual <= threshold if comparator == "<=" else actual >= threshold)
        )
        _check(checks, check_id, actual, f"{comparator}{threshold}", passed)

    soak = measurements["soak"]
    soak_calls = int(soak.get("calls", 0))
    valid_responses = int(soak.get("valid_responses", 0))
    valid_rate = valid_responses / soak_calls if soak_calls else 0.0
    rel_cfg = config["reliability"]
    _check(
        checks,
        "reliability.minimum_soak_calls",
        soak_calls,
        f">={rel_cfg['minimum_soak_calls']}",
        soak_calls >= rel_cfg["minimum_soak_calls"],
    )
    _check(
        checks,
        "reliability.valid_response_rate",
        valid_rate,
        f">={rel_cfg['minimum_valid_response_rate']}",
        soak_calls > 0
        and valid_responses <= soak_calls
        and valid_rate >= rel_cfg["minimum_valid_response_rate"],
    )

    guards = measurements["guards"]
    observed_turns = int(guards.get("turns", 0))
    interventions = int(guards.get("interventions", 0))
    guard_rate = interventions / observed_turns if observed_turns else 1.0
    guard_cfg = config["guards"]
    _check(
        checks,
        "guards.minimum_observed_turns",
        observed_turns,
        f">={guard_cfg['minimum_observed_turns']}",
        observed_turns >= guard_cfg["minimum_observed_turns"],
    )
    _check(
        checks,
        "guards.intervention_rate",
        guard_rate,
        f"<={guard_cfg['maximum_intervention_rate']}",
        observed_turns > 0
        and interventions <= observed_turns
        and guard_rate <= guard_cfg["maximum_intervention_rate"],
    )

    hardware = measurements["hardware"]
    total_memory = float(hardware.get("total_memory_gb", 0.0))
    peak_memory = float(hardware.get("peak_memory_gb", 0.0))
    memory_utilization = peak_memory / total_memory if total_memory else 1.0
    _check(
        checks,
        "hardware.registered_memory_range",
        total_memory,
        (
            f"{profile['minimum_total_memory_gb']}.."
            f"{profile['maximum_total_memory_gb']} GiB"
        ),
        profile["minimum_total_memory_gb"]
        <= total_memory
        <= profile["maximum_total_memory_gb"],
    )
    _check(
        checks,
        "hardware.memory_utilization",
        memory_utilization,
        f"<={config['resources']['maximum_memory_utilization']}",
        total_memory > 0
        and peak_memory > 0
        and memory_utilization <= config["resources"]["maximum_memory_utilization"],
    )

    return {
        "version": 1,
        "candidate": candidate,
        "hardware_profile_id": evidence["hardware_profile_id"],
        "request_profile": request_profile,
        "dataset_merkle": manifest_merkle,
        "promotion_policy_sha256": _canonical_digest(
            {"config": config, "profiles": profiles}
        ),
        "passed": all(check["passed"] for check in checks),
        "metrics": {
            "player_ready": {
                "successes": ready,
                "records": ready_total,
                "rate": ready_rate,
                "wilson_lower_95": ready_lower,
            },
            "dialogue_quality": {
                "mean": quality_mean,
                "bootstrap_ci95": [quality_lo, quality_hi],
            },
            "multiturn": {
                "mean": _mean(mt_scores),
                "axis_means": mt_axis_means,
            },
            "hard_failures": hard_counts,
            "performance": perf,
            "reliability": {
                "calls": soak_calls,
                "valid_responses": valid_responses,
                "valid_response_rate": valid_rate,
            },
            "guards": {
                "turns": observed_turns,
                "interventions": interventions,
                "intervention_rate": guard_rate,
            },
            "hardware": {
                **hardware,
                "memory_utilization": memory_utilization,
            },
        },
        "checks": checks,
    }


def _render_markdown(result: dict[str, Any]) -> str:
    verdict = "PASS" if result["passed"] else "FAIL"
    lines = [
        "# Local dialogue promotion receipt",
        "",
        f"- Verdict: **{verdict}**",
        f"- Candidate: `{result['candidate']}`",
        f"- Hardware profile: `{result['hardware_profile_id']}`",
        f"- Dataset merkle: `{result['dataset_merkle']}`",
        f"- Promotion policy: `{result['promotion_policy_sha256']}`",
        "",
        "| Check | Actual | Requirement | Result |",
        "| --- | --- | --- | --- |",
    ]
    for check in result["checks"]:
        actual = json.dumps(check["actual"], sort_keys=True)
        lines.append(
            f"| `{check['id']}` | `{actual}` | `{check['requirement']}` | "
            f"{'PASS' if check['passed'] else 'FAIL'} |"
        )
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--profiles", type=Path, default=DEFAULT_PROFILES)
    parser.add_argument("--receipt-dir", type=Path)
    args = parser.parse_args(argv)

    try:
        config = _read_json(args.config)
        profiles = _read_json(args.profiles)
        evidence = _read_json(args.evidence)
        _validate_provenance(evidence, args.evidence.resolve())
        manifest = _read_json(rb.V2_DIR / "MANIFEST.json")
        result = evaluate(
            args.output_dir,
            candidate=args.candidate,
            evidence=evidence,
            config=config,
            profiles=profiles,
            manifest_merkle=manifest["merkle_root_sha256"],
        )
    except EvidenceError as exc:
        print(f"promotion evidence error: {exc}", file=sys.stderr)
        return 2

    receipt_dir = args.receipt_dir or args.output_dir
    receipt_dir.mkdir(parents=True, exist_ok=True)
    (receipt_dir / "promotion.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    (receipt_dir / "promotion.md").write_text(_render_markdown(result), encoding="utf-8")
    print(_render_markdown(result))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
