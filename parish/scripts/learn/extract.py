"""LLM step: signals → lesson candidates."""

from __future__ import annotations

import dataclasses
import json
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

from .signals import Signal, dump_signals

MODEL = os.environ.get("LEARN_MODEL", "claude-sonnet-4-6")
PROMPT_PATH = Path(__file__).parent / "prompts" / "extract.md"


@dataclass
class LessonCandidate:
    section: str
    bullet: str
    anchor_file: str
    source_signal_indices: list[int] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return dataclasses.asdict(self)


@dataclass
class ExtractResult:
    candidates: list[LessonCandidate]
    usage: dict[str, int]
    raw_response: str


def _strip_fences(text: str) -> str:
    text = text.strip()
    if text.startswith("```"):
        first_newline = text.find("\n")
        if first_newline >= 0:
            text = text[first_newline + 1 :]
        if text.endswith("```"):
            text = text[:-3]
    return text.strip()


def call_anthropic(system: str, user: str, *, model: str = MODEL) -> tuple[str, dict[str, int]]:
    """Single call to Anthropic Messages API with prompt caching on the
    system block. Returns (text, usage_dict)."""
    try:
        import anthropic  # type: ignore
    except ImportError as exc:  # pragma: no cover
        raise SystemExit(
            "anthropic SDK not installed — pip install -r parish/scripts/learn/requirements.txt"
        ) from exc

    client = anthropic.Anthropic()
    resp = client.messages.create(
        model=model,
        max_tokens=4000,
        system=[
            {
                "type": "text",
                "text": system,
                "cache_control": {"type": "ephemeral"},
            }
        ],
        messages=[{"role": "user", "content": user}],
    )
    text = "".join(
        block.text for block in resp.content if getattr(block, "type", None) == "text"
    )
    usage = {
        "input_tokens": getattr(resp.usage, "input_tokens", 0),
        "output_tokens": getattr(resp.usage, "output_tokens", 0),
        "cache_creation_input_tokens": getattr(
            resp.usage, "cache_creation_input_tokens", 0
        ),
        "cache_read_input_tokens": getattr(
            resp.usage, "cache_read_input_tokens", 0
        ),
    }
    return text, usage


def extract(
    signals: list[Signal],
    learnings_md: str,
    *,
    model: str = MODEL,
    system: Optional[str] = None,
) -> ExtractResult:
    system_prompt = system or PROMPT_PATH.read_text(encoding="utf-8")
    user = (
        "# Current LEARNINGS.md\n\n"
        f"```markdown\n{learnings_md}\n```\n\n"
        "# Signals\n\n"
        f"```json\n{dump_signals(signals)}\n```\n"
    )
    text, usage = call_anthropic(system_prompt, user, model=model)
    payload = json.loads(_strip_fences(text))
    raw_candidates = payload.get("candidates", [])
    candidates = [
        LessonCandidate(
            section=c["section"],
            bullet=c["bullet"].rstrip(),
            anchor_file=c.get("anchor_file", ""),
            source_signal_indices=list(c.get("source_signal_indices", [])),
        )
        for c in raw_candidates
    ]
    return ExtractResult(candidates=candidates, usage=usage, raw_response=text)
