#!/usr/bin/env python3
"""Content-addressed judgment cache for rundale-bench.

A judgment is keyed by the tuple that fully determines its value:

    cache_key = sha256( prompt_id ‖ response_sha256 ‖ rubric_sha256 ‖ judge_model )

where ``response_sha256 = sha256(response)`` and ``‖`` is a NUL-delimited
join (the delimiter keeps ``("a", "bc")`` distinct from ``("ab", "c")``).

The locked semantics (see the redesign plan):
  - Adding a model judges only that model — its responses hash to new keys.
  - Changing the rubric or the judge model re-judges everything — the
    rubric_sha256 / judge_model component changes for every key.
  - Re-running the same (prompt, response) under the same rubric+judge is a
    no-op — the key already has a stored judgment.

Judgments are stored under the ignored local archive at
``docs/proofs/rundale-bench/judgments/<key>.json``. On the primary macOS
workstation that path is an iCloud-backed symlink; concise summaries and
content hashes, rather than raw paid receipts, are committed.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCH_DIR.parent
JUDGMENTS_DIR = _REPO_ROOT / "docs" / "proofs" / "rundale-bench" / "judgments"


def response_sha256(response: str) -> str:
    """SHA-256 of the candidate response text."""
    return hashlib.sha256(response.encode("utf-8")).hexdigest()


def cache_key(prompt_id: str, response: str, rubric_sha256: str, judge_model: str) -> str:
    """Compute the content-addressed cache key for one judgment.

    The four components are joined with NUL bytes so no concatenation
    collision is possible across differing field boundaries.
    """
    parts = [prompt_id, response_sha256(response), rubric_sha256, judge_model]
    joined = "\x00".join(parts).encode("utf-8")
    return hashlib.sha256(joined).hexdigest()


def _path_for(key: str) -> Path:
    return JUDGMENTS_DIR / f"{key}.json"


def has(key: str) -> bool:
    """True if a judgment is already stored for this key."""
    return _path_for(key).exists()


def get(key: str) -> dict | None:
    """Return the stored judgment dict, or None if absent."""
    path = _path_for(key)
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def put(key: str, judgment: dict) -> Path:
    """Persist a judgment under its key. Returns the written path.

    The stored object always carries its own ``cache_key`` so a loose file
    can be traced back to its inputs during audits.
    """
    JUDGMENTS_DIR.mkdir(parents=True, exist_ok=True)
    record = {"cache_key": key, **judgment}
    path = _path_for(key)
    path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path
