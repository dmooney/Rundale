#!/usr/bin/env python3
"""Reject local-preset qualification claims without passing receipts."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "parish/crates/parish-config/src/local_dialogue.rs"
QUALIFIED = ROOT / "promptfoo/qualified"
MANIFEST = ROOT / "promptfoo/v2/MANIFEST.json"
LOCAL_PRESETS = (
    ROOT / "parish/crates/parish-config/src/builtin_providers/vllm_mlx.toml",
    ROOT / "parish/crates/parish-config/src/builtin_providers/vllm.toml",
    ROOT / "parish/crates/parish-config/src/builtin_providers/ollama.toml",
)


def registered_profiles() -> list[tuple[str, str]]:
    text = REGISTRY.read_text(encoding="utf-8")
    body = text.split("QUALIFIED_LOCAL_DIALOGUE_PROFILES", 1)[1].split("];", 1)[0]
    return re.findall(r'\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)', body)


def validate() -> list[str]:
    errors: list[str] = []
    manifest_merkle = json.loads(MANIFEST.read_text(encoding="utf-8"))["merkle_root_sha256"]
    profiles = registered_profiles()
    receipts = list(QUALIFIED.glob("*/promotion.json")) if QUALIFIED.exists() else []

    receipt_by_model: dict[str, list[dict]] = {}
    for path in receipts:
        try:
            receipt = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            errors.append(f"invalid qualification receipt {path.relative_to(ROOT)}: {exc}")
            continue
        model = str((receipt.get("request_profile") or {}).get("model", ""))
        receipt_by_model.setdefault(model, []).append(receipt)

    for provider, model in profiles:
        matches = [
            receipt
            for receipt in receipt_by_model.get(model, [])
            if receipt.get("passed") is True and receipt.get("dataset_merkle") == manifest_merkle
        ]
        if not matches:
            errors.append(
                f"qualified profile {provider}/{model} has no passing receipt "
                "for the current frozen manifest"
            )

    qualified_models = {model for _, model in profiles}
    for path in LOCAL_PRESETS:
        text = path.read_text(encoding="utf-8")
        local_models = re.findall(
            r'^(?:dialogue|simulation|intent|reaction)\s*=\s*"([^"]+)"',
            text,
            flags=re.MULTILINE,
        )
        if not any(model in qualified_models for model in local_models):
            labels = re.findall(r'^label\s*=\s*"([^"]+)"', text, flags=re.MULTILINE)
            if any(label.strip().lower() == "recommended" for label in labels):
                errors.append(
                    f"{path.relative_to(ROOT)} labels an unqualified local preset Recommended"
                )

    return errors


def main() -> int:
    errors = validate()
    if errors:
        for error in errors:
            print(f"qualification drift: {error}", file=sys.stderr)
        return 1
    print(
        f"local dialogue qualification registry valid "
        f"({len(registered_profiles())} qualified profiles)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
