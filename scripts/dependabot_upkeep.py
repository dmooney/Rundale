#!/usr/bin/env python3
"""Weekly Dependabot upkeep: keep each PR current, verify it, and hand it to the gatekeeper.

Runs on the owner's Mac (a launchd LaunchAgent, Monday morning, after
Dependabot's 06:17 UTC run), because bumps that change app inputs need the
local `just mobile-verify --phase all` pass that CI's "Recorded mobile-verify
pass" job looks for (issue #2098). For every open Dependabot pull request it:

1. skips drafts, PRs on another base, `--exclude`d PRs, and PRs already
   handed to the owner (`needs-owner`);
2. asks Dependabot to recreate a PR that conflicts with `main`;
3. brings the PR up to date with `main`, using the owner's `gh` token so CI
   runs on the new head. A PR that changes app inputs
   (`verify_record.py check` says so) is merged with `main` locally, passes
   `just mobile-verify --phase all` in a clean checkout, is pushed, and gets
   its pass recorded (`just mobile-verify-record`). Only one mobile-verify
   runs at a time;
4. waits for the required checks. On a red PR it makes one bounded fix
   attempt (`claude -p`), and leaves the PR open if that fails. A red PR is
   never handed on for merge;
5. dry-runs `release.yml` on the PR branch when an Actions bump changes it,
   since that workflow never runs on pull requests;
6. runs a fresh gatekeeper on a green PR, which merges it, sends it back, or
   hands it to the owner. The gatekeeper is never the author's model family:
   Claude for Dependabot's own commits, Codex once this routine (Claude) has
   added a fix.

It then comments a summary on the pinned "Dependabot upkeep reports" issue,
creating and pinning that issue the first time. Nothing here merges a PR;
merging is the gatekeeper's decision alone.

    dependabot_upkeep.py run --dry-run            # plan only: no pushes, runs, comments
    dependabot_upkeep.py run [--pr N] [--exclude N] [--no-fix]
    dependabot_upkeep.py install [--exclude N]    # LaunchAgent + routine checkouts
    dependabot_upkeep.py uninstall
"""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import json
import os
import plistlib
import re
import shlex
import subprocess
import sys
import time
from collections.abc import Callable, Iterator, Sequence
from contextlib import contextmanager
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, TextIO

import yaml

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_STATE = Path.home() / "Library" / "Application Support" / "rundale-dependabot-upkeep"
AGENT_LABEL = "com.rundale.dependabot-upkeep"
LAUNCH_AGENT = Path.home() / "Library" / "LaunchAgents" / f"{AGENT_LABEL}.plist"
REVIEW_BRANCH = "dependabot-upkeep/review"
TRACKING_TITLE = "Dependabot upkeep reports"
TRACKING_MARKER = "<!-- dependabot-upkeep:tracking -->"
OWNER_LABEL = "needs-owner"
CLAUDE_MODEL = "claude-opus-5-5"
CODEX_MODEL = "gpt-6.1-sol"
# Another tool's mobile-verify run: `just mobile-verify ...` or its script.
VERIFY_PROCESS = r"just mobile-verify( |$)|mobile/scripts/verify\.py"
CLAUDE_TRAILER = re.compile(r"^co-authored-by:\s*claude\b", re.IGNORECASE | re.MULTILINE)
CODEX_TRAILER = re.compile(r"^co-authored-by:\s*codex\b", re.IGNORECASE | re.MULTILINE)
VERDICT = re.compile(r"<!--\s*gatekeeper:\s*sha=([0-9a-f]+)\s+verdict=([a-z-]+)")
# Workflows that never run on pull requests but have a side-effect-free manual
# run, dispatched on the PR branch when an Actions bump changes them.
DISPATCH_PROBES: dict[str, dict[str, str]] = {
    "release.yml": {"version": "0.0.0-upkeep", "dry_run": "true"},
}
SECONDS = {
    "head": 180,
    "checks": 3 * 3600,
    "probe": 3600,
    "verify": 4 * 3600,
    "fix": 3600,
    "gatekeeper": 3600,
    "verify_lock": 6 * 3600,
}
POLL = 60

# verify_record.py check exit statuses.
RECORDED, NEEDS_PASS, NOT_APP_INPUT = 0, 1, 3

NEEDS_ATTENTION = {
    "red",
    "conflict",
    "changes",
    "handoff",
    "escalated",
    "blocked",
    "timeout",
    "error",
    "owner",
}


@dataclass(frozen=True)
class Result:
    returncode: int
    stdout: str = ""
    stderr: str = ""
    skipped: bool = False  # a mutating command not run in --dry-run

    @property
    def ok(self) -> bool:
        return self.returncode == 0


Executor = Callable[[Sequence[str], "Path | None", "float | None", "Path | None"], Result]


def subprocess_executor(
    cmd: Sequence[str], cwd: Path | None, timeout: float | None, output: Path | None
) -> Result:
    """Run a command with stdin closed: `claude -p` and `codex exec` wait on an open stdin."""

    try:
        if output is not None:
            output.parent.mkdir(parents=True, exist_ok=True)
            with output.open("a", encoding="utf-8") as sink:
                logged = subprocess.run(
                    list(cmd),
                    cwd=cwd,
                    stdin=subprocess.DEVNULL,
                    stdout=sink,
                    stderr=subprocess.STDOUT,
                    timeout=timeout,
                )
            return Result(logged.returncode, f"(output in {output})")
        done = subprocess.run(
            list(cmd),
            cwd=cwd,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return Result(done.returncode, done.stdout, done.stderr)
    except subprocess.TimeoutExpired:
        return Result(124, "", f"timed out after {timeout}s: {shlex.join(cmd)}")
    except FileNotFoundError as error:
        return Result(127, "", str(error))


class Shell:
    """Runs commands; in dry-run mode, logs mutating ones instead of running them."""

    def __init__(self, *, dry_run: bool, log: TextIO, executor: Executor = subprocess_executor):
        self.dry_run, self.log, self.executor = dry_run, log, executor

    def say(self, message: str) -> None:
        print(message, file=self.log, flush=True)

    def run(
        self,
        cmd: Sequence[str],
        *,
        cwd: Path | None = None,
        timeout: float | None = 600,
        mutating: bool = False,
        output: Path | None = None,
    ) -> Result:
        where = f" (in {cwd})" if cwd else ""
        if mutating and self.dry_run:
            self.say(f"[dry-run] would run: {shlex.join(cmd)}{where}")
            return Result(0, skipped=True)
        if mutating:
            self.say(f"$ {shlex.join(cmd)}{where}")
        result = self.executor(cmd, cwd, timeout, output)
        if mutating and not result.ok:
            self.say(
                f"  exit {result.returncode}: {(result.stderr or result.stdout).strip()[:2000]}"
            )
        return result

    def json(self, cmd: Sequence[str], **kwargs: Any) -> Any:
        result = self.run(cmd, **kwargs)
        if not result.ok:
            raise CommandError(cmd, result)
        return json.loads(result.stdout) if result.stdout.strip() else None


class MergeConflict(RuntimeError):
    """Bringing the PR up to date with main conflicts."""


class CommandError(RuntimeError):
    def __init__(self, cmd: Sequence[str], result: Result):
        super().__init__(f"{shlex.join(cmd)} exited {result.returncode}: {result.stderr.strip()}")
        self.result = result


@dataclass
class PullRequest:
    number: int
    title: str
    url: str
    head_ref: str
    head_sha: str
    base_ref: str
    labels: tuple[str, ...] = ()
    draft: bool = False
    mergeable: str = "UNKNOWN"

    @classmethod
    def from_gh(cls, data: dict[str, Any]) -> PullRequest:
        return cls(
            number=data["number"],
            title=data["title"],
            url=data["url"],
            head_ref=data["headRefName"],
            head_sha=data["headRefOid"],
            base_ref=data["baseRefName"],
            labels=tuple(label["name"] for label in data.get("labels") or []),
            draft=bool(data.get("isDraft")),
            mergeable=data.get("mergeable") or "UNKNOWN",
        )


@dataclass
class Outcome:
    number: int
    title: str
    url: str
    status: str  # merged | planned | skipped | NEEDS_ATTENTION member
    detail: str
    app_input: bool | None = None
    notes: list[str] = field(default_factory=list)


def gatekeeper_family(messages: Sequence[str], *, routine_fixed: bool) -> str | None:
    """The reviewer family for a PR: never the family that wrote any of its commits."""

    claude = routine_fixed or any(CLAUDE_TRAILER.search(m) for m in messages)
    codex = any(CODEX_TRAILER.search(m) for m in messages)
    if claude and codex:
        return None  # both families wrote commits: no independent reviewer
    return "codex" if claude else "claude"


def latest_verdict(reviews: Sequence[dict[str, Any]]) -> tuple[str, str] | None:
    """(head sha, verdict) from the newest gatekeeper marker among PR reviews."""

    found = None
    for review in reviews:
        match = VERDICT.search(review.get("body") or "")
        if match:
            found = (match.group(1), match.group(2))
    return found


def summarize_checks(checks: Sequence[dict[str, Any]]) -> tuple[str, list[str]]:
    """'pending', 'pass' or 'fail', and the failing checks, from `gh pr checks --json`."""

    if not checks:
        return "pending", []
    failing = [
        f"{c['name']} ({c.get('link') or 'no link'})"
        for c in checks
        if c.get("bucket") in ("fail", "cancel")
    ]
    if any(c.get("bucket") == "pending" for c in checks):
        return "pending", failing
    return ("fail" if failing else "pass"), failing


def workflow_triggers(text: str) -> set[str]:
    document = yaml.safe_load(text) or {}
    on = document.get("on", document.get(True)) if isinstance(document, dict) else None
    if isinstance(on, str):
        return {on}
    if isinstance(on, (list, dict)):
        return {str(trigger) for trigger in on}
    return set()


def unexercised_workflows(changed: Sequence[str], workflows: dict[str, str]) -> list[str]:
    """Changed workflow files that no pull_request event runs from the PR's own copy.

    `workflows` maps each `.github/workflows/<name>` at the PR head to its text.
    A workflow is exercised when it has a `pull_request` trigger or a
    pull_request workflow calls it (`uses: ./.github/workflows/<name>`).
    `pull_request_target` runs the base branch's copy, so it does not count.
    """

    on_pr = {path for path, text in workflows.items() if "pull_request" in workflow_triggers(text)}
    called = {
        f".github/workflows/{name}"
        for path in on_pr
        for name in re.findall(r"uses:\s*\./\.github/workflows/([\w.-]+)", workflows[path])
    }
    return sorted(
        path
        for path in changed
        if path.startswith(".github/workflows/")
        and path.endswith((".yml", ".yaml"))
        and path in workflows
        and path not in on_pr | called
    )


def render_summary(outcomes: Sequence[Outcome], *, when: str, dry_run: bool) -> str:
    def line(o: Outcome) -> str:
        app = {True: " (app input)", False: "", None: ""}[o.app_input]
        notes = "".join(f"\n  - {note}" for note in o.notes)
        return f"- #{o.number} {o.title}{app}: **{o.status}**, {o.detail}{notes}"

    merged = [o for o in outcomes if o.status == "merged"]
    attention = [o for o in outcomes if o.status in NEEDS_ATTENTION]
    rest = [o for o in outcomes if o not in merged and o not in attention]
    title = "Dependabot upkeep dry run" if dry_run else "Dependabot upkeep"
    parts = [f"## {title}, {when}", ""]
    parts.append(f"Merged: {len(merged)}. Needs attention: {len(attention)}.")
    for heading, group in (("Merged", merged), ("Needs attention", attention), ("Other", rest)):
        if group:
            parts += ["", f"### {heading}", "", *map(line, group)]
    if not outcomes:
        parts += ["", "No open Dependabot pull requests."]
    return "\n".join(parts) + "\n"


class Upkeep:
    def __init__(
        self,
        shell: Shell,
        *,
        repo: str,
        state: Path,
        review: Path,
        work: Path,
        fix: bool = True,
        sleep: Callable[[float], None] = time.sleep,
        clock: Callable[[], float] = time.monotonic,
    ):
        self.sh, self.repo, self.state = shell, repo, state
        self.review, self.work, self.fix = review, work, fix
        self.sleep, self.clock = sleep, clock
        stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        self.logs = state / "logs" / stamp

    def did(self, past: str, present: str) -> str:
        return f"would {present}" if self.sh.dry_run else past

    # GitHub reads -------------------------------------------------------------

    def gh(self, *args: str, **kwargs: Any) -> Any:
        return self.sh.json(["gh", *args], **kwargs)

    def open_prs(self) -> list[PullRequest]:
        fields = "number,title,url,headRefName,headRefOid,baseRefName,labels,isDraft,mergeable"
        data = self.gh(
            "pr", "list", "-R", self.repo, "--author", "app/dependabot", "--state", "open",
            "--json", fields, "--limit", "100",
        )  # fmt: skip
        return sorted((PullRequest.from_gh(d) for d in data or []), key=lambda p: p.number)

    def refresh(self, pr: PullRequest) -> PullRequest:
        fields = "number,title,url,headRefName,headRefOid,baseRefName,labels,isDraft,mergeable"
        return PullRequest.from_gh(
            self.gh("pr", "view", str(pr.number), "-R", self.repo, "--json", fields)
        )

    def behind_by(self, pr: PullRequest) -> int:
        data = self.gh("api", f"repos/{self.repo}/compare/{pr.base_ref}...{pr.head_sha}")
        return int(data["behind_by"])

    def commit_messages(self, pr: PullRequest) -> list[str]:
        pages = self.gh(
            "api", "--paginate", "--slurp", f"repos/{self.repo}/pulls/{pr.number}/commits"
        )
        return [c["commit"]["message"] for page in pages or [] for c in page]

    def commit_authors(self, pr: PullRequest) -> set[str]:
        pages = self.gh(
            "api", "--paginate", "--slurp", f"repos/{self.repo}/pulls/{pr.number}/commits"
        )
        return {(c.get("author") or {}).get("login", "") for page in pages or [] for c in page}

    def checks(self, pr: PullRequest) -> tuple[str, list[str]]:
        result = self.sh.run(
            ["gh", "pr", "checks", str(pr.number), "-R", self.repo, "--required",
             "--json", "name,bucket,link"],
        )  # fmt: skip
        # Exit 1 means a failing check (JSON still printed) or no checks yet; 8 is pending.
        try:
            data = json.loads(result.stdout) if result.stdout.strip() else []
        except json.JSONDecodeError:
            data = []
        return summarize_checks(data)

    # Steps ------------------------------------------------------------------

    def classify(self, pr: PullRequest) -> int:
        """verify_record.py's view of the PR head: RECORDED, NEEDS_PASS or NOT_APP_INPUT."""

        self.sh.run(
            ["git", "fetch", "-q", "origin", pr.base_ref, f"+refs/pull/{pr.number}/head:refs/upkeep/pr/{pr.number}"],
            cwd=self.review,
        )  # fmt: skip
        result = self.sh.run(
            [sys.executable, "mobile/scripts/verify_record.py", "check", "--pr", str(pr.number)],
            cwd=self.review,
        )
        if result.returncode not in (RECORDED, NEEDS_PASS, NOT_APP_INPUT):
            raise CommandError(["verify_record.py", "check"], result)
        return result.returncode

    def conflicts(self, pr: PullRequest) -> bool:
        """Whether merging main into the PR head conflicts (read-only; classify() fetched both)."""

        result = self.sh.run(
            ["git", "merge-tree", "--write-tree", "--quiet", f"origin/{pr.base_ref}", f"refs/upkeep/pr/{pr.number}"],
            cwd=self.review,
        )  # fmt: skip
        if result.returncode not in (0, 1):
            raise CommandError(["git", "merge-tree"], result)
        return result.returncode == 1

    def wait_for_head(self, pr: PullRequest, old: str) -> PullRequest:
        deadline = self.clock() + SECONDS["head"]
        while True:
            current = self.refresh(pr)
            if current.head_sha != old:
                return current
            if self.clock() >= deadline:
                raise RuntimeError(f"the PR head did not move from {old[:12]} after the update")
            self.sleep(5)

    def update_branch(self, pr: PullRequest) -> PullRequest:
        result = self.sh.run(
            ["gh", "api", "-X", "PUT", f"repos/{self.repo}/pulls/{pr.number}/update-branch",
             "-f", f"expected_head_sha={pr.head_sha}"],
            mutating=True,
        )  # fmt: skip
        if result.skipped:
            return pr
        if not result.ok:
            if "conflict" in (result.stderr + result.stdout).lower():
                raise MergeConflict("updating the branch from main conflicts")
            raise CommandError(["update-branch"], result)
        return self.wait_for_head(pr, pr.head_sha)

    def checkout_work(self, pr: PullRequest) -> bool:
        steps = [
            ["git", "fetch", "-q", "origin", pr.base_ref, f"+refs/pull/{pr.number}/head:refs/upkeep/pr/{pr.number}"],
            ["git", "checkout", "-q", "--detach", "-f", pr.head_sha],
            ["git", "clean", "-q", "-fd"],
        ]  # fmt: skip
        return all(self.sh.run(step, cwd=self.work, mutating=True).ok for step in steps)

    @contextmanager
    def verify_slot(self) -> Iterator[None]:
        """Hold the routine's mobile-verify lock and wait out any other mobile-verify run.

        The simulator and the shared Cargo target allow one run at a time.
        Runs started outside this routine do not take the lock, so also wait
        while `pgrep` finds one.
        """

        self.state.mkdir(parents=True, exist_ok=True)
        deadline = self.clock() + SECONDS["verify_lock"]
        with (self.state / "mobile-verify.lock").open("w") as lock:
            while True:
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    if self.clock() >= deadline:
                        raise TimeoutError("mobile-verify lock still held") from None
                    self.sleep(POLL)
            while self.sh.run(["pgrep", "-fl", VERIFY_PROCESS]).ok:
                if self.clock() >= deadline:
                    raise TimeoutError("another mobile-verify run is still going")
                self.sh.say("  waiting for another mobile-verify run to finish")
                self.sleep(POLL)
            yield

    def mobile_verify(self, pr: PullRequest) -> bool:
        """Run the full gate on the work checkout's HEAD; True when it passed."""

        if self.sh.dry_run:
            self.sh.say(
                "[dry-run] would run: just mobile-verify --phase all (in the work checkout)"
            )
            return True
        with self.verify_slot():
            result = self.sh.run(
                ["just", "mobile-verify", "--phase", "all"],
                cwd=self.work,
                timeout=SECONDS["verify"],
                mutating=True,
                output=self.logs / f"pr-{pr.number}-mobile-verify.log",
            )
        return result.ok

    def record(self) -> bool:
        return self.sh.run(["just", "mobile-verify-record"], cwd=self.work, mutating=True).ok

    def push_work(self, pr: PullRequest) -> bool:
        # Not forced: a push fails rather than overwrite commits pushed since.
        return self.sh.run(
            ["git", "push", "-q", "origin", f"HEAD:refs/heads/{pr.head_ref}"],
            cwd=self.work,
            mutating=True,
        ).ok

    def rerun_failed_ci(self, sha: str) -> None:
        """Rerun CI's failed jobs for a head whose pass was recorded after CI looked for it."""

        runs = self.gh(
            "run", "list", "-R", self.repo, "--commit", sha, "--workflow", "ci.yml",
            "--json", "databaseId,status,conclusion", "--limit", "5",
        )  # fmt: skip
        deadline = self.clock() + SECONDS["checks"]
        for run in runs or []:
            while run["status"] != "completed" and self.clock() < deadline:
                self.sleep(POLL)
                run = self.gh(
                    "run", "view", str(run["databaseId"]), "-R", self.repo,
                    "--json", "databaseId,status,conclusion",
                )  # fmt: skip
            if run.get("conclusion") == "failure":
                self.sh.run(
                    ["gh", "run", "rerun", str(run["databaseId"]), "-R", self.repo, "--failed"],
                    mutating=True,
                )

    def bring_current(self, pr: PullRequest, outcome: Outcome) -> tuple[PullRequest, bool]:
        """Update the PR from main and record its mobile-verify pass when needed.

        Returns the refreshed PR and whether the app-input gate is satisfied
        (always True for a PR that changes no app input).
        """

        behind = self.behind_by(pr)
        status = self.classify(pr)
        outcome.app_input = status != NOT_APP_INPUT
        if behind and self.conflicts(pr):
            raise MergeConflict("merging main into the PR head conflicts")
        if status == NOT_APP_INPUT:
            if behind:
                outcome.notes.append(
                    f"{self.did('updated', 'update')} from main ({behind} commits behind)"
                )
                pr = self.update_branch(pr)
            return pr, True
        if status == RECORDED and not behind:
            outcome.notes.append("mobile-verify pass already recorded for this input tree")
            return pr, True
        if not self.checkout_work(pr) and not self.sh.dry_run:
            raise RuntimeError("could not check out the PR head in the work checkout")
        if behind:
            merge = self.sh.run(
                ["git", "merge", "-q", "--no-edit", f"origin/{pr.base_ref}"],
                cwd=self.work,
                mutating=True,
            )
            if not merge.ok:
                self.sh.run(["git", "merge", "--abort"], cwd=self.work, mutating=True)
                raise MergeConflict("merging main into the PR head conflicts")
            outcome.notes.append(
                f"{self.did('merged', 'merge')} main locally ({behind} commits behind)"
            )
        passed = self.mobile_verify(pr)
        outcome.notes.append(
            "would run mobile-verify --phase all and record the pass"
            if self.sh.dry_run
            else f"mobile-verify --phase all {'passed' if passed else 'FAILED'}"
        )
        pushed_now = False
        if behind:
            if not self.push_work(pr):
                raise RuntimeError("pushing the update to the PR branch failed")
            pushed_now = not self.sh.dry_run
        if passed and not self.record() and not self.sh.dry_run:
            raise RuntimeError("just mobile-verify-record failed")
        if pushed_now:
            pr = self.wait_for_head(pr, pr.head_sha)
        elif passed and not self.sh.dry_run:
            # CI already ran on this head and found no pass.
            self.rerun_failed_ci(pr.head_sha)
        return pr, passed

    def wait_for_checks(self, pr: PullRequest) -> tuple[str, list[str]]:
        deadline = self.clock() + SECONDS["checks"]
        while True:
            state, failing = self.checks(pr)
            if state != "pending" or self.sh.dry_run:
                return state, failing
            if self.clock() >= deadline:
                return "timeout", failing
            self.sleep(POLL)

    def attempt_fix(self, pr: PullRequest, failing: Sequence[str]) -> PullRequest | None:
        """One bounded `claude -p` fix on the PR head; the pushed PR, or None."""

        if not self.checkout_work(pr) and not self.sh.dry_run:
            return None
        prompt = (
            f"Dependabot pull request #{pr.number} ({pr.title}) in {self.repo} fails these "
            f"checks: {'; '.join(failing) or 'see the PR'}. This checkout is at its head "
            f"{pr.head_sha} (detached). Make the smallest source change that adapts the "
            "repository to the dependency update so those checks pass. Follow AGENTS.md. "
            "Do not change the bumped versions, and do not weaken, skip, or delete tests, "
            "gates, lint rules, or coverage floors. Run the relevant local checks. Commit "
            "each logical change with a conventional commit message that ends with the "
            "trailer `Co-Authored-By: Claude <noreply@anthropic.com>`. Do not push, and do "
            "not comment on or change the pull request. If it cannot be fixed within these "
            "rules, make no commit and say why."
        )
        result = self.sh.run(
            ["claude", "-p", "--model", CLAUDE_MODEL, prompt],
            cwd=self.work,
            timeout=SECONDS["fix"],
            mutating=True,
            output=self.logs / f"pr-{pr.number}-fix.log",
        )
        if result.skipped:
            return pr
        added = self.sh.run(["git", "rev-list", "--count", f"{pr.head_sha}..HEAD"], cwd=self.work)
        if not result.ok or added.stdout.strip() in ("", "0"):
            return None
        return pr

    def push_fix(self, pr: PullRequest, outcome: Outcome) -> tuple[PullRequest, bool]:
        """Verify (when app input), push and record the fix; (PR, app gate satisfied)."""

        passed = True
        if outcome.app_input:
            passed = self.mobile_verify(pr)
            outcome.notes.append(f"mobile-verify after the fix {'passed' if passed else 'FAILED'}")
        if not self.push_work(pr):
            raise RuntimeError("pushing the fix failed")
        if outcome.app_input and passed:
            self.record()
        return self.wait_for_head(pr, pr.head_sha), passed

    def probe_workflows(self, pr: PullRequest, outcome: Outcome) -> bool:
        """Dry-run changed workflows that pull requests never run; False on a failed probe."""

        files = self.gh("pr", "view", str(pr.number), "-R", self.repo, "--json", "files")
        changed = [f["path"] for f in (files or {}).get("files", [])]
        if not any(p.startswith(".github/workflows/") for p in changed):
            return True
        # The head may have moved since classify() fetched it.
        self.sh.run(
            [
                "git",
                "fetch",
                "-q",
                "origin",
                f"+refs/pull/{pr.number}/head:refs/upkeep/pr/{pr.number}",
            ],
            cwd=self.review,
        )
        listing = self.sh.run(
            ["git", "ls-tree", "--name-only", f"refs/upkeep/pr/{pr.number}", ".github/workflows/"],
            cwd=self.review,
        )
        workflows = {}
        for path in listing.stdout.split():
            text = self.sh.run(
                ["git", "show", f"refs/upkeep/pr/{pr.number}:{path}"], cwd=self.review
            )
            workflows[path] = text.stdout
        ok = True
        for path in unexercised_workflows(changed, workflows):
            name = path.rsplit("/", 1)[1]
            inputs = DISPATCH_PROBES.get(name)
            if inputs is None:
                outcome.notes.append(
                    f"{name} does not run on pull requests; covered by CI's static workflow checks only"
                )
                continue
            ok = self.dispatch_probe(pr, name, inputs, outcome) and ok
        return ok

    def dispatch_probe(
        self, pr: PullRequest, name: str, inputs: dict[str, str], outcome: Outcome
    ) -> bool:
        args = [arg for key, value in inputs.items() for arg in ("-f", f"{key}={value}")]
        started = self.sh.run(
            ["gh", "workflow", "run", name, "-R", self.repo, "--ref", pr.head_ref, *args],
            mutating=True,
        )
        if started.skipped:
            outcome.notes.append(f"would dry-run {name} on the PR branch")
            return True
        deadline = self.clock() + SECONDS["probe"]
        while self.clock() < deadline:
            self.sleep(POLL)
            runs = self.gh(
                "run", "list", "-R", self.repo, "--workflow", name, "--branch", pr.head_ref,
                "--event", "workflow_dispatch", "--json", "databaseId,headSha,status,conclusion,url",
                "--limit", "5",
            )  # fmt: skip
            run = next((r for r in runs or [] if r["headSha"] == pr.head_sha), None)
            if run and run["status"] == "completed":
                passed = run["conclusion"] == "success"
                note = f"{name} dry run on {pr.head_sha[:12]}: {run['conclusion']} ({run['url']})"
                outcome.notes.append(note)
                self.sh.run(
                    ["gh", "pr", "comment", str(pr.number), "-R", self.repo, "--body",
                     f"Dependabot upkeep: {note}. `{name}` never runs on pull requests, so "
                     "this manual dry run is its only check before merge."],
                    mutating=True,
                )  # fmt: skip
                return passed
        outcome.notes.append(f"{name} dry run did not finish within an hour")
        return False

    def gatekeeper(self, pr: PullRequest, family: str) -> Result:
        self.sh.run(["git", "fetch", "-q", "origin"], cwd=self.review)
        self.sh.run(["git", "reset", "-q", "--hard", "origin/main"], cwd=self.review, mutating=True)
        prompt = f"Run the gatekeeper skill on PR {pr.number}."
        output = self.logs / f"pr-{pr.number}-gatekeeper-{family}.log"
        if family == "codex":
            report = self.logs / f"pr-{pr.number}-gatekeeper-report.md"
            cmd = [
                "codex", "exec", "-C", str(self.review), "-m", CODEX_MODEL,
                "-c", "model_reasoning_effort=high", "-o", str(report), prompt,
            ]  # fmt: skip
        else:
            cmd = ["claude", "-p", "--model", CLAUDE_MODEL, prompt]
        return self.sh.run(
            cmd, cwd=self.review, timeout=SECONDS["gatekeeper"], mutating=True, output=output
        )

    def verdict(self, pr: PullRequest) -> tuple[str, str]:
        state = self.gh("pr", "view", str(pr.number), "-R", self.repo, "--json", "state")
        if state and state.get("state") == "MERGED":
            return "merged", "merged by the gatekeeper"
        pages = self.gh(
            "api", "--paginate", "--slurp", f"repos/{self.repo}/pulls/{pr.number}/reviews"
        )
        found = latest_verdict([r for page in pages or [] for r in page])
        if found is None or found[0] != pr.head_sha:
            return "error", "the gatekeeper posted no verdict for the current head"
        verdict = found[1]
        status = {"approve": "error", "changes": "changes", "handoff": "handoff"}.get(
            verdict, verdict
        )
        detail = {
            "approve": "gatekeeper approved but did not merge",
            "changes": "gatekeeper requested changes",
            "handoff": "gatekeeper handed it to the owner",
            "escalated": "gatekeeper escalated it to the owner",
            "blocked": "gatekeeper found it blocked",
        }.get(verdict, f"gatekeeper verdict {verdict}")
        return (status if status in NEEDS_ATTENTION else "error"), detail

    # Per-PR flow --------------------------------------------------------------

    def process(self, pr: PullRequest, exclude: set[int]) -> Outcome:
        outcome = Outcome(pr.number, pr.title, pr.url, "planned" if self.sh.dry_run else "open", "")
        if pr.number in exclude:
            outcome.status, outcome.detail = "skipped", "excluded for this run"
            return outcome
        if pr.draft or pr.base_ref != "main":
            outcome.status, outcome.detail = "skipped", "draft or not based on main"
            return outcome
        if OWNER_LABEL in pr.labels:
            outcome.status, outcome.detail = "owner", "waiting for the owner (needs-owner)"
            return outcome
        try:
            if pr.mergeable == "CONFLICTING":
                raise MergeConflict("conflicts with main")
            return self._process(pr, outcome)
        except MergeConflict as conflict:
            return self.ask_dependabot(pr, outcome, str(conflict))
        except (CommandError, RuntimeError, TimeoutError) as error:
            outcome.status, outcome.detail = "error", str(error)
            return outcome

    def ask_dependabot(self, pr: PullRequest, outcome: Outcome, reason: str) -> Outcome:
        """Have Dependabot rebuild a PR that conflicts with main."""

        # Dependabot rebases only while no one else has pushed to its branch.
        others = self.commit_authors(pr) - {"dependabot[bot]"}
        command = "@dependabot recreate" if others else "@dependabot rebase"
        self.sh.run(
            ["gh", "pr", "comment", str(pr.number), "-R", self.repo, "--body", command],
            mutating=True,
        )
        outcome.status = "conflict"
        outcome.detail = f"{reason}; {self.did('asked', 'ask')} `{command}`"
        return outcome

    def _process(self, pr: PullRequest, outcome: Outcome) -> Outcome:
        pr, app_ok = self.bring_current(pr, outcome)
        state, failing = self.wait_for_checks(pr)
        fixed = False
        if (state == "fail" or not app_ok) and self.fix and not self.sh.dry_run:
            reasons = failing or []
            if not app_ok:
                reasons = [
                    *reasons,
                    "local `just mobile-verify --phase all` (mobile/.verification/summary.txt)",
                ]
            outcome.notes.append("attempting one bounded fix")
            patched = self.attempt_fix(pr, reasons)
            if patched is None:
                outcome.notes.append("the fix attempt made no commit")
            else:
                pr, app_ok = self.push_fix(patched, outcome)
                fixed = True
                state, failing = self.wait_for_checks(pr)
        if state == "timeout":
            outcome.status, outcome.detail = (
                "timeout",
                "required checks still pending after 3 hours",
            )
            return outcome
        if (state == "fail" or not app_ok) and self.sh.dry_run:
            outcome.notes.append(
                "red at the current head; a real run would wait for CI on the updated head"
                + (" and attempt one bounded fix if still red" if self.fix else "")
            )
        if state == "fail" or not app_ok:
            outcome.status = "red"
            outcome.detail = (
                "red: "
                + ("; ".join(failing) or "mobile-verify failed")
                + (" (after one fix attempt)" if fixed else "")
            )
            return outcome
        if self.sh.dry_run:
            outcome.notes.append(f"required checks now: {state}")
        if not self.probe_workflows(pr, outcome):
            outcome.status, outcome.detail = "red", "a workflow dry run failed"
            return outcome
        family = gatekeeper_family(self.commit_messages(pr), routine_fixed=fixed)
        if family is None:
            outcome.status = "owner"
            outcome.detail = "both Claude and Codex wrote commits; no independent gatekeeper"
            return outcome
        ran = self.gatekeeper(pr, family)
        if ran.skipped:
            outcome.detail = f"would run the {family} gatekeeper"
            return outcome
        outcome.notes.append(
            f"gatekeeper log: {self.logs / f'pr-{pr.number}-gatekeeper-{family}.log'}"
        )
        if not ran.ok:
            outcome.notes.append(f"{family} gatekeeper exited {ran.returncode}")
        outcome.status, outcome.detail = self.verdict(pr)
        outcome.detail += f" ({family})"
        return outcome

    # Summary ------------------------------------------------------------------

    def tracking_issue(self) -> int | None:
        issues = self.gh(
            "issue", "list", "-R", self.repo, "--state", "open", "--search",
            f'"{TRACKING_TITLE}" in:title', "--json", "number,body", "--limit", "20",
        )  # fmt: skip
        for issue in issues or []:
            if TRACKING_MARKER in (issue.get("body") or ""):
                return int(issue["number"])
        body = (
            f"{TRACKING_MARKER}\nThe weekly Dependabot upkeep routine "
            "(`scripts/dependabot_upkeep.py`, issue #2098) comments here after each run: "
            "what the gatekeeper merged and what needs attention. Keep this issue open "
            "and pinned."
        )
        created = self.sh.run(
            ["gh", "issue", "create", "-R", self.repo, "--title", TRACKING_TITLE, "--body", body,
             "--label", "dependencies", "--label", "infra", "--label", "P3"],
            mutating=True,
        )  # fmt: skip
        if created.skipped:
            return None
        if not created.ok:
            raise CommandError(["gh", "issue", "create"], created)
        number = int(created.stdout.strip().rsplit("/", 1)[1])
        self.sh.run(["gh", "issue", "pin", str(number), "-R", self.repo], mutating=True)
        return number

    def run(self, *, only: set[int], exclude: set[int]) -> tuple[list[Outcome], str]:
        self.sh.say(f"Dependabot upkeep for {self.repo}{' (dry run)' if self.sh.dry_run else ''}")
        prs = [pr for pr in self.open_prs() if not only or pr.number in only]
        outcomes = []
        for pr in prs:
            self.sh.say(f"\n#{pr.number} {pr.title}")
            outcome = self.process(pr, exclude)
            for note in outcome.notes:
                self.sh.say(f"  - {note}")
            self.sh.say(f"  => {outcome.status}: {outcome.detail}")
            outcomes.append(outcome)
        if not self.sh.dry_run:
            self.final_sweep(outcomes)
        when = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
        summary = render_summary(outcomes, when=when, dry_run=self.sh.dry_run)
        return outcomes, summary

    def final_sweep(self, outcomes: Sequence[Outcome]) -> None:
        """Leave every PR still open current with main, after this run's merges."""

        still_open = {
            o.number
            for o in outcomes
            if o.status in {"red", "changes", "handoff", "escalated", "blocked", "error", "timeout"}
        }
        for pr in self.open_prs():
            if pr.number not in still_open:
                continue
            outcome = next(o for o in outcomes if o.number == pr.number)
            try:
                if self.behind_by(pr):
                    self.bring_current(pr, outcome)
            except (CommandError, RuntimeError, TimeoutError) as error:
                outcome.notes.append(f"final update failed: {error}")

    def post_summary(self, summary: str) -> None:
        number = self.tracking_issue()
        if number is None:  # dry run, no tracking issue yet
            self.sh.say(f"[dry-run] would comment the summary on a new '{TRACKING_TITLE}' issue")
            return
        self.sh.run(
            ["gh", "issue", "comment", str(number), "-R", self.repo, "--body", summary],
            mutating=True,
        )


# Install / uninstall ------------------------------------------------------------


def schedule_local(hour_utc: int = 9, minute: int = 0) -> tuple[int, int]:
    """Monday 09:00 UTC (about three hours after Dependabot) in this Mac's local time.

    launchd schedules in local time, so the UTC hour shifts by an hour across
    daylight saving; either way the run comes well after 06:17 UTC.
    """

    now = dt.datetime.now().astimezone()
    offset = now.utcoffset() or dt.timedelta()
    local = (dt.datetime(2000, 1, 3, hour_utc, minute) + offset).time()
    return local.hour, local.minute


def launch_agent_plist(
    *, python: str, script: Path, state: Path, path_env: str, exclude: Sequence[int]
) -> dict[str, Any]:
    hour, minute = schedule_local()
    args = ["/usr/bin/caffeinate", "-i", python, str(script), "run", "--self-update"]
    for number in exclude:
        args += ["--exclude", str(number)]
    return {
        "Label": AGENT_LABEL,
        "ProgramArguments": args,
        "WorkingDirectory": str(script.parents[1]),
        "EnvironmentVariables": {"PATH": path_env, "RUNDALE_UPKEEP_STATE": str(state)},
        # A Monday the Mac sleeps through runs once on wake (cron would skip it).
        "StartCalendarInterval": {"Weekday": 1, "Hour": hour, "Minute": minute},
        "ProcessType": "Background",
        "StandardOutPath": str(state / "logs" / "launchd.log"),
        "StandardErrorPath": str(state / "logs" / "launchd.log"),
    }


def install(args: argparse.Namespace, shell: Shell) -> int:
    state: Path = args.state
    review, work = state / "review", state / "work"
    (state / "logs").mkdir(parents=True, exist_ok=True)
    source = ROOT
    shell.run(["git", "fetch", "-q", "origin", "main"], cwd=source)
    if not review.exists():
        steps = [
            ["git", "worktree", "add", "-q", "-B", REVIEW_BRANCH, str(review), "origin/main"],
            ["git", "-C", str(review), "branch", "-q", "--set-upstream-to", "origin/main"],
        ]
        for step in steps:
            if not shell.run(step, cwd=source, mutating=True).ok:
                return 1
    if (
        not work.exists()
        and not shell.run(
            ["git", "worktree", "add", "-q", "--detach", str(work), "origin/main"],
            cwd=source,
            mutating=True,
        ).ok
    ):
        return 1
    plist = launch_agent_plist(
        python=sys.executable,
        script=review / "scripts" / "dependabot_upkeep.py",
        state=state,
        path_env=os.environ.get("PATH", "/usr/bin:/bin"),
        exclude=args.exclude,
    )
    if args.dry_run:
        shell.say(plistlib.dumps(plist).decode())
        return 0
    LAUNCH_AGENT.parent.mkdir(parents=True, exist_ok=True)
    LAUNCH_AGENT.write_bytes(plistlib.dumps(plist))
    domain = f"gui/{os.getuid()}"
    shell.run(["launchctl", "bootout", f"{domain}/{AGENT_LABEL}"])
    if not shell.run(["launchctl", "bootstrap", domain, str(LAUNCH_AGENT)], mutating=True).ok:
        return 1
    when = plist["StartCalendarInterval"]
    shell.say(
        f"Installed {LAUNCH_AGENT}: Mondays at {when['Hour']:02d}:{when['Minute']:02d} local "
        f"time. Logs in {state / 'logs'}. Remove with `{shlex.join([sys.executable, str(Path(__file__).resolve()), 'uninstall'])}`."
    )
    return 0


def uninstall(args: argparse.Namespace, shell: Shell) -> int:
    shell.run(["launchctl", "bootout", f"gui/{os.getuid()}/{AGENT_LABEL}"], mutating=True)
    if LAUNCH_AGENT.exists() and not args.dry_run:
        LAUNCH_AGENT.unlink()
    for checkout in (args.state / "review", args.state / "work"):
        if checkout.exists():
            shell.run(
                ["git", "worktree", "remove", "--force", str(checkout)], cwd=ROOT, mutating=True
            )
    shell.run(["git", "branch", "-q", "-D", REVIEW_BRANCH], cwd=ROOT, mutating=True)
    shell.say(
        f"Removed the LaunchAgent and routine checkouts; logs remain in {args.state / 'logs'}."
    )
    return 0


# Entry point ---------------------------------------------------------------------


def self_update(review: Path, argv: Sequence[str]) -> None:
    """Move the routine's checkout to origin/main and re-exec the updated script."""

    for step in (["git", "fetch", "-q", "origin"], ["git", "reset", "-q", "--hard", "origin/main"]):
        subprocess.run(step, cwd=review, check=True, stdin=subprocess.DEVNULL)
    rest = [a for a in argv if a != "--self-update"]
    script = review / "scripts" / "dependabot_upkeep.py"
    os.execv(sys.executable, [sys.executable, str(script), *rest])


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--state",
        type=Path,
        default=Path(os.environ.get("RUNDALE_UPKEEP_STATE", DEFAULT_STATE)),
        help=f"routine checkouts, locks and logs (default {DEFAULT_STATE})",
    )
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run", help="process the open Dependabot pull requests")
    run.add_argument("--dry-run", action="store_true", help="plan only; change nothing")
    run.add_argument("--pr", type=int, action="append", default=[], help="only these PRs")
    run.add_argument("--exclude", type=int, action="append", default=[], help="skip these PRs")
    run.add_argument("--no-fix", dest="fix", action="store_false", help="no fix attempts")
    run.add_argument("--no-summary", dest="summary", action="store_false")
    run.add_argument("--repo", default=os.environ.get("RUNDALE_UPKEEP_REPO", "dmooney/Rundale"))
    run.add_argument("--self-update", action="store_true", help=argparse.SUPPRESS)
    inst = sub.add_parser("install", help="install the weekly LaunchAgent")
    inst.add_argument("--exclude", type=int, action="append", default=[])
    inst.add_argument("--dry-run", action="store_true")
    uninst = sub.add_parser("uninstall", help="remove the LaunchAgent and routine checkouts")
    uninst.add_argument("--dry-run", action="store_true")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    args = build_parser().parse_args(argv)
    shell = Shell(dry_run=args.dry_run, log=sys.stdout)
    if args.command == "install":
        return install(args, shell)
    if args.command == "uninstall":
        return uninstall(args, shell)
    review, work = args.state / "review", args.state / "work"
    if args.self_update:
        self_update(review, argv)
    if not review.exists():
        # Before `install`, plan from this checkout (dry runs and tests only).
        if not args.dry_run:
            print(f"No routine checkout at {review}; run `install` first.", file=sys.stderr)
            return 2
        review = work = ROOT
    args.state.mkdir(parents=True, exist_ok=True)
    with (args.state / "upkeep.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print("Another upkeep run holds the lock; exiting.", file=sys.stderr)
            return 3
        upkeep = Upkeep(
            shell, repo=args.repo, state=args.state, review=review, work=work, fix=args.fix
        )
        outcomes, summary = upkeep.run(only=set(args.pr), exclude=set(args.exclude))
        upkeep.logs.mkdir(parents=True, exist_ok=True)
        (upkeep.logs / "summary.md").write_text(summary, encoding="utf-8")
        print("\n" + summary)
        if args.summary:
            upkeep.post_summary(summary)
    return 1 if any(o.status == "error" for o in outcomes) else 0


if __name__ == "__main__":
    raise SystemExit(main())
