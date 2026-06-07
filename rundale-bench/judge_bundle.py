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
from typing import Any

from pydantic import ValidationError
from schemas import BundleSchema, ResultSchema

_BENCH_DIR = Path(__file__).resolve().parent
QUEUE_DIR = _BENCH_DIR / ".bench-queue"
PENDING_DIR = QUEUE_DIR / "pending"
DONE_DIR = QUEUE_DIR / "done"

AXES = ("character", "authenticity", "language", "responsiveness", "craft")

# Some slices (gaeilge) include "overall" in their axes list because the
# judge config's `axes` array is also the user-facing leaderboard-axis list.
# For validation we treat `overall` as the holistic field, never as a 1-5
# axis to coerce, so it is filtered out of the per-axis loop.
_OVERALL_KEYS = {"overall"}


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
        "bundle_id": bundle_id(
            slice_name, candidate.get("model_id") or candidate.get("resolved_target", "unknown")
        ),
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
    # Validate shape before committing to disk — raises ValidationError on
    # missing required fields or malformed items (failure modes 3, 4, 5).
    BundleSchema.model_validate(bundle)
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
        return json.loads(text[start : end + 1])
    raise ValueError("no JSON object found in judge reply")


def read_result(path: Path) -> dict:
    """Read a done/ result file, tolerating prose/fence-wrapped JSON."""
    return extract_json(path.read_text(encoding="utf-8"))


def recover_result(done_path: Path, reply_text: str | None = None) -> dict:
    """Post-validate a subagent dispatch, recovering JSON from reply text.

    The Agent tool occasionally prints its judgment JSON in the reply text
    instead of calling ``Write`` (observed: DS-R1-Qwen tier3-sim, round 3),
    leaving ``done_path`` absent or empty. Rather than force a manual recovery
    step, the orchestrator calls this after every dispatch:

      1. If ``done_path`` exists and parses, return it unchanged (fast path).
      2. Otherwise, regex-extract the outermost JSON object from ``reply_text``
         (via :func:`extract_json`), write it to ``done_path`` so the queue is
         consistent, and return the parsed dict.
      3. If neither yields JSON, raise ``ValueError`` so the caller can retry the
         agent with a corrective prompt.

    Returns the parsed result dict.
    """
    if done_path.exists():
        try:
            return extract_json(done_path.read_text(encoding="utf-8"))
        except (ValueError, json.JSONDecodeError):
            pass  # fall through to reply-text recovery

    if reply_text is None:
        raise ValueError(
            f"result file {done_path} missing/unparseable and no reply text to recover from"
        )

    recovered = extract_json(reply_text)  # raises ValueError if no JSON found
    done_path.parent.mkdir(parents=True, exist_ok=True)
    done_path.write_text(json.dumps(recovered, indent=2) + "\n", encoding="utf-8")
    return recovered


def unjudged_bundles() -> list[Path]:
    """Pending bundles with no matching done file (resume after abort)."""
    done_stems = {p.stem for p in list_done()}
    return [p for p in list_pending() if p.stem not in done_stems]


def _coerce_axis(value: Any, *, allow_zero: bool = False) -> int | None:
    """Coerce a judge-supplied axis value to an int. By default the range
    is 1-5; `allow_zero=True` widens it to 0-5 (used when the item is
    flagged as a bench-bug, where every axis must be 0)."""
    if isinstance(value, bool):  # bool is an int subclass — reject explicitly
        return None
    if not isinstance(value, (int, float)):
        return None
    iv = int(value)
    if iv != value:
        return None
    lower = 0 if allow_zero else 1
    if iv < lower or iv > 5:
        return None
    return iv


def _bundle_axes(bundle: dict | None) -> tuple[str, ...]:
    """Per-axis names to validate for a given bundle. Falls back to the
    dialogue rubric's 5-axis set if the bundle is missing or has no
    declared axes (legacy path before judge_*.json grew an `axes` list)."""
    if bundle is None:
        return AXES
    declared = bundle.get("axes")
    if not isinstance(declared, list) or not declared:
        return AXES
    # Strip the holistic `overall` key — it is validated separately and is
    # not a 1-5 integer axis (gaeilge ships overall as a 1.0-5.0 float).
    return tuple(k for k in declared if k not in _OVERALL_KEYS)


def validate_item(item: dict, bundle: dict | None = None) -> tuple[bool, dict]:
    """Validate one judged item against the Judgment schema.

    `bundle` carries the slice's per-judge axes (and is required for any
    slice other than dialogue). When omitted, the dialogue 5-axis schema is
    used for backwards compatibility.

    Returns ``(ok, cleaned)``. On failure ``cleaned`` is a failure marker with
    ``axes=None`` and ``flags.judge_retry=True`` so the orchestrator can
    exclude it from the aggregate and surface it as a judge failure.

    Bench-bug items (``flags.bench_bug == true``) are valid with every axis
    AND `overall` set to 0; the aggregator skips them from the mean and
    surfaces them via a separate `bench_bugs` count instead of floor-1
    scoring them.
    """
    axes = _bundle_axes(bundle)
    pid = item.get("prompt_id")
    _flags_raw = item.get("flags")
    flags_in: dict = _flags_raw if isinstance(_flags_raw, dict) else {}
    bench_bug = bool(flags_in.get("bench_bug", False))
    fail = {
        "prompt_id": pid,
        "axes": None,
        "overall": None,
        "rationales": item.get("rationales") if isinstance(item.get("rationales"), dict) else {},
        "flags": {
            "non_latin_detected": False,
            "refused": False,
            "bench_bug": False,
            "judge_retry": True,
        },
    }
    if not pid:
        fail["error"] = "missing prompt_id"
        return False, fail

    axes_in = item.get("axes")
    if not isinstance(axes_in, dict):
        fail["error"] = "axes missing or not an object"
        return False, fail

    axes_out: dict[str, int] = {}
    for k in axes:
        coerced = _coerce_axis(axes_in.get(k), allow_zero=bench_bug)
        if coerced is None:
            fail["error"] = f"axis {k!r} out of range or missing: {axes_in.get(k)!r}"
            return False, fail
        axes_out[k] = coerced

    # All-or-nothing: a bench-bug item must have every axis == 0.
    if bench_bug and any(v != 0 for v in axes_out.values()):
        fail["error"] = f"bench_bug=true but axes not all 0: {axes_out}"
        return False, fail

    overall = item.get("overall")
    lower = 0.0 if bench_bug else 1.0
    if (
        isinstance(overall, bool)
        or not isinstance(overall, (int, float))
        or not (lower <= float(overall) <= 5.0)
    ):
        fail["error"] = f"overall out of range: {overall!r}"
        return False, fail
    if bench_bug and float(overall) != 0.0:
        fail["error"] = f"bench_bug=true but overall != 0.0: {overall!r}"
        return False, fail

    cleaned = {
        "prompt_id": pid,
        "axes": axes_out,
        "overall": round(float(overall), 1),
        "rationales": item.get("rationales") if isinstance(item.get("rationales"), dict) else {},
        "flags": {
            "non_latin_detected": bool(flags_in.get("non_latin_detected", False)),
            "refused": bool(flags_in.get("refused", False)),
            "bench_bug": bench_bug,
            "judge_retry": False,
        },
    }
    return True, cleaned


def validate_result(result: dict, bundle: dict) -> tuple[list[dict], list[dict]]:
    """Validate every item in a subagent result against its bundle.

    Returns ``(valid_items, failed_items)``. A result whose rubric_sha256 does
    not match the bundle is rejected wholesale (every item becomes a failure)
    — a judge that scored against a different rubric cannot be trusted.

    ``ResultSchema.model_validate`` is called first to assert that
    ``rubric_sha256`` is present (failure mode 3/4) and that ``items`` is a
    list (failure mode 5). A malformed result (``ValidationError``) is caught
    and treated as a full failure: every expected item is marked
    ``judge_retry=True`` and the function returns ``([], failed)`` rather than
    raising, so the ingest loop continues with remaining bundles.
    """
    # Validate result shape. A ValidationError means the done/ file is
    # malformed (e.g. missing rubric_sha256 or items not a list). Treat it
    # the same as a rubric_sha256 mismatch: mark every expected item as a
    # judge failure and return early, so the ingest loop skips rather than
    # crashes. The error is surfaced via the returned failed-item entries
    # (each carries the validation error message) and the caller prints them.
    try:
        ResultSchema.model_validate(result)
    except ValidationError as exc:
        failed: list[dict] = []
        error_msg = f"malformed result (ValidationError): {exc}"
        for it in bundle["items"]:
            failed.append(
                {
                    "prompt_id": it["prompt_id"],
                    "axes": None,
                    "overall": None,
                    "rationales": {},
                    "flags": {
                        "non_latin_detected": False,
                        "refused": False,
                        "bench_bug": False,
                        "judge_retry": True,
                    },
                    "error": error_msg,
                }
            )
        return [], failed
    expected_ids = {it["prompt_id"] for it in bundle["items"]}
    if result.get("rubric_sha256") != bundle["rubric_sha256"]:
        mismatch_failed: list[dict] = []
        for it in bundle["items"]:
            mismatch_failed.append(
                {
                    "prompt_id": it["prompt_id"],
                    "axes": None,
                    "overall": None,
                    "rationales": {},
                    "flags": {
                        "non_latin_detected": False,
                        "refused": False,
                        "bench_bug": False,
                        "judge_retry": True,
                    },
                    "error": "rubric_sha256 mismatch between result and bundle",
                }
            )
        return [], mismatch_failed

    valid: list[dict] = []
    failed = []
    seen: set[str] = set()
    for it in result.get("items", []):
        ok, cleaned = validate_item(it, bundle)
        if cleaned.get("prompt_id") in expected_ids:
            seen.add(cleaned["prompt_id"])
        (valid if ok else failed).append(cleaned)

    # Any expected prompt the subagent silently dropped is a failure.
    for pid in expected_ids - seen:
        failed.append(
            {
                "prompt_id": pid,
                "axes": None,
                "overall": None,
                "rationales": {},
                "flags": {
                    "non_latin_detected": False,
                    "refused": False,
                    "bench_bug": False,
                    "judge_retry": True,
                },
                "error": "prompt_id absent from judge result",
            }
        )
    return valid, failed
