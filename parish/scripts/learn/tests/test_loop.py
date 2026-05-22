"""Offline unit tests — no network, no LLM calls."""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PKG = HERE.parent
sys.path.insert(0, str(PKG.parent))

from learn.extract import LessonCandidate  # noqa: E402
from learn.judge import judge_format, _format_asserts  # noqa: E402
from learn.signals import load_signals, dump_signals, truncate  # noqa: E402
from learn.write import _split_existing, render, FOOTER_RE  # noqa: E402
from learn.judge import JudgedCandidate  # noqa: E402

REPO_ROOT = HERE.parent.parent.parent.parent


def test_load_dump_roundtrip() -> None:
    raw = (HERE / "fixtures" / "signals.json").read_text(encoding="utf-8")
    sigs = load_signals(raw)
    assert len(sigs) == 4
    assert sigs[0].source == "ci"
    assert sigs[2].extra["user"] == "ultrareview-bot"
    redumped = json.loads(dump_signals(sigs))
    assert len(redumped) == 4


def test_truncate_short_passthrough() -> None:
    assert truncate("hello") == "hello"
    long = "x" * 500
    out = truncate(long, 100)
    assert "[truncated" in out
    assert len(out) < len(long)


def test_format_asserts_accept() -> None:
    good = LessonCandidate(
        section="Engine + runtime",
        bullet="- **`apply_movement` is the shared movement seam.** Call it from `parish/crates/parish-core/src/game_session.rs`, not direct location writes in `parish-npc/src/autonomous/mod.rs`.",
        anchor_file="parish/crates/parish-core/src/game_session.rs",
    )
    assert _format_asserts(good, REPO_ROOT) is None


def test_format_asserts_reject_model_leak() -> None:
    bad = LessonCandidate(
        section="Engine + runtime",
        bullet="- **Claude should run `cargo test` first.** Always check `parish/crates/parish-cli/src/main.rs` after edits.",
        anchor_file="",
    )
    reason = _format_asserts(bad, REPO_ROOT)
    assert reason is not None
    assert "model" in reason or "agent" in reason


def test_format_asserts_reject_no_anchor() -> None:
    bad = LessonCandidate(
        section="Engine + runtime",
        bullet="- **Always be careful with imports.** Otherwise you get errors.",
        anchor_file="",
    )
    assert _format_asserts(bad, REPO_ROOT) is not None


def test_format_asserts_reject_too_long() -> None:
    bad = LessonCandidate(
        section="Engine + runtime",
        bullet=(
            "- **Foo `parish/crates/parish-cli/src/main.rs`.** Line one.\n"
            "  Line two.\n"
            "  Line three.\n"
            "  Line four."
        ),
        anchor_file="",
    )
    reason = _format_asserts(bad, REPO_ROOT)
    assert reason is not None
    assert "too long" in reason


def test_split_existing_preserves_intro() -> None:
    text = (
        "# LEARNINGS\n\n"
        "intro paragraph.\n\n"
        "## Section A\n\n"
        "- **First.** body of first bullet referencing `foo.rs`.\n\n"
        "- **Second.** body of second referencing `bar.rs`.\n\n"
        "## Section B\n\n"
        "- **Third.** body of third referencing `baz.rs`.\n"
    )
    preamble, sections, order = _split_existing(text)
    assert "intro paragraph" in preamble
    assert order == ["Section A", "Section B"]
    assert len(sections["Section A"]) == 2
    assert sections["Section A"][0].startswith("- **First.**")
    assert sections["Section B"][0].startswith("- **Third.**")


def test_render_adds_new_bullet_and_footer() -> None:
    text = (
        "# LEARNINGS\n\nintro.\n\n## Engine + runtime\n\n"
        "- **Existing.** existing rationale referencing `parish/foo.rs`.\n"
    )
    new = LessonCandidate(
        section="Engine + runtime",
        bullet="- **New thing.** rationale referencing `parish/bar.rs`.",
        anchor_file="parish/bar.rs",
    )
    judged = JudgedCandidate(candidate=new, accepted=True, reason="format ok")
    out = render(text, [judged], signal_counts={"ci": 2})
    assert "- **Existing.**" in out
    assert "- **New thing.**" in out
    assert FOOTER_RE.search(out)
    assert "ci:2" in out


def test_render_creates_new_section() -> None:
    text = "# LEARNINGS\n\nintro.\n\n## Engine + runtime\n\n- **Existing.** ref `parish/foo.rs`.\n"
    new = LessonCandidate(
        section="New Section",
        bullet="- **Novel.** ref `parish/x.rs`.",
        anchor_file="",
    )
    judged = JudgedCandidate(candidate=new, accepted=True, reason="format ok")
    out = render(text, [judged])
    assert "## New Section" in out
    assert "- **Novel.**" in out


def test_judge_format_batch() -> None:
    cands = [
        LessonCandidate(
            section="Engine + runtime",
            bullet="- **`apply_movement` is shared.** See `parish/crates/parish-core/src/game_session.rs`.",
            anchor_file="parish/crates/parish-core/src/game_session.rs",
        ),
        LessonCandidate(
            section="Other",
            bullet="- **No path here.** Just vibes.",
            anchor_file="",
        ),
    ]
    judged = judge_format(cands, REPO_ROOT)
    assert judged[0].accepted is True
    assert judged[1].accepted is False
