"""Collector: rejected proof bundles.

Two modes:
  - GitHub mode (default): walks recent merged PRs for the
    `parish-proof-bundle` comment posted by `just attach-proof`, greps
    for `Acceptance criteria: not met` or other rejection markers.
  - Local mode (--local): walks `.proofs/<id>/judge.md` files on disk.
"""

from __future__ import annotations

import argparse
import re
from datetime import datetime, timedelta, timezone
from pathlib import Path

from . import github_api as gh
from .signals import Signal, dump_signals, truncate

REJECT_PATTERNS = [
    re.compile(r"Acceptance criteria:\s*not met", re.IGNORECASE),
    re.compile(r"verdict:\s*reject", re.IGNORECASE),
    re.compile(r"reject(ed)?\b", re.IGNORECASE),
    re.compile(r"insufficient evidence", re.IGNORECASE),
]


def _is_rejection(body: str) -> bool:
    return any(p.search(body) for p in REJECT_PATTERNS[:2])


def _context_around(body: str, max_chars: int = 2000) -> str:
    for pat in REJECT_PATTERNS:
        m = pat.search(body)
        if m:
            start = max(0, m.start() - max_chars // 2)
            end = min(len(body), m.end() + max_chars // 2)
            return truncate(body[start:end], max_chars)
    return truncate(body[-max_chars:], max_chars)


def collect_github(since_days: int = 14, repo: str = gh.DEFAULT_REPO) -> list[Signal]:
    cutoff = datetime.now(timezone.utc) - timedelta(days=since_days)
    out: list[Signal] = []
    for pr in gh.paginate(
        f"/repos/{repo}/pulls",
        per_page=30,
        max_pages=2,
        state="closed",
        sort="updated",
        direction="desc",
    ):
        updated = pr.get("updated_at")
        if not updated:
            continue
        if datetime.fromisoformat(updated.replace("Z", "+00:00")) < cutoff:
            break
        if not pr.get("merged_at"):
            continue
        number = pr["number"]
        comments = gh.paginate(
            f"/repos/{repo}/issues/{number}/comments", per_page=50, max_pages=1
        )
        for comment in comments:
            body = comment.get("body") or ""
            if "parish-proof-bundle" not in body and "judge.md" not in body:
                continue
            if not _is_rejection(body):
                continue
            out.append(
                Signal(
                    source="judge",
                    anchor=f"PR#{number} judge",
                    summary=f"PR #{number}: {pr.get('title', '').strip()}",
                    raw_excerpt=_context_around(body),
                    extra={"html_url": comment.get("html_url"), "pr": number},
                )
            )
    return out


def collect_local(root: Path) -> list[Signal]:
    proofs_dir = root / ".proofs"
    if not proofs_dir.exists():
        return []
    out: list[Signal] = []
    for judge in sorted(proofs_dir.glob("*/judge.md")):
        try:
            body = judge.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if not _is_rejection(body):
            continue
        task_id = judge.parent.name
        out.append(
            Signal(
                source="judge",
                anchor=f"local .proofs/{task_id}/judge.md",
                summary=f"local judge rejection ({task_id})",
                raw_excerpt=_context_around(body),
                extra={"path": str(judge)},
            )
        )
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--since-days", type=int, default=14)
    parser.add_argument("--repo", default=gh.DEFAULT_REPO)
    parser.add_argument("--local", action="store_true")
    parser.add_argument("--root", default=".")
    args = parser.parse_args()
    if args.local:
        sigs = collect_local(Path(args.root).resolve())
    else:
        sigs = collect_github(args.since_days, args.repo)
    print(dump_signals(sigs))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
