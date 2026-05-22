"""Signal record passed between collectors and the extractor.

A Signal is one piece of evidence that some agent-driven workflow went
wrong — a failed CI step, a rejected proof bundle, a review-bot
comment, a Stop-hook block. The extractor consumes a list of these and
emits LessonCandidate records (see extract.py).
"""

from __future__ import annotations

import dataclasses
import json
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Iterable, Optional


@dataclass
class Signal:
    source: str
    anchor: str
    summary: str
    raw_excerpt: str
    ts: str = field(
        default_factory=lambda: datetime.now(timezone.utc).isoformat(timespec="seconds")
    )
    extra: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return dataclasses.asdict(self)


def dump_signals(signals: Iterable[Signal]) -> str:
    return json.dumps([s.to_dict() for s in signals], indent=2, sort_keys=True)


def load_signals(text: str) -> list[Signal]:
    raw = json.loads(text)
    out: list[Signal] = []
    for item in raw:
        out.append(
            Signal(
                source=item["source"],
                anchor=item["anchor"],
                summary=item["summary"],
                raw_excerpt=item["raw_excerpt"],
                ts=item.get(
                    "ts",
                    datetime.now(timezone.utc).isoformat(timespec="seconds"),
                ),
                extra=item.get("extra", {}),
            )
        )
    return out


def truncate(text: Optional[str], limit: int = 4000) -> str:
    if not text:
        return ""
    if len(text) <= limit:
        return text
    head = text[: limit // 2]
    tail = text[-limit // 2 :]
    return f"{head}\n... [truncated {len(text) - limit} chars] ...\n{tail}"
