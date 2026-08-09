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

import hashlib
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
    records = [json.loads(line) for line in raw.splitlines() if line.strip()]
    manifest = json.loads((rb.V2_DIR / "MANIFEST.json").read_text(encoding="utf-8"))
    pin = (manifest.get("slices") or {}).get(path.name)
    if not pin:
        raise RuntimeError(
            f"{path.name} is not pinned in v2/MANIFEST.json; run "
            "python3 promptfoo/scripts/pin_manifest.py after an intentional dataset change"
        )
    digest = hashlib.sha256(raw.encode("utf-8")).hexdigest()
    if digest != pin.get("sha256") or len(records) != pin.get("records"):
        raise RuntimeError(
            f"frozen dataset drift for {path.name}: expected "
            f"sha256={pin.get('sha256')} records={pin.get('records')}, got "
            f"sha256={digest} records={len(records)}"
        )
    return records


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
    split = os.environ.get("RB_SPLIT", "dev")
    records = _load_records("dialogue", split)
    dialogue = {r["id"]: r for r in records}

    # The corpus split is hash-based, so a previously selected perf ID can move
    # to the opposite split. Never silently lose the warmup or shrink the
    # measurement sample: retain configured IDs that are present, then fill the
    # fixed-size panel deterministically from this exact frozen split.
    warmup = dialogue.get(warmup_id)
    if warmup is None:
        warmup = min(records, key=lambda rec: rec["id"])
    # One record per exact system prompt makes the first pass an honest
    # per-persona cold-prefix panel. Later repeats of the same records measure
    # the steady-state cache path. Multiple records from one NPC would
    # accidentally over-represent a single warmed prefix.
    selected = []
    selected_ids = set()
    selected_systems = set()
    for pid in measure_ids:
        rec = dialogue.get(pid)
        if rec is None or rec["id"] == warmup["id"]:
            continue
        system = rec.get("system")
        if system in selected_systems:
            continue
        selected.append(rec)
        selected_ids.add(rec["id"])
        selected_systems.add(system)
    desired = max(1, len(measure_ids))
    for rec in sorted(records, key=lambda row: row["id"]):
        if len(selected) >= desired:
            break
        system = rec.get("system")
        if (
            rec["id"] != warmup["id"]
            and rec["id"] not in selected_ids
            and system not in selected_systems
        ):
            selected.append(rec)
            selected_ids.add(rec["id"])
            selected_systems.add(system)

    # Keep the generic model-load warmup from warming any measured system.
    # Seven or more distinct NPC systems are expected in the promotion corpus.
    if warmup.get("system") in selected_systems:
        replacement = next(
            (
                rec
                for rec in sorted(records, key=lambda row: row["id"])
                if rec.get("system") not in selected_systems
            ),
            None,
        )
        if replacement is not None:
            warmup = replacement

    # A limit is a development-smoke affordance. Production runs leave it
    # unset and get warmup + RB_PERF_REPEAT × the full fixed-size panel.
    limit = os.environ.get("RB_LIMIT")
    if limit:
        selected = selected[: max(1, int(limit))]
        repeat = 1

    def _row(rec, *, warmup=False, cache_state=None):
        variables = {
            "record": json.dumps(rec, ensure_ascii=False),
            "display_prompt": rec.get("user") or rec.get("prompt", ""),
            "rb_id": rec["id"],
            "perf_warmup": warmup,
            "perf_cache_state": cache_state or "warmup",
        }
        return {
            "vars": variables,
            "description": f"perf:{'warmup:' if warmup else ''}{rec['id']}",
        }

    tests = []
    tests.append(_row(warmup, warmup=True))
    for repetition in range(repeat):
        for rec in selected:
            tests.append(_row(rec, cache_state="cold" if repetition == 0 else "warm"))
    return tests
