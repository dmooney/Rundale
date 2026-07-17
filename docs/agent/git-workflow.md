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

## Merge protection

GitHub merge queues are unavailable while this repository is user-owned. The
exact unblock trigger is transfer to an eligible GitHub organization or future
GitHub support for queues on user-owned repositories.

Until then, protect `main` with the closest available gate:

- require the fast `CI gate` with strict status checks, so an out-of-date PR
  must be brought current before merge;
- make that single required gate aggregate the complete Playwright suite for
  every pull request whose path detector reports a shipped UI change;
- require all review conversations to be resolved;
- run `Full CI` on the PR head when the change needs the Rust, coverage,
  harness, UI, or end-to-end gates; and
- stop new starts and repair immediately if the post-merge `Full CI` run makes
  `main` red.

`full-ci.yml` retains its `merge_group` trigger so the stronger queue gate is
ready if repository ownership changes.

## CI cost controls

The fast `ci.yml` workflow uses path filtering so a doc/chore/CI-agent-only PR
pays only the relevant proof, documentation, and format checks. A pull request
that changes `parish/apps/ui/**` runs the complete Playwright contract before
`CI gate` can pass. Expensive Rust, coverage, harness, and the remaining UI jobs
live in `full-ci.yml`; it runs on pushes to `main`/`develop`, `merge_group`,
nightly schedule, and manual dispatch. Until a merge queue is available,
dispatch it explicitly for high-risk PR heads and let the post-merge run catch
any remaining integration failure.

Replacing the shipped default UI surface is one logical contract migration:
the same pull request must migrate or explicitly retire every canonical E2E
assertion for the prior surface, and the complete `just ui-e2e` suite must pass.
A focused smoke test does not satisfy this gate by itself.
