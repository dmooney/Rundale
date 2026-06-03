# parish/scripts — agent scope

Shell and Python dev scripts used by CI, agents, and local development. Most enforce or support the proof-evidence gate (rule #10 in root [`AGENTS.md`](../../AGENTS.md)). Scripts are invoked from the repo root via `bash parish/scripts/<name>` or through `just` recipes.

## Scoped commands

```sh
bash parish/scripts/agent-check.sh --source=local          # validate .proofs/ on disk
bash parish/scripts/parish-mcp-backend.sh start            # boot backend for mcp__parish__* tools
just attach-proof <task-id>                                # post proof bundle as PR comment
```

## Local gotchas

- **`agent-check.sh` runs before Rust/Node/just are installed.** It is fully self-contained — no dependencies beyond POSIX shell and (for `--source=pr`) `gh`.
- **Proof-gate convention.** The three-pipeline — `agent-check.sh` + `attach-proof.sh` + `render-proof-comment.sh` — enforces the acceptance-criteria-first workflow (rule #13). Any change to these scripts risks breaking CI's proof validation.
- **`parish-mcp-backend.sh` is the standard backend boot.** Referenced in root `AGENTS.md` as the prerequisite before using `mcp__parish__*` tools. Spawns `parish-server --port 3030` in background with pid and log files under `parish/`.
- **`gh` is required** by `agent-check.sh` (PR mode), `attach-proof.sh`, and `render-proof-comment.sh`. Not installed in minimal sandboxes — those scripts degrade gracefully with clear error messages.
- **Convention.** Shell scripts use `set -euo pipefail`; Python scripts use `#!/usr/bin/env python3` and are invoked directly (`python3 parish/scripts/...`).
- **Scripts are not library code.** They are standalone scripts meant to be read end-to-end. Avoid extracting shared shell libraries — duplicate small helpers inline for clarity.

## Script index

### `agent-check.sh` — PR proof gate for agent-assisted changes

- Two modes: `--source=local` (validate `.proofs/` on disk, used by `just agent-check` and Stop hook) and `--source=pr <number>` (validate PR comments via `gh`, used by CI).
- Diffs the working tree against the base ref, categorises changed files into proof-relevant / runtime-shipping, validates proof artifacts exist with required headers, rejects placeholder debt markers and `.proofs/` paths in diff.
- Self-contained (no Rust/Node/just needed).

### `attach-proof.sh` — Post proof bundle as PR comment

- Reads `.proofs/<task-id>/` artifacts, formats them into a structured comment, and posts via `gh pr comment`.
- Used by `just attach-proof <task-id>`. Depends on `render-proof-comment.sh`.

### `render-proof-comment.sh` — Render proof artifacts into PR comment format

- Reads evidence, judge verdict, and acceptance-criteria markdown from a task bundle and produces a structured comment body for `gh`.

### `parish-mcp-backend.sh` — Start/stop/status/log helper for parish-mcp backend

- Subcommands: `start`, `stop`, `status`, `logs`. Spawns `parish-server --port 3030` in background.
- Config: `PARISH_MCP_BACKEND_PORT` (default 3030), pid in `parish/.parish-mcp-backend.pid`, log in `parish/.parish-mcp-backend.log`.

### `release.sh` — Release workflow script

- Orchestrates the release process. Invoked by CI or manually.

### `reset-onboarding.sh` — Reset first-run onboarding state

- Clears keychain entries and config file sections that track first-run setup. Useful for testing the BYOK flow end-to-end.

### `check-doc-paths.sh` — Validate documentation cross-reference paths

- Scans `docs/` for broken relative links and missing cross-references. Run after restructuring documentation.

### `harness-audit.sh` — Audit the game harness

- Validates harness configuration and checks for drift between harness tests and the actual game state.

### `profile-demo-requests.py` — Profile inference request volume during `just demo`

- Python script that starts a local OpenAI-compatible proxy, points Parish at it with `PARISH_PROVIDER=custom`, runs `just demo`, and records every request/response pair for analysis.

### `project_stats.py` — Project statistics dashboard

- Python script that produces a comprehensive health dashboard: LOC by language, commit frequency, crate dependency graph, and test coverage summaries.

### `local-eval/` — Local evaluation tooling directory

- Contains `eval_lib.py` (shared eval library), `flaw_scan.py` (flaw scanning), `gen_dlg.py` (dialogue generation), `gen_samples.py` (sample generation), and a `README.md`.
