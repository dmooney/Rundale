# limerick/scripts — agent scope

Shell and Python dev scripts used by CI, agents, and local development. Several enforce or support the PR evidence gate ([agent-check](../../docs/agent/agent-check.md)). Scripts are invoked from the repo root via `bash limerick/scripts/<name>` or through `just` recipes.

## Scoped commands

```sh
bash limerick/scripts/agent-check.sh --source=local          # lints; reports if an evidence link is needed
bash limerick/scripts/check-repository-artifacts.sh          # validate tracked artifacts
bash limerick/scripts/check-docs-format.sh                   # Prettier + markdownlint on tracked files (just docs-check)
bash limerick/scripts/limerick-mcp-backend.sh start            # boot backend for mcp__limerick__* tools
bash limerick/scripts/publish-pr-page.sh <pr> <dir>         # publish the PR's evidence page
```

## Local gotchas

- **`agent-check.sh` is fully self-contained.** No Rust/Node/just needed — only POSIX shell and (for `--source=pr`) `gh`.
- **`agent-check.sh` gates every PR in CI.** A classification change can block or wave through runtime PRs; add a case to `tests/agent-check-evidence-link.test.sh` with it.
- **`limerick-mcp-backend.sh` is the standard backend boot.** Spawns `limerick-server --port 3030`; pid in `limerick/.limerick-mcp-backend.pid`, log in `limerick/.limerick-mcp-backend.log`. `LIMERICK_MCP_BACKEND_PORT` overrides 3030.
- **`gh` required** by `agent-check.sh` in PR mode only; local mode needs no network.
- **Shell scripts use `set -euo pipefail`.** Python scripts use `#!/usr/bin/env python3` and are invoked directly.
- **Scripts are standalone.** Do not extract shared shell libraries — duplicate small helpers inline.

## Script index

### `agent-check.sh` — PR evidence gate

- Two modes: `--source=local` (lints, and reports whether the diff is runtime-shipping) and `--source=pr <number>` (also requires `https://dmooney.github.io/rundale-pages/pr/<number>/` in the PR body via `gh`, used by CI).
- Runtime-shipping: `mobile/**` (except `mobile/scripts/**`), `mods/**`, and the engine runtime crates; Markdown, `graphify-out/`, and Rust/UI test code never are. Rejects `.proofs/` paths and placeholder debt markers in every mode.

### `check-docs-format.sh` — Docs/data formatting gate

- Prettier and markdownlint over `git ls-files` only, so git-ignored copies (worktrees under `.claude/worktrees/`) never fail it; markdownlint runs with `--no-globs` so the config's `globs` cannot re-add every file on disk.
- Fails, not skips, without root `node_modules` (`npm ci`). `just docs-check`, `just check`, `just verify`, and `.githooks/pre-push` run it; `tests/check-docs-format.test.sh` covers it (fully only with root `node_modules`, so CI runs it in the docs-format job).

### `publish-pr-page.sh` — Publish a PR's evidence page

- Copies a directory with `index.html` (plus `.mp4`/`.gif`) to `pr/<number>/` in the public `rundale-pages` repository and pushes.

### `proof/` — Differential proof tools

- `prove_diff.py` (`just prove-diff`) builds the merge-base with `origin/main` in a detached worktree and this working tree, copies each side's binaries, runs a live scenario and every `--script` fixture on both, and fails on differences not listed in an intended-differences TOML.
- `scripted_openai.py` is the scripted model server (real HTTP, canned replies per workload, request-body log); `drive_session.py` drives `limerick-server` or an attached bridge over `POST /api/submit-input`; `body_diff.py` and `script_compare.py` are the comparators; `noise.py` masks the live server's wall-clock seconds before `/pause`, the one run-to-run difference left by design.
- CI runs it on runtime pull requests (`Differential proof` in `ci.yml`), reading `toml intended-diffs` blocks the author writes in the PR body (`--intended-markdown`).
- Documented in [`docs/agent/agent-check.md`](../../docs/agent/agent-check.md#differential-proof).

### `limerick-mcp-backend.sh` — Start/stop/status/log helper

- Subcommands: `start`, `stop`, `status`, `logs`. Spawns `limerick-server --port 3030` in background.

### `limerick-mcp-launch.sh` — Alternative MCP backend launcher

- Variant launch helper; used when cold-start sequencing differs from the standard `limerick-mcp-backend.sh` flow.

### `limerick-mcp-cold-shim.py` — Python cold-start shim for MCP backend

- Python script bridging cold-start scenarios for the MCP backend.

### `limerick-mcp-audit.sh` — Audit a backend command session

- Historical filename: calls limerick-server HTTP routes directly, preserving one
  cookie-backed session across commands. It validates engine-state continuity;
  it does not exercise the stdio MCP server or player-visible UI.

### `harness-shadow.sh` — Shadow-mode harness runner

- Runs the harness in shadow mode (real-loop vs legacy router comparison, #1159). Divergences are reported but do not fail the run; compilation and test failures propagate nonzero. See `src/shadow.rs` in `limerick-engine`.

### `harness-shadow-summarize.py` — Summarise shadow-mode diff output

- Post-processes `harness-shadow.sh` output into a human-readable summary.

### `normalize-mod-source.sh` — Normalize mod source metadata

- Tidies `mod_source` fields in world and NPC files for consistency.

### `release.sh` — Release workflow

- Orchestrates the release process. Invoked by CI or manually.

### `reset-onboarding.sh` — Reset first-run onboarding state

- Clears keychain entries and config sections that track first-run setup. Useful for testing the BYOK flow end-to-end.

### `check-doc-paths.sh` — Validate documentation cross-reference paths

- Scans `docs/` for broken relative links. Run after restructuring documentation.

### `check-repository-artifacts.sh` — Validate tracked artifacts

- Rejects tracked generated-output paths, retired binaries, stale large-file
  exceptions, and unreferenced documentation screenshots.
- Enforces the 8 MiB tracked-file ceiling using exact size/hash/owner/purpose
  exceptions in `repository-artifact-exceptions.txt`.

### `harness-audit.sh` — Audit the game harness

- Validates harness configuration and checks for drift between harness tests and actual game state.

### `profile-demo-requests.py` — Profile inference request volume during `just demo`

- Starts a local OpenAI-compatible proxy, points Limerick at it with `LIMERICK_PROVIDER=custom`, runs `just demo`, and records every request/response pair.

### `project_stats.py` — Project statistics dashboard

- Produces LOC by language, commit frequency, crate dependency graph, and test coverage summaries.

### `local-eval/` — Local evaluation tooling

- `eval_lib.py` (shared eval library), `flaw_scan.py` (flaw scanning), `gen_dlg.py` (dialogue generation), `gen_samples.py` (sample generation), `serve_local.sh` (local model server), and a `README.md`.
