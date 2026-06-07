#!/usr/bin/env python3
"""HF preflight: is an ``mlx-community`` repo benchmarkable before we download it?

Rounds 3-4 vetted each new candidate ad-hoc via a subagent ("is this a VL model?
does it have a chat template?"). This codifies that check. The pure classifier
``classify_repo`` takes a parsed ``config.json`` dict plus the repo's file list
and decides:

  - ``is_multimodal`` — the architecture is a vision/VL/multimodal model
    (``mlx_lm.server`` cannot serve these for text-only dialogue scoring), or the
    config carries a ``vision_config`` / ``image_token_index`` block.
  - ``has_chat_template`` — a ``chat_template`` is present (in config or as a
    ``tokenizer_config.json`` / ``chat_template.jinja`` file). Without one the
    bench prompt formatting falls back to a raw template and scores are garbage.

The network fetch (``fetch_repo``) is optional and only runs when invoked as a
CLI with ``huggingface_hub`` installed; the classifier itself is offline and
fully testable.
"""

from __future__ import annotations

from typing import Any

# Substrings that mark a vision / multimodal architecture. Matched
# case-insensitively against each entry in config["architectures"].
_MULTIMODAL_MARKERS = ("vl", "vision", "multimodal", "image", "clip", "siglip")
# Config keys that only appear on multimodal models.
_MULTIMODAL_CONFIG_KEYS = ("vision_config", "image_token_index", "vision_tower")


def _architectures(config: dict[str, Any]) -> list[str]:
    arch = config.get("architectures")
    if isinstance(arch, list):
        return [str(a) for a in arch]
    if isinstance(arch, str):
        return [arch]
    return []


def is_multimodal(config: dict[str, Any]) -> bool:
    """True if the config describes a vision/VL/multimodal model."""
    for key in _MULTIMODAL_CONFIG_KEYS:
        if key in config:
            return True
    model_type = str(config.get("model_type", "")).lower()
    if any(marker in model_type for marker in _MULTIMODAL_MARKERS):
        return True
    for arch in _architectures(config):
        low = arch.lower()
        if any(marker in low for marker in _MULTIMODAL_MARKERS):
            return True
    return False


def has_chat_template(config: dict[str, Any], files: list[str]) -> bool:
    """True if a chat template ships with the repo.

    A chat template may live inline in ``config.json`` / ``tokenizer_config.json``
    (key ``chat_template``) or as a standalone ``chat_template.jinja`` file.
    """
    if config.get("chat_template"):
        return True
    tok_cfg = config.get("tokenizer_config")
    if isinstance(tok_cfg, dict) and tok_cfg.get("chat_template"):
        return True
    return any(f.endswith("chat_template.jinja") for f in files)


def classify_repo(config: dict[str, Any], files: list[str]) -> dict[str, Any]:
    """Classify a repo for benchmarkability.

    Returns ``{ok, is_multimodal, has_chat_template, reasons}``. ``ok`` is True
    only when the model is text-only AND ships a chat template. ``reasons`` lists
    every disqualifier so the caller can print actionable feedback.
    """
    multimodal = is_multimodal(config)
    chat = has_chat_template(config, files)
    reasons: list[str] = []
    if multimodal:
        reasons.append("multimodal/VL architecture — mlx_lm.server serves text only")
    if not chat:
        reasons.append("no chat_template — bench prompt formatting would be unreliable")
    return {
        "ok": not multimodal and chat,
        "is_multimodal": multimodal,
        "has_chat_template": chat,
        "reasons": reasons,
    }


def fetch_repo(repo_id: str) -> tuple[dict[str, Any], list[str]]:
    """Fetch ``config.json`` + file list from an HF repo (network).

    Imported lazily so the module is importable (and the classifier testable)
    without ``huggingface_hub`` installed.
    """
    import json
    from pathlib import Path

    from huggingface_hub import HfApi, hf_hub_download

    api = HfApi()
    info = api.model_info(repo_id)
    files = [s.rfilename for s in (info.siblings or [])]
    config: dict[str, Any] = {}
    if "config.json" in files:
        cfg_path = hf_hub_download(repo_id, "config.json")
        config = json.loads(Path(cfg_path).read_text(encoding="utf-8"))
    if "tokenizer_config.json" in files:
        tok_path = hf_hub_download(repo_id, "tokenizer_config.json")
        config["tokenizer_config"] = json.loads(Path(tok_path).read_text(encoding="utf-8"))
    return config, files


def main() -> int:
    import argparse
    import json

    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("repo", help="mlx-community repo id, e.g. mlx-community/Qwen3-4B-4bit")
    args = ap.parse_args()

    try:
        config, files = fetch_repo(args.repo)
    except ImportError:
        print("huggingface_hub not installed — install it to run the network preflight.")
        return 2

    verdict = classify_repo(config, files)
    print(json.dumps(verdict, indent=2))
    return 0 if verdict["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
