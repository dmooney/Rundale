#!/usr/bin/env python3
"""Judge deterministic cloud-dialogue survivors from retained perf outputs.

Each profile contributes the same panel: six frozen production prompts with
three samples apiece (one cold and two warm). Paid judge responses are written
before validation and are never overwritten.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import sys
import threading
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

PF = Path(__file__).resolve().parents[1]
REPO = PF.parent
sys.path.insert(0, str(PF))
sys.path.insert(0, str(PF / "scripts"))
sys.path.insert(0, str(REPO / "rundale-bench"))

import grade  # noqa: E402
import qualification_dashboard as dashboard  # noqa: E402
import rb_common as rb  # noqa: E402
import report  # noqa: E402

RUBRIC = rb.load_rubric("judge_sonnet_v2")
SYSTEM_PROMPT = PF / "v2" / "rubrics" / "dialogue.system.md"
POLICY = json.loads((PF / "config" / "dialogue_promotion.json").read_text())
SCREENING_POLICY = json.loads((PF / "config" / "cloud_dialogue_screening.json").read_text())
JUDGES = SCREENING_POLICY["judgment"]["judges"]
# Kept as compatibility aliases for older callers. New code must select an
# explicit judge profile so the model family and paid receipt are unambiguous.
JUDGE_MODEL = JUDGES[0]["model"]
JUDGE_BASE_URL = JUDGES[0]["base_url"]
JUDGE_REASONING_EFFORT = JUDGES[0]["reasoning_effort"]
JUDGE_MAX_TOKENS = JUDGES[0]["max_tokens"]
WEIGHTS = {
    "character": 1.5,
    "mood_fidelity": 1.5,
    "grounding": 1.5,
    "brevity": 1.25,
    "repetition": 1.25,
    "responsiveness": 1.0,
    "authenticity": 1.0,
    "language": 0.75,
    "craft": 0.5,
}
HARD_FLAGS = ("fabricated", "degenerate_loop", "non_latin_detected", "refused")
CIRCUITS = {judge["id"]: threading.Event() for judge in JUDGES}


def _load_dotenv(path: Path) -> None:
    """Load project credentials without overriding explicit environment values."""

    if not path.is_file():
        return
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        if key and key not in os.environ:
            os.environ[key] = value


_load_dotenv(REPO / ".env")


def model_family(model: str) -> str:
    """Return the organisation-level family used for self-judge exclusion."""

    lowered = model.lower().lstrip("~")
    if "/" in lowered:
        vendor = lowered.split("/", 1)[0]
        aliases = {"moonshotai": "moonshot", "z-ai": "z-ai", "x-ai": "x-ai"}
        return aliases.get(vendor, vendor)
    for prefix, family in (
        ("gpt-", "openai"), ("o1", "openai"), ("o3", "openai"),
        ("claude-", "anthropic"), ("gemini-", "google"),
        ("deepseek-", "deepseek"), ("kimi-", "moonshot"),
    ):
        if lowered.startswith(prefix):
            return family
    return lowered.split("-", 1)[0]


def eligible_judges(run: dict[str, Any]) -> list[dict[str, Any]]:
    candidate_family = model_family(run["model"])
    if not SCREENING_POLICY["judgment"].get("exclude_same_family", True):
        return list(JUDGES)
    return [judge for judge in JUDGES if judge["family"] != candidate_family]


def _judge_slug(judge: dict[str, Any]) -> str:
    return re.sub(r"[^a-zA-Z0-9._-]+", "-", judge["id"]).strip("-")


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _write_once(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("xb") as handle:
            handle.write(data)
    except FileExistsError:
        if path.read_bytes() != data:
            raise RuntimeError(f"immutable artifact collision: {path}")


def _items(run: dict[str, Any]) -> list[dict[str, str]]:
    perf_path = REPO / run["performance"]["artifact"]["path"]
    rows = report._results(json.loads(perf_path.read_text(encoding="utf-8")))
    by_prompt: dict[str, list[dict[str, Any]]] = {}
    for row in rows:
        response = row.get("response") or {}
        metadata = response.get("metadata") or {}
        if metadata.get("perf_cache_state") == "warmup" or not response.get("output"):
            continue
        record = metadata.get("record") or {}
        prompt_id = str(record.get("id") or row.get("id"))
        by_prompt.setdefault(prompt_id, []).append(row)

    items: list[dict[str, str]] = []
    for prompt_id in sorted(by_prompt):
        rows_for_prompt = sorted(
            by_prompt[prompt_id],
            key=lambda row: (
                0 if ((row.get("response") or {}).get("metadata") or {}).get("perf_cache_state") == "cold" else 1,
                str(row.get("id", "")),
            ),
        )[:3]
        if len(rows_for_prompt) != 3:
            raise RuntimeError(f"{run['run_id']} {prompt_id}: expected 3 retained samples")
        for sample, row in enumerate(rows_for_prompt, 1):
            response = row["response"]
            record = (response.get("metadata") or {})["record"]
            items.append({
                "prompt_id": f"{prompt_id}/sample-{sample}",
                "prompt": (
                    f"SYSTEM PROMPT:\n{record.get('system', '')}\n\n"
                    f"USER PROMPT:\n{record.get('user', '')}"
                ),
                "response": grade.extract_dialogue_for_judging(str(response["output"])),
            })
    if len(items) != 18:
        raise RuntimeError(f"{run['run_id']}: expected 18 comparable items, got {len(items)}")
    return items


def _bundle(run: dict[str, Any], judge: dict[str, Any] | None = None) -> tuple[Path, dict[str, Any]]:
    judge = judge or JUDGES[0]
    payload = {
        "version": 2,
        "slice": "dialogue",
        "rubric_sha256": RUBRIC["rubric_sha256"],
        "score_range": RUBRIC["score_range"],
        "judge_profile": {
            "id": judge["id"],
            "model": judge["model"],
            "family": judge["family"],
            "provider": judge["provider"],
            "reasoning_effort": judge["reasoning_effort"],
            "max_tokens": judge["max_tokens"],
            "temperature": 0.0,
        },
        "sampling": {
            "unique_prompts": 6,
            "samples_per_prompt": 3,
            "judge_input": "full production system prompt plus dynamic user prompt",
        },
        "items": _items(run),
    }
    data = (json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode()
    slug = run["run_id"].split("/", 1)[1]
    date = run["tested_on"]
    path = dashboard.DEFAULT_RUNS / date / (
        f"{slug}-judgment-{_judge_slug(judge)}-{_sha(data)[:16]}.bundle.json"
    )
    _write_once(path, data)
    return path, payload


def _validate(bundle: dict[str, Any], result: dict[str, Any]) -> list[dict[str, Any]]:
    if result.get("rubric_sha256") != bundle["rubric_sha256"]:
        raise ValueError("judge rubric hash mismatch")
    expected = {item["prompt_id"] for item in bundle["items"]}
    items = result.get("items") or []
    if {item.get("prompt_id") for item in items} != expected or len(items) != len(expected):
        raise ValueError("judge did not score every prompt exactly once")
    axes = set(WEIGHTS)
    for item in items:
        values = item.get("axes") or {}
        flags = item.get("flags") or {}
        bench_bug = bool(flags.get("bench_bug"))
        if set(values) != axes:
            raise ValueError(f"axis mismatch for {item.get('prompt_id')}")
        allowed = {0} if bench_bug else {1, 2, 3, 4, 5}
        if any(value not in allowed for value in values.values()):
            raise ValueError(f"invalid axis score for {item.get('prompt_id')}")
    return items


def _aggregate(
    bundle: dict[str, Any],
    result: dict[str, Any],
    raw: dict[str, Any],
    judge: dict[str, Any] | None = None,
) -> dict[str, Any]:
    judge = judge or JUDGES[0]
    items = _validate(bundle, result)
    scored = [item for item in items if not (item.get("flags") or {}).get("bench_bug")]
    unusable_outputs = len(items) - len(scored)
    if not scored:
        raise ValueError("judge found no usable dialogue in the quality panel")
    axes = {
        axis: sum(item["axes"][axis] for item in scored) / len(scored)
        for axis in WEIGHTS
    }
    overall = sum(WEIGHTS[axis] * axes[axis] for axis in WEIGHTS) / sum(WEIGHTS.values())
    hard_failures = {
        flag: sum(bool((item.get("flags") or {}).get(flag)) for item in scored)
        for flag in HARD_FLAGS
    }
    hard_failures["unusable_output"] = unusable_outputs
    ready = POLICY["player_ready"]
    critical = ready["critical_axes"]
    quality_pass = (
        overall >= ready["minimum_overall"]
        and all(axes[axis] >= ready["minimum_critical_axis"] for axis in critical)
        and not any(hard_failures.values())
    )
    return {
        "version": 1,
        "judge": {
            "id": judge["id"],
            "model": judge["model"],
            "family": judge["family"],
            "provider": judge["provider"],
            "reasoning_effort": judge["reasoning_effort"],
            "rubric_sha256": bundle["rubric_sha256"],
            "routed_model": raw.get("model"),
            "routed_provider": raw.get("provider"),
            "request_id": raw.get("id"),
            "cost_usd": (raw.get("usage") or {}).get("cost"),
            "usage": raw.get("usage") or {},
        },
        "sample": bundle["sampling"] | {
            "items": len(items),
            "judged_items": len(scored),
            "unusable_outputs": unusable_outputs,
        },
        "quality": {
            "overall": round(overall, 4),
            "axes": {axis: round(value, 4) for axis, value in axes.items()},
            "hard_failures": hard_failures,
            "pass": quality_pass,
            "thresholds": {
                "minimum_overall": ready["minimum_overall"],
                "minimum_critical_axis": ready["minimum_critical_axis"],
                "critical_axes": critical,
            },
        },
        "items": items,
    }


def _call_openrouter(bundle: dict[str, Any], judge: dict[str, Any]) -> bytes:
    key = os.environ.get(judge["api_key_env"], "").strip()
    if not key:
        raise RuntimeError(f"{judge['api_key_env']} is required")
    body = {
        "model": judge["model"],
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT.read_text(encoding="utf-8")},
            {"role": "user", "content": json.dumps(bundle, ensure_ascii=False)},
        ],
        "reasoning": {"effort": judge["reasoning_effort"]},
        "max_tokens": judge["max_tokens"],
        "temperature": 0.0,
        "response_format": {"type": "json_object"},
    }
    request = urllib.request.Request(
        f"{judge['base_url'].rstrip('/')}/chat/completions",
        data=json.dumps(body, ensure_ascii=False).encode(),
        headers={
            "Authorization": f"Bearer {key}",
            "Content-Type": "application/json",
            "User-Agent": "rundale-bench/1.0 (+https://github.com/davidmooney/Rundale)",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=900) as response:
            return response.read()
    except urllib.error.HTTPError as error:
        return error.read() or json.dumps({
            "error": {"code": error.code, "message": str(error)},
        }).encode()


def _raw_is_resumable(raw: dict[str, Any], bundle: dict[str, Any]) -> bool:
    """Return true only when an interrupted run can produce a valid receipt."""

    if raw.get("is_error") or raw.get("error"):
        return False
    choice = (raw.get("choices") or [{}])[0]
    if choice.get("finish_reason") != "stop":
        return False
    try:
        result = rb.extract_json(str((choice.get("message") or {}).get("content", "")))
        _validate(bundle, result)
    except (KeyError, TypeError, ValueError, json.JSONDecodeError):
        return False
    return True


def _judge(run: dict[str, Any], judge: dict[str, Any]) -> tuple[str, str, str]:
    circuit = CIRCUITS[judge["id"]]
    if circuit.is_set():
        return run["run_id"], judge["id"], "skipped after judge-specific failure"
    bundle_path, bundle = _bundle(run, judge)
    stem = bundle_path.name.removesuffix(".bundle.json")
    judgment_path = bundle_path.with_name(f"{stem}.json")
    if judgment_path.is_file():
        return run["run_id"], judge["id"], "cached"

    legacy_raw = bundle_path.with_name(f"{stem}.raw.json")
    attempt_raws = sorted(bundle_path.parent.glob(f"{stem}.attempt-*.raw.json"))
    existing_raws = ([legacy_raw] if legacy_raw.is_file() else []) + attempt_raws
    raw_path = existing_raws[-1] if existing_raws else legacy_raw
    reuse_raw = False
    if raw_path.is_file():
        try:
            existing = json.loads(raw_path.read_text(encoding="utf-8"))
            reuse_raw = _raw_is_resumable(existing, bundle)
        except (ValueError, json.JSONDecodeError):
            reuse_raw = False
    if not reuse_raw:
        attempt = len(existing_raws) + 1
        raw_path = bundle_path.with_name(f"{stem}.attempt-{attempt}.raw.json")
        raw_bytes = _call_openrouter(bundle, judge)
        _write_once(raw_path, raw_bytes)
        if not raw_bytes:
            raise RuntimeError(f"judge returned an empty response for {run['run_id']}")

    raw = json.loads(raw_path.read_text(encoding="utf-8"))
    if raw.get("error"):
        code = (raw.get("error") or {}).get("code")
        if code in {400, 401, 402, 403, 429}:
            circuit.set()
        raise RuntimeError(f"judge API failed for {run['run_id']}: {raw.get('error')}")
    choice = (raw.get("choices") or [{}])[0]
    finish_reason = choice.get("finish_reason")
    if finish_reason != "stop":
        # A non-stop completion cannot yield a valid immutable receipt. Stop
        # this judge profile immediately: every queued job has the same bundle
        # shape and request budget, so continuing would repeat paid truncation.
        circuit.set()
        raise RuntimeError(f"judge did not finish cleanly for {run['run_id']}: {finish_reason}")
    try:
        result = rb.extract_json(str((choice.get("message") or {}).get("content", "")))
        receipt = _aggregate(bundle, result, raw, judge)
    except (KeyError, TypeError, ValueError, json.JSONDecodeError):
        # A schema-invalid response is likewise profile-wide evidence: pause
        # the judge until its effort/budget contract changes, retaining this
        # raw paid attempt for diagnosis.
        circuit.set()
        raise
    receipt["source"] = {
        "bundle": str(bundle_path.relative_to(REPO)),
        "bundle_sha256": _sha(bundle_path.read_bytes()),
        "raw": str(raw_path.relative_to(REPO)),
        "raw_sha256": _sha(raw_path.read_bytes()),
    }
    data = (json.dumps(receipt, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode()
    _write_once(judgment_path, data)
    return run["run_id"], judge["id"], "judged"


def _diagnose_legacy_attempts() -> int:
    written = 0
    for bundle_path in dashboard.DEFAULT_RUNS.glob("*/*-judgment-*.bundle.json"):
        bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
        if bundle.get("version") != 1:
            continue
        stem = bundle_path.name.removesuffix(".bundle.json")
        judgment_path = bundle_path.with_name(f"{stem}.json")
        raw_path = bundle_path.with_name(f"{stem}.raw.json")
        if not judgment_path.is_file() or not raw_path.is_file():
            continue
        diagnosis_path = bundle_path.with_name(f"{stem}.diagnosis.json")
        payload = {
            "version": 1,
            "classification": "invalid_judge_input",
            "finding": (
                "The paid judge saw only dynamic user context, not the production system prompt "
                "containing character identity and authoritative PEOPLE/PLACES lists. Character "
                "and grounding scores are therefore invalid; this receipt must not be ranked."
            ),
            "invalidates": {
                "bundle": str(bundle_path.relative_to(REPO)),
                "bundle_sha256": _sha(bundle_path.read_bytes()),
                "judgment": str(judgment_path.relative_to(REPO)),
                "judgment_sha256": _sha(judgment_path.read_bytes()),
                "raw": str(raw_path.relative_to(REPO)),
                "raw_sha256": _sha(raw_path.read_bytes()),
            },
            "replacement_contract": "bundle version 2: full production system plus user prompt",
        }
        data = (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode()
        existed = diagnosis_path.is_file()
        _write_once(diagnosis_path, data)
        written += not existed
    return written


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "diagnose":
        print(f"[cloud-judge] wrote {_diagnose_legacy_attempts()} invalid-attempt diagnoses")
        return 0
    args = list(sys.argv[1:])
    selected_run_ids: set[str] = set()
    while "--run-id" in args:
        index = args.index("--run-id")
        if index + 1 >= len(args):
            raise SystemExit("--run-id requires an exact qualification run id")
        selected_run_ids.add(args[index + 1])
        del args[index:index + 2]
    feed = dashboard.build(dashboard.DEFAULT_RUNS)
    runs = [run for run in feed["runs"] if run["status"] in {"needs_judgment", "needs_adjudication"}]
    if selected_run_ids:
        known_run_ids = {run["run_id"] for run in feed["runs"]}
        unknown_runs = selected_run_ids - known_run_ids
        if unknown_runs:
            raise SystemExit(f"unknown run ids: {', '.join(sorted(unknown_runs))}")
        runs = [run for run in runs if run["run_id"] in selected_run_ids]
    selected_ids = set(args)
    judges = [judge for judge in JUDGES if not selected_ids or judge["id"] in selected_ids]
    unknown = selected_ids - {judge["id"] for judge in JUDGES}
    if unknown:
        raise SystemExit(f"unknown judge ids: {', '.join(sorted(unknown))}")
    missing_keys = sorted({
        judge["api_key_env"] for judge in judges
        if not os.environ.get(judge["api_key_env"], "").strip()
    })
    if missing_keys:
        raise SystemExit(f"missing judge API keys: {', '.join(missing_keys)}")
    failures = []
    with ThreadPoolExecutor(max_workers=2) as pool:
        jobs = [
            (run, judge)
            for run in runs
            for judge in eligible_judges(run)
            if judge in judges
            and judge["id"] not in {
                item["id"] for item in (run.get("judgment") or {}).get("judges", [])
            }
        ]
        futures = {
            pool.submit(_judge, run, judge): (run["run_id"], judge["id"])
            for run, judge in jobs
        }
        for future in as_completed(futures):
            try:
                run_id, judge_id, state = future.result()
                print(f"[cloud-judge] {run_id} [{judge_id}]: {state}", flush=True)
            except Exception as exc:  # noqa: BLE001
                run_id, judge_id = futures[future]
                failures.append(f"{run_id} [{judge_id}]: {exc}")
                print(f"[cloud-judge] ERROR {failures[-1]}", file=sys.stderr, flush=True)
    if failures:
        raise SystemExit("\n".join(failures))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
