"""Regression contract: any failure in the TestFlight upload job opens the failure issue (#2474)."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

import yaml

ROOT = Path(__file__).resolve().parents[1]
ISSUE_STEP = "Report the failed upload in an issue"
DECIDE_STEP = "Check that main has an app change not yet delivered"


def upload_steps() -> list[dict[str, Any]]:
    document: dict[str, Any] = yaml.safe_load(
        (ROOT / ".github" / "workflows" / "testflight.yml").read_text(encoding="utf-8")
    )
    return document["jobs"]["upload"]["steps"]


def step_named(name: str) -> dict[str, Any]:
    for step in upload_steps():
        if step.get("name") == name:
            return step
    raise AssertionError(f"step {name!r} is missing from testflight.yml's upload job")


def test_failure_issue_does_not_depend_on_the_decide_output() -> None:
    decide = step_named(DECIDE_STEP)
    assert decide["id"] == "changed"  # the step whose output gates the rest
    condition = str(step_named(ISSUE_STEP)["if"])
    assert "failure()" in condition
    assert "cancelled()" in condition
    # `upload` is unset when the decide step fails, so the issue step must not read it.
    assert not re.search(r"steps\.changed\.outputs", condition), condition


def test_failure_issue_runs_after_every_step_it_reports_on() -> None:
    names = [step.get("name") for step in upload_steps()]
    issue = names.index(ISSUE_STEP)
    assert names.index(DECIDE_STEP) < issue
    assert names.index("Sign and upload") < issue


def test_closing_the_issue_still_requires_a_delivered_build() -> None:
    close = str(step_named("Close the failed upload issue")["if"])
    assert "success()" in close
    assert "steps.changed.outputs.upload == 'true'" in close
