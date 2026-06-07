#!/usr/bin/env python3
"""Tests for the HF preflight classifier (AC6).

``classify_repo`` is the offline decision: is an mlx-community repo a
text-only LM with a chat template (benchmarkable) or a VL/multimodal /
template-less model we should not download?
"""

from __future__ import annotations

import sys
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

from preflight import classify_repo, has_chat_template, is_multimodal  # noqa: E402

_TEXT_FILES = ["config.json", "tokenizer_config.json", "model.safetensors"]


# ---------------------------------------------------------------------------
# is_multimodal
# ---------------------------------------------------------------------------


def test_text_only_arch_not_multimodal() -> None:
    assert is_multimodal({"architectures": ["Qwen3ForCausalLM"]}) is False


def test_vl_architecture_is_multimodal() -> None:
    assert is_multimodal({"architectures": ["Qwen2VLForConditionalGeneration"]}) is True


def test_vision_config_key_is_multimodal() -> None:
    assert is_multimodal({"architectures": ["LlamaForCausalLM"], "vision_config": {}}) is True


def test_image_token_index_is_multimodal() -> None:
    assert is_multimodal({"image_token_index": 32000}) is True


def test_model_type_marker_is_multimodal() -> None:
    assert is_multimodal({"model_type": "llava"}) is False  # not in markers
    assert is_multimodal({"model_type": "qwen2_vl"}) is True


# ---------------------------------------------------------------------------
# has_chat_template
# ---------------------------------------------------------------------------


def test_chat_template_inline_config() -> None:
    assert has_chat_template({"chat_template": "{{ messages }}"}, []) is True


def test_chat_template_in_tokenizer_config() -> None:
    cfg = {"tokenizer_config": {"chat_template": "{{ messages }}"}}
    assert has_chat_template(cfg, []) is True


def test_chat_template_jinja_file() -> None:
    assert has_chat_template({}, ["chat_template.jinja"]) is True


def test_no_chat_template() -> None:
    assert has_chat_template({}, ["config.json", "model.safetensors"]) is False


# ---------------------------------------------------------------------------
# classify_repo
# ---------------------------------------------------------------------------


def test_text_only_with_template_accepted() -> None:
    cfg = {"architectures": ["Qwen3ForCausalLM"], "chat_template": "{{ x }}"}
    verdict = classify_repo(cfg, _TEXT_FILES)
    assert verdict["ok"] is True
    assert verdict["is_multimodal"] is False
    assert verdict["has_chat_template"] is True
    assert verdict["reasons"] == []


def test_multimodal_rejected() -> None:
    cfg = {"architectures": ["Qwen2VLForConditionalGeneration"], "chat_template": "{{ x }}"}
    verdict = classify_repo(cfg, _TEXT_FILES)
    assert verdict["ok"] is False
    assert verdict["is_multimodal"] is True
    assert any("multimodal" in r for r in verdict["reasons"])


def test_missing_chat_template_rejected() -> None:
    cfg = {"architectures": ["Qwen3ForCausalLM"]}
    verdict = classify_repo(cfg, ["config.json", "model.safetensors"])
    assert verdict["ok"] is False
    assert verdict["has_chat_template"] is False
    assert any("chat_template" in r for r in verdict["reasons"])


def test_multimodal_and_no_template_lists_both_reasons() -> None:
    cfg = {"architectures": ["LlavaForConditionalGeneration"], "vision_config": {}}
    verdict = classify_repo(cfg, ["config.json"])
    assert verdict["ok"] is False
    assert len(verdict["reasons"]) == 2
