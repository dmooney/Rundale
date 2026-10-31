"""Regression contract for exported mobile-verify failure evidence uploads."""

from __future__ import annotations

from pathlib import Path
from typing import Any

import yaml

ROOT = Path(__file__).resolve().parents[1]


def report_artifact_paths(workflow: str, artifact_name: str) -> list[str]:
    document: dict[str, Any] = yaml.safe_load(
        (ROOT / ".github" / "workflows" / workflow).read_text(encoding="utf-8")
    )
    for job in document["jobs"].values():
        for step in job.get("steps", []):
            if step.get("uses", "").startswith("actions/upload-artifact@"):
                config = step.get("with", {})
                if config.get("name") == artifact_name:
                    paths = config.get("path", "")
                    return paths.splitlines() if isinstance(paths, str) else paths
    raise AssertionError(f"{artifact_name!r} upload is missing from {workflow}")


def test_verification_report_artifacts_include_bounded_failure_evidence() -> None:
    report_artifacts = (
        ("ci.yml", "swift-quality-report"),
        ("ci.yml", "mobile-verify-report"),
        ("testflight.yml", "mobile-verify-nightly-report"),
    )

    for workflow, artifact in report_artifacts:
        paths = report_artifact_paths(workflow, artifact)
        assert "mobile/.verification/failures/" in paths, (workflow, artifact, paths)
        assert not any("xcresult" in path for path in paths), (workflow, artifact, paths)
