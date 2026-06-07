#!/usr/bin/env python3
"""Tokenizer audit — tokens/char on Irish-side text across candidate bases.

Phase 1 of the gemma4-rundale training plan picks a base model partly on Irish
tokenizer efficiency: a base that fragments Gaeilge into many sub-word tokens
wastes context and inflates training cost. This script measures tokens-per-char
on the Irish-side corpora (Hyde, Brooke) for each candidate base
(Gemma, Qwen3, EuroLLM, OLMo-2, Mistral-Small) so the pick is data-driven.

The aggregation helpers (``tokens_per_char``, ``summarize_audit``) are pure and
tested. The tokenizer-load + count path needs ``transformers`` + a base-model
download (external infra), so it is wired but degrades gracefully: if
``transformers`` is missing, ``count_tokens`` raises a clear ``ImportError`` and
``main`` reports the gap instead of crashing.
"""

from __future__ import annotations

from collections import defaultdict
from typing import Any


def tokens_per_char(token_count: int, char_count: int) -> float:
    """Tokens-per-char ratio. Lower is a more Irish-efficient tokenizer.

    Returns 0.0 for empty text rather than dividing by zero, so an empty corpus
    row contributes nothing to the mean instead of poisoning it with NaN.
    """
    if char_count <= 0:
        return 0.0
    return token_count / char_count


def summarize_audit(rows: list[dict[str, Any]]) -> dict[str, dict[str, float]]:
    """Aggregate per-(model) tokens/char across corpus rows.

    Each row is ``{model, corpus, tokens, chars}``. Returns
    ``{model: {tokens_per_char, tokens, chars, rows}}`` where ``tokens_per_char``
    is the corpus-weighted ratio (total tokens / total chars), not a mean of
    per-row ratios — that weights longer passages correctly.
    """
    by_model: dict[str, dict[str, float]] = defaultdict(
        lambda: {"tokens": 0.0, "chars": 0.0, "rows": 0.0}
    )
    for row in rows:
        model = str(row["model"])
        by_model[model]["tokens"] += float(row.get("tokens", 0))
        by_model[model]["chars"] += float(row.get("chars", 0))
        by_model[model]["rows"] += 1

    out: dict[str, dict[str, float]] = {}
    for model, agg in by_model.items():
        out[model] = {
            "tokens_per_char": tokens_per_char(int(agg["tokens"]), int(agg["chars"])),
            "tokens": agg["tokens"],
            "chars": agg["chars"],
            "rows": agg["rows"],
        }
    return out


def count_tokens(text: str, model_id: str) -> int:
    """Token count for ``text`` under ``model_id``'s tokenizer (network).

    Imported lazily so the module is importable without ``transformers``.
    """
    from transformers import AutoTokenizer

    tok = AutoTokenizer.from_pretrained(model_id)
    return len(tok.encode(text, add_special_tokens=False))


def audit_corpora(corpora: dict[str, str], model_ids: list[str]) -> list[dict[str, Any]]:
    """Build audit rows for every (model, corpus) pair.

    ``corpora`` maps a corpus name to its Irish-side text. Returns rows suitable
    for ``summarize_audit``. Requires ``transformers`` + model downloads.
    """
    rows: list[dict[str, Any]] = []
    for model_id in model_ids:
        for corpus_name, text in corpora.items():
            rows.append(
                {
                    "model": model_id,
                    "corpus": corpus_name,
                    "tokens": count_tokens(text, model_id),
                    "chars": len(text),
                }
            )
    return rows


def main() -> int:
    import argparse
    import json

    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--model",
        action="append",
        default=[],
        dest="models",
        help="HF model id to audit (repeatable)",
    )
    ap.add_argument(
        "--corpus",
        action="append",
        default=[],
        dest="corpora",
        help="path to an Irish-side text file (repeatable)",
    )
    args = ap.parse_args()

    if not args.models or not args.corpora:
        print("provide at least one --model and one --corpus")
        return 2

    from pathlib import Path

    corpora = {Path(p).name: Path(p).read_text(encoding="utf-8") for p in args.corpora}
    try:
        rows = audit_corpora(corpora, args.models)
    except ImportError:
        print(
            "transformers not installed — install it (and authenticate to HF) to run "
            "the live tokenizer audit. The aggregation helpers are still importable."
        )
        return 2

    print(json.dumps(summarize_audit(rows), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
