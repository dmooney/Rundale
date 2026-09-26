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

- Behavior changes require appropriate tests of observable behavior and failure cases.
- The Rust coverage ratchet must pass (`just coverage-check`). Raise the ratchet floor as coverage-recovery work lands; the long-term target is **90%**.
- No `#[allow]` without a justifying comment.
- When creating PRs, make sure the PR content makes it into a design doc.

## Play-test verification

After implementing any gameplay feature, run `/limerick-engine prove <feature description>` to verify it works at runtime. Unit tests passing is **not** sufficient — you must see the feature working in actual game output.

## Pull requests

Explain the behavior change, link related issues, list commands run (`just check`, `just verify`, UI tests), and include screenshots or updated Playwright baselines for visible UI changes.

## Merge protection

`ci.yml` supports GitHub's merge queue and makes the required `CI gate` rerun
against the queue's synthetic merge commit. Queue entries always run the full
runtime suite because `merge_group` events do not provide a pull-request diff.
The PR-only agent proof gate is not repeated: GitHub admits only a PR whose
required check already validated its proof body, while the queued commit is
validated by every code and runtime sensor.

GitHub merge queues remain unavailable while this repository is user-owned.
Transfer it to an eligible GitHub organization, then enable “Require merge
queue” for `main`, retain `CI gate` as the sole required check, and turn off
“Require branches to be up to date before merging.” Until then:

- require the fast `CI gate` with strict status checks, so an out-of-date PR
  must be brought current before merge;
- make that single required gate aggregate the complete Playwright suite for
  every pull request whose path detector reports a shipped UI change;
- require all review conversations to be resolved;
- let runtime-changing PRs call `Full CI` through the required gate; and
- stop new starts and repair immediately if the post-merge `Full CI` run makes
  `main` red.

`full-ci.yml` retains its `merge_group` trigger so the stronger queue gate is
ready if repository ownership changes.

## CI cost controls

The fast `ci.yml` workflow uses path filtering so a doc/chore/CI-agent-only PR
pays only the relevant proof, documentation, and format checks. A pull request
that changes `limerick/apps/ui/**` runs the complete Playwright contract before
`CI gate` can pass. Queue entries always run the complete correctness suite
through `ci.yml`, preserving the guarantee that a PR is tested with the `main`
and earlier queued changes it will merge behind. Expensive Rust, coverage,
harness, and the remaining UI jobs live in `full-ci.yml`; it also runs on pushes
to `main`/`develop`, the nightly schedule, and manual dispatch. Until a merge
queue is available, strict status checks provide the closest equivalent.

Replacing the existing Svelte default UI surface is one logical contract migration:
the same pull request must migrate or explicitly retire every canonical E2E
assertion for the prior surface, and the complete `just ui-e2e` suite must pass.
A focused smoke test does not satisfy this gate by itself.

The separate native mobile reset does not require Svelte feature parity. It follows
the [current product specifications](../product-specs/README.md); preserve existing
shared-engine contracts and do not silently retire tests of maintained surfaces.
Mobile interaction evidence includes the required physical-iPhone gates.
