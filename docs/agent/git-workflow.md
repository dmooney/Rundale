# Git Workflow & Engineering Standards

## Conventional commits

Prefixes: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`. One logical change per commit. Imperative summaries. No trailers (`Co-Authored-By`, session links, or other attribution). Reference issues with `resolve #135` when relevant.

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

## Dependency updates

Dependabot opens its weekly pull requests on Monday at 06:17 UTC
([`.github/dependabot.yml`](../../.github/dependabot.yml)). Every tracked
lockfile must have an entry there or a reasoned exclusion in
[`scripts/check_dependabot_coverage.py`](../../scripts/check_dependabot_coverage.py);
CI's `python-quality` job and `just dependabot-coverage` fail otherwise. No
workflow merges them. A LaunchAgent on the owner's Mac runs
[`scripts/dependabot_upkeep.py`](../../scripts/dependabot_upkeep.py) every
Monday at about 09:00 UTC (issue #2098). For each open Dependabot pull request
it:

1. skips drafts, `--exclude`d pull requests, and pull requests labelled
   `needs-owner`, and asks Dependabot to rebase or recreate one that conflicts
   with `main`;
2. brings it up to date with `main` using the owner's `gh` token, so CI runs.
   A pull request that changes app inputs (`verify_record.py check`, the same
   test CI's "Recorded mobile-verify pass" job applies) is merged with `main`
   in a clean checkout, passes `just mobile-verify --phase all`, is pushed, and
   gets `just mobile-verify-record` (a recorded pass is reused when the merge
   changed no app input). One mobile-verify runs at a time: `verify.py` holds
   a lock every worktree on the Mac shares (`RUNDALE_MOBILE_VERIFY_LOCK`), and
   the routine also waits while `pgrep` finds a run from an older checkout;
3. waits for the required checks. A red pull request gets one bounded
   `claude -p` fix; if that makes no commit or the checks stay red, the pull
   request stays open and is reported. A red pull request is never handed on
   for merge;
4. dry-runs `release.yml` on the branch when an Actions bump changes it
   (`release.yml` never runs on pull requests; CI's `workflow-quality` job
   checks every workflow's action inputs statically);
5. runs a fresh [gatekeeper](../../.agents/skills/gatekeeper/SKILL.md), which
   merges it, sends it back, or hands it to the owner. The gatekeeper is
   always Gemini 4 Argon through agy on the owner's Google subscription,
   whether the commits are Dependabot's or the routine's own (Claude) fix.

It then comments a summary (merged, needs attention, other) on the pinned
"Dependabot upkeep reports" issue, creating and pinning the issue on its first
run. The comment is posted by the `upkeep-summary.yml` workflow (dispatched on
`main`) as github-actions[bot] with an `@dmooney` mention, because GitHub does not
notify you of your own comments; if the dispatch fails the routine logs it and
comments directly, without a notification. Logs are under `~/Library/Application Support/rundale-dependabot-upkeep/logs/`.

```sh
just dependabot-upkeep run --dry-run      # plan only: no pushes, runs, or comments
just dependabot-upkeep run --pr 2234      # one real pass over one pull request
just dependabot-upkeep install            # LaunchAgent + the routine's own checkouts
just dependabot-upkeep uninstall
```

The routine runs from its own worktrees (`review/`, kept at `origin/main`,
where the gatekeeper runs, and `work/`, where mobile-verify and fixes run),
never from the owner's checkout. A launchd calendar job runs on wake when the
Mac slept through Monday morning, which cron would skip; `caffeinate -i` keeps
the Mac awake for the run.

## CI cost controls

The fast `ci.yml` workflow uses path filtering so a doc/chore/CI-agent-only PR
pays only the relevant proof, documentation, and format checks. A pull request
that changes `limerick/apps/ui/**` runs the complete Playwright contract before
`CI gate` can pass. Expensive Rust, coverage, harness, and the remaining UI jobs
live in `full-ci.yml`; it runs on pushes to `main`/`develop`, `merge_group`,
nightly schedule, and manual dispatch. Until a merge queue is available,
dispatch it explicitly for high-risk PR heads and let the post-merge run catch
any remaining integration failure.

Replacing the existing Svelte default UI surface is one logical contract migration:
the same pull request must migrate or explicitly retire every canonical E2E
assertion for the prior surface, and the complete `just ui-e2e` suite must pass.
A focused smoke test does not satisfy this gate by itself.

The separate native mobile reset does not require Svelte feature parity. It follows
the [current product specifications](../product-specs/README.md); preserve existing
shared-engine contracts and do not silently retire tests of maintained surfaces.
Mobile interaction evidence includes the required physical-iPhone gates.
