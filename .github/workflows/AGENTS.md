# .github/workflows — agent scope

CI/CD pipeline definitions: fast PR/push gates, preserved full-suite Rust/UI/harness gates, inference evals, security scanning, releases, and housekeeping. `ci.yml` enforces the proof-evidence gate (root AGENTS.md rule #10). See [`docs/agent/act-local.md`](../../docs/agent/act-local.md) for running workflows locally with `act`.

## Scoped commands

```sh
just check          # fmt + clippy + tests (mirrors rust-quality-gate)
just agent-check    # proof-evidence gate (mirrors agent-check job)
just verify         # check + harness walkthrough

# act-local — run CI workflows in Docker (see docs/agent/act-local.md)
just act-list       # enumerate all jobs (no Docker execution)
just act-ci         # sub-minute ci.yml fast lane
just act-full-ci    # preserved full-suite workflow
just act-audit      # audit.yml cargo-audit job — fastest smoke test
just act-fmt        # full-ci.yml rust-quality-gate (fmt + clippy + tests)
just act-harness    # full-ci.yml game-harness fixture sweep
just act-ui         # full-ci.yml ui-quality (svelte-check + vitest + build)
just act-e2e        # full-ci.yml ui-e2e (Playwright)
just act-pr         # simulate the pull_request fast lane
```

## Local gotchas

- **`ci.yml` is the fast lane for non-runtime changes.** Pull requests whose path detector reports `changes.runtime == true` call the reusable `full-ci.yml` suite, and the single required `CI gate` fails closed unless it succeeds. Its docs-consistency job also enforces the tracked-artifact size/path/orphan policy. Main/develop pushes, merge-group events, the nightly schedule, and manual dispatch remain independent full-suite backstops.
- **A shipped default-surface replacement owns the complete E2E contract.** Migrate or explicitly retire every prior Playwright assertion in the same pull request; a focused smoke spec is not a substitute for a green complete suite.
- **Agent-check runs on PRs only (non-dependabot).** Push events to `main`/`develop` skip the gate — it already ran on the PR. Dependabot bumps are exempt (root AGENTS.md rule #10).
- **Key PR-author exemptions to immutable authorship.** Use `github.event.pull_request.user.login`, never `github.actor`: the event actor changes when a coordinator refreshes an existing automation-authored branch, while the pull-request author does not.
- **CI-only edits skip the proof gate (root rule #10).** `.github/**` changes with no source diff do not require a proof bundle.
- **Linux native deps are inlined in every Rust job** (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`). Update every workflow that contains the apt install block when the dep list changes.
- **Rust toolchain is pinned to 1.95.0** in `full-ci.yml` and `release.yml`. Bump in a dedicated PR alongside any lint fixes.
- **No YAML anchors** — setup steps (checkout, toolchain, cache, native deps) are inlined per job.
- **`concurrency: cancel-in-progress: true`** on most workflows; `release.yml` sets `cancel-in-progress: false` (releases must not be cancelled).
- **Secrets:** `GITHUB_TOKEN` (all), `OPENROUTER_API_KEY` (inference eval, via `secrets: inherit`). The disabled Gemini sources retain references to `GEMINI_API_KEY`/`GOOGLE_API_KEY`/`APP_PRIVATE_KEY` for a future re-enable. Add new secrets to repo-level GitHub secrets and the consuming job's `env:` block.
- **`concurrency: pages`** in `publish-bench-site.yml` — do not rename without checking the `deploy-pages` action's concurrency expectations.
- **`act` does not reproduce GitHub-side concerns** — concurrency groups, branch protections, required-check status, and `permissions:` are server-side only. See `docs/agent/act-local.md` for caveats.

## Workflow index

### `ci.yml` — Fast CI pipeline

- **Triggers:** `pull_request`, `push` to `main`/`develop`, `workflow_dispatch`.
- **Jobs:** changes, agent-check, docs-consistency (links + repository artifacts), format-quality, python-quality, shell-quality, toml-quality, Windows launcher lifecycle, conditional reusable `runtime-suite`, and the aggregate `ci-gate`.
- **Runtime contract:** `runtime-suite` calls `full-ci.yml` only for pull requests with `changes.runtime == true`. `ci-gate.sh` requires `success` when the suite is expected and `skipped` when it is not, so a failure, cancellation, or unexpected skip cannot produce a green required check.
- **agent-check** runs `bash parish/scripts/agent-check.sh --source=pr "$PR_NUMBER"`. Skipped for dependabot.
- **Concurrency:** `ci-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.

### `full-ci.yml` — Preserved full-suite pipeline

- **Triggers:** reusable `workflow_call`, `push` to `main`/`develop`, `merge_group`, nightly `schedule`, `workflow_dispatch`.
- **Jobs:** rust-quality-gate (fmt+clippy+tests), rust-coverage-ratchet (cargo-llvm-cov floor 60.8%), rust-multi-channel (stable+beta), game-harness (fixture sweep + parish-client smoke), ui-quality (svelte-check+lint+format+build+vitest), ui-e2e (Playwright), and `Full CI gate`.
- **Concurrency:** `full-ci-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.

### `gemini-dispatch.yml.disabled` + `gemini-review.yml.disabled` — paused Gemini review

- **Status:** disabled on 2026-08-09 after the provider rejected reviews because prepaid credits were depleted. The non-YAML extension keeps both workflows out of GitHub Actions entirely, so PRs receive neither a Gemini check nor failure comments.
- The former dispatcher handled PR opens and authorized `@gemini-cli /review` requests; the reusable workflow ran `google-github-actions/run-gemini-cli` with the GitHub MCP integration.
- To re-enable it, restore both `.yml` filenames together, confirm provider billing, and run `actionlint` on both files before merging.

### `audit.yml` — Security audit (cargo-audit)

- **Triggers:** `schedule` (daily 06:17 UTC), dependency-changing `pull_request`, `push` to `main` on Cargo manifest/lock changes, and `workflow_dispatch`.
- Installs `cargo-audit` via `cargo install --locked`; caches binary with `Swatinem/rust-cache` (no target dir).
- While the temporary xcb/wayland-scanner security pins remain, verifies they are unreachable on supported macOS, Linux, and Windows target triples.
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

- **Triggers:** `schedule` (nightly 02:00 UTC), `workflow_dispatch` with scenario selection. The player shares one cookie jar across all Parish HTTP requests so the run stays in one server session.
- Builds `parish-server`, spawns it with `PARISH_PROVIDER=github_models` and `PLAYER_MODEL=microsoft/Phi-4`, runs a Python player agent across scenarios (smoke=10t, intent=25t, reactions=15t, tier2=12t, dialogue=20t, full_session=50t). Judges with gpt-4o via `actions/ai-inference@v1`. Aggregates into a CI summary table.
- **Concurrency:** `eval-inference-${{ github.ref }}`, cancel-in-progress.

### `publish-bench-site.yml` — Publish the v2 (promptfoo) bench site

- **Triggers:** `push` to `main` when `promptfoo/leaderboard/**`, `promptfoo/bench-site/**`, `promptfoo/catalog/**`, `promptfoo/v2/MANIFEST.json`, `promptfoo/config/judge.yaml`, or the workflow itself changes; `workflow_dispatch`.
- The Astro site reads `promptfoo/leaderboard/leaderboard.jsonl` directly at build time (no Python data step). Installs `promptfoo/bench-site` with pnpm (`--frozen-lockfile`), runs `pnpm check` before `pnpm build`, then deploys `dist/` to GitHub Pages via `actions/deploy-pages@v4`. Uses `pnpm/action-setup@v6`. (Retired v1 site lived in `rundale-bench/bench-site`.)
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

### `triage-audit.yml` — Portfolio and issue triage audit

- **Triggers:** `schedule` (weekly Monday 09:00 UTC), issue creation, lifecycle, and label changes, `workflow_dispatch`, and ordinary pull-request lifecycle/label changes on `main`.
- Checks open issues against `triage-labels.json` for missing P0-P3 severity or theme labels, audits active-item readiness and authoritative closing PR linkage, requires unblock triggers, compares explicitly mapped epic state to roadmap rows, and reports the 3/3 implementation/review buffers. Reports via CI step summary and warning annotations while the reset backlog is reconciled.
- **Permissions:** `contents: read`, `issues: read`, `pull-requests: read`.
