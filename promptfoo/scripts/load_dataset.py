"""Promptfoo dataset generator: a frozen v2 slice → promptfoo test cases.

Referenced from a promptfooconfig as:

    tests: file://scripts/load_dataset.py:generate_tests

The slice + split + limit are taken from env vars so one loader serves every
config:
    RB_SLICE  (required)  dialogue | intent | reaction | tier2-sim | tier3-sim | gaeilge
    RB_SPLIT  (dev|holdout, default dev)
    RB_LIMIT  (int, optional — first N records)

Each test carries the full dataset record as a JSON string in the `record` var;
the provider rebuilds the per-slice request and the assertions read gold /
schema / persona back out of it.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402


def _load_records(slice_name: str, split: str) -> list[dict]:
    suffix = ".holdout.jsonl" if split == "holdout" else ".jsonl"
    path = rb.DATASETS_DIR / f"{slice_name}{suffix}"
    raw = path.read_text(encoding="utf-8")
    return [json.loads(line) for line in raw.splitlines() if line.strip()]


def generate_tests(*_args, **_kwargs):
    slice_name = os.environ.get("RB_SLICE")
    if not slice_name:
        raise ValueError("RB_SLICE env var required for load_dataset")
    split = os.environ.get("RB_SPLIT", "dev")
    limit = os.environ.get("RB_LIMIT")

    records = _load_records(slice_name, split)
    if slice_name in ("tier2-sim", "tier3-sim") and os.environ.get("RB_PERF"):
        # perf path reuses dialogue prompts; never invoked for sim
        pass
    if limit:
        records = records[: int(limit)]

    tests = []
    for rec in records:
        display = rec.get("user") or rec.get("prompt", "")
        tests.append(
            {
                "vars": {
                    "record": json.dumps(rec, ensure_ascii=False),
                    "display_prompt": display,
                    "rb_id": rec["id"],
                },
                "description": f"{slice_name}:{rec['id']}",
            }
        )
    return tests


def generate_perf_tests(*_args, **_kwargs):
    """Perf slice: warmup (discarded) + measure ids from perf.ids.json, against
    dialogue prompts. Each measure prompt is emitted `RB_PERF_REPEAT` times
    (default 3) and the warmup id is emitted first flagged `perf_warmup` so the
    report drops it — mirroring v1's warmup-then-N-measured-calls methodology so
    cold-start latency and a too-small sample don't skew p50/p95/tok-s."""
    ids_cfg = json.loads((rb.V2_DIR / "perf.ids.json").read_text(encoding="utf-8"))
    warmup_id = ids_cfg.get("warmup")
    measure_ids = list(ids_cfg.get("measure", []))
    repeat = max(1, int(os.environ.get("RB_PERF_REPEAT", "3")))
    dialogue = {r["id"]: r for r in _load_records("dialogue", "dev")}

    def _row(rec, *, warmup=False):
        return {
            "vars": {
                "record": json.dumps(rec, ensure_ascii=False),
                "display_prompt": rec.get("user") or rec.get("prompt", ""),
                "rb_id": rec["id"],
                "perf_warmup": warmup,
            },
            "description": f"perf:{'warmup:' if warmup else ''}{rec['id']}",
        }

    tests = []
    if warmup_id and warmup_id in dialogue:
        tests.append(_row(dialogue[warmup_id], warmup=True))
    for _ in range(repeat):
        for pid in measure_ids:
            rec = dialogue.get(pid)
            if rec:
                tests.append(_row(rec))
    return tests
