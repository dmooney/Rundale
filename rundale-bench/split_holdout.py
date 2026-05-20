#!/usr/bin/env python3
"""Split a rundale-bench slice into dev (80%) and holdout (20%) subsets.

Split is deterministic by record `id`: the lowest 20% of SHA-256(id) → holdout.
This is reproducible across machines + immune to record-order changes.

Usage::

    python3 rundale-bench/split_holdout.py v1                # all slices
    python3 rundale-bench/split_holdout.py v1 intent        # one slice

After running, rebuild the manifest:

    python3 rundale-bench/build_manifest.py v1

The holdout files (`<slice>.holdout.jsonl`) live next to their dev sources.
For v1-dev they are stored in plaintext — anyone with repo read can see
them. Phase 7 freeze will rotate them out of the public dataset and into
an age-encrypted form gated behind a CI-only key; until then, treat the
holdout as "moral seal", not cryptographic.
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

HOLDOUT_FRACTION = 0.20


def _holdout_bucket(record_id: str) -> bool:
    """Return True if this id falls in the bottom 20% by SHA-256 ordering."""
    digest = hashlib.sha256(record_id.encode("utf-8")).digest()
    bucket = int.from_bytes(digest[:8], "big") / 2**64
    return bucket < HOLDOUT_FRACTION


def split(version: str, slice_name: str) -> tuple[int, int]:
    suite = Path(__file__).resolve().parent / version
    src = suite / f"{slice_name}.jsonl"
    if not src.exists():
        raise SystemExit(f"slice not found: {src}")

    dev_path = src
    holdout_path = suite / f"{slice_name}.holdout.jsonl"

    records = [json.loads(line) for line in src.read_text(encoding="utf-8").splitlines() if line.strip()]
    dev, holdout = [], []
    for rec in records:
        # core tier is the always-run smoke set — never moves to holdout, so
        # `gen_dlg.py` and equivalents keep working from the dev split alone.
        if rec.get("tier") == "core":
            dev.append(rec)
            continue
        (holdout if _holdout_bucket(rec["id"]) else dev).append(rec)

    dev_path.write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in dev) + "\n", encoding="utf-8")
    holdout_path.write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in holdout) + "\n", encoding="utf-8")
    print(f"{slice_name}: {len(dev)} dev / {len(holdout)} holdout ({100*len(holdout)/max(1,len(records)):.1f}%)")
    return len(dev), len(holdout)


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit("usage: split_holdout.py <version> [<slice>]")
    version = sys.argv[1]
    suite = Path(__file__).resolve().parent / version
    if len(sys.argv) >= 3:
        split(version, sys.argv[2])
    else:
        slices = sorted(
            p.stem for p in suite.glob("*.jsonl")
            if not p.name.endswith(".holdout.jsonl")
        )
        for s in slices:
            split(version, s)


if __name__ == "__main__":
    main()
