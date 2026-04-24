# Railway deployment watchdog

The production Parish web service deploys to Railway via [`deploy/Dockerfile`](../../deploy/Dockerfile) (config in [`railway.toml`](../../railway.toml)). When a deployment fails, the service stays broken silently until someone notices. The **watchdog** is an automated check that turns a silent failure into a labeled GitHub issue.

Scope is deliberately small. It **detects and notifies** today; it does not auto-fix. Auto-fix is the v2 goal tracked in [#562](https://github.com/dmooney/Parish/issues/562).

## How it works

- [`.github/workflows/railway-watchdog.yml`](../../.github/workflows/railway-watchdog.yml) runs every 10 minutes (and on push to `main`).
- It shells out to [`deploy/railway-watchdog.sh`](../../deploy/railway-watchdog.sh), which:
  1. Queries `railway service status --json` for the latest deployment.
  2. On `FAILED`/`CRASHED`: searches for an open issue whose title contains the deployment ID. If none exists, opens one labeled `railway-failure,bug,requires-human` with a tail of the build log.
  3. On `SUCCESS`: closes every open issue carrying the `railway-failure` label (the next green deploy self-heals stale notifications).
  4. On transient states (`BUILDING`, `DEPLOYING`, etc.): exits 0.

Idempotence is enforced by the deployment ID in the issue title — repeated runs against the same failed deployment are no-ops.

## Setup

One-time, per repo:

1. **Create a Railway API token** scoped to read the Parish service and project.
   - Railway dashboard → Account Settings → Tokens → *Create Token*.
   - Give it a label like `github-watchdog`.
2. **Store it as a GitHub Actions secret.**
   ```sh
   gh secret set RAILWAY_TOKEN --body "<token>"
   ```
3. The `railway-failure` label already exists. If it's been deleted, recreate it:
   ```sh
   gh label create railway-failure --color B60205 \
     --description "Auto-filed by railway-watchdog workflow — a Railway deployment failed"
   ```

That's the whole install. The workflow's `GITHUB_TOKEN` is provided automatically by Actions.

## Running locally

Useful for tuning the log excerpt or testing label behavior before landing changes:

```sh
export RAILWAY_TOKEN=...   # same token Actions uses
export GH_TOKEN=$(gh auth token)
./deploy/railway-watchdog.sh
```

Override the service or project by setting `RAILWAY_SERVICE`, `RAILWAY_ENVIRONMENT`, or `RAILWAY_PROJECT`.

## Roadmap — toward v2 (Claude-driven auto-fix)

The watchdog is step one of [#562](https://github.com/dmooney/Parish/issues/562). Once notification is reliable, the intended next moves:

- **Diagnose:** extend the script to attach a short Claude-authored summary of the log excerpt (not a fix — just a diagnosis).
- **Fix draft:** on well-understood failure patterns (workspace-manifest errors, missing files referenced by `COPY`, lockfile/toolchain mismatches), spawn a Claude agent to open a fix PR against `main`.
- **Gate:** never auto-merge without the existing codex `+1` gate the team already uses for `codex-automation` PRs.
- **Loop-breaker:** cap auto-retries per 24h; escalate with `requires-human` once the cap is hit.

Each increment is independently useful — ship them one at a time rather than as a big-bang rewrite.
