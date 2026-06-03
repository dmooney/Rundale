# Releasing Rundale

Rundale ships from a single source of truth — a `vMAJOR.MINOR.PATCH` git tag.
Pushing the tag triggers `.github/workflows/release.yml`, which builds a
Linux x86_64 binary tarball and publishes it as a GitHub Release with
auto-generated notes.

## Files that get bumped

`parish/scripts/release.sh` rewrites three user-visible version fields:

| File                                         | Field               |
| -------------------------------------------- | ------------------- |
| `parish/crates/parish-cli/Cargo.toml`        | `[package].version` |
| `parish/crates/parish-tauri/tauri.conf.json` | `.version`          |
| `parish/apps/ui/package.json`                | `.version`          |

`Cargo.lock` is refreshed as a side effect. Internal leaf crates keep their
`0.1.0` baseline — they are unpublished workspace members and don't carry an
external contract.

## Standard release (human or Claude)

```sh
# 1. From a clean working tree on the branch you want to release from:
just release-dry-run 0.2.0     # prints diffs only — nothing is written

# 2. If the diffs look right, apply for real:
just release 0.2.0             # bumps the three files, commits, tags v0.2.0

# 3. Land the bump and the tag separately:
git push origin <branch>       # land the chore(release): v0.2.0 commit
git push origin v0.2.0         # triggers .github/workflows/release.yml
```

The workflow runs `validate-tag` first and fails fast if `v0.2.0` and
`parish-cli` Cargo.toml disagree, so a stray `git tag` without `release.sh`
won't slip a half-bumped release into production.

## Dry-running without an actual release showing up

There are two complementary ways to dry-run, depending on whether you want to
verify the **bump** or the **build/publish pipeline**:

1. **Bump dry-run (local, no network):**

   ```sh
   just release-dry-run 0.2.0
   ```

   Prints the three diffs and exits. No files written, no commit, no tag.

2. **Pipeline dry-run (CI builds, no Release published):**
   Trigger `release.yml` manually via the GitHub Actions UI →
   "Release" workflow → "Run workflow" → set `version=0.2.0`,
   leave `dry_run=true` (default).
   - `validate-tag` is skipped on `workflow_dispatch` (it only enforces
     against real tag pushes), so dispatch-mode dry-runs work even if the
     bump hasn't landed.
   - The build job runs end-to-end and uploads
     `parish-v0.2.0-x86_64-linux-gnu.tar.gz` as a workflow artifact.
   - The `publish` job is hard-gated on `github.event_name == 'push'`, so
     **`workflow_dispatch` can never publish a Release**, even with
     `dry_run=false`. No tag is created either way.

   The artifact is downloadable from the workflow run page for ~14 days.

If you want to test the _real_ tag-driven flow without polluting the public
release list, push to a throwaway tag like `v0.0.0-rc.1` (a prerelease semver),
delete it after, and delete the resulting Release manually. The workflow's
tag pattern accepts prereleases (`v0.0.0-rc.1`, etc.).

## How Claude executes a release

Claude has the tools required to run this end-to-end:

- `Bash` to run `just release <version>` and `git push origin v<version>`.
- `mcp__github__list_releases` / `mcp__github__get_release_by_tag` to confirm
  the workflow published the Release.
- No `mcp__github__create_release` tool is needed — the workflow handles
  creation. (Claude doesn't have a direct `workflow_dispatch` tool, so
  pipeline dry-runs are a human action via the Actions UI.)

A typical Claude session:

```sh
# Verify the bump first
just release-dry-run 0.2.0

# Apply, land, and trigger
just release 0.2.0
git push origin main
git push origin v0.2.0
```

Then poll `mcp__github__list_releases` for `dmooney/rundale` until the
release lands, or `mcp__github__get_release_by_tag` with `v0.2.0`.

## Failure modes worth knowing

- **Tag/Cargo.toml drift.** `validate-tag` fails. Fix: `git tag -d v0.2.0`,
  push `:refs/tags/v0.2.0` to delete the remote tag, re-run `just release`.
- **Working tree dirty.** `release.sh` refuses to run unless `--dry-run`.
  Fix: commit or stash first.
- **Tag already exists locally.** `release.sh` refuses to overwrite. Either
  bump to the next version or `git tag -d v0.2.0` first.
- **Cargo.lock update fails for `parish`.** `release.sh` warns but continues;
  inspect and recommit if needed before pushing the tag.
- **`softprops/action-gh-release` upload fails.** The build artifact is still
  attached to the workflow run; download and upload manually via the GitHub
  Releases UI.

## Scope (what this pipeline does NOT do)

- No macOS / Windows builds — Linux x86_64 only for now. Adding a runner
  matrix is straightforward (mirror the patterns in `.github/workflows/ci.yml`)
  but bigger than the initial cut.
- No Tauri desktop bundle (`.deb` / `.AppImage` / `.dmg` / `.msi`). The
  `tauri.conf.json` version is bumped so a future `cargo tauri build` will
  produce correctly-versioned bundles, but no automated bundle build is wired.
- No publish to crates.io or npm — every workspace crate is `publish = false`,
  and the UI package is `private: true`.
- No CHANGELOG.md generation. GitHub's auto-generated release notes (commit
  list + PR titles since the previous tag) cover this for now.
