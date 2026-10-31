"""promptfoo hooks for the prompt lab: tests, provider and judge assertion.

promptfoo is the viewer and one-off runner; the research loop (lab.py loop)
calls the same functions directly so it can meter spend and stop on its rules.
Each prompt is a variant name ("baseline" or a directory under prompts/); the
provider renders the production request for the test's case.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from promptlab import content, judges, models, render, research  # noqa: E402
from promptlab import variant as variant_mod  # noqa: E402


def _case(case_id: str) -> content.Case:
    return next(c for c in content.all_cases() if c.id == case_id)


def tests(config=None):
    """Training cases by default; LAB_SPLIT=heldout for the owner's review."""
    split = os.environ.get("LAB_SPLIT", "train")
    tags = set(filter(None, os.environ.get("LAB_TAGS", "").split(",")))
    return [
        {
            "description": f"{c.id} ({', '.join(c.tags)})",
            "vars": {"case": c.id, "player_line": c.player_line},
            "metadata": {"tags": list(c.tags), "split": c.split},
        }
        for c in content.cases(split, tags=tags or None)
    ]


def call_api(prompt, options, context):
    """Provider: config.model names a key of config.json `models`, or stand_in."""
    cfg = research.load_config()
    which = (options.get("config") or {}).get("model", "production")
    mcfg = cfg["stand_in"] if which == "stand_in" else cfg["models"][which]
    name = prompt.strip()
    variant = render.BASELINE if name == render.BASELINE else variant_mod.load_named(name)
    request = render.render(variant, _case(context["vars"]["case"]))
    reply = models.generate(mcfg, request)
    if not reply.ok:
        return {"error": reply.error, "output": reply.raw or ""}
    return {
        "output": reply.dialogue,
        "tokenUsage": {
            "prompt": reply.input_tokens,
            "completion": reply.output_tokens,
            "total": reply.input_tokens + reply.output_tokens,
        },
        "cost": reply.cost_usd,
        "metadata": {"request_chars": request.size_chars, "model": reply.model},
    }


def assert_reply(output, context):
    """Mean of the configured judges, 0-100 mapped to 0-1; a split over the gap flag fails."""
    cfg = research.load_config()
    case = _case(context["vars"]["case"])
    verdicts = [
        judges.judge(name, judges.resolve(j), case, output) for name, j in cfg["judges"].items()
    ]
    scores = [v.score for v in verdicts if v.ok]
    if not scores:
        return {
            "pass": False,
            "score": 0,
            "reason": "no judge returned a score: " + "; ".join(str(v.error) for v in verdicts),
        }
    mean = sum(scores) / len(scores)
    gap = max(scores) - min(scores)
    flagged = gap > cfg["research"]["judge_gap_flag"] or len(scores) < len(verdicts)
    reason = " | ".join(
        f"{v.judge} {v.score}: {v.reason}" if v.ok else f"{v.judge} failed: {v.error}"
        for v in verdicts
    )
    return {
        "pass": mean >= 70 and not flagged,
        "score": mean / 100,
        "reason": ("JUDGES SPLIT. " if flagged else "") + reason,
    }
