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

# Hard cap on candidates returned per run. Bounds dedupe-judge LLM
# calls (one per surviving candidate) so a runaway extractor response
# can't fan out into 100+ paid API calls.
MAX_CANDIDATES = 12


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


class ExtractParseError(RuntimeError):
    """Extractor returned non-JSON or schema-invalid output."""

    def __init__(self, message: str, raw_response: str):
        super().__init__(message)
        self.raw_response = raw_response


def parse_candidates(text: str) -> list[LessonCandidate]:
    """Parse the LLM response into LessonCandidate records.

    Raises ExtractParseError with the raw text attached so the driver
    can persist it to the rejection log instead of crashing the run.
    """
    try:
        payload = json.loads(_strip_fences(text))
    except json.JSONDecodeError as exc:
        raise ExtractParseError(
            f"extractor returned non-JSON: {exc.msg} at pos {exc.pos}", text
        ) from exc
    if not isinstance(payload, dict):
        raise ExtractParseError(
            f"extractor returned non-object JSON (got {type(payload).__name__})", text
        )
    raw_candidates = payload.get("candidates")
    if raw_candidates is None:
        raise ExtractParseError("extractor JSON missing 'candidates' key", text)
    if not isinstance(raw_candidates, list):
        raise ExtractParseError(
            f"'candidates' must be a list, got {type(raw_candidates).__name__}", text
        )
    out: list[LessonCandidate] = []
    for i, c in enumerate(raw_candidates):
        if not isinstance(c, dict):
            raise ExtractParseError(
                f"candidates[{i}] is not an object (got {type(c).__name__})", text
            )
        for required in ("section", "bullet"):
            if not isinstance(c.get(required), str) or not c[required].strip():
                raise ExtractParseError(
                    f"candidates[{i}].{required} must be a non-empty string", text
                )
        out.append(
            LessonCandidate(
                section=c["section"].strip(),
                bullet=c["bullet"].rstrip(),
                anchor_file=str(c.get("anchor_file") or "").strip(),
                source_signal_indices=[
                    int(x) for x in (c.get("source_signal_indices") or [])
                ],
            )
        )
    return out


def extract(
    signals: list[Signal],
    learnings_md: str,
    *,
    model: str = MODEL,
    system: Optional[str] = None,
    max_candidates: int = MAX_CANDIDATES,
) -> ExtractResult:
    """Run the extractor LLM call.

    The system block holds the static prompt + the (slow-changing)
    current LEARNINGS.md, so multiple calls within the same run pay
    for it once (prompt caching). The user block holds the signals,
    which are unique per run.
    """
    system_prompt = system or PROMPT_PATH.read_text(encoding="utf-8")
    full_system = (
        f"{system_prompt}\n\n"
        "# Current LEARNINGS.md (do not paraphrase existing bullets)\n\n"
        f"```markdown\n{learnings_md}\n```\n"
    )
    user = "# Signals\n\n" f"```json\n{dump_signals(signals)}\n```\n"
    text, usage = call_anthropic(full_system, user, model=model)
    candidates = parse_candidates(text)
    if len(candidates) > max_candidates:
        candidates = candidates[:max_candidates]
    return ExtractResult(candidates=candidates, usage=usage, raw_response=text)
