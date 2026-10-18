#!/usr/bin/env python3
"""Check that every remote action a workflow uses exists and accepts its inputs.

GitHub Actions bumps (Dependabot's `github-actions` ecosystem) are exercised on
a pull request only when the changed workflow itself runs there. `release.yml`,
`testflight.yml`, `deploy-limerick-prod.yml`, `publish-bench-site.yml`,
`stale.yml`, and the `pull_request_target` workflows (which run the base
branch's copy) never do, so a major bump that renames or removes an input would
first fail after merge. This check reads each referenced action's metadata at
the pinned ref, from GitHub, and fails when:

- the ref or the action's `action.yml` does not exist;
- a step passes a `with:` key the action does not declare;
- a step omits an input the action marks required and gives no default;
- the action runs on a JavaScript runtime GitHub no longer supports.

Reusable workflows (`owner/repo/.github/workflows/x.yml@ref`) are checked the
same way against their `workflow_call` inputs. Local actions (`./...`) and
`docker://` images are skipped. CI runs it with `actionlint` in the
`workflow-quality` job; run it locally with
`python3 scripts/check_action_refs.py` (needs `gh` signed in).
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"
SUPPORTED_RUNTIMES = {"node20", "node24", "composite", "docker"}
USES = re.compile(r"(?P<repo>[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)(?:/(?P<path>[^@]+))?@(?P<ref>.+)")

# Fetches a file from GitHub: (owner/repo, path, ref) -> text, or None when absent.
Fetch = Callable[[str, str, str], "str | None"]


@dataclass(frozen=True)
class Usage:
    workflow: str
    location: str  # "job build step 3" or "job call"
    uses: str
    inputs: tuple[str, ...]


def _on(document: dict[Any, Any]) -> Any:
    # PyYAML (YAML 1.1) reads the bare key `on` as boolean True.
    return document.get("on", document.get(True))


def iter_usages(workflow: Path) -> Iterator[Usage]:
    document = yaml.safe_load(workflow.read_text(encoding="utf-8")) or {}
    for job_id, job in (document.get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        if isinstance(job.get("uses"), str):
            yield Usage(
                workflow.name,
                f"job {job_id}",
                job["uses"],
                tuple(sorted((job.get("with") or {}).keys())),
            )
        for index, step in enumerate(job.get("steps") or [], start=1):
            if isinstance(step, dict) and isinstance(step.get("uses"), str):
                yield Usage(
                    workflow.name,
                    f"job {job_id} step {index}",
                    step["uses"],
                    tuple(sorted((step.get("with") or {}).keys())),
                )


def gh_fetch(repo: str, path: str, ref: str) -> str | None:
    result = subprocess.run(
        [
            "gh",
            "api",
            "-H",
            "Accept: application/vnd.github.raw",
            f"repos/{repo}/contents/{path}?ref={ref}",
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode == 0:
        return result.stdout
    if "404" in result.stderr or "Not Found" in result.stderr:
        return None
    raise RuntimeError(f"gh api failed for {repo}/{path}@{ref}: {result.stderr.strip()}")


class Checker:
    def __init__(self, fetch: Fetch):
        self.fetch = fetch
        self._metadata: dict[tuple[str, str, str], dict[str, Any] | None] = {}

    def metadata(self, repo: str, path: str, ref: str) -> dict[str, Any] | None:
        key = (repo, path, ref)
        if key not in self._metadata:
            self._metadata[key] = self._load(repo, path, ref)
        return self._metadata[key]

    def _load(self, repo: str, path: str, ref: str) -> dict[str, Any] | None:
        if path.endswith((".yml", ".yaml")) and "/workflows/" in f"/{path}":
            text = self.fetch(repo, path, ref)
            if text is None:
                return None
            document = yaml.safe_load(text) or {}
            on = _on(document)
            triggers = on if isinstance(on, (dict, list)) else [on]
            call = on.get("workflow_call") if isinstance(on, dict) else None
            inputs = (call.get("inputs") or {}) if isinstance(call, dict) else {}
            return {"kind": "workflow", "inputs": inputs, "callable": "workflow_call" in triggers}
        base = f"{path.rstrip('/')}/" if path else ""
        for name in ("action.yml", "action.yaml"):
            text = self.fetch(repo, f"{base}{name}", ref)
            if text is not None:
                document = yaml.safe_load(text) or {}
                return {
                    "kind": "action",
                    "inputs": document.get("inputs") or {},
                    "using": str((document.get("runs") or {}).get("using", "")),
                }
        return None

    def check(self, usage: Usage) -> list[str]:
        if usage.uses.startswith(("./", "docker://")):
            return []
        match = USES.fullmatch(usage.uses.strip())
        where = f"{usage.workflow} ({usage.location}): {usage.uses}"
        if not match:
            return [f"{where}: not an owner/repo[/path]@ref reference"]
        repo, path, ref = match["repo"], match["path"] or "", match["ref"]
        meta = self.metadata(repo, path, ref)
        if meta is None:
            return [f"{where}: no action.yml or workflow at that ref"]
        problems = []
        if meta["kind"] == "workflow" and not meta["callable"]:
            problems.append(f"{where}: the workflow has no workflow_call trigger")
        if meta["kind"] == "action" and meta["using"] not in SUPPORTED_RUNTIMES:
            problems.append(f"{where}: runs.using '{meta['using']}' is not supported")
        declared = {str(name).lower(): spec or {} for name, spec in meta["inputs"].items()}
        given = {name.lower() for name in usage.inputs}
        for name in sorted(given - declared.keys()):
            problems.append(f"{where}: input '{name}' is not declared by the action")
        for name, spec in sorted(declared.items()):
            required = isinstance(spec, dict) and spec.get("required") in (True, "true")
            has_default = isinstance(spec, dict) and "default" in spec
            if required and not has_default and name not in given:
                problems.append(f"{where}: required input '{name}' is missing")
        return problems


def check_workflows(paths: list[Path], fetch: Fetch) -> list[str]:
    checker = Checker(fetch)
    problems: list[str] = []
    for path in paths:
        for usage in iter_usages(path):
            problems.extend(checker.check(usage))
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("workflows", nargs="*", type=Path, help="default: .github/workflows/*.yml")
    args = parser.parse_args(argv)
    paths = args.workflows or sorted(WORKFLOWS.glob("*.yml"))
    problems = check_workflows(paths, gh_fetch)
    for problem in problems:
        print(f"::error::{problem}" if _in_actions() else problem, file=sys.stderr)
    if problems:
        return 1
    print(f"Checked the actions used by {len(paths)} workflow(s); every reference resolves.")
    return 0


def _in_actions() -> bool:
    return os.environ.get("GITHUB_ACTIONS") == "true"


if __name__ == "__main__":
    raise SystemExit(main())
