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
    (root / "ship.txt").write_text(
        cn.FINGERPRINT
        + cn.lock_fingerprint(lock)
        + "\n"
        + "\n".join(cn.notice_texts(notices))
        + "\n"
    )
    (root / "package-lock.json").write_text(ui_lock(a={"version": "1.0.0"}))
    (root / "ui.md").write_text("- [a@1.0.0](https://x) - MIT\n")
    return [
        "--lockfile",
        str(root / "Cargo.lock"),
        "--dev-only",
        str(root / "dev.txt"),
        "--shipped",
        str(root / "ship.txt"),
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
    base = [
        "--lockfile", str(tmp_path / "Cargo.lock"),
        "--dev-only", str(tmp_path / "dev.txt"),
        "--shipped", str(tmp_path / "no-snapshot.txt"),
    ]  # fmt: skip
    refresh = [
        "refresh",
        "--previous", str(previous),
        "--supplement", str(tmp_path / "no-supplement.txt"),
        "--generated",
    ]  # fmt: skip
    assert cn.main([*base, *refresh, str(good)]) == 0
    assert cn.dev_only_crates((tmp_path / "dev.txt").read_text()) == {"dev@1.0.0"}

    truncated = tmp_path / "truncated.md"
    truncated.write_text(notice(("MIT", [("a", "1.0.0")])))
    (tmp_path / "dev.txt").write_text("sentinel\n")
    assert cn.main([*base, *refresh, str(truncated)]) == 1
    assert "b@1.0.0" in capsys.readouterr().err
    assert (tmp_path / "dev.txt").read_text() == "sentinel\n"


def test_license_text_hash_ignores_whitespace_only_changes() -> None:
    """A template or wrapping change is not a lost license (issue 2477)."""
    wrapped = hashes(("Permission is hereby granted,\n\n  free of charge  ", [("a", "1.0.0")]))
    reflowed = hashes(("Permission is hereby granted, free of charge", [("a", "1.0.0")]))
    assert wrapped == reflowed
    assert cn.find_refresh_problems(reflowed, wrapped, {"a@1.0.0"}) == []
    changed = hashes(("Permission is NOT granted", [("a", "1.0.0")]))
    assert cn.find_refresh_problems(changed, wrapped, {"a@1.0.0"}) != []


def test_ui_check_does_not_require_dev_optional_packages() -> None:
    every, required = cn.ui_locked(
        ui_lock(a={"version": "1.0.0"}, peer={"version": "2.0.0", "devOptional": True})
    )
    assert every == {"a@1.0.0", "peer@2.0.0"}
    assert required == {"a@1.0.0"}


def lock_with_edges(*crates: tuple[str, str, list[str]]) -> str:
    out = ""
    for name, version, deps in crates:
        out += (
            f'[[package]]\nname = "{name}"\nversion = "{version}"\n'
            'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
            f"dependencies = {json.dumps(deps)}\n\n"
        )
    return out


def tree(*crates: str) -> str:
    lines = ["limerick-x v0.1.0 (/repo/crates/x)"]
    for crate in crates:
        name, version = crate.split("@")
        lines.append(f"{name} v{version}")
        lines.append(f"{name} v{version} (*)")
    lines.append("limerick-types v0.1.0 (/repo/crates/types)")
    return "\n".join(lines)


def test_parse_tree_keeps_only_locked_third_party_crates() -> None:
    assert cn.parse_tree(tree("a@1.0.0", "b@2.0.0"), {"a@1.0.0", "c@1.0.0"}) == {"a@1.0.0"}


def test_lock_fingerprint_tracks_graph_not_first_party_versions() -> None:
    base = lock_text(("a", "1.0.0")) + lock_with_edges(("b", "1.0.0", ["a"]))
    first_party_bump = base.replace('version = "0.1.0"', 'version = "0.2.0"')
    assert cn.lock_fingerprint(first_party_bump) == cn.lock_fingerprint(base)
    assert cn.lock_fingerprint(base.replace('["a"]', '["a", "c"]')) != cn.lock_fingerprint(base)
    assert cn.lock_fingerprint(base.replace('"1.0.0"', '"1.0.1"', 1)) != cn.lock_fingerprint(base)


def snapshot_for(lock: str, *shipped: str) -> str:
    return cn.FINGERPRINT + cn.lock_fingerprint(lock) + "\n" + "".join(f"{c}\n" for c in shipped)


def test_shipped_crate_filed_as_dev_only_is_flagged() -> None:
    """The failure this check exists for: cargo-about omits a crate that ships."""
    lock = lock_text(("a", "1.0.0"), ("hidden", "1.0.0"), ("dev", "1.0.0"))
    snap = snapshot_for(lock, "a@1.0.0", "hidden@1.0.0")
    problems = cn.find_shipped_problems(snap, lock, {"a@1.0.0"}, {"hidden@1.0.0", "dev@1.0.0"})
    assert len(problems) == 1
    assert "hidden@1.0.0" in problems[0]
    assert "dev@1.0.0" not in problems[0]
    assert "dev-only" in problems[0]


def test_shipped_crate_without_any_entry_is_flagged() -> None:
    lock = lock_text(("a", "1.0.0"), ("new", "1.0.0"))
    snap = snapshot_for(lock, "a@1.0.0", "new@1.0.0")
    problems = cn.find_shipped_problems(snap, lock, {"a@1.0.0"}, set())
    assert problems == ["shipped crates with no notice: new@1.0.0"]


def test_shipped_snapshot_older_than_the_lockfile_is_flagged() -> None:
    old = lock_text(("a", "1.0.0"))
    new = lock_text(("a", "1.0.0"), ("b", "1.0.0"))
    problems = cn.find_shipped_problems(
        snapshot_for(old, "a@1.0.0"), new, {"a@1.0.0", "b@1.0.0"}, set()
    )
    assert len(problems) == 1
    assert "different Cargo.lock" in problems[0]


def test_shipped_snapshot_naming_unlocked_crates_or_nothing_is_flagged() -> None:
    lock = lock_text(("a", "1.0.0"))
    problems = cn.find_shipped_problems(snapshot_for(lock, "gone@1.0.0"), lock, {"a@1.0.0"}, set())
    assert any("gone@1.0.0" in p and "not in Cargo.lock" in p for p in problems)
    empty = cn.find_shipped_problems(snapshot_for(lock), lock, {"a@1.0.0"}, set())
    assert any("no crates at all" in p for p in empty)


def test_shipped_snapshot_is_the_union_over_shipped_artifacts() -> None:
    lock = lock_text(("a", "1.0.0"), ("b", "1.0.0"), ("dev", "1.0.0"))
    trees = {(p, t): tree("a@1.0.0") for p, t, _ in cn.SHIPPED}
    last = cn.SHIPPED[-1]
    trees[(last[0], last[1])] = tree("a@1.0.0", "b@1.0.0")
    snap = cn.shipped_snapshot(lock, trees)
    assert cn.dev_only_crates(snap) == {"a@1.0.0", "b@1.0.0"}
    assert cn.find_shipped_problems(snap, lock, {"a@1.0.0", "b@1.0.0"}, {"dev@1.0.0"}) == []


def test_shipped_matrix_covers_the_ios_targets_the_build_script_builds() -> None:
    script = (
        Path(__file__).resolve().parents[3] / "mobile/scripts/build-rust-mobile.sh"
    ).read_text()
    ios = {t for p, t, _ in cn.SHIPPED if p == "limerick-mobile-ffi"}
    assert ios == {"aarch64-apple-ios", "aarch64-apple-ios-sim"}
    assert all(target in script for target in ios)


SUPPLEMENTED = (
    "# Rust third-party notices\n\n## Overview\n\n- [MIT License](#MIT) — 3 crates\n"
    "- [Apache License 2.0](#Apache-2.0) — 1 crate\n\n## MIT License {#MIT}\n\n"
    "**Used by:**\n\n- [sf-sys 2.0.0](https://x)\n- [sf 2.0.0](https://repo)\n\n```\nMIT text\n```\n"
    "\n**Used by:**\n\n- [other 1.0.0](https://y)\n\n```\nMIT text 2\n```\n\n"
    "## Apache License 2.0 {#Apache-2.0}\n\n**Used by:**\n\n- [sf 2.0.0](https://repo)\n\n"
    "```\nApache text\n```\n"
)


def test_supplement_adds_the_crate_beside_its_sibling_and_counts_it() -> None:
    out, problems = cn.apply_supplement(SUPPLEMENTED, {"sf@3.0.0": "sf@2.0.0"}, {"sf@3.0.0"})
    assert problems == []
    parsed = cn.notice_texts(out)
    assert set(parsed) == {"sf-sys@2.0.0", "sf@2.0.0", "sf@3.0.0", "other@1.0.0"}
    assert parsed["sf@3.0.0"] == parsed["sf@2.0.0"]
    assert "- [sf 3.0.0](https://repo)" in out
    assert "[MIT License](#MIT) — 4 crates" in out
    assert "[Apache License 2.0](#Apache-2.0) — 2 crates" in out


def test_supplement_expires_when_cargo_about_finds_the_crate_or_it_leaves_the_lock() -> None:
    found = SUPPLEMENTED.replace("- [other 1.0.0]", "- [sf 3.0.0](https://repo)\n- [other 1.0.0]")
    _, problems = cn.apply_supplement(found, {"sf@3.0.0": "sf@2.0.0"}, {"sf@3.0.0"})
    assert any("remove its notices-supplement.txt line" in p for p in problems)
    _, problems = cn.apply_supplement(SUPPLEMENTED, {"sf@3.0.0": "sf@2.0.0"}, set())
    assert any("not in Cargo.lock" in p for p in problems)
    _, problems = cn.apply_supplement(SUPPLEMENTED, {"sf@3.0.0": "gone@1.0.0"}, {"sf@3.0.0"})
    assert any("gone@1.0.0" in p for p in problems)


def test_parse_supplement_reads_lines_and_rejects_garbage() -> None:
    text = "# c\n\nsf@3.0.0 same-as sf@2.0.0  # why\n"
    assert cn.parse_supplement(text) == {"sf@3.0.0": "sf@2.0.0"}
    with pytest.raises(SystemExit):
        cn.parse_supplement("sf@3.0.0 sf@2.0.0\n")


def test_cli_refresh_supplements_and_refuses_a_shipped_crate_filed_as_dev_only(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    lock = lock_text(("sf", "2.0.0"), ("sf", "3.0.0"), ("dev", "1.0.0"))
    (tmp_path / "Cargo.lock").write_text(lock)
    generated = tmp_path / "gen.md"
    generated.write_text(notice(("MIT", [("sf", "2.0.0")])))
    (tmp_path / "supp.txt").write_text("sf@3.0.0 same-as sf@2.0.0\n")
    (tmp_path / "ship.txt").write_text(snapshot_for(lock, "sf@2.0.0", "sf@3.0.0"))
    base = [
        "--lockfile", str(tmp_path / "Cargo.lock"),
        "--dev-only", str(tmp_path / "dev.txt"),
        "--shipped", str(tmp_path / "ship.txt"),
    ]  # fmt: skip
    refresh = ["refresh", "--previous", str(tmp_path / "none.md"), "--generated", str(generated)]
    assert cn.main([*base, *refresh, "--supplement", str(tmp_path / "supp.txt")]) == 0
    assert set(cn.notice_texts(generated.read_text())) == {"sf@2.0.0", "sf@3.0.0"}
    assert cn.dev_only_crates((tmp_path / "dev.txt").read_text()) == {"dev@1.0.0"}

    # Without the supplement the shipped sf@3.0.0 would be filed as dev-only: refuse.
    generated.write_text(notice(("MIT", [("sf", "2.0.0")])))
    (tmp_path / "dev.txt").write_text("sentinel\n")
    assert cn.main([*base, *refresh, "--supplement", str(tmp_path / "missing.txt")]) == 1
    assert "sf@3.0.0" in capsys.readouterr().err
    assert (tmp_path / "dev.txt").read_text() == "sentinel\n"
