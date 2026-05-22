"""Collector: Stop-hook proof-required blocks.

Reads `.claude/logs/proof-blocks.jsonl` — one JSON object per line,
appended by `.claude/hooks/Stop--proof-required.sh` whenever the hook
blocks a session. The log is gitignored and lives in the workspace.
Workflow runs that want stop-block signal must upload the file as an
artifact and download it on the next learn run.
"""

from __future__ import annotations

import argparse
import json
from datetime import datetime, timedelta, timezone
from pathlib import Path

from .signals import Signal, dump_signals, truncate


def collect(root: Path, since_days: int = 14) -> list[Signal]:
    log = root / ".claude" / "logs" / "proof-blocks.jsonl"
    if not log.exists():
        return []
    cutoff = datetime.now(timezone.utc) - timedelta(days=since_days)
    out: list[Signal] = []
    for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        ts_str = rec.get("ts")
        if ts_str:
            try:
                ts = datetime.fromisoformat(ts_str.replace("Z", "+00:00"))
            except ValueError:
                ts = None
            if ts and ts < cutoff:
                continue
        reason = rec.get("reason") or ""
        paths = rec.get("paths_touched") or []
        anchor = paths[0] if paths else "session"
        out.append(
            Signal(
                source="stop-block",
                anchor=f"stop-hook {anchor}",
                summary=f"Stop hook blocked — {rec.get('evidence_type', 'no proof')}",
                raw_excerpt=truncate(reason, 1500),
                extra={"paths_touched": paths, "evidence_type": rec.get("evidence_type")},
            )
        )
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--since-days", type=int, default=14)
    args = parser.parse_args()
    print(dump_signals(collect(Path(args.root).resolve(), args.since_days)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
