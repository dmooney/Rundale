# Running CI locally with `act`

[`nektos/act`](https://github.com/nektos/act) runs our GitHub Actions
workflows inside Docker against the working tree, so the same jobs that gate
PRs can be exercised locally — no pushed branch, no billed minutes. Every
job in `ci.yml` and `audit.yml` is runnable this way, including Playwright
e2e.

This doc is the source of truth for the setup; `.actrc` and the `act-*`
recipes in `justfile` point back here.

## One-time setup

Everything that isn't checked in needs to happen once on your Mac.

```sh
# 1. Make sure Docker Desktop is running.
#    act talks to the local Docker daemon; no daemon, no runs.

# 2. Install act.
brew install act

# 3. Pull the full runner image (~60GB). This is the only image that
#    mirrors GitHub's ubuntu-latest closely enough for our workflows —
#    the medium image is missing apt, Node, and most of the toolchain
#    that setup-node and dtolnay/rust-toolchain expect to find.
docker pull catthehacker/ubuntu:full-latest

# 4. (Optional) Seed a secrets file for future workflows that need one.
#    Current workflows don't, so you can skip this until you add one.
cp .secrets.example .secrets
```

Verify:

```sh
act --version
just act-list
```

`just act-list` should print every job in every workflow without touching
Docker. If that works, act is wired up correctly.

## Day-to-day usage

All of these are defined in `justfile`:

| Command | What it runs |
|---|---|
| `just act-list` | Enumerate all jobs (fast, no Docker execution) |
| `just act-audit` | `audit.yml` cargo-audit job — fastest smoke test |
| `just act-fmt` | `ci.yml` rust-quality-gate (fmt + clippy + tests) |
| `just act-harness` | `ci.yml` game-harness fixture sweep |
| `just act-ui` | `ci.yml` ui-quality (svelte-check + vitest + build) |
| `just act-e2e` | `ci.yml` ui-e2e (Playwright) |
| `just act-ci` | All of `ci.yml` — matches what PRs see |
| `just act-job JOB=<id>` | Run a specific job by id from `act-list` |
| `just act-pr` | Simulate the `pull_request` event |
| `just act-refresh` | Re-fetch third-party actions after a version bump |
| `just act-clean` | Tear down cached containers + artifact output |

## Configuration

`.actrc` sets shared flags for every `act` invocation. The key ones:

- **`-P ubuntu-latest=catthehacker/ubuntu:full-latest`** — full image
  (~60GB). Needed because our workflows `apt-get install` Tauri/WebKit
  dev headers, and the medium image doesn't ship apt.
- **`--container-architecture linux/amd64`** — forces amd64 emulation on
  Apple Silicon. Some actions (notably `dtolnay/rust-toolchain` and
  `actions/setup-node`) won't run under Rosetta without it set explicitly.
- **`--artifact-server-path /tmp/act-artifacts`** — `upload-artifact@v4`
  requires a real endpoint (unlike v3, it won't no-op). The ui-e2e job
  uploads its `playwright-report/` here after runs.
- **`--reuse`** — keeps containers between runs so `target/` and
  `node_modules/` survive. First run is slow; subsequent runs are fast.
- **`--action-offline-mode`** — skips re-fetching cached action source.
  Use `just act-refresh` after bumping an action version.

Edit `.actrc` if you need to deviate; per-command overrides also work
(e.g. `act -P ubuntu-latest=catthehacker/ubuntu:medium-latest ...`).

## Caveats and gotchas

**First run is slow.** The full image pull is ~60GB compressed. Rust
builds inside the container start cold — `Swatinem/rust-cache` caches
to `~/.cache` inside the container, and `--reuse` keeps that around,
but the very first `cargo build` in the `ui-e2e` job will take several
minutes. Budget 30–60 minutes for the first full `just act-ci`.

**Apple Silicon emulation tax.** amd64 under Rosetta runs roughly 2–3×
slower than the same job on GitHub's x86 runners. Expect `just act-ci`
to take meaningfully longer end-to-end than a pushed PR.

**Disk usage grows.** Every `--reuse` container keeps its own `target/`
and `node_modules/`. After a couple weeks of daily use you may see
20–40GB of cached data in Docker. `just act-clean` drops it all.

**Scheduled jobs.** `audit.yml` runs on a cron in prod. act can
simulate the `schedule` event with `act schedule`, but `just act-audit`
runs it directly by job id, which is what you want day to day.

**`GITHUB_TOKEN` is a stub.** act injects a dummy token automatically.
If you ever add a workflow that genuinely needs to call the GitHub API
(post PR comments, push to ghcr.io), put a real fine-scoped PAT in
`.secrets` — never commit it.

**Don't use act as your only gate.** act faithfully reproduces a lot,
but not everything: GitHub-side concurrency groups, branch protections,
required-check status, and `permissions:` enforcement are server-side
concerns that only get exercised on a real push. act is for tightening
the inner loop, not replacing the PR check.

## Upgrading the runner image

When GitHub's `ubuntu-latest` upgrades (currently 24.04), pull the new
`catthehacker/ubuntu:full-latest` — it tracks upstream:

```sh
docker pull catthehacker/ubuntu:full-latest
just act-clean   # drop cached containers built on the old image
```

No `.actrc` change needed; the tag is already `:full-latest`.
