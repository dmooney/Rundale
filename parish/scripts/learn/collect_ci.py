"""Collector: failed CI runs on recent pushes to main."""

from __future__ import annotations

import argparse
import re
from datetime import datetime, timedelta, timezone

from . import github_api as gh
from .signals import Signal, dump_signals, truncate

FAIL_HINT = re.compile(
    r"(error\[E\d+\]|panicked at|FAILED|assertion `?\w+`? failed|Error:\s|"
    r"thread '\w+' panicked|Failed to compile|cannot find|undefined reference|"
    r"npm ERR!|cypress run failed|Process completed with exit code [1-9])",
    re.IGNORECASE,
)


def _extract_failure_lines(log: str, window: int = 30) -> str:
    """Return up to ~120 lines anchored at the first hit per file region."""
    if not log:
        return ""
    lines = log.splitlines()
    hits = [i for i, ln in enumerate(lines) if FAIL_HINT.search(ln)]
    if not hits:
        return truncate("\n".join(lines[-window * 2 :]), 4000)
    chosen: list[str] = []
    seen_regions: set[int] = set()
    for idx in hits[:4]:
        region = idx // 50
        if region in seen_regions:
            continue
        seen_regions.add(region)
        start = max(0, idx - window // 2)
        end = min(len(lines), idx + window)
        chosen.append("\n".join(lines[start:end]))
    return truncate("\n---\n".join(chosen), 4000)


def collect(since_days: int = 14, repo: str = gh.DEFAULT_REPO) -> list[Signal]:
    cutoff = datetime.now(timezone.utc) - timedelta(days=since_days)
    cutoff_str = cutoff.isoformat(timespec="seconds").replace("+00:00", "Z")
    out: list[Signal] = []
    for run in gh.paginate(
        f"/repos/{repo}/actions/runs",
        per_page=30,
        max_pages=2,
        status="completed",
        branch="main",
        created=f">={cutoff_str}",
    ):
        if run.get("conclusion") != "failure":
            continue
        run_id = run["id"]
        log_text = gh.fetch_log_text(
            f"/repos/{repo}/actions/runs/{run_id}/logs", max_bytes=120_000
        )
        excerpt = _extract_failure_lines(log_text)
        if not excerpt:
            continue
        out.append(
            Signal(
                source="ci",
                anchor=f"run#{run_id} ({run.get('name', 'workflow')}, {run.get('head_sha', '')[:7]})",
                summary=f"{run.get('name')}: {run.get('display_title', '').strip()}",
                raw_excerpt=excerpt,
                extra={
                    "html_url": run.get("html_url"),
                    "head_sha": run.get("head_sha"),
                    "workflow": run.get("name"),
                    "event": run.get("event"),
                },
            )
        )
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--since-days", type=int, default=14)
    parser.add_argument("--repo", default=gh.DEFAULT_REPO)
    args = parser.parse_args()
    print(dump_signals(collect(args.since_days, args.repo)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
