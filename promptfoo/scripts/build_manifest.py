"""Pin sha256 of the copied v2 datasets so the suite detects silent drift.

    python3 promptfoo/scripts/build_manifest.py

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

SLICES = ["dialogue", "intent", "reaction", "tier2-sim", "tier3-sim", "gaeilge"]


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
    manifest = {
        "suite": "rundale-bench-v2",
        "source": "copied from rundale-bench/v1",
        "merkle_root_sha256": h.hexdigest(),
        "slices": slices,
    }
    out = rb.V2_DIR / "MANIFEST.json"
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"[manifest] {out} — {len(slices)} files, merkle={manifest['merkle_root_sha256'][:12]}…")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
