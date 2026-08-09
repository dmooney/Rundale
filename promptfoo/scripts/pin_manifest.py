"""Pin sha256 of the copied v2 datasets so the suite detects silent drift.

    python3 promptfoo/scripts/pin_manifest.py

Writes promptfoo/v2/MANIFEST.json. Re-run after intentionally refreshing a
copied dataset from rundale-bench/v1/.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rb_common as rb  # noqa: E402

SLICES = [
    "dialogue",
    "intent",
    "reaction",
    "tier2-sim",
    "tier3-sim",
    "gaeilge",
    "multiturn",
]


def main() -> int:
    slices: dict[str, dict] = {}
    h = hashlib.sha256()
    for stem in sorted(SLICES):
        for suffix in (".jsonl", ".holdout.jsonl"):
            name = f"{stem}{suffix}"
            path = rb.DATASETS_DIR / name
            if not path.exists():
                continue
            raw = path.read_bytes()
            digest = hashlib.sha256(raw).hexdigest()
            records = sum(1 for line in raw.decode("utf-8").splitlines() if line.strip())
            slices[name] = {"sha256": digest, "records": records, "bytes": len(raw)}
            h.update(digest.encode())
    # Pin the copied rubrics (system prompts + judge configs) and perf id set
    # too — they define what the judge scores and which perf prompts run, so a
    # silent edit must show up as drift just like a dataset change would.
    assets: dict[str, dict] = {}
    asset_paths = sorted(rb.RUBRICS_DIR.glob("*.system.md")) + sorted(rb.RUBRICS_DIR.glob("*.json"))
    asset_paths.append(rb.V2_DIR / "perf.ids.json")
    for path in asset_paths:
        if not path.exists():
            continue
        raw = path.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        assets[str(path.relative_to(rb.V2_DIR))] = {"sha256": digest, "bytes": len(raw)}
        h.update(digest.encode())
    merkle = h.hexdigest()
    manifest = {
        "suite": "rundale-bench-v2",
        "source": "runtime-captured production prompts plus curated competence probes",
        "merkle_root_sha256": merkle,
        "slices": slices,
        "assets": assets,
    }
    out = rb.V2_DIR / "MANIFEST.json"
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"[manifest] {out} — {len(slices)} slices + {len(assets)} assets, merkle={merkle[:12]}…")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
