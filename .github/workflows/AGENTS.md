# .github/workflows — agent scope

CI/CD pipeline definitions: Rust quality gates, UI builds, e2e tests, inference evals, security scanning, releases, and housekeeping. `ci.yml` enforces the proof-evidence gate (root AGENTS.md rule #10). See [`docs/agent/act-local.md`](../../docs/agent/act-local.md) for running workflows locally with `act`.

## Scoped commands

```sh
just check          # fmt + clippy + tests (mirrors rust-quality-gate)
just agent-check    # proof-evidence gate (mirrors agent-check job)
just verify         # check + harness walkthrough

# act-local — run CI workflows in Docker (see docs/agent/act-local.md)
just act-list       # enumerate all jobs (no Docker execution)
just act-ci         # default-event ci.yml run
just act-audit      # audit.yml cargo-audit job — fastest smoke test
just act-fmt        # ci.yml rust-quality-gate (fmt + clippy + tests)
just act-harness    # ci.yml game-harness fixture sweep
just act-ui         # ci.yml ui-quality (svelte-check + vitest + build)
just act-e2e        # ci.yml ui-e2e (Playwright)
just act-pr         # simulate the pull_request fast lane
```

## Local gotchas

- **Pull-request CI is the fast lane.** `ci.yml` keeps PR runs under a minute by running proof/docs/script/data checks there and deferring expensive Rust, coverage, harness, and UI runtime jobs to `merge_group`, `push`, `schedule`, and `workflow_dispatch`.
- **Agent-check runs on PRs only (non-dependabot).** Push events to `main`/`develop` skip the gate — it already ran on the PR. Dependabot bumps are exempt (root AGENTS.md rule #10).
- **CI-only edits skip the proof gate (root rule #10).** `.github/**` changes with no source diff do not require a proof bundle.
- **Linux native deps are inlined in every Rust job** (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`). Update every workflow that contains the apt install block when the dep list changes.
- **Rust toolchain is pinned to 1.95.0** in `ci.yml` and `release.yml`. Bump in a dedicated PR alongside any lint fixes.
- **No YAML anchors** — setup steps (checkout, toolchain, cache, native deps) are inlined per job.
- **`concurrency: cancel-in-progress: true`** on most workflows; `release.yml` sets `cancel-in-progress: false` (releases must not be cancelled).
- **Secrets:** `GITHUB_TOKEN` (all), `GEMINI_API_KEY`/`GOOGLE_API_KEY`/`APP_PRIVATE_KEY` (Gemini review), `OPENROUTER_API_KEY` (inference eval, via `secrets: inherit`). Add new secrets to repo-level GitHub secrets and the consuming job's `env:` block.
- **`concurrency: pages`** in `publish-bench-site.yml` — do not rename without checking the `deploy-pages` action's concurrency expectations.
- **`act` does not reproduce GitHub-side concerns** — concurrency groups, branch protections, required-check status, and `permissions:` are server-side only. See `docs/agent/act-local.md` for caveats.

## Workflow index

### `ci.yml` — Main CI pipeline

- **Triggers:** `pull_request`, `push` to `main`/`develop`, `merge_group`, nightly `schedule`, `workflow_dispatch`.
- **PR fast lane:** changes, agent-check, docs-consistency, format-quality, python-quality, shell-quality, toml-quality, and the aggregate `ci-gate`.
- **Full-suite events:** rust-quality-gate (fmt+clippy+tests), rust-coverage-ratchet (cargo-llvm-cov floor 60.8%), rust-multi-channel (stable+beta), game-harness (fixture sweep + parish-client smoke), ui-quality (svelte-check+lint+format+build+vitest), ui-e2e (Playwright).
- **agent-check** runs `bash parish/scripts/agent-check.sh --source=pr "$PR_NUMBER"`. Skipped for dependabot.
- **Concurrency:** `ci-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.

### `gemini-dispatch.yml` — Gemini review dispatch

- **Triggers:** PR opened, PR review submitted, PR review comment, issue comment.
- Routes to `gemini-review.yml` via `workflow_call`. Dispatches only for non-fork PRs or `@gemini-cli` mentions from OWNER/MEMBER/COLLABORATOR users. Uses GitHub App identity token.
- **Permissions:** `issues: write`, `pull-requests: write`.

### `gemini-review.yml` — Gemini code review

- **Trigger:** `workflow_call` from `gemini-dispatch.yml`.
- Runs `google-github-actions/run-gemini-cli` with GCP workload identity federation, MCP server for GitHub tools, and code-review extension.
- **Timeout:** 7 minutes.

### `audit.yml` — Security audit (cargo-audit)

- **Triggers:** `schedule` (daily 06:17 UTC), `push` to `main` on `Cargo.lock`/`Cargo.toml` changes, `workflow_dispatch`.
- Installs `cargo-audit` via `cargo install --locked`; caches binary with `Swatinem/rust-cache` (no target dir).
- **Concurrency:** `audit-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.

### `osv-scanner.yml` — OSV vulnerability scanner

- **Triggers:** `pull_request`/`push`/`merge_group` to `main`, `schedule` (weekly 22:42 UTC Saturday).
- Uses Google's reusable `osv-scanner-reusable.yml`/`osv-scanner-reusable-pr.yml` v2.3.5. Scan args: `-r --skip-git ./`.
- **Permissions:** `security-events: write` (uploads SARIF to Security tab).

### `build-vllm-mlx-bundle.yml` — Build vllm-mlx distribution bundle

- **Trigger:** `workflow_dispatch` only (manual, ~80-100 MB compressed).
- Runs on `macos-14` (Apple Silicon); calls `just build-vllm-mlx-bundle`. Produces a `.tar.zst` consumed by the Tauri `.dmg` build. Re-run when vllm-mlx, python-build-standalone, or HfModelDownloader cache layout changes.
- **Retention:** 90 days, compression-level 0 (already zstd-compressed).

### `eval-inference.yml` — Inference evaluation

- **Triggers:** `schedule` (nightly 02:00 UTC), `workflow_dispatch` with scenario selection.
- Builds `parish-server`, spawns it with `PARISH_PROVIDER=github_models` and `PLAYER_MODEL=microsoft/Phi-4`, runs a Python player agent across scenarios (smoke=10t, intent=25t, reactions=15t, tier2=12t, dialogue=20t, full_session=50t). Judges with gpt-4o via `actions/ai-inference@v1`. Aggregates into a CI summary table.
- **Concurrency:** `eval-inference-${{ github.ref }}`, cancel-in-progress.

### `publish-bench-site.yml` — Publish the v2 (promptfoo) bench site

- **Triggers:** `push` to `main` when `promptfoo/leaderboard/**`, `promptfoo/bench-site/**`, `promptfoo/catalog/**`, `promptfoo/v2/MANIFEST.json`, `promptfoo/config/judge.yaml`, or the workflow itself changes; `workflow_dispatch`.
- The Astro site reads `promptfoo/leaderboard/leaderboard.jsonl` directly at build time (no Python data step). Builds `promptfoo/bench-site` with pnpm (`--frozen-lockfile`), deploys `dist/` to GitHub Pages via `actions/deploy-pages@v4`. Uses `pnpm/action-setup@v4`. (Retired v1 site lived in `rundale-bench/bench-site`.)
- **Concurrency:** `pages`, cancel-in-progress.

### `release.yml` — Tag-driven release pipeline

- **Triggers:** `push` tags matching `v[0-9]+.[0-9]+.[0-9]+*`; `workflow_dispatch` with `dry_run: true` (default).
- Validates tag matches `parish-engine/Cargo.toml` version. Builds Linux x86_64 release binary, packages tarball with `LICENSE`, `NOTICE`, `README.md`, creates GitHub Release via `softprops/action-gh-release`.
- **Permissions:** `contents: write`.
- **Concurrency:** `release-${{ github.ref }}`, cancel-in-progress: **false**.

### `stale.yml` — Stale issue/PR management

- **Trigger:** `schedule` (daily 04:36 UTC).
- Marks issues/PRs stale after 90 days, closes after 14 more. Labels: `no-issue-activity`, `no-pr-activity`.
- **Permissions:** `issues: write`, `pull-requests: write`.

### `triage-audit.yml` — Issue triage label audit

- **Triggers:** `schedule` (weekly Monday 09:00 UTC), `issues` opened/reopened, `workflow_dispatch`, `pull_request` to `main` on workflow changes.
- Checks open issues against `triage-labels.json` for missing P0-P3 priority or theme labels; on open/reopened checks the triggering issue. Reports via CI step summary and warning annotations.
- **Permissions:** `contents: read`, `issues: read`, `pull-requests: read`.
