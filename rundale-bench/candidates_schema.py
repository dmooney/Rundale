#!/usr/bin/env python3
"""Schema / consistency checks for ``candidates_local_mlx.toml`` (TD-004).

The local MLX fleet catalog is hand-edited: model ids, quantization tags, RAM
estimates, and slot assignments are all typed by hand. A typo (duplicate
``hf_repo``, missing ``peak_ram_gb_est``, a negative RAM estimate, or an unknown
``slot``) used to surface only mid-sweep — after a model had already downloaded
— or silently as a wrong leaderboard row. This module validates the catalog up
front so ``local_runner.py`` and a unit test can both reject a malformed fleet
before any server is spawned.

Pure functions only: no I/O beyond ``load_candidates`` reading the TOML. That
keeps the validation logic testable without a live MLX install.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import tomllib

# Fields every candidate row must carry. ``peak_ram_gb_est`` gates the fitness
# check (OOM safety) so its absence or a non-positive value is a hard error.
REQUIRED_FIELDS = ("hf_repo", "slot", "quant", "peak_ram_gb_est")
VALID_SLOTS = {"tiny", "large"}


def load_candidates(path: Path) -> list[dict[str, Any]]:
    """Parse the ``[[candidate]]`` array from a candidates TOML file."""
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    return data.get("candidate", [])


def validate_candidates(rows: list[dict[str, Any]]) -> list[str]:
    """Return a list of human-readable problems. Empty list = catalog is valid.

    Checks, per row and across rows:
      - every ``REQUIRED_FIELDS`` key is present,
      - ``slot`` is one of ``VALID_SLOTS``,
      - ``peak_ram_gb_est`` is a positive number,
      - ``params_b`` (when present) is a non-negative number,
      - ``delete_after_bench`` (when present) is a bool,
      - ``hf_repo`` is unique across the fleet.
    """
    problems: list[str] = []
    seen: dict[str, int] = {}

    for i, row in enumerate(rows):
        label = row.get("hf_repo", f"<row {i}>")

        for field in REQUIRED_FIELDS:
            if field not in row:
                problems.append(f"{label}: missing required field {field!r}")

        slot = row.get("slot")
        if slot is not None and slot not in VALID_SLOTS:
            problems.append(f"{label}: slot {slot!r} not in {sorted(VALID_SLOTS)}")

        est = row.get("peak_ram_gb_est")
        if est is not None:
            if not isinstance(est, (int, float)) or isinstance(est, bool):
                problems.append(f"{label}: peak_ram_gb_est must be a number, got {est!r}")
            elif est <= 0:
                problems.append(f"{label}: peak_ram_gb_est must be positive, got {est!r}")

        params = row.get("params_b")
        if params is not None:
            if not isinstance(params, (int, float)) or isinstance(params, bool):
                problems.append(f"{label}: params_b must be a number, got {params!r}")
            elif params < 0:
                problems.append(f"{label}: params_b must be non-negative, got {params!r}")

        flag = row.get("delete_after_bench")
        if flag is not None and not isinstance(flag, bool):
            problems.append(f"{label}: delete_after_bench must be a bool, got {flag!r}")

        repo = row.get("hf_repo")
        if isinstance(repo, str):
            if repo in seen:
                problems.append(f"{repo}: duplicate hf_repo (rows {seen[repo]} and {i})")
            else:
                seen[repo] = i

    return problems


def candidates_to_delete(rows: list[dict[str, Any]]) -> list[str]:
    """Return the ``hf_repo`` ids flagged ``delete_after_bench = true``.

    A sweep finaliser honours these by evicting the HF cache slot after the
    candidate's rows are written, so disk discipline is data-driven rather than
    a manual post-sweep cleanup step.
    """
    return [
        str(row["hf_repo"])
        for row in rows
        if row.get("delete_after_bench") is True and row.get("hf_repo")
    ]


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "toml",
        nargs="?",
        default=str(Path(__file__).resolve().parent / "candidates_local_mlx.toml"),
        help="path to candidates TOML (default: candidates_local_mlx.toml)",
    )
    args = ap.parse_args()

    rows = load_candidates(Path(args.toml))
    problems = validate_candidates(rows)
    if problems:
        print(f"{len(problems)} problem(s) in {args.toml}:")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"{len(rows)} candidate(s) OK in {args.toml}")
    to_delete = candidates_to_delete(rows)
    if to_delete:
        print(f"flagged delete_after_bench: {', '.join(to_delete)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
