"""Tests for endpoints_changed: run CI's Endpoints job exactly when CI would."""

from __future__ import annotations

import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))
import endpoints_changed  # noqa: E402


class FakeRunner:
    """Answers git with fixed change lists and records the pnpm steps."""

    def __init__(
        self,
        committed: Sequence[str] = (),
        uncommitted: Sequence[str] = (),
        untracked: Sequence[str] = (),
        failing: str | None = None,
    ) -> None:
        self.git = {
            "committed": "\n".join(committed),
            "uncommitted": "\n".join(uncommitted),
            "untracked": "\n".join(untracked),
        }
        self.failing = failing
        self.pnpm: list[list[str]] = []

    def __call__(
        self, argv: Sequence[str], cwd: str | None = None
    ) -> subprocess.CompletedProcess[str]:
        argv = list(argv)
        if argv[:2] == ["git", "merge-base"]:
            return subprocess.CompletedProcess(argv, 0, "abc123\n", "")
        if argv[:2] == ["git", "diff"]:
            # A rename-detecting diff would list only a move's new path.
            assert "--no-renames" in argv, argv
            key = "committed" if argv[-1].endswith("...HEAD") else "uncommitted"
            return subprocess.CompletedProcess(argv, 0, self.git[key], "")
        if argv[:2] == ["git", "ls-files"]:
            return subprocess.CompletedProcess(argv, 0, self.git["untracked"], "")
        assert argv[0] == "pnpm" and cwd == "endpoints", argv
        self.pnpm.append(argv)
        code = 1 if self.failing and argv[1] == self.failing else 0
        return subprocess.CompletedProcess(argv, code, "", "")


@pytest.mark.parametrize(
    "where",
    ["committed", "uncommitted", "untracked"],
)
@pytest.mark.parametrize(
    "path",
    [
        "endpoints/examples/rundale-intent-comparison/src/policy.ts",
        "mods/rundale/endpoints/rundale-intent.v1.json",
        "mods/rundale/npcs.json",
        "mods/rundale/world.json",
        "mobile/endpoint/server.ts",
        ".github/workflows/ci.yml",
    ],
)
def test_a_change_to_any_ci_endpoint_input_runs_the_check(where: str, path: str) -> None:
    changes: dict[str, list[str]] = {"committed": [], "uncommitted": [], "untracked": []}
    changes[where] = [path]
    runner = FakeRunner(changes["committed"], changes["uncommitted"], changes["untracked"])
    assert endpoints_changed.main([], runner) == 0
    assert runner.pnpm == [["pnpm", "install", "--frozen-lockfile", "--silent"], ["pnpm", "check"]]


@pytest.mark.parametrize(
    "paths",
    [
        ["limerick/crates/limerick-core/src/lib.rs"],
        ["mods/rundale/prompts/dialogue.md"],
        ["endpoints/README.md"],
        ["endpoints/README.md", "docs/agent/build-test.md"],
    ],
)
def test_unrelated_and_documentation_only_changes_skip(paths: list[str]) -> None:
    runner = FakeRunner(committed=paths)
    assert endpoints_changed.main([], runner) == 0
    assert runner.pnpm == []


def test_a_markdown_endpoint_change_runs_when_any_other_file_is_code() -> None:
    runner = FakeRunner(
        committed=["endpoints/README.md", "limerick/crates/limerick-core/src/lib.rs"]
    )
    assert endpoints_changed.main([], runner) == 0
    assert len(runner.pnpm) == 2


@pytest.mark.parametrize("where", ["committed", "uncommitted"])
def test_moving_a_file_out_of_a_watched_directory_runs_the_check(where: str) -> None:
    # With --no-renames, git lists the move's old path as deleted.
    moved = ["endpoints/packages/runtime/src/old.ts", "limerick/new.ts"]
    runner = FakeRunner(committed=moved) if where == "committed" else FakeRunner(uncommitted=moved)
    assert endpoints_changed.main([], runner) == 0
    assert len(runner.pnpm) == 2


def test_a_failed_git_command_fails_instead_of_skipping() -> None:
    def runner(argv: Sequence[str], cwd: str | None = None) -> subprocess.CompletedProcess[str]:
        return subprocess.CompletedProcess(list(argv), 128, "", "fatal: not a git repository")

    with pytest.raises(SystemExit, match="failed"):
        endpoints_changed.main([], runner)


@pytest.mark.parametrize("step", ["install", "check"])
def test_a_failing_step_fails_the_gate(step: str) -> None:
    runner = FakeRunner(committed=["endpoints/packages/runtime/src/index.ts"], failing=step)
    assert endpoints_changed.main([], runner) == 1
    assert runner.pnpm[-1][1] == step
