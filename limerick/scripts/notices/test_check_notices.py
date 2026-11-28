#!/usr/bin/env python3
"""Tests for the third-party notices staleness and truncation checks (issue 2441)."""

from __future__ import annotations

import importlib
import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

if TYPE_CHECKING:
    import check_notices as cn
else:
    cn = importlib.import_module("check_notices")


def lock_text(*crates: tuple[str, str], first_party: bool = True) -> str:
    out = ""
    if first_party:
        out += '[[package]]\nname = "limerick-core"\nversion = "0.1.0"\n\n'
    for name, version in crates:
        out += (
            f'[[package]]\nname = "{name}"\nversion = "{version}"\n'
            'source = "registry+https://github.com/rust-lang/crates.io-index"\n\n'
        )
    return out


def notice(*groups: tuple[str, list[tuple[str, str]]]) -> str:
    """A cargo-about style file: each group is (license text, [(crate, version), ...])."""
    out = "# Rust third-party notices\n\n## Overview\n\n- [MIT License](#MIT) - 1 crate\n\n"
    for text, crates in groups:
        out += "## MIT License {#MIT}\n\n**Used by:**\n\n"
        for name, version in crates:
            out += f"- [{name} {version}](https://crates.io/crates/{name})\n"
        out += f"\n```\n{text}\n```\n\n"
    return out


def test_locked_crates_skips_first_party() -> None:
    assert cn.locked_crates(lock_text(("a", "1.0.0"), ("b", "2.0.0"))) == {"a@1.0.0", "b@2.0.0"}


def test_notice_texts_maps_crates_to_text_hashes() -> None:
    parsed = cn.notice_texts(
        notice(("MIT one", [("a", "1.0.0"), ("b", "1.0.0")]), ("MIT two", [("a", "1.0.0")]))
    )
    assert set(parsed) == {"a@1.0.0", "b@1.0.0"}
    assert len(parsed["a@1.0.0"]) == 2
    assert len(parsed["b@1.0.0"]) == 1


def test_in_step_passes() -> None:
    assert cn.find_problems({"a@1", "b@1", "dev@1"}, {"a@1", "b@1"}, {"dev@1"}) == []


def test_bumped_lock_version_is_stale() -> None:
    """rustix 1.1.4 in the notices after the lock moved to 1.1.6."""
    problems = cn.find_problems({"rustix@1.1.6"}, {"rustix@1.1.4"}, set())
    assert len(problems) == 2
    assert any("rustix@1.1.4" in p and "not in Cargo.lock" in p for p in problems)
    assert any("rustix@1.1.6" in p and "neither" in p for p in problems)


def test_new_dependency_without_notice_is_stale() -> None:
    problems = cn.find_problems({"a@1", "new@1"}, {"a@1"}, set())
    assert problems == ["in Cargo.lock but in neither notices file: new@1"]


def test_crate_in_both_files_is_flagged() -> None:
    problems = cn.find_problems({"a@1"}, {"a@1"}, {"a@1"})
    assert problems == ["in both the notices and notices-dev-only.txt: a@1"]


def test_dev_only_parsing_ignores_comments_and_blanks() -> None:
    assert cn.dev_only_crates("# header\n\nx@1\n  y@2  \n") == {"x@1", "y@2"}


def hashes(*groups: tuple[str, list[tuple[str, str]]]) -> dict[str, set[str]]:
    return cn.notice_texts(notice(*groups))


def test_refresh_accepts_a_bump_and_new_crate() -> None:
    previous = hashes(("MIT", [("a", "1.0.0")]))
    generated = hashes(("MIT", [("a", "1.1.0"), ("b", "1.0.0")]))
    assert cn.find_refresh_problems(generated, previous, {"a@1.1.0", "b@1.0.0"}) == []


def test_refresh_rejects_truncated_crate_list() -> None:
    previous = hashes(("MIT", [("a", "1.0.0"), ("b", "1.0.0")]))
    generated = hashes(("MIT", [("a", "1.0.0")]))
    problems = cn.find_refresh_problems(generated, previous, {"a@1.0.0", "b@1.0.0"})
    assert len(problems) == 1
    assert "b@1.0.0" in problems[0]
    assert "missing" in problems[0]


def test_refresh_allows_a_crate_that_left_the_lockfile() -> None:
    previous = hashes(("MIT", [("a", "1.0.0"), ("gone", "1.0.0")]))
    generated = hashes(("MIT", [("a", "1.0.0")]))
    assert cn.find_refresh_problems(generated, previous, {"a@1.0.0"}) == []


def test_refresh_rejects_lost_license_text() -> None:
    """cargo-about 0.9.0 kept the crate but dropped one of its license files."""
    previous = hashes(("MIT", [("ring", "0.17.14")]), ("ISC", [("ring", "0.17.14")]))
    generated = hashes(("MIT", [("ring", "0.17.14")]))
    problems = cn.find_refresh_problems(generated, previous, {"ring@0.17.14"})
    assert len(problems) == 1
    assert "1 license text" in problems[0]
    assert "ring@0.17.14" in problems[0]


def test_refresh_allows_extra_license_text() -> None:
    previous = hashes(("MIT", [("ring", "0.17.14")]))
    generated = hashes(("MIT", [("ring", "0.17.14")]), ("ISC", [("ring", "0.17.14")]))
    assert cn.find_refresh_problems(generated, previous, {"ring@0.17.14"}) == []


def test_refresh_rejects_empty_and_unlocked_output() -> None:
    assert cn.find_refresh_problems({}, {}, {"a@1"}) == [
        "the generated notices list no crates at all"
    ]
    problems = cn.find_refresh_problems(hashes(("MIT", [("x", "1.0.0")])), {}, {"a@1"})
    assert len(problems) == 1
    assert "x@1.0.0" in problems[0]


def ui_lock(**packages: dict[str, object]) -> str:
    entries: dict[str, object] = {"": {"name": "ui"}}
    entries.update({f"node_modules/{k.replace('__', '/')}": v for k, v in packages.items()})
    return json.dumps({"packages": entries})


def test_ui_check_requires_every_production_package() -> None:
    every, required = cn.ui_locked(
        ui_lock(
            a={"version": "1.0.0"},
            dev={"version": "1.0.0", "dev": True},
            native={"version": "1.0.0", "optional": True},
        )
    )
    assert every == {"a@1.0.0", "dev@1.0.0", "native@1.0.0"}
    assert required == {"a@1.0.0"}
    assert cn.find_ui_problems(every, required, {"a@1.0.0"}) == []
    assert "a@1.0.0" in cn.find_ui_problems(every, required, set())[0]


def test_ui_check_flags_a_stale_version() -> None:
    every, required = cn.ui_locked(ui_lock(vite={"version": "8.3.8"}))
    problems = cn.find_ui_problems(every, required, {"vite@8.3.5"})
    assert len(problems) == 2
    assert any("vite@8.3.5" in p for p in problems)


def test_ui_check_handles_scoped_and_nested_paths() -> None:
    lock = json.dumps(
        {
            "packages": {
                "": {},
                "node_modules/@scope/pkg": {"version": "2.0.0"},
                "node_modules/a/node_modules/@scope/inner": {"version": "3.0.0"},
            }
        }
    )
    every, _ = cn.ui_locked(lock)
    assert every == {"@scope/pkg@2.0.0", "@scope/inner@3.0.0"}


def write_tree(root: Path, lock: str, notices: str, dev_only: str) -> list[str]:
    (root / "Cargo.lock").write_text(lock)
    (root / "rust.md").write_text(notices)
    (root / "dev.txt").write_text(dev_only)
    (root / "package-lock.json").write_text(ui_lock(a={"version": "1.0.0"}))
    (root / "ui.md").write_text("- [a@1.0.0](https://x) - MIT\n")
    return [
        "--lockfile",
        str(root / "Cargo.lock"),
        "--dev-only",
        str(root / "dev.txt"),
        "check",
        "--notices",
        str(root / "rust.md"),
        "--ui-lockfile",
        str(root / "package-lock.json"),
        "--ui-notices",
        str(root / "ui.md"),
    ]


def test_cli_check_passes_then_fails_after_a_lock_bump(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    argv = write_tree(
        tmp_path,
        lock_text(("a", "1.0.0"), ("dev", "1.0.0")),
        notice(("MIT", [("a", "1.0.0")])),
        "dev@1.0.0\n",
    )
    assert cn.main(argv) == 0
    (tmp_path / "Cargo.lock").write_text(lock_text(("a", "1.0.1"), ("dev", "1.0.0")))
    assert cn.main(argv) == 1
    err = capsys.readouterr().err
    assert "a@1.0.0" in err
    assert "a@1.0.1" in err
    assert "just notices" in err


def test_cli_refresh_writes_dev_only_and_refuses_bad_output(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    (tmp_path / "Cargo.lock").write_text(
        lock_text(("a", "1.0.0"), ("b", "1.0.0"), ("dev", "1.0.0"))
    )
    previous = tmp_path / "previous.md"
    previous.write_text(notice(("MIT", [("a", "1.0.0"), ("b", "1.0.0")])))
    good = tmp_path / "good.md"
    good.write_text(notice(("MIT", [("a", "1.0.0"), ("b", "1.0.0")])))
    base = ["--lockfile", str(tmp_path / "Cargo.lock"), "--dev-only", str(tmp_path / "dev.txt")]
    refresh = ["refresh", "--previous", str(previous), "--generated"]
    assert cn.main([*base, *refresh, str(good)]) == 0
    assert cn.dev_only_crates((tmp_path / "dev.txt").read_text()) == {"dev@1.0.0"}

    truncated = tmp_path / "truncated.md"
    truncated.write_text(notice(("MIT", [("a", "1.0.0")])))
    (tmp_path / "dev.txt").write_text("sentinel\n")
    assert cn.main([*base, *refresh, str(truncated)]) == 1
    assert "b@1.0.0" in capsys.readouterr().err
    assert (tmp_path / "dev.txt").read_text() == "sentinel\n"
