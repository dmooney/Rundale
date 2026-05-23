#!/usr/bin/env python3
"""Bundle/queue plumbing for subagent-driven judging.

The orchestrator generates candidate responses, then writes one *bundle* per
(slice, candidate) into ``.bench-queue/pending/``. A Claude Code session (the
``/rundale-bench`` skill) drains the queue: it dispatches a Sonnet 4.6
subagent per bundle and writes the subagent's JSON reply to
``.bench-queue/done/``. Finally ``rundale_bench.py ingest`` validates each
result, writes content-addressed judgments via ``cache``, and folds the
scores back into the run aggregate.

This module owns: bundle assembly, queue I/O, and result validation. It does
not call any model — judging is the subagent's job, perf the orchestrator's.
"""
from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any, Optional

_BENCH_DIR = Path(__file__).resolve().parent
QUEUE_DIR = _BENCH_DIR / ".bench-queue"
PENDING_DIR = QUEUE_DIR / "pending"
DONE_DIR = QUEUE_DIR / "done"

AXES = ("character", "authenticity", "language", "responsiveness", "craft")


def _slug(s: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "_", s).strip("_")[:80]


def bundle_id(slice_name: str, candidate_label: str) -> str:
    """Stable id for a (slice, candidate) bundle. Deterministic so a re-run
    overwrites the same pending file rather than piling up duplicates."""
    return f"{slice_name}__{_slug(candidate_label)}"


def assemble_bundle(
    *,
    slice_name: str,
    candidate: dict,
    judge: dict,
    items: list[dict],
) -> dict:
    """Build a judging bundle. `items` are `{prompt_id, prompt, response}`."""
    return {
        "bundle_id": bundle_id(slice_name, candidate.get("model_id") or candidate.get("resolved_target", "unknown")),
        "slice": slice_name,
        "candidate": candidate,
        "judge_id": judge["judge_id"],
        "judge_model": judge["model"],
        "rubric_sha256": judge["rubric_sha256"],
        "rubric": judge["rubric"],
        "axes": list(judge.get("axes", AXES)),
        "system_prompt_file": judge.get("system_prompt_file"),
        "items": items,
    }


def write_pending(bundle: dict) -> Path:
    PENDING_DIR.mkdir(parents=True, exist_ok=True)
    path = PENDING_DIR / f"{bundle['bundle_id']}.json"
    path.write_text(json.dumps(bundle, indent=2) + "\n", encoding="utf-8")
    return path


def list_pending() -> list[Path]:
    if not PENDING_DIR.exists():
        return []
    return sorted(PENDING_DIR.glob("*.json"))


def list_done() -> list[Path]:
    if not DONE_DIR.exists():
        return []
    return sorted(DONE_DIR.glob("*.json"))


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def extract_json(text: str) -> dict:
    """Parse a judge reply that may be wrapped in prose or a ```json fence.

    A Sonnet subagent, despite the JSON-only instruction, sometimes prepends
    analysis or wraps the object in a code fence. Rather than reject the run,
    recover the outermost JSON object: try a clean parse, then a fenced parse,
    then the substring from the first ``{`` to the last ``}``.
    """
    text = text.strip()
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass
    fence = re.search(r"```(?:json)?\s*(\{.*?\})\s*```", text, re.DOTALL)
    if fence:
        return json.loads(fence.group(1))
    start, end = text.find("{"), text.rfind("}")
    if start != -1 and end > start:
        return json.loads(text[start:end + 1])
    raise ValueError("no JSON object found in judge reply")


def read_result(path: Path) -> dict:
    """Read a done/ result file, tolerating prose/fence-wrapped JSON."""
    return extract_json(path.read_text(encoding="utf-8"))


def unjudged_bundles() -> list[Path]:
    """Pending bundles with no matching done file (resume after abort)."""
    done_stems = {p.stem for p in list_done()}
    return [p for p in list_pending() if p.stem not in done_stems]


def _coerce_axis(value: Any) -> Optional[int]:
    if isinstance(value, bool):  # bool is an int subclass — reject explicitly
        return None
    if not isinstance(value, (int, float)):
        return None
    iv = int(value)
    if iv != value or iv < 1 or iv > 5:
        return None
    return iv


def validate_item(item: dict) -> tuple[bool, dict]:
    """Validate one judged item against the Judgment schema.

    Returns ``(ok, cleaned)``. On failure ``cleaned`` is a failure marker with
    ``axes=None`` and ``flags.judge_retry=True`` so the orchestrator can
    exclude it from the aggregate and surface it as a judge failure.
    """
    pid = item.get("prompt_id")
    fail = {
        "prompt_id": pid,
        "axes": None,
        "overall": None,
        "rationales": item.get("rationales") if isinstance(item.get("rationales"), dict) else {},
        "flags": {"non_latin_detected": False, "refused": False, "judge_retry": True},
    }
    if not pid:
        fail["error"] = "missing prompt_id"
        return False, fail

    axes_in = item.get("axes")
    if not isinstance(axes_in, dict):
        fail["error"] = "axes missing or not an object"
        return False, fail

    axes_out: dict[str, int] = {}
    for k in AXES:
        coerced = _coerce_axis(axes_in.get(k))
        if coerced is None:
            fail["error"] = f"axis {k!r} out of range or missing: {axes_in.get(k)!r}"
            return False, fail
        axes_out[k] = coerced

    overall = item.get("overall")
    if isinstance(overall, bool) or not isinstance(overall, (int, float)) or not (1.0 <= float(overall) <= 5.0):
        fail["error"] = f"overall out of range: {overall!r}"
        return False, fail

    flags_in = item.get("flags") if isinstance(item.get("flags"), dict) else {}
    cleaned = {
        "prompt_id": pid,
        "axes": axes_out,
        "overall": round(float(overall), 1),
        "rationales": item.get("rationales") if isinstance(item.get("rationales"), dict) else {},
        "flags": {
            "non_latin_detected": bool(flags_in.get("non_latin_detected", False)),
            "refused": bool(flags_in.get("refused", False)),
            "judge_retry": False,
        },
    }
    return True, cleaned


def validate_result(result: dict, bundle: dict) -> tuple[list[dict], list[dict]]:
    """Validate every item in a subagent result against its bundle.

    Returns ``(valid_items, failed_items)``. A result whose rubric_sha256 does
    not match the bundle is rejected wholesale (every item becomes a failure)
    — a judge that scored against a different rubric cannot be trusted.
    """
    expected_ids = {it["prompt_id"] for it in bundle["items"]}
    if result.get("rubric_sha256") != bundle["rubric_sha256"]:
        failed = []
        for it in bundle["items"]:
            failed.append({
                "prompt_id": it["prompt_id"],
                "axes": None,
                "overall": None,
                "rationales": {},
                "flags": {"non_latin_detected": False, "refused": False, "judge_retry": True},
                "error": "rubric_sha256 mismatch between result and bundle",
            })
        return [], failed

    valid: list[dict] = []
    failed: list[dict] = []
    seen: set[str] = set()
    for it in result.get("items", []):
        ok, cleaned = validate_item(it)
        if cleaned.get("prompt_id") in expected_ids:
            seen.add(cleaned["prompt_id"])
        (valid if ok else failed).append(cleaned)

    # Any expected prompt the subagent silently dropped is a failure.
    for pid in expected_ids - seen:
        failed.append({
            "prompt_id": pid,
            "axes": None,
            "overall": None,
            "rationales": {},
            "flags": {"non_latin_detected": False, "refused": False, "judge_retry": True},
            "error": "prompt_id absent from judge result",
        })
    return valid, failed
