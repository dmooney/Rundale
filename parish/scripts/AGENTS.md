# parish/scripts — agent scope

Shell and Python dev scripts used by CI, agents, and local development. Most enforce or support the proof-evidence gate (rule #10 in root [`AGENTS.md`](../../AGENTS.md)). Scripts are invoked from the repo root via `bash parish/scripts/<name>` or through `just` recipes.

## Scoped commands

```sh
bash parish/scripts/agent-check.sh --source=local          # validate .proofs/ on disk
bash parish/scripts/check-repository-artifacts.sh          # validate tracked artifacts
bash parish/scripts/parish-mcp-backend.sh start            # boot backend for mcp__parish__* tools
just attach-proof <task-id>                                # post proof bundle as PR comment
```

## Local gotchas

- **`agent-check.sh` is fully self-contained.** No Rust/Node/just needed — only POSIX shell and (for `--source=pr`) `gh`.
- **The proof pipeline is fragile.** `agent-check.sh` + `attach-proof.sh` + `render-proof-comment.sh` + `compose-proof-body.sh` enforce the acceptance-criteria-first workflow (rule #13). Changes here risk breaking CI proof validation.
- **`parish-mcp-backend.sh` is the standard backend boot.** Spawns `parish-server --port 3030`; pid in `parish/.parish-mcp-backend.pid`, log in `parish/.parish-mcp-backend.log`. `PARISH_MCP_BACKEND_PORT` overrides 3030.
- **`gh` required** by `agent-check.sh` (PR mode), `attach-proof.sh`, and `render-proof-comment.sh`. Degrades gracefully in minimal sandboxes.
- **Shell scripts use `set -euo pipefail`.** Python scripts use `#!/usr/bin/env python3` and are invoked directly.
- **Scripts are standalone.** Do not extract shared shell libraries — duplicate small helpers inline.

## Script index

### `agent-check.sh` — PR proof gate

- Two modes: `--source=local` (validate `.proofs/` on disk) and `--source=pr <number>` (validate PR body/comments via `gh`, used by CI).
- Diffs the working tree, categorises changed files as proof-relevant / runtime-shipping, validates artifacts and required headers, rejects placeholder debt markers.

### `attach-proof.sh` — Post proof bundle as PR comment

- Reads `.proofs/<task-id>/`, formats a structured comment, posts via `gh pr comment`. Used by `just attach-proof <task-id>`. Depends on `render-proof-comment.sh`.

### `compose-proof-body.sh` — Compose proof bundle into PR body

- Used by `gh pr create --body-file <(... | bash parish/scripts/compose-proof-body.sh <task-id>)` to inline the bundle on PR creation.

### `render-proof-comment.sh` — Render proof artifacts into comment format

- Reads evidence, judge verdict, and acceptance-criteria from a task bundle; produces the structured comment body for `gh`.

### `parish-mcp-backend.sh` — Start/stop/status/log helper

- Subcommands: `start`, `stop`, `status`, `logs`. Spawns `parish-server --port 3030` in background.

### `parish-mcp-launch.sh` — Alternative MCP backend launcher

- Variant launch helper; used when cold-start sequencing differs from the standard `parish-mcp-backend.sh` flow.

### `parish-mcp-cold-shim.py` — Python cold-start shim for MCP backend

- Python script bridging cold-start scenarios for the MCP backend.

### `parish-mcp-audit.sh` — Audit a backend command session

- Historical filename: calls parish-server HTTP routes directly, preserving one
  cookie-backed session across commands. It validates engine-state continuity;
  it does not exercise the stdio MCP server or player-visible UI.

### `harness-shadow.sh` — Shadow-mode harness runner

- Runs the harness in shadow mode (real-loop vs legacy router comparison, #1159). Divergences are reported but do not fail the run; compilation and test failures propagate nonzero. See `src/shadow.rs` in `parish-engine`.

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

- Starts a local OpenAI-compatible proxy, points Parish at it with `PARISH_PROVIDER=custom`, runs `just demo`, and records every request/response pair.

### `project_stats.py` — Project statistics dashboard

- Produces LOC by language, commit frequency, crate dependency graph, and test coverage summaries.

### `local-eval/` — Local evaluation tooling

- `eval_lib.py` (shared eval library), `flaw_scan.py` (flaw scanning), `gen_dlg.py` (dialogue generation), `gen_samples.py` (sample generation), `serve_local.sh` (local model server), and a `README.md`.
