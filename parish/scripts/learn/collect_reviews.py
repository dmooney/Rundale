"""Collector: review-bot comments on recent PRs.

Picks up actionable findings from automated reviewers (ultrareview,
code-reviewer, Copilot, Gemini). Skips human reviewer comments — the
loop only learns from machine-readable patterns at scale.
"""

from __future__ import annotations

import argparse
import re
from datetime import datetime, timedelta, timezone

from . import github_api as gh
from .signals import Signal, dump_signals, truncate

BOT_LOGINS = {
    "copilot-pull-request-reviewer[bot]",
    "ultrareview-bot",
    "code-reviewer-bot",
    "gemini-code-assist[bot]",
    "claude-bot[bot]",
}

ACTIONABLE_HINT = re.compile(
    r"(should|must|consider|bug|security|leak|race|null|panic|undefined|"
    r"missing|incorrect|broken|regression|TODO|FIXME)",
    re.IGNORECASE,
)


def collect(since_days: int = 14, repo: str = gh.DEFAULT_REPO) -> list[Signal]:
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
        number = pr["number"]
        for comment in gh.paginate(
            f"/repos/{repo}/pulls/{number}/comments",
            per_page=50,
            max_pages=1,
        ):
            user = (comment.get("user") or {}).get("login", "")
            if user.lower() not in {b.lower() for b in BOT_LOGINS}:
                continue
            body = comment.get("body") or ""
            if not ACTIONABLE_HINT.search(body):
                continue
            anchor = f"PR#{number} {comment.get('path', '?')}:{comment.get('line') or comment.get('original_line') or '?'}"
            out.append(
                Signal(
                    source="review",
                    anchor=anchor,
                    summary=f"{user} on PR #{number}",
                    raw_excerpt=truncate(body, 1500),
                    extra={
                        "html_url": comment.get("html_url"),
                        "path": comment.get("path"),
                        "line": comment.get("line"),
                        "user": user,
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
