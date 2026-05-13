#!/usr/bin/env python3
"""Rebuild `MANIFEST.json` for a rundale-bench version directory.

Usage::

    python3 parish/testing/rundale-bench/build_manifest.py v1

The manifest records per-slice SHA-256, record count, and byte count, plus
a single `merkle_root_sha256` computed over the sorted list of per-slice
hashes. `eval_lib.load_slice` verifies the on-disk slice bytes against the
matching manifest entry on every load — drift surfaces as a `RuntimeError`.

Run this after intentional dataset edits and commit the new manifest in
the same PR as the slice change so reviewers see the hash delta.
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path


def build(version: str) -> dict:
    suite_dir = Path(__file__).resolve().parent / version
    if not suite_dir.is_dir():
        raise SystemExit(f"version dir not found: {suite_dir}")

    slices = sorted(p.name for p in suite_dir.iterdir() if p.suffix == ".jsonl")
    if not slices:
        raise SystemExit(f"no *.jsonl slices in {suite_dir}")

    entries: dict[str, dict] = {}
    for name in slices:
        data = (suite_dir / name).read_bytes()
        entries[name] = {
            "sha256": hashlib.sha256(data).hexdigest(),
            "records": data.decode("utf-8").count("\n"),
            "bytes": len(data),
        }

    concat = b"".join(bytes.fromhex(entries[n]["sha256"]) for n in slices)
    merkle = hashlib.sha256(concat).hexdigest()

    manifest_path = suite_dir / "MANIFEST.json"
    frozen = False
    if manifest_path.exists():
        existing = json.loads(manifest_path.read_text())
        frozen = bool(existing.get("frozen"))
        if frozen and existing.get("merkle_root_sha256") != merkle:
            raise SystemExit(
                f"{version} is frozen but slice hashes changed. Cut a new "
                f"version directory instead of mutating this one."
            )

    manifest = {
        "suite": "rundale-bench",
        "version": f"{version}-dev" if not frozen else version,
        "frozen": frozen,
        "merkle_root_sha256": merkle,
        "slices": entries,
        "notes": (
            "Dataset is in v1-dev (in-development). Once Phase 7 of the rollout "
            "ships, this file is frozen by flipping frozen=true and tagging the "
            "repo rundale-bench-v1.0. Any change after that ships under a new "
            "version."
        ),
    }
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"wrote {manifest_path}")
    print(f"merkle_root_sha256: {merkle}")
    for name, e in entries.items():
        print(f"  {name}: {e['records']} records, {e['bytes']} bytes, {e['sha256'][:16]}…")
    return manifest


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: build_manifest.py <version>")
    build(sys.argv[1])
