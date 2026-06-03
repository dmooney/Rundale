# Git Workflow & Engineering Standards

## Conventional commits

Prefixes: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`. One logical change per commit. Imperative summaries. Reference issues with `resolve #135` when relevant.

## Pre-push

Run the full test suite before pushing:

```sh
just check     # fmt + clippy + tests
just verify    # check + harness walkthrough
```

## Engineering standards

- All new code must have accompanying unit tests.
- The Rust coverage ratchet must pass (`just coverage-check`). Raise the ratchet floor as coverage-recovery work lands; the long-term target is **90%**.
- No `#[allow]` without a justifying comment.
- When creating PRs, make sure the PR content makes it into a design doc.

## Play-test verification

After implementing any gameplay feature, run `/parish-engine prove <feature description>` to verify it works at runtime. Unit tests passing is **not** sufficient — you must see the feature working in actual game output.

## Pull requests

Explain the behavior change, link related issues, list commands run (`just check`, `just verify`, UI tests), and include screenshots or updated Playwright baselines for visible UI changes.

## Merge queue

CI declares an `on: merge_group` trigger, so the full suite re-runs against the
tip of `main` when a PR enters the queue. This catches *semantic* conflicts —
two PRs each green in isolation that break once combined — which plain git
merges miss. With many worktrees landing in parallel this is the main guard on
`main` staying green.

Enabling the queue itself is a one-time **repo-admin** action (Settings →
Branches), outside the codebase:

- Require a merge queue on the protected `main` branch.
- Set required status checks to the jobs that always run on `merge_group`
  (e.g. `rust-quality-gate`, `game-harness`, `ui-quality`). Do **not** mark a
  path-filtered job required in a way that blocks on `skipped` for doc-only PRs:
  a job skipped by the `changes` filter reports `skipped`, which classic branch
  protection treats as pending. Require the always-on jobs, or rely on the
  `merge_group` run (where nothing is skipped) as the gate.

## CI cost controls

A `changes` job (dorny/paths-filter) runs first on every PR and gates the heavy
Rust/UI jobs: a doc/chore/CI-agent-only PR skips them and pays only
`agent-check` and `docs-consistency`. The filter applies **only on
`pull_request`** — on `push`, `merge_group`, and the nightly `schedule` every
job runs regardless, so the merge gate and nightly catch anything a per-PR skip
might miss.
