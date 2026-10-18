#!/usr/bin/env python3
"""Tests for the workflow action-reference check, against fake action metadata."""

from __future__ import annotations

import importlib
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from typing import TYPE_CHECKING

sys.path.insert(0, str(Path(__file__).resolve().parent))

if TYPE_CHECKING:
    import check_action_refs as refs
else:
    refs = importlib.import_module("check_action_refs")

ACTIONS = {
    ("actions/checkout", "action.yml", "v7"): """
        inputs:
          ref: {required: false}
          fetch-depth: {default: "1"}
        runs: {using: node24}
    """,
    ("actions/old", "action.yaml", "v1"): "runs: {using: node12}\n",
    ("acme/tools", "lint/action.yml", "abc123"): """
        inputs:
          token: {required: true}
          level: {required: true, default: warn}
        runs: {using: composite}
    """,
    ("google/osv", ".github/workflows/scan.yml", "v2"): """
        on:
          workflow_call:
            inputs:
              scan-args: {type: string}
    """,
    ("google/osv", ".github/workflows/push-only.yml", "v2"): "on: push\n",
}


def fetch(repo: str, path: str, ref: str) -> str | None:
    text = ACTIONS.get((repo, path, ref))
    return textwrap.dedent(text) if text is not None else None


class CheckActionRefsTests(unittest.TestCase):
    def check(self, workflow: str) -> list[str]:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "wf.yml"
            path.write_text(textwrap.dedent(workflow), encoding="utf-8")
            return refs.check_workflows([path], fetch)

    def test_valid_steps_and_reusable_workflows_pass(self) -> None:
        problems = self.check("""
            on: pull_request
            jobs:
              build:
                runs-on: ubuntu-latest
                steps:
                  - uses: actions/checkout@v7
                    with: {fetch-depth: 0, REF: main}
                  - uses: acme/tools/lint@abc123
                    with: {token: x}
                  - uses: ./local-action
                  - uses: docker://alpine:3
                  - run: echo hi
              scan:
                uses: google/osv/.github/workflows/scan.yml@v2
                with: {scan-args: -r ./}
        """)
        self.assertEqual(problems, [])

    def test_breaking_bumps_are_reported(self) -> None:
        problems = self.check("""
            on: push
            jobs:
              build:
                runs-on: ubuntu-latest
                steps:
                  - uses: actions/checkout@v7
                    with: {persist-creds: false}
                  - uses: actions/checkout@v99
                  - uses: actions/old@v1
                  - uses: acme/tools/lint@abc123
                  - uses: not-a-reference
              call:
                uses: google/osv/.github/workflows/push-only.yml@v2
        """)
        expected = [
            "input 'persist-creds' is not declared",
            "actions/checkout@v99: no action.yml or workflow at that ref",
            "runs.using 'node12' is not supported",
            "required input 'token' is missing",
            "not an owner/repo[/path]@ref reference",
            "the workflow has no workflow_call trigger",
        ]
        for fragment in expected:
            self.assertTrue(any(fragment in p for p in problems), (fragment, problems))
        self.assertFalse(any("'level'" in p for p in problems), problems)  # has a default
        self.assertEqual(len(problems), len(expected))

    def test_metadata_is_fetched_once_per_reference(self) -> None:
        calls: list[tuple[str, str, str]] = []

        def counting(repo: str, path: str, ref: str) -> str | None:
            calls.append((repo, path, ref))
            return fetch(repo, path, ref)

        checker = refs.Checker(counting)
        usage = refs.Usage("wf.yml", "job a step 1", "actions/checkout@v7", ())
        checker.check(usage)
        checker.check(usage)
        self.assertEqual(calls, [("actions/checkout", "action.yml", "v7")])


if __name__ == "__main__":
    unittest.main()
