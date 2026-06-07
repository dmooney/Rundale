#!/usr/bin/env python3
"""Tests for the tokenizer-audit aggregation helpers (AC8).

The live tokenize path needs ``transformers`` + base-model downloads (external
infra), but the pure aggregation logic — tokens/char and per-model rollup — is
fully testable offline.
"""

from __future__ import annotations

import sys
from pathlib import Path

_BENCH_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_BENCH_DIR))

import pytest  # noqa: E402
from tokenizer_audit import summarize_audit, tokens_per_char  # noqa: E402


def test_tokens_per_char() -> None:
    assert tokens_per_char(50, 200) == pytest.approx(0.25)


def test_tokens_per_char_empty_text_is_zero() -> None:
    # Empty corpus → 0.0, never a ZeroDivisionError / NaN.
    assert tokens_per_char(0, 0) == 0.0
    assert tokens_per_char(5, 0) == 0.0


def test_summarize_audit_corpus_weighted() -> None:
    rows = [
        {"model": "gemma", "corpus": "hyde", "tokens": 100, "chars": 400},
        {"model": "gemma", "corpus": "brooke", "tokens": 100, "chars": 100},
        {"model": "qwen3", "corpus": "hyde", "tokens": 120, "chars": 400},
    ]
    out = summarize_audit(rows)
    # gemma: (100+100) tokens / (400+100) chars = 0.4 (corpus-weighted, not the
    # 0.625 mean of the two per-row ratios).
    assert out["gemma"]["tokens_per_char"] == pytest.approx(0.4)
    assert out["gemma"]["tokens"] == 200
    assert out["gemma"]["chars"] == 500
    assert out["gemma"]["rows"] == 2
    assert out["qwen3"]["tokens_per_char"] == pytest.approx(0.3)
    assert out["qwen3"]["rows"] == 1


def test_summarize_audit_empty() -> None:
    assert summarize_audit([]) == {}


def test_summarize_audit_lower_ratio_is_more_efficient() -> None:
    rows = [
        {"model": "efficient", "corpus": "irish", "tokens": 80, "chars": 400},
        {"model": "fragmenting", "corpus": "irish", "tokens": 200, "chars": 400},
    ]
    out = summarize_audit(rows)
    assert out["efficient"]["tokens_per_char"] < out["fragmenting"]["tokens_per_char"]
