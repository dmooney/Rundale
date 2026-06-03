# .github/workflows — agent scope

CI/CD pipeline definitions covering Rust quality gates, UI builds, e2e tests,
inference evals, security scanning, releases, and housekeeping. The primary CI
gate is `ci.yml` which enforces the proof-evidence gate (rule #10 in root
[`AGENTS.md`](../../AGENTS.md)). See [`docs/agent/act-local.md`](../../docs/agent/act-local.md)
for running workflows locally with `act`.

## Scoped commands

```sh
just check          # fmt + clippy + tests (mirrors rust-quality-gate)
just agent-check    # proof-evidence gate (mirrors agent-check job)
just verify         # check + harness walkthrough

# act-local — run CI workflows in Docker (see docs/agent/act-local.md)
just act-list       # enumerate all jobs (no Docker execution)
just act-ci         # full ci.yml — matches what PRs see
just act-audit      # audit.yml cargo-audit job — fastest smoke test
just act-fmt        # ci.yml rust-quality-gate (fmt + clippy + tests)
just act-harness    # ci.yml game-harness fixture sweep
just act-ui         # ci.yml ui-quality (svelte-check + vitest + build)
just act-e2e        # ci.yml ui-e2e (Playwright)
just act-pr         # simulate the pull_request event
```

## Local gotchas

- **Agent-check job only runs on PRs from non-dependabot actors.** Push events to
  `main`/`develop` skip the gate — the gate already ran on the PR that produced
  the commit. Dependabot bumps are exempt from proof-evidence (root AGENTS.md
  rule #10).
- **CI-only edits skip the proof gate per root rule #10.** `.github/**` path
  changes that carry no source-code diff do not require proof bundles.
- **Linux native deps pattern is repeated across jobs.** The `libgtk-3-dev`,
  `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev` apt install
  block appears in every Rust job. When the dep list changes, update every
  workflow that has it.
- **Rust toolchain is pinned to 1.95.0** in `ci.yml` and `release.yml`. Bump
  deliberately in a dedicated PR alongside any lint fixes the new version
  surfaces.
- **No YAML anchors in current workflows.** Setup steps (checkout, toolchain,
  cache, native deps) are inlined per job rather than factored into anchors.
- **Convention: `concurrency` groups with `cancel-in-progress: true`** on most
  workflows to avoid wasted runs on stale branches. `release.yml` explicitly
  sets `cancel-in-progress: false` (releases must not be cancelled).
- **Secrets used across workflows:** `GITHUB_TOKEN` (all), `GEMINI_API_KEY` /
  `GOOGLE_API_KEY` / `APP_PRIVATE_KEY` (Gemini review), `OPENROUTER_API_KEY`
  (inference eval — via `secrets: inherit`). Add new secrets to the repo-level
  GitHub secrets and the `env:` block of the consuming job.
- **Workflow concurrency group with `pages`** in `publish-bench-site.yml` —
  do not rename without checking the `deploy-pages` action's concurrency
  expectations.
- **`act` reproduces most jobs but not GitHub-side concerns.** Concurrency
  groups, branch protections, required-check status, and `permissions:`
  enforcement are server-side only. See `docs/agent/act-local.md` for caveats.

## Workflow index

### `ci.yml` — Main CI pipeline

- **Triggers:** `pull_request`, `push` to `main`/`develop`, `workflow_dispatch`.
- **Jobs:** agent-check, rust-quality-gate (fmt+clippy+tests),
  rust-coverage-ratchet (tarpaulin, floor 60.8%), rust-multi-channel
  (stable+beta check), docs-consistency (doc path validation),
  game-harness (fixture sweep), ui-quality (svelte-check+build+vitest),
  ui-e2e (Playwright).
- **Key detail:** The `agent-check` job runs
  `bash parish/scripts/agent-check.sh --source=pr "$PR_NUMBER"`, validates
  proof evidence in PR comments. Skipped for dependabot.
- **Concurrency:** `ci-${{ github.workflow }}-${{ github.ref }}`,
  cancel-in-progress.

### `gemini-dispatch.yml` — Gemini review dispatch

- **Triggers:** PR opened, PR review submitted, PR review comment, issue comment.
- **Key detail:** Routes requests to `gemini-review.yml` via `workflow_call`.
  Only dispatches for non-fork PRs or `@gemini-cli` mentions from
  OWNER/MEMBER/COLLABORATOR users. Uses GitHub App identity token for API
  calls.
- **Permissions:** `issues: write`, `pull-requests: write` on dispatch job.

### `gemini-review.yml` — Gemini code review

- **Trigger:** `workflow_call` from `gemini-dispatch.yml`.
- **Key detail:** Runs `google-github-actions/run-gemini-cli` with Gemini Code
  Assist. Configured with GCP workload identity federation, MCP server for
  GitHub tools, and code-review extension.
- **Timeout:** 7 minutes.

### `audit.yml` — Security audit (cargo-audit)

- **Triggers:** `schedule` (daily 06:17 UTC), `push` to `main` when
  `Cargo.lock`/`Cargo.toml` changes, `workflow_dispatch`.
- **Key detail:** Installs `cargo-audit` via `cargo install --locked`, caches
  the binary across runs with `Swatinem/rust-cache` (no target dir).
- **Concurrency:** `audit-${{ github.workflow }}-${{ github.ref }}`,
  cancel-in-progress.

### `osv-scanner.yml` — OSV vulnerability scanner

- **Triggers:** `pull_request`/`push`/`merge_group` to `main`, `schedule`
  (weekly 22:42 UTC Saturday).
- **Key detail:** Uses Google's reusable OSV-Scanner workflows
  (`osv-scanner-reusable.yml` / `osv-scanner-reusable-pr.yml` v2.3.5).
  Scan args: `-r --skip-git ./`.
- **Permissions:** `security-events: write` (uploads SARIF to Security tab).

### `build-vllm-mlx-bundle.yml` — Build vllm-mlx distribution bundle

- **Trigger:** `workflow_dispatch` only (manual, ~80-100 MB compressed).
- **Key detail:** Runs on `macos-14` (Apple Silicon). Calls
  `just build-vllm-mlx-bundle`. Produces a `.tar.zst` artifact consumed by the
  Tauri `.dmg` build step. Re-run when vllm-mlx, python-build-standalone, or
  HfModelDownloader cache layout changes.
- **Retention:** 90 days, compression-level 0 (already zstd-compressed).

### `eval-inference.yml` — Inference evaluation

- **Triggers:** `schedule` (nightly 02:00 UTC), `workflow_dispatch` with
  scenario selection.
- **Key detail:** Builds `parish-server` on ubuntu-latest, spawns it with
  `PARISH_PROVIDER=github_models` and `PLAYER_MODEL=microsoft/Phi-4`, runs a
  Python player agent across scenarios (smoke=10t, intent=25t, reactions=15t,
  tier2=12t, dialogue=20t, full_session=50t). Judges with gpt-4o via
  `actions/ai-inference@v1`. Aggregates results into a CI summary table.
- **Concurrency:** `eval-inference-${{ github.ref }}`, cancel-in-progress.

### `publish-bench-site.yml` — Publish rundale-bench leaderboard

- **Triggers:** `push` to `main` when `docs/proofs/rundale-bench/**`,
  `rundale-bench/**`, or the workflow itself changes; `workflow_dispatch`.
- **Key detail:** Builds site data via Python, builds with pnpm, deploys to
  GitHub Pages with `actions/deploy-pages@v4`. Uses `pnpm/action-setup@v4`.
- **Concurrency:** `pages`, cancel-in-progress.

### `release.yml` — Tag-driven release pipeline

- **Triggers:** `push` tags matching `v[0-9]+.[0-9]+.[0-9]+*`; `workflow_dispatch`
  with `dry_run: true` (default) to test the build without publishing.
- **Key detail:** Validates tag matches `parish-engine/Cargo.toml` version.
  Builds Linux x86_64 release binary, packages tarball with `LICENSE`,
  `NOTICE`, `README.md`, creates GitHub Release via `softprops/action-gh-release`.
- **Permissions:** `contents: write` (required for release creation).
- **Concurrency:** `release-${{ github.ref }}`, cancel-in-progress: **false**.

### `stale.yml` — Stale issue/PR management

- **Trigger:** `schedule` (daily 04:36 UTC).
- **Key detail:** Marks issues/PRs stale after 90 days, closes after 14 more.
  Labels: `no-issue-activity`, `no-pr-activity`.
- **Permissions:** `issues: write`, `pull-requests: write`.

### `triage-audit.yml` — Issue triage label audit

- **Triggers:** `schedule` (weekly Monday 09:00 UTC), `issues` opened/reopened,
  `workflow_dispatch`, `pull_request` to `main` on workflow changes.
- **Key detail:** Checks every open issue against `triage-labels.json` for
  missing P0-P3 priority or theme labels. On issue open/reopened, checks the
  triggering issue. Reports via CI step summary and warning annotations.
- **Permissions:** `contents: read`, `issues: read`, `pull-requests: read`.
