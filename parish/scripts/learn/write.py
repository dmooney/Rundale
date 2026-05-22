"""Rewrite LEARNINGS.md with accepted lesson candidates.

"Takes over" the file: preserves the H1 + the intro paragraph (lines
1..N up to the first `## `), then re-emits all existing bullets grouped
under their original `## ` sections, then inserts accepted new bullets
at the bottom of each matching section (creating a new section if no
existing one matches). Appends a single-line metadata footer.
"""

from __future__ import annotations

import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from .judge import JudgedCandidate

FOOTER_RE = re.compile(
    r"\n*<!-- self-improving-loop:[^\n]*-->\s*$", re.DOTALL
)
SECTION_HEADER = re.compile(r"^##\s+(.+?)\s*$", re.MULTILINE)


def _split_existing(text: str) -> tuple[str, dict[str, list[str]], list[str]]:
    """Return (preamble, {section_name: [bullet_block, ...]}, ordered_sections)."""
    text = FOOTER_RE.sub("", text)
    matches = list(SECTION_HEADER.finditer(text))
    if not matches:
        return text.rstrip() + "\n", {}, []
    preamble = text[: matches[0].start()].rstrip() + "\n\n"
    sections: dict[str, list[str]] = {}
    order: list[str] = []
    for i, m in enumerate(matches):
        name = m.group(1).strip()
        start = m.end()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        body = text[start:end]
        bullets = _split_bullets(body)
        sections[name] = bullets
        order.append(name)
    return preamble, sections, order


def _split_bullets(body: str) -> list[str]:
    """Each top-level bullet (line starting with `- `) plus its indented
    continuation lines is one block. Blank lines between bullets are
    boundaries."""
    blocks: list[str] = []
    current: list[str] = []
    for line in body.splitlines():
        if line.startswith("- "):
            if current:
                blocks.append("\n".join(current).rstrip())
            current = [line]
        elif current:
            if line.strip() == "" and current:
                blocks.append("\n".join(current).rstrip())
                current = []
            else:
                current.append(line)
    if current:
        blocks.append("\n".join(current).rstrip())
    return [b for b in blocks if b.strip()]


def render(
    existing: str,
    accepted: list[JudgedCandidate],
    *,
    signal_counts: Optional[dict[str, int]] = None,
) -> str:
    preamble, sections, order = _split_existing(existing)
    for j in accepted:
        sec = j.candidate.section.strip()
        if sec not in sections:
            sections[sec] = []
            order.append(sec)
        sections[sec].append(j.candidate.bullet.rstrip())

    parts = [preamble.rstrip() + "\n"]
    for name in order:
        bullets = sections.get(name, [])
        if not bullets:
            continue
        parts.append(f"\n## {name}\n\n")
        parts.append("\n\n".join(bullets))
        parts.append("\n")

    ts = datetime.now(timezone.utc).isoformat(timespec="minutes").replace("+00:00", "Z")
    counts = signal_counts or {}
    counts_str = ",".join(f"{k}:{v}" for k, v in sorted(counts.items())) or "none"
    parts.append(f"\n<!-- self-improving-loop: run={ts} signals={counts_str} -->\n")
    return "".join(parts)


def write_file(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")


def diff_preview(old: str, new: str) -> str:
    import difflib

    return "".join(
        difflib.unified_diff(
            old.splitlines(keepends=True),
            new.splitlines(keepends=True),
            fromfile="LEARNINGS.md (before)",
            tofile="LEARNINGS.md (after)",
            n=3,
        )
    )
