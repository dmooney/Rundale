#!/usr/bin/env python3
"""Tier-promotion logic for the rundale-bench funnel.

A candidate advances screen → contender → finalist when its aggregate clears
the tier's bar: overall mean at or above a threshold AND every axis strictly
above a floor (one weak axis sinks an otherwise-strong model). `--include`
force-promotes regardless of score; `--exclude` blocks regardless. Decisions
are explicit so a leaderboard can show why each model advanced or stalled.

Pure module — no I/O, no globals — so the thresholds are unit-testable and the
`promote` subcommand is a thin wrapper.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

AXES = ("character", "authenticity", "language", "responsiveness", "craft")

# (overall_threshold, axis_floor): promote when overall >= threshold AND
# min(axis) > floor. The next tier each maps to.
TIER_RULES = {
    "screen": {"overall": 3.5, "axis_floor": 2.0, "next": "contender"},
    "contender": {"overall": 4.0, "axis_floor": 3.0, "next": "finalist"},
}


@dataclass(frozen=True)
class Decision:
    model_id: str
    promoted: bool
    to_tier: Optional[str]
    reason: str
    overall: Optional[float] = None
    min_axis: Optional[float] = None


def _min_axis(aggregate: dict) -> Optional[float]:
    vals = [aggregate.get(a) for a in AXES if isinstance(aggregate.get(a), (int, float))]
    return min(vals) if vals else None


def decide(
    model_id: str,
    aggregate: dict,
    from_tier: str,
    *,
    include: Optional[set[str]] = None,
    exclude: Optional[set[str]] = None,
) -> Decision:
    """Promotion decision for one candidate's aggregate at `from_tier`."""
    include = include or set()
    exclude = exclude or set()
    if from_tier not in TIER_RULES:
        raise ValueError(f"no promotion rule from tier {from_tier!r} (finalist is terminal)")
    rule = TIER_RULES[from_tier]
    nxt = rule["next"]

    # Exclude wins over everything (an explicit block).
    if model_id in exclude:
        return Decision(model_id, False, None, "excluded by --exclude-models")
    if model_id in include:
        return Decision(model_id, True, nxt, "force-promoted by --include-models")

    overall = aggregate.get("overall")
    min_axis = _min_axis(aggregate)
    if not isinstance(overall, (int, float)) or min_axis is None:
        return Decision(model_id, False, None, "no usable aggregate (unjudged?)", overall, min_axis)

    if overall < rule["overall"]:
        return Decision(model_id, False, None,
                        f"overall {overall:.2f} < {rule['overall']}", overall, min_axis)
    if min_axis <= rule["axis_floor"]:
        return Decision(model_id, False, None,
                        f"min axis {min_axis:.2f} <= {rule['axis_floor']}", overall, min_axis)
    return Decision(model_id, True, nxt,
                    f"overall {overall:.2f} >= {rule['overall']} and min axis {min_axis:.2f} > {rule['axis_floor']}",
                    overall, min_axis)


def promote(
    aggregates: dict[str, dict],
    from_tier: str,
    *,
    include: Optional[set[str]] = None,
    exclude: Optional[set[str]] = None,
) -> list[Decision]:
    """Promotion decisions for every candidate, sorted by model id.

    `aggregates` maps model_id -> the dialogue summary dict (overall + per-axis
    means). Returns one Decision per candidate (and per force-included id even
    if it has no aggregate).
    """
    include = include or set()
    exclude = exclude or set()
    ids = set(aggregates) | include
    return [decide(mid, aggregates.get(mid, {}), from_tier, include=include, exclude=exclude)
            for mid in sorted(ids)]


def promoted_ids(decisions: list[Decision]) -> list[str]:
    return [d.model_id for d in decisions if d.promoted]
