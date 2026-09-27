"""Normaliser for the one run-to-run difference left by design.

`limerick-server`'s game clock runs in real time from session creation until
the scenario's `/pause`, so game timestamps in the live session (e.g. a
task's `assigned_at`) carry wall-clock seconds. Everything else is
deterministic at the source (#2033 item 2); do not add a normaliser for a
new difference without first removing its cause.
"""

from __future__ import annotations

import re

GAME_SECONDS = re.compile(r"(\d{4}-\d\d-\d\dT\d\d:\d\d):\d\d(\.\d+)?Z")


def game_seconds(text: str) -> str:
    return GAME_SECONDS.sub(r"\1:<s>Z", text)
