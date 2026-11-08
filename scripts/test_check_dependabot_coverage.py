#!/usr/bin/env python3
"""Tests for the Dependabot lockfile-coverage check."""

from __future__ import annotations

import importlib
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

if TYPE_CHECKING:
    import check_dependabot_coverage as cdc
else:
    cdc = importlib.import_module("check_dependabot_coverage")


def config(*entries: dict[str, Any]) -> dict[str, Any]:
    return {"version": 2, "updates": list(entries)}


def npm(*dirs: str) -> dict[str, Any]:
    return {"package-ecosystem": "npm", "directories": list(dirs)}


def test_all_covered_passes() -> None:
    cfg = config(npm("/", "/app"), {"package-ecosystem": "cargo", "directory": "/limerick"})
    files = ["package-lock.json", "app/pnpm-lock.yaml", "limerick/Cargo.lock", "README.md"]
    assert cdc.find_problems(files, cfg, {}) == []


def test_uncovered_lockfile_fails_with_clear_message() -> None:
    problems = cdc.find_problems(["endpoints/pnpm-lock.yaml"], config(npm("/")), {})
    assert len(problems) == 1
    assert "endpoints/pnpm-lock.yaml" in problems[0]
    assert "'npm'" in problems[0]
    assert "'/endpoints'" in problems[0]
    assert "EXCLUSIONS" in problems[0]


def test_wrong_ecosystem_does_not_cover() -> None:
    cfg = config({"package-ecosystem": "pip", "directory": "/limerick"})
    assert cdc.find_problems(["limerick/Cargo.lock"], cfg, {})


def test_excluded_lockfile_passes() -> None:
    files = ["vendor/Cargo.lock"]
    assert cdc.find_problems(files, config(npm("/")), {"vendor/Cargo.lock": "vendored"}) == []


def test_exclusion_needs_reason() -> None:
    problems = cdc.find_problems(["a/Cargo.lock"], config(), {"a/Cargo.lock": " "})
    assert problems == ["a/Cargo.lock: exclusion has no reason."]


def test_stale_exclusions_fail() -> None:
    cfg = config(npm("/a"))
    problems = cdc.find_problems(["a/package-lock.json"], cfg, {"a/package-lock.json": "x"})
    assert "now covers it" in problems[0]
    problems = cdc.find_problems([], cfg, {"gone/Cargo.lock": "x"})
    assert "no longer tracked" in problems[0]


def test_directory_globs() -> None:
    cfg = config(npm("/apps/*", "/docs/**"))
    covered = [
        "apps/web/package-lock.json",
        "docs/package-lock.json",
        "docs/a/b/package-lock.json",
    ]
    assert cdc.find_problems(covered, cfg, {}) == []
    # `*` stays within one segment and does not match /apps itself.
    assert len(cdc.find_problems(["apps/a/b/package-lock.json"], cfg, {})) == 1
    assert len(cdc.find_problems(["apps/package-lock.json"], cfg, {})) == 1


def test_root_directory_matches_only_root() -> None:
    cfg = config({"package-ecosystem": "npm", "directory": "/"})
    assert cdc.find_problems(["package-lock.json"], cfg, {}) == []
    assert len(cdc.find_problems(["sub/package-lock.json"], cfg, {})) == 1


def test_singular_directory_is_literal_not_a_glob() -> None:
    cfg = config({"package-ecosystem": "npm", "directory": "/apps/*"})
    assert len(cdc.find_problems(["apps/web/package-lock.json"], cfg, {})) == 1


def test_directory_and_directories_both_read() -> None:
    cfg = config({"package-ecosystem": "npm", "directory": "/a", "directories": ["/b"]})
    assert cdc.find_problems(["a/yarn.lock", "b/yarn.lock"], cfg, {}) == []


@pytest.mark.parametrize(
    ("path", "ecosystem"),
    [
        ("x/package-lock.json", "npm"),
        ("x/pnpm-lock.yaml", "npm"),
        ("x/Cargo.lock", "cargo"),
        ("x/requirements.txt", "pip"),
        ("x/requirements-dev.txt", "pip"),
        ("x/uv.lock", "uv"),
        ("x/Package.resolved", "swift"),
        ("x/package.json", None),
        ("x/Cargo.toml", None),
    ],
)
def test_ecosystem_for(path: str, ecosystem: str | None) -> None:
    assert cdc.ecosystem_for(path) == ecosystem


def test_main_reads_yaml_and_git(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(cdc, "EXCLUSIONS", {})
    (tmp_path / ".github").mkdir()
    (tmp_path / ".github" / "dependabot.yml").write_text(
        'version: 2\nupdates:\n  - package-ecosystem: "npm"\n    directories: ["/", "/a"]\n',
        encoding="utf-8",
    )
    (tmp_path / "a").mkdir()
    (tmp_path / "a" / "package-lock.json").write_text("{}", encoding="utf-8")
    (tmp_path / "package-lock.json").write_text("{}", encoding="utf-8")
    subprocess.run(["git", "init", "-q", str(tmp_path)], check=True)
    subprocess.run(["git", "-C", str(tmp_path), "add", "-A"], check=True)
    assert cdc.main(["--root", str(tmp_path)]) == 0

    (tmp_path / "b").mkdir()
    (tmp_path / "b" / "Cargo.lock").write_text("", encoding="utf-8")
    subprocess.run(["git", "-C", str(tmp_path), "add", "-A"], check=True)
    assert cdc.main(["--root", str(tmp_path)]) == 1
    assert "b/Cargo.lock" in capsys.readouterr().err


def test_real_repository_is_covered() -> None:
    assert cdc.main([]) == 0
