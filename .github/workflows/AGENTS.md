# .github/workflows — agent scope

CI/CD pipeline definitions: fast PR/push gates, preserved full-suite Rust/UI/harness gates, inference evals, security scanning, releases, and housekeeping. `ci.yml` enforces the evidence-link gate ([agent-check](../../docs/agent/agent-check.md)). See [`docs/agent/act-local.md`](../../docs/agent/act-local.md) for running workflows locally with `act`.

## Scoped commands

```sh
just check          # docs format + markdownlint, OSV scan of changed lockfiles, complete Python quality job, then fmt + clippy + tests
just agent-check    # evidence-link gate, local mode (agent-check job reads the PR body)
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

- **`ci.yml` is the fast lane for non-runtime changes.** Pull requests whose path detector reports `changes.runtime == true` call the reusable `full-ci.yml` suite, and the single required `CI gate` fails closed unless it succeeds. A separate `nondoc` filter (`predicate-quantifier: every`, excluding `**/*.md` and `docs/**`) forces `runtime`, `differential`, and `promptfoo` to false for documentation-only PRs, including Markdown inside crates, mods, and scripts. Its docs-consistency job also enforces the tracked-artifact size/path/orphan policy. Main/develop pushes, merge-group events, the nightly schedule, and manual dispatch remain independent full-suite backstops.
- **`paths-filter` exclusions need `some-with-excludes`.** Its default `some` quantifier ORs every pattern, so a final `!**/AGENTS.md` still matches paths caught by an earlier broad positive such as `mobile/**`. Use `predicate-quantifier: some-with-excludes` when a filter mixes positive patterns and exclusions; see the [action's predicate quantifier docs](https://github.com/dorny/paths-filter#predicate-quantifier).
- **A shipped default-surface replacement owns the complete E2E contract.** Migrate or explicitly retire every prior Playwright assertion in the same pull request; a focused smoke spec is not a substitute for a green complete suite.
- **Agent-check runs on PRs only (non-dependabot).** Push events to `main`/`develop` skip the gate — it already ran on the PR. Dependabot bumps are exempt ([truthful test automation](../../docs/agent/test-tooling-rules.md#truthful-test-automation)).
- **Key PR-author exemptions to immutable authorship.** Use `github.event.pull_request.user.login`, never `github.actor`: the event actor changes when a coordinator refreshes an existing automation-authored branch, while the pull-request author does not.
- **CI-only edits need no evidence page ([agent-check](../../docs/agent/agent-check.md)).** Only runtime-shipping paths (engine runtime crates, `mods/**`, `mobile/**`) require the `rundale-pages/pr/<number>/` link in the PR body.
- **The agent-check job reads the body when it runs.** `pull_request` runs on `opened`/`synchronize`/`reopened`, not `edited`; after adding the link to an existing PR, re-run the job or push a commit.
- **Linux native deps are inlined in every Rust job** (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`). Update every workflow that contains the apt install block when the dep list changes.
- **Rust cache workspace paths are repository-root relative.** Every `Swatinem/rust-cache` step that builds the nested Cargo workspace must set `workspaces: limerick -> target`; a job or workflow `working-directory` applies only to `run`, not `uses`.
- **Rust toolchain is pinned by root `rust-toolchain.toml`.** Keep that file in
  `ci.yml`'s runtime path filter, and bump it in a dedicated PR alongside any
  compiler or lint fixes.
- **No YAML anchors** — setup steps (checkout, toolchain, cache, native deps) are inlined per job.
- **`concurrency: cancel-in-progress: true`** on most workflows; `release.yml` sets `cancel-in-progress: false` (releases must not be cancelled).
- **Secrets:** `GITHUB_TOKEN` (all), `OPENROUTER_API_KEY` (inference eval, via `secrets: inherit`). The disabled Gemini sources retain references to `GEMINI_API_KEY`/`GOOGLE_API_KEY`/`APP_PRIVATE_KEY` for a future re-enable. Add new secrets to repo-level GitHub secrets and the consuming job's `env:` block.
- **`concurrency: pages`** in `publish-bench-site.yml` — do not rename without checking the `deploy-pages` action's concurrency expectations.
- **`act` does not reproduce GitHub-side concerns** — concurrency groups, branch protections, required-check status, and `permissions:` are server-side only. See `docs/agent/act-local.md` for caveats.
- **CI jobs default to `working-directory: limerick` (`.github/workflows/ci.yml` `defaults.run`).** Any `run:` step that invokes a repo script must use a path relative to `limerick/` (e.g. `bash scripts/foo.sh`), NOT `bash limerick/scripts/foo.sh` — the latter resolves to `limerick/limerick/scripts/...` and dies with `No such file or directory` (exit 127). `actions/*` step inputs like `upload-artifact` `path:` are the exception: they're relative to the repo root (`GITHUB_WORKSPACE`), so `limerick/target/...` is correct there. Mismatch between the two bit the `harness-shadow` job (#1159).
- **Gemini review CI must set `GEMINI_CLI_TRUST_WORKSPACE=true`** before installing extensions. Gemini CLI 0.46.0 otherwise prompts to trust the workspace during `gemini extensions install`, ignores the action's piped `Y`, and the 7-minute review job times out as cancelled.
- **`ci.yml` is sub-minute only for non-UI pull requests.** Shipped UI changes run the complete Playwright suite and feed its exact result into the required `CI gate`; `full-ci.yml` still owns Rust/coverage/harness and post-merge integration checks.
- **Dependabot automation classifies but never lands PRs.** A fresh gatekeeper owns every merge. The weekly upkeep routine on the owner's Mac ([`scripts/dependabot_upkeep.py`](../../scripts/dependabot_upkeep.py), [dependency updates](../../docs/agent/git-workflow.md#dependency-updates)) keeps each PR current, records mobile-verify passes for app-input bumps, waits for the required checks, and runs the gatekeeper; it never merges itself. `.github/scripts/dependabot-merge-ownership.test.cjs` rejects workflow-level merge mechanisms, and `scripts/test_dependabot_upkeep.py` rejects merge calls in the routine.
- **Actions bumps to workflows that never run on pull requests** (`release.yml`, `testflight.yml`, deploys, schedules, `pull_request_target` workflows, which run the base copy) are checked statically by `workflow-quality`: actionlint, then [`scripts/check_action_refs.py`](../../scripts/check_action_refs.py), which reads each remote action's `action.yml` at its pinned ref and fails on a missing ref, an undeclared or missing required input, or an unsupported runtime. The upkeep routine also dry-runs a changed `release.yml` on the PR branch.
- **A conditional reusable workflow needs an explicit fail-closed result in the required gate.** Letting a generic aggregate accept `skipped` is not enough: the gate must separately require `success` when the path detector says the full runtime suite is required, and require `skipped` otherwise.
- **GitHub Models is retired (2026-07-30).** `eval-inference` and `ai-issue-triage` are `.yml.disabled`; do not re-enable against `models.github.ai`. The 410 “retirement brownout” message is permanent, not transient.
- **OSV-Scanner v2.6 dropped `--skip-git`.** Passing it fails the action with `flag provided but not defined`; use `scan-args: -r ./` only.

## Workflow index

### `ci.yml` — Fast CI pipeline

- **Triggers:** `pull_request`, `push` to `main`/`develop`, `workflow_dispatch`.
- **Jobs:** changes, agent-check, docs-consistency (links + repository artifacts), format-quality, python-quality, shell-quality, swift-quality, toml-quality, workflow-quality (actionlint + remote action inputs), Windows launcher lifecycle, conditional reusable `runtime-suite`, and the aggregate `ci-gate`.
- **Swift contract:** `swift-quality` runs `python3 mobile/scripts/verify.py --fast` (pinned SwiftLint/SwiftFormat, the three Swift packages, coverage floors) on `ubuntu-latest` in the `swift:6.3.3-noble` container when `changes.swift` is true (`mobile/**`). It is skipped otherwise, which `ci-gate.sh` accepts. The Xcode app and simulator suites run in `mobile-verify-recorded` instead ([Swift quality gates](../../docs/agent/swift-quality-gates.md)).
- **App-input contract:** `mobile-verify-recorded` runs on hosted `macos-26` when `changes.mobile_app` is true. It looks for a recorded `mobile-verify` pass for the PR head's input tree (`verify_record.py check --pr`, waiting up to two minutes). Without one it runs `verify.py --smoke --skip-time-sensitive` (the speed-dependent `TIME_SENSITIVE_TESTS` run only in local runs) with Xcode 26.6 and the Rust iOS targets and 2 simulator workers: every Mac gate but the full UI suites and the Swift package suites (CI's Linux `swift-quality` job runs those and the style checks), plus `SMOKE_UI_TESTS`. The full gate took 75 minutes on this runner, most of it UI suites. A smoke pass is not a full pass, so the job records nothing. A PR labelled `mobile-verify-full` before its push runs `verify.py --phase all --skip-time-sensitive` there instead and records the pass (`statuses: write`, `continue-on-error`), to prove a change to the UI suites before merge.
- **Runtime contract:** `runtime-suite` calls `full-ci.yml` only for pull requests with `changes.runtime == true`. `ci-gate.sh` requires `success` when the suite is expected and `skipped` when it is not, so a failure, cancellation, or unexpected skip cannot produce a green required check.
- **agent-check** runs `bash limerick/scripts/agent-check.sh --source=pr "$PR_NUMBER"`, which requires the evidence-page link in the PR body for runtime-shipping diffs. Skipped for dependabot.
- **Concurrency:** `ci-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.

### `full-ci.yml` — Preserved full-suite pipeline

- **Triggers:** reusable `workflow_call`, `push` to `main`/`develop`, `merge_group`, nightly `schedule`, `workflow_dispatch`.
- **Jobs:** rust-quality-gate (fmt+clippy+tests), rust-coverage-ratchet (cargo-llvm-cov floor 60.8%), rust-multi-channel (stable+beta), game-harness (fixture sweep + limerick-client smoke), ui-quality (svelte-check+lint+format+build+vitest), ui-e2e (Playwright), and `Full CI gate`.
- **Concurrency:** `full-ci-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.
- **Linux native packages:** every job that links the Tauri crates installs them with the local `.github/actions/install-tauri-deps` action, not an inline `apt-get install`. It caches the downloaded `.deb` files per runner image, because the hosted runners' Ubuntu mirror has served them slowly enough (about 45 KB/s, 25 minutes for the set) to push the Rust quality gate past its 30-minute limit. Change the package list in the action, not in a job.

### `gemini-dispatch.yml.disabled` + `gemini-review.yml.disabled` — paused Gemini review

- **Status:** disabled on 2026-08-09 after the provider rejected reviews because prepaid credits were depleted. The non-YAML extension keeps both workflows out of GitHub Actions entirely, so PRs receive neither a Gemini check nor failure comments.
- The former dispatcher handled PR opens and authorized `@gemini-cli /review` requests; the reusable workflow ran `google-github-actions/run-gemini-cli` with the GitHub MCP integration.
- To re-enable it, restore both `.yml` filenames together, confirm provider billing, and run `actionlint` on both files before merging.

### `ai-issue-triage.yml.disabled` — AI issue triage (retired)

- **Status:** disabled 2026-09-26 after GitHub Models retirement (2026-07-30). Non-YAML extension keeps it out of Actions.
- Former triggers were `issues` opened/reopened + `workflow_dispatch`. Classification called `actions/ai-inference` against Models and failed permanently with 410 / non-JSON responses.
- Manual priority/theme labels from `.github/triage-labels.json` still apply; `triage-audit.yml` continues to audit coverage.
- To re-enable: restore the `.yml` suffix and rewire classification to another provider.

### `audit.yml` — Security audit (cargo-audit)

- **Triggers:** `schedule` (daily 06:17 UTC), dependency-changing `pull_request`, `push` to `main` on Cargo manifest/lock changes, and `workflow_dispatch`.
- Installs `cargo-audit` via `cargo install --locked`; caches binary with `Swatinem/rust-cache` (no target dir).
- Fails if `Cargo.lock` has git sources, and checks that each supported macOS, Linux, and Windows target resolves from crates.io (`--locked`).
- **Concurrency:** `audit-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.

### `osv-scanner.yml` — OSV vulnerability scanner

- **Triggers:** `pull_request`/`push`/`merge_group` to `main`, `schedule` (daily 22:42 UTC), and `workflow_dispatch`.
- Uses Google's reusable `osv-scanner-reusable.yml`/`osv-scanner-reusable-pr.yml` v2.6.0. Scan args: `-r ./` (`--skip-git` was removed upstream; do not pass it).
- Full push/scheduled/manual scans fail on all unaccepted advisories; PR scans only fail on additions. Keep both gates when updating the scanner.
- **Permissions:** `security-events: write` (uploads SARIF to Security tab).

### `build-vllm-mlx-bundle.yml` — Build vllm-mlx distribution bundle

- **Trigger:** `workflow_dispatch` only (manual, ~80-100 MB compressed).
- Runs on `macos-14` (Apple Silicon); calls `just build-vllm-mlx-bundle`. Produces a `.tar.zst` consumed by the Tauri `.dmg` build. Re-run when vllm-mlx, python-build-standalone, or HfModelDownloader cache layout changes.
- **Retention:** 90 days, compression-level 0 (already zstd-compressed).

### `eval-inference.yml.disabled` — Inference evaluation (retired)

- **Status:** disabled 2026-09-26 after GitHub Models retirement (2026-07-30). Non-YAML extension keeps it out of Actions.
- Former triggers were nightly schedule + `workflow_dispatch`. Player and judge both called Models (`github_models` / `actions/ai-inference`) and failed permanently with 410 / non-JSON responses.
- To re-enable: restore the `.yml` suffix and rewire player + judge to another provider.

### `publish-bench-site.yml` — Publish the v2 (promptfoo) bench site

- **Triggers:** `push` to `main` when `promptfoo/leaderboard/**`, `promptfoo/bench-site/**`, `promptfoo/catalog/**`, `promptfoo/v2/MANIFEST.json`, `promptfoo/config/judge.yaml`, or the workflow itself changes; `workflow_dispatch`.
- The Astro site reads `promptfoo/leaderboard/leaderboard.jsonl` directly at build time (no Python data step). Installs `promptfoo/bench-site` with pnpm (`--frozen-lockfile`), runs `pnpm check` before `pnpm build`, then deploys `dist/` to GitHub Pages via `actions/deploy-pages@v5`. Uses `pnpm/action-setup@v6`. (Retired v1 site lived in `rundale-bench/bench-site`.)
- **Concurrency:** `pages`, cancel-in-progress.

### `deploy-limerick-prod.yml` — Deploy Limerick Endpoints to limerick-prod

- **Triggers:** `push` to `main` changing `endpoints/**`, `mods/rundale/endpoints/**`, `.gcloudignore` or the workflow; `workflow_dispatch` (on `main` only).
- Runs `endpoints/deploy/limerick-prod.sh deploy <sha>` when server code changed, then `definitions replace` and `definitions verify`.
- **Auth:** Workload Identity Federation as `limerick-deployer`; the provider accepts only this workflow's `job_workflow_ref` on `main` in the `limerick-prod` environment. Renaming the file or the environment needs `endpoints/deploy/setup-github-deploy.sh` re-run. Signs in after `pnpm install`.
- **Concurrency:** `deploy-limerick-prod`, cancel-in-progress: **false**.

### `testflight.yml` — Internal TestFlight upload

- **Triggers:** `push` to `main` changing `mobile/**`, `mods/**`, `limerick/crates/**`, `limerick/Cargo.toml`, `limerick/Cargo.lock`, `limerick/config/**`, `rust-toolchain.toml`, `.cargo/**` or the workflow; `AGENTS.md` files are excluded; nightly `schedule` (07:41 UTC); `workflow_dispatch`.
- **`upload` job** (push and `workflow_dispatch`, on `main` only): when the push's input tree differs from `main`'s before the push (`verify_record.py tree`), or on dispatch, runs `python3 mobile/scripts/release.py testflight --verify none` on `macos-26` with Xcode 26.6: Rust build (target dir cached), archive, distribution export check and upload. CI's required gate (a recorded pass or `verify.py --smoke`) passed the PR head's input tree first; the squash merge matches it unless `main` moved under the PR. The build number is App Store Connect's highest upload plus one, fetched with the API key; full history is checked out for the commit-count fallback.
- **`gate` job** (nightly `schedule` only, no environment, its own `testflight-nightly-gate` concurrency group): when `check --latest` finds no recorded full pass for `main`'s input tree it runs `verify.py --phase all --skip-time-sensitive`, records the pass with `verify_record.py record` (`statuses: write`), and on failure opens or comments on a "Nightly mobile-verify failed" issue (`issues: write`); a passing run closes that issue. It uploads nothing.
- **Auth:** an App Store Connect API key (`ASC_API_KEY_P8`, `ASC_KEY_ID`, `ASC_ISSUER_ID`) and the private Firebase plist (`FIREBASE_GOOGLE_SERVICE_INFO_PLIST_B64`), secrets of the `testflight` environment ([runbook](../../mobile/testflight.md#uploading-from-github)).
- **Concurrency:** `upload` uses `testflight`, `gate` uses `testflight-nightly-gate`; cancel-in-progress: **false**.

### `release.yml` — Tag-driven release pipeline

- **Triggers:** `push` tags matching `v[0-9]+.[0-9]+.[0-9]+*`; `workflow_dispatch` with `dry_run: true` (default).
- Validates tag matches `limerick-engine/Cargo.toml` version. Builds Linux x86_64 release binary, packages tarball with `LICENSE`, `NOTICE`, `README.md`, creates GitHub Release via `softprops/action-gh-release`.
- **Permissions:** `contents: write`.
- **Concurrency:** `release-${{ github.ref }}`, cancel-in-progress: **false**.

### `dependabot-review.yml` — Dependabot classification

- **Trigger:** `pull_request_target` (opened, reopened, synchronize), Dependabot-authored PRs only.
- Reads `dependabot/fetch-metadata` and comments on major bumps so the gatekeeper reads the release notes. It never enables or performs a merge; the weekly upkeep routine hands each green PR to a gatekeeper ([dependency updates](../../docs/agent/git-workflow.md#dependency-updates)).
- **Permissions:** `contents: read`, `pull-requests: write`.

### `stale.yml` — Stale issue/PR management

- **Trigger:** `schedule` (daily 04:36 UTC).
- Marks issues/PRs stale after 90 days, closes after 14 more. Labels: `no-issue-activity`, `no-pr-activity`.
- **Permissions:** `issues: write`, `pull-requests: write`.

### `triage-audit.yml` — Portfolio and issue triage audit

- **Triggers:** `schedule` (weekly Monday 09:00 UTC), issue creation, lifecycle, and label changes, `workflow_dispatch`, and ordinary pull-request lifecycle/label changes on `main`.
- Checks open issues against `triage-labels.json` for missing P0-P3 severity or theme labels, audits active-item readiness and authoritative closing PR linkage, requires unblock triggers, compares explicitly mapped epic state to roadmap rows, and reports the 3/3 implementation/review buffers. Reports via CI step summary and warning annotations while the reset backlog is reconciled.
- **Permissions:** `contents: read`, `issues: read`, `pull-requests: read`.
