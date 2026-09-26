#!/usr/bin/env python3
"""Differential proof: runs the same scenarios on base and head, diffs them.

Builds `limerick-server` and `limerick-engine` from the base revision (in a
detached worktree) and from this working tree (including uncommitted
changes), copies each side's binaries out of the shared cargo target, then
runs on both sides:

  * each live scenario: `limerick-server` driven over HTTP against the
    scripted model server (responses, provider request bodies, engine state);
  * every `--script` fixture through `limerick-engine --script`.

Every difference is reported and checked against the author's intended
differences (see `differences.py`). Undeclared differences fail, and so do
declarations that match nothing. Exit status: 0 clean, 1 failed check.

    prove_diff.py [--scenario talk-and-task] [--intended FILE] [--base REV]
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import shutil
import subprocess
import sys
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import noise  # noqa: E402
from body_diff import load_requests  # noqa: E402
from differences import Item, Unit, check, diff_units, load_intended, unstable_units  # noqa: E402
from drive_session import isolated_env  # noqa: E402
from script_compare import script_units  # noqa: E402

BINARIES = ("limerick-server", "limerick-engine")
FIXTURE_DIR = "limerick/testing/fixtures"
SCRIPT_TIMEOUT = 600


@dataclass
class Side:
    name: str
    tree: Path
    rev: str
    bin_dir: Path

    def run_dir(self, out: Path, run: int) -> Path:
        return out / self.name / f"run{run}"


def git(*args: str, cwd: Path) -> str:
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout.strip()


def log(message: str) -> None:
    print(f"[prove-diff] {message}", file=sys.stderr, flush=True)


def prepare_base_tree(repo: Path, tree: Path, rev: str) -> None:
    if (tree / ".git").exists():
        git("checkout", "-q", "--detach", "--force", rev, cwd=tree)
        return
    git("worktree", "prune", cwd=repo)
    tree.parent.mkdir(parents=True, exist_ok=True)
    git("worktree", "add", "-q", "--detach", "--force", str(tree), rev, cwd=repo)


def default_out(repo: Path) -> Path:
    """Outside the repo, so linters and cargo never see the base worktree."""
    cache = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
    return cache / "limerick" / "prove-diff" / repo.name


def touch_changed(trees: list[Path], changed: list[str]) -> None:
    """Bumps mtimes so the shared target cannot reuse a fingerprint across trees."""
    for tree in trees:
        for rel in changed:
            path = tree / rel
            if path.is_file():
                path.touch()


def build(side: Side) -> None:
    log(f"building {side.name} ({side.rev[:12]}) in {side.tree}")
    subprocess.run(
        ["cargo", "build", "-q", *sum((["-p", b] for b in BINARIES), [])],
        cwd=side.tree / "limerick",
        check=True,
    )
    metadata = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=side.tree / "limerick",
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    target = Path(json.loads(metadata)["target_directory"]) / "debug"
    side.bin_dir.mkdir(parents=True, exist_ok=True)
    for binary in BINARIES:
        shutil.copy2(target / binary, side.bin_dir / binary)


def run_fixture(side: Side, run_dir: Path, fixture: str) -> None:
    source = side.tree / FIXTURE_DIR / f"{fixture}.txt"
    out = run_dir / "script"
    work = out / "work" / fixture
    (work / "cwd").mkdir(parents=True, exist_ok=True)
    if not source.exists():
        return
    with (out / f"{fixture}.jsonl").open("w") as stdout, (work / "stderr.txt").open("w") as err:
        result = subprocess.run(
            [
                str(side.bin_dir / "limerick-engine"),
                "--script",
                str(source),
                "--game-mod",
                str(side.tree / "mods/rundale"),
            ],
            cwd=work / "cwd",
            env=isolated_env(work),
            stdout=stdout,
            stderr=err,
            timeout=SCRIPT_TIMEOUT,
        )
    (out / f"{fixture}.exit").write_text(f"{result.returncode}\n")


def run_scenario(side: Side, run_dir: Path, scenario: Path) -> None:
    out = run_dir / "session" / scenario.stem
    out.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [
            sys.executable,
            str(HERE / "drive_session.py"),
            "--scenario",
            str(scenario),
            "--out",
            str(out),
            "--server-bin",
            str(side.bin_dir / "limerick-server"),
            "--mod-dir",
            str(side.tree / "mods/rundale"),
        ],
        stderr=subprocess.PIPE,
        text=True,
    )
    # A session that fails (e.g. the server never starts) is a difference,
    # not a tool error: record it where the responses surface reads it.
    (out / "driver.exit").write_text(f"{result.returncode}\n{result.stderr[-2000:]}")


def response_units(path: Path) -> list[Unit]:
    units: list[Unit] = []
    exit_file = path.parent / "driver.exit"
    if exit_file.exists():
        units.append(("driver", exit_file.read_text().rstrip("\n").split("\n")))
    if not path.exists():
        return units
    for index, raw in enumerate(path.read_text().splitlines(), 1):
        entry = json.loads(raw)
        body = {"status": entry["status"], "response": entry["response"]}
        text = json.dumps(body, indent=1, sort_keys=True, ensure_ascii=False)
        units.append((f"turn {index} {entry['input']!r}", text.split("\n")))
    return units


def state_units(path: Path) -> list[Unit]:
    if not path.exists():
        return []
    try:
        state = json.loads(path.read_text())
    except json.JSONDecodeError:
        return [("engine-state", [path.read_text()])]
    return [
        (
            f"engine-state {key}",
            noise.game_seconds(json.dumps(value, indent=1, sort_keys=True)).split("\n"),
        )
        for key, value in sorted(state.items())
    ]


def compare(
    out: Path, base: Side, head: Side, runs: int, scenarios: list[Path], fixtures: list[str]
) -> tuple[list[Item], list[str]]:
    base_dirs = [base.run_dir(out, r) for r in range(1, runs + 1)]
    head_dirs = [head.run_dir(out, r) for r in range(1, runs + 1)]
    items: list[Item] = []
    notes: list[str] = []

    def unstable(label: str, base_runs: list[list[Unit]], head_runs: list[list[Unit]]) -> None:
        for side, side_runs in (("base", base_runs), ("head", head_runs)):
            count = unstable_units(side_runs)
            if count:
                notes.append(f"{label}: {side} runs disagree on {count} unit(s)")

    def surface(label: str, name: str, load: Callable[[Path], list[Unit]], rel: str) -> None:
        base_runs = [load(d / rel) for d in base_dirs]
        head_runs = [load(d / rel) for d in head_dirs]
        unstable(f"{label}/{name}", base_runs, head_runs)
        items.extend(diff_units(label, name, base_runs, head_runs))

    for scenario in scenarios:
        rel = f"session/{scenario.stem}"
        surface("responses", scenario.stem, response_units, f"{rel}/responses.jsonl")
        surface("state", scenario.stem, state_units, f"{rel}/engine-state.json")
        surface("requests", scenario.stem, load_requests, f"{rel}/requests.jsonl")

    encounters = noise.encounter_texts([base.tree, head.tree])
    for fixture in fixtures:
        surface(
            "script",
            fixture,
            lambda path: script_units(path, encounters),
            f"script/{fixture}.jsonl",
        )
    return items, notes


def resolve_scenarios(names: list[str]) -> list[Path]:
    paths = []
    for name in names:
        path = Path(name)
        if not path.exists():
            path = HERE / "scenarios" / f"{name}.txt"
        if not path.exists():
            raise SystemExit(f"unknown scenario {name!r} (see {HERE / 'scenarios'})")
        paths.append(path.resolve())
    return paths


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenario", action="append", default=[], help="name or path")
    parser.add_argument("--intended", type=Path, help="intended-differences TOML")
    parser.add_argument("--base", help="base revision (default: merge-base with origin/main)")
    parser.add_argument("--fixtures", default="test_*", help="fixture glob; '' for none")
    parser.add_argument("--runs", type=int, default=3, help="runs per side")
    parser.add_argument("--jobs", type=int, default=max(2, (os.cpu_count() or 4) // 2))
    parser.add_argument(
        "--out", type=Path, help="output dir (default: ~/.cache/limerick/prove-diff/<repo>)"
    )
    parser.add_argument("--no-build", action="store_true", help="reuse the copied binaries")
    args = parser.parse_args()

    repo = Path(git("rev-parse", "--show-toplevel", cwd=Path.cwd()))
    out = (args.out or default_out(repo)).resolve()
    base_rev = git(
        "rev-parse", args.base or git("merge-base", "origin/main", "HEAD", cwd=repo), cwd=repo
    )
    head_rev = git("rev-parse", "HEAD", cwd=repo)
    dirty = bool(git("status", "--porcelain", cwd=repo))
    base = Side("base", out / "base-tree", base_rev, out / "base" / "bin")
    head = Side("head", repo, head_rev, out / "head" / "bin")
    intended = load_intended(args.intended)
    scenarios = resolve_scenarios(args.scenario)

    if not args.no_build:
        prepare_base_tree(repo, base.tree, base_rev)
        changed = git("diff", "--name-only", base_rev, cwd=repo).splitlines()
        touch_changed([base.tree, head.tree], changed)
        build(base)
        touch_changed([base.tree, head.tree], changed)
        build(head)

    fixtures = []
    if args.fixtures:
        fixtures = sorted(
            {
                p.stem
                for side in (base, head)
                for p in (side.tree / FIXTURE_DIR).glob(f"{args.fixtures}.txt")
            }
        )
    for side in (base, head):
        for run in range(1, args.runs + 1):
            shutil.rmtree(side.run_dir(out, run), ignore_errors=True)

    log(
        f"running {len(scenarios)} scenario(s) and {len(fixtures)} fixture(s), {args.runs} run(s) per side"
    )
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        jobs = []
        for side in (base, head):
            for run in range(1, args.runs + 1):
                run_dir = side.run_dir(out, run)
                jobs += [pool.submit(run_scenario, side, run_dir, s) for s in scenarios]
                jobs += [pool.submit(run_fixture, side, run_dir, f) for f in fixtures]
        for job in concurrent.futures.as_completed(jobs):
            job.result()

    items, notes = compare(out, base, head, args.runs, scenarios, fixtures)
    undeclared = check(items, intended)
    unobserved = [entry for entry in intended if entry.hits == 0]

    report = [
        "# prove-diff report",
        "",
        f"base: {base_rev}",
        f"head: {head_rev}{' + uncommitted changes' if dirty else ''}",
        f"runs per side: {args.runs}; scenarios: {', '.join(s.stem for s in scenarios) or 'none'};"
        f" fixtures: {len(fixtures)}",
        "",
        f"differences: {len(items)} ({len(items) - len(undeclared)} declared, {len(undeclared)} undeclared)",
        f"declarations not observed: {len(unobserved)}",
    ]
    if notes:
        report += ["", "## Nondeterminism (compared as a range)", ""]
        report += [f"- {note}" for note in notes]
    if intended:
        report += ["", "## Intended differences", ""]
        report += [
            f"- {e.hits} hit(s){'' if e.hits else ' (NOT OBSERVED)'}: `{e.match}` ({e.reason})"
            for e in intended
        ]
    if undeclared:
        report += ["", "## Undeclared differences", "", "```text"]
        report += [str(item) for item in undeclared] + ["```"]
    if items and len(undeclared) < len(items):
        report += ["", "## Declared differences", "", "```text"]
        report += [str(item) for item in items if item not in undeclared] + ["```"]
    verdict = "FAIL" if undeclared or unobserved else "PASS"
    report += ["", f"result: {verdict}"]
    text = "\n".join(report) + "\n"
    (out / "report.md").write_text(text)
    print(text)
    sys.exit(1 if verdict == "FAIL" else 0)


if __name__ == "__main__":
    main()
