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

After implementing any gameplay feature, run `/prove <feature description>` to verify it works at runtime. Unit tests passing is **not** sufficient — you must see the feature working in actual game output.

## Pull requests

Explain the behavior change, link related issues, list commands run (`just check`, `just verify`, UI tests), and include screenshots or updated Playwright baselines for visible UI changes.
