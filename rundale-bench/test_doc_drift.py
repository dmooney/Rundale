#!/usr/bin/env python3
"""Doc-drift guard for the dataset record count (TD-006) + MLX_VENV doc (AC9).

The README/AGENTS prose used to claim "155 prompts" while ``MANIFEST.json``
recorded 309 records. This test parses the manifest and asserts the docs no
longer carry the stale 155, that the documented total equals the manifest sum,
and that ``MLX_VENV`` is documented in the README setup section.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

_MANIFEST = _BENCH_DIR / "v1" / "MANIFEST.json"
_README = _BENCH_DIR / "README.md"
_AGENTS = _BENCH_DIR / "AGENTS.md"


def _manifest_total() -> int:
    manifest = json.loads(_MANIFEST.read_text(encoding="utf-8"))
    return sum(int(s["records"]) for s in manifest["slices"].values())


def test_manifest_total_is_nonzero() -> None:
    assert _manifest_total() > 0


def test_docs_do_not_claim_stale_155() -> None:
    total = _manifest_total()
    # Guard only fires while the real total is not 155 (it is 309 today). If the
    # corpus ever shrinks back to 155 this assertion is vacuously satisfied.
    if total == 155:
        return
    for doc in (_README, _AGENTS):
        text = doc.read_text(encoding="utf-8")
        assert "155 prompt" not in text, f"{doc.name} still claims a stale '155 prompts'"


def test_docs_state_the_live_total() -> None:
    total = _manifest_total()
    for doc in (_README, _AGENTS):
        text = doc.read_text(encoding="utf-8")
        assert re.search(rf"\b{total}\b", text), (
            f"{doc.name} should state the live manifest total ({total})"
        )


def test_mlx_venv_documented_in_readme() -> None:
    text = _README.read_text(encoding="utf-8")
    assert "MLX_VENV" in text, "README must document the MLX_VENV env-var override"


def test_max_ram_gb_documented_in_readme() -> None:
    text = _README.read_text(encoding="utf-8")
    assert "--max-ram-gb" in text, "README must document the RAM-cap kill switch"
