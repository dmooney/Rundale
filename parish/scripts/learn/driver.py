"""End-to-end driver for the self-improving-agent feedback loop.

Usage:

    python parish/scripts/learn/driver.py [--since-days 14] [--local]
        [--dry-run] [--fixture parish/scripts/learn/tests/fixtures/signals.json]

Flow:
  1. Each collector returns a list of Signal records.
  2. extract.py asks Claude for lesson candidates in the existing bullet
     format.
  3. judge.py applies deterministic format asserts and an LLM dedupe
     judge.
  4. write.py rewrites LEARNINGS.md with the accepted candidates.

Costs are reported at the end. Rejection log is written to
docs/proofs/learn-runs/<utc>.md for review.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent.parent

# Allow `python parish/scripts/learn/driver.py` (no -m) by adding the
# parent directory to sys.path so the package import resolves.
if __package__ in (None, ""):
    sys.path.insert(0, str(HERE.parent))
    __package__ = "learn"  # type: ignore[assignment]

from learn import collect_ci, collect_judges, collect_reviews, collect_stop_blocks  # noqa: E402
from learn.extract import ExtractParseError, MAX_CANDIDATES, MODEL, extract  # noqa: E402
from learn.judge import run as run_judge  # noqa: E402
from learn.signals import Signal, dump_signals, load_signals  # noqa: E402
from learn.write import diff_preview, render, write_file  # noqa: E402


def gather_signals(args: argparse.Namespace) -> list[Signal]:
    if args.fixture:
        return load_signals(Path(args.fixture).read_text(encoding="utf-8"))
    out: list[Signal] = []
    if args.local:
        out.extend(collect_judges.collect_local(REPO_ROOT))
    else:
        try:
            out.extend(collect_ci.collect(args.since_days))
        except Exception as exc:  # pragma: no cover
            print(f"[collect_ci] failed: {exc}", file=sys.stderr)
        try:
            out.extend(collect_judges.collect_github(args.since_days))
        except Exception as exc:  # pragma: no cover
            print(f"[collect_judges] failed: {exc}", file=sys.stderr)
        try:
            out.extend(collect_reviews.collect(args.since_days))
        except Exception as exc:  # pragma: no cover
            print(f"[collect_reviews] failed: {exc}", file=sys.stderr)
    out.extend(collect_stop_blocks.collect(REPO_ROOT, args.since_days))
    return out


def signal_counts(signals: list[Signal]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for s in signals:
        counts[s.source] = counts.get(s.source, 0) + 1
    return counts


def write_rejection_log(
    judged: list,
    extract_raw: str,
    out_path: Path,
) -> None:
    out_path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        f"# Learn run — {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "",
        "## Extractor raw response (first 4000 chars)",
        "",
        "```",
        extract_raw[:4000],
        "```",
        "",
        "## Rejected candidates",
        "",
    ]
    rejected = [j for j in judged if not j.accepted]
    if not rejected:
        lines.append("(none)")
    else:
        for j in rejected:
            lines.append(f"### {j.candidate.section}")
            lines.append("")
            lines.append("```markdown")
            lines.append(j.candidate.bullet)
            lines.append("```")
            lines.append("")
            lines.append(f"- reason: {j.reason}")
            if j.duplicate_of:
                lines.append(f"- duplicate_of: `{j.duplicate_of[:160]}`")
            lines.append("")
    out_path.write_text("\n".join(lines), encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--since-days", type=int, default=14)
    parser.add_argument(
        "--local",
        action="store_true",
        help="Use local-only collectors (no GitHub API calls).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the proposed diff against LEARNINGS.md; do not write.",
    )
    parser.add_argument(
        "--fixture",
        help="Read signals from a JSON file instead of running collectors.",
    )
    parser.add_argument(
        "--skip-llm",
        action="store_true",
        help="Skip the extractor + dedupe judge calls (format-asserts only).",
    )
    parser.add_argument(
        "--model",
        default=MODEL,
        help=f"Anthropic model id (default {MODEL}).",
    )
    parser.add_argument(
        "--dump-signals",
        action="store_true",
        help="Print collected signals as JSON and exit.",
    )
    parser.add_argument(
        "--max-candidates",
        type=int,
        default=MAX_CANDIDATES,
        help=(
            f"Hard cap on extractor candidates per run (default {MAX_CANDIDATES}). "
            "Bounds dedupe-judge LLM fan-out."
        ),
    )
    args = parser.parse_args(argv)

    print(f"[learn] collecting signals (since-days={args.since_days}, local={args.local})", file=sys.stderr)
    signals = gather_signals(args)
    counts = signal_counts(signals)
    print(f"[learn] collected {len(signals)} signals: {counts}", file=sys.stderr)

    if args.dump_signals:
        print(dump_signals(signals))
        return 0

    if not signals:
        print("[learn] no signals; nothing to do.", file=sys.stderr)
        return 0

    if args.skip_llm:
        print("[learn] --skip-llm set; exiting after signal dump.", file=sys.stderr)
        print(dump_signals(signals))
        return 0

    if not os.environ.get("ANTHROPIC_API_KEY"):
        print(
            "[learn] ANTHROPIC_API_KEY not set; cannot call extractor. "
            "Pass --dump-signals or --skip-llm to inspect signal flow.",
            file=sys.stderr,
        )
        return 2

    learnings_path = REPO_ROOT / "LEARNINGS.md"
    current = learnings_path.read_text(encoding="utf-8")

    print(f"[learn] extracting candidates with model={args.model}", file=sys.stderr)
    try:
        result = extract(
            signals,
            current,
            model=args.model,
            max_candidates=args.max_candidates,
        )
    except ExtractParseError as exc:
        # Persist the unparseable response so a human can debug the
        # prompt regression, then bail without touching LEARNINGS.md.
        bad_path = REPO_ROOT / "docs" / "proofs" / "learn-runs" / (
            datetime.now(timezone.utc).strftime("%Y-%m-%dT%H-%M-%SZ") + "-parse-error.md"
        )
        bad_path.parent.mkdir(parents=True, exist_ok=True)
        bad_path.write_text(
            f"# Extractor parse error\n\n{exc}\n\n## Raw response\n\n```\n{exc.raw_response[:8000]}\n```\n",
            encoding="utf-8",
        )
        print(
            f"[learn] extractor parse error: {exc}; raw written to "
            f"{bad_path.relative_to(REPO_ROOT)}",
            file=sys.stderr,
        )
        return 3
    print(
        f"[learn] extractor returned {len(result.candidates)} candidate(s); "
        f"usage={result.usage}",
        file=sys.stderr,
    )

    judge_result = run_judge(
        result.candidates, current, repo_root=REPO_ROOT, model=args.model
    )
    accepted = [j for j in judge_result.judged if j.accepted]
    print(
        f"[learn] accepted={len(accepted)} rejected={len(judge_result.judged) - len(accepted)} "
        f"dedupe_usage={judge_result.usage}",
        file=sys.stderr,
    )

    rejection_log = REPO_ROOT / "docs" / "proofs" / "learn-runs" / (
        datetime.now(timezone.utc).strftime("%Y-%m-%dT%H-%M-%SZ") + ".md"
    )
    write_rejection_log(judge_result.judged, result.raw_response, rejection_log)
    print(f"[learn] rejection log: {rejection_log.relative_to(REPO_ROOT)}", file=sys.stderr)

    if not accepted:
        print("[learn] no accepted candidates; LEARNINGS.md unchanged.", file=sys.stderr)
        return 0

    new_text = render(current, accepted, signal_counts=counts)
    if new_text == current:
        print("[learn] LEARNINGS.md unchanged.", file=sys.stderr)
        return 0

    diff = diff_preview(current, new_text)
    if args.dry_run:
        print(diff)
        return 0

    write_file(learnings_path, new_text)
    print(f"[learn] wrote {learnings_path.relative_to(REPO_ROOT)} (+{len(accepted)} bullet(s))", file=sys.stderr)
    print(diff, file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
