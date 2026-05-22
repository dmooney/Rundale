"""Dedupe + format judge for lesson candidates.

Format asserts are deterministic (no LLM call). The dedupe assert
batches all candidates into a single LLM call to keep cost down.
"""

from __future__ import annotations

import dataclasses
import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from .extract import LessonCandidate, _strip_fences, call_anthropic, MODEL

PROMPT_PATH = Path(__file__).parent / "prompts" / "dedupe.md"

BULLET_PREFIX = re.compile(r"^\s*-\s+\*\*[^*]+\.\*\*\s")
MODEL_LEAK = re.compile(r"\b(claude|opus|sonnet|haiku|gpt-?\d|gemini)\b", re.IGNORECASE)
PATH_OR_SYMBOL = re.compile(r"`[^`]+`|[\w./-]+/[\w./-]+\.(rs|svelte|ts|tsx|js|py|sh|toml|yml|yaml|md)")


@dataclass
class JudgedCandidate:
    candidate: LessonCandidate
    accepted: bool
    reason: str
    duplicate_of: Optional[str] = None


@dataclass
class JudgeResult:
    judged: list[JudgedCandidate]
    usage: dict[str, int] = field(default_factory=dict)


def _format_asserts(candidate: LessonCandidate, repo_root: Path) -> Optional[str]:
    bullet = candidate.bullet.strip()
    if not bullet.startswith("- "):
        return "must start with `- `"
    lines = bullet.splitlines()
    if len(lines) > 3:
        return f"too long ({len(lines)} lines; max 3)"
    if not BULLET_PREFIX.match(bullet):
        return "must start with `- **<bold claim>.** ` (note the period and space)"
    if MODEL_LEAK.search(bullet):
        return "mentions a model / agent name — keep lessons tool-agnostic"
    if not PATH_OR_SYMBOL.search(bullet):
        return "no file path or backticked symbol referenced — un-anchored claim"
    anchor = candidate.anchor_file
    if anchor:
        if not (repo_root / anchor).exists():
            return f"anchor_file does not exist on disk: {anchor}"
    return None


def judge_format(
    candidates: list[LessonCandidate], repo_root: Path
) -> list[JudgedCandidate]:
    out: list[JudgedCandidate] = []
    for c in candidates:
        fail = _format_asserts(c, repo_root)
        if fail:
            out.append(JudgedCandidate(candidate=c, accepted=False, reason=fail))
        else:
            out.append(JudgedCandidate(candidate=c, accepted=True, reason="format ok"))
    return out


def judge_dedupe(
    judged: list[JudgedCandidate],
    learnings_md: str,
    *,
    model: str = MODEL,
) -> dict[str, int]:
    """LLM-judge each surviving candidate for substantive duplication.
    Updates `judged` in place. Returns aggregated token usage."""
    survivors = [j for j in judged if j.accepted]
    if not survivors:
        return {}
    system = PROMPT_PATH.read_text(encoding="utf-8")
    usage_total = {
        "input_tokens": 0,
        "output_tokens": 0,
        "cache_creation_input_tokens": 0,
        "cache_read_input_tokens": 0,
    }
    for j in survivors:
        user = (
            "# Existing LEARNINGS.md\n\n"
            f"```markdown\n{learnings_md}\n```\n\n"
            "# Candidate\n\n"
            f"section: {j.candidate.section}\n"
            f"bullet:\n{j.candidate.bullet}\n"
        )
        text, usage = call_anthropic(system, user, model=model)
        for k in usage_total:
            usage_total[k] += usage.get(k, 0)
        try:
            payload = json.loads(_strip_fences(text))
        except json.JSONDecodeError:
            j.accepted = False
            j.reason = f"dedupe judge returned non-JSON: {text[:200]!r}"
            continue
        dup = payload.get("duplicate_of")
        if dup:
            j.accepted = False
            j.reason = payload.get("reasoning") or "duplicate"
            j.duplicate_of = dup
    return usage_total


def run(
    candidates: list[LessonCandidate],
    learnings_md: str,
    *,
    repo_root: Path,
    model: str = MODEL,
    skip_llm: bool = False,
) -> JudgeResult:
    judged = judge_format(candidates, repo_root)
    if skip_llm:
        return JudgeResult(judged=judged, usage={})
    usage = judge_dedupe(judged, learnings_md, model=model)
    return JudgeResult(judged=judged, usage=usage)
