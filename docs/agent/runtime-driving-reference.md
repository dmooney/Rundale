# Existing runtime command reference

Scope: the existing desktop, server, and CLI runtimes. These commands do not
prove on-device iOS execution. For the mobile reset, use the
[product specifications](../product-specs/README.md) and their phase gates.
For desktop visual QA, also read [the live-game guide](driving-the-game-via-mcp.md).
The server is suitable for server/API proof when configured with the required
inference provider; simulator output is not real-inference evidence.

## Driving Parish via MCP (`parish-mcp`)

`.mcp.json` at the repo root registers `parish-mcp` as a project-level MCP
server. When you start a Claude Code session here, the tools below are
available as `mcp__parish__*`:

| Tool                       | Effect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `parish_world_snapshot`    | Read clock, player location, weather, recent log.                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `parish_map`               | Read the location graph plus the player's position.                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `parish_npcs_here`         | List NPCs co-located with the player.                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `parish_engine_state`      | Read the canonical deterministic engine state for QA validation — `active_scene`, `clock`, `weather`, `player`, `npcs`, `grapevine`. Assert the UI against this after each interaction to detect UI-vs-engine drift (#1331).                                                                                                                                                                                                                                                                             |
| `parish_save_state`        | Read save-file / branch metadata.                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `parish_turn`              | Read recent canonical exchanges, the newest 20 unseen retained world events in chronological order, and current scene state. Pass its lifetime-monotonic `event_cursor` back as `since`; overflow drops the oldest unseen events and advances the cursor to the coherent current total.                                                                                                                                                                                                                  |
| `parish_submit_input`      | Send player input — movement, action, dialogue, system commands. Optional `addressed_to` array scopes dialogue.                                                                                                                                                                                                                                                                                                                                                                                          |
| `parish_new_game`          | Start a fresh game on a new save branch.                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `parish_save_game`         | Save the current branch.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `parish_load_branch`       | Load a branch by integer id.                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `parish_setup_status`      | Reads first-run setup state: `{implemented, complete, provider, model, base_url, has_api_key, has_env_key}`.                                                                                                                                                                                                                                                                                                                                                                                             |
| `parish_setup_byok`        | Submits a BYOK provider config (`provider`, `api_key`, optional `base_url`/`model`). Persists to keychain + `parish.toml`, rebuilds the inference worker, emits `setup-done`.                                                                                                                                                                                                                                                                                                                            |
| `parish_latest_screenshot` | Read metadata for the most recent player-triggered screenshot (`path`, `taken_at`, `size_bytes`). Capture is initiated by pressing F2 in the live desktop window.                                                                                                                                                                                                                                                                                                                                        |
| `parish_file_bug`          | File a bug report (`title`, optional `description`/`context`). Bundles a live screenshot + recent logs + game state into a GitHub issue on the configured repo (`dmooney/rundale` by default) and returns the issue URL. Auto-appends a "black box" diagnostic payload — raw LLM prompt/response history, the `get_engine_state` snapshot, and the last raw user intent (#1331). In dry-run / no-token mode writes the composed report to disk (`created:false`, `bundle_path` set). For auto-QA agents. |
| `tauri_invoke`             | Generic escape hatch — call any backend command (e.g. `editor_*`, `get_debug_snapshot`) by name.                                                                                                                                                                                                                                                                                                                                                                                                         |

The MCP server is a _bridge_: it speaks HTTP to a running Parish backend on
`127.0.0.1:3030`. **Before using any `mcp__parish__*` tool**, ensure a
backend is up:

```sh
# Headless web server (works in any sandbox; recommended in CI / on the web):
bash parish/scripts/parish-mcp-backend.sh start    # spawn + wait for /api/health
bash parish/scripts/parish-mcp-backend.sh status   # report pid + health
bash parish/scripts/parish-mcp-backend.sh stop     # graceful shutdown

# Desktop, when a display is available — drives the live window:
cargo run -p parish-tauri -- --mcp-port 3030
```

If a tool call returns an MCP `isError: true` with `transport error: ...`,
the backend isn't running — call `parish-mcp-backend.sh start` first.

Two distinct things get called "MCP bridge" (#1366 §6) — be precise:
the `parish-mcp` **crate** is the stdio MCP server that exposes the
`mcp__parish__*` tools and forwards them over HTTP, while
`parish-tauri/src/mcp_bridge.rs` is the **embedded HTTP listener** inside the
desktop app that answers those forwarded calls when Tauri is the backend
(the web server answers them natively). For deeper context on both see
[parish/crates/parish-mcp/README.md](../../parish/crates/parish-mcp/README.md)
and [parish/crates/parish-tauri/src/mcp_bridge.rs](../../parish/crates/parish-tauri/src/mcp_bridge.rs).

## Driving Parish via the `parish` CLI client

The `parish` binary (`parish-client` crate) is a thin synchronous HTTP client for a
**running** Parish server. Calls `POST /api/command` and returns the full response in one
round-trip — no WebSocket, no polling.

```sh
# Start the server first:
bash parish/scripts/parish-mcp-backend.sh start     # port 3030
# or: just web 3001                                  # port 3001

# Drive it:
parish [--server http://localhost:3001] "look"       # single-shot
parish --script testing/fixtures/test_walkthrough.txt  # batch
parish                                               # interactive REPL
parish --json "go to the church" | jq .kind         # raw JSON

# PARISH_SERVER env var sets the default URL:
PARISH_SERVER=http://localhost:3001 parish "look"
```

Use `parish --script` for proof transcripts requiring real NPC inference.
Use `just run-headless --script` for deterministic, fast harness-level testing.
