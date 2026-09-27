# Existing runtime command reference

Scope: the existing desktop, server, and CLI runtimes. These commands do not
prove on-device iOS execution. For the mobile reset, use the
[product specifications](../product-specs/README.md) and their phase gates.
For desktop visual QA, also read [the live-game guide](driving-the-game-via-mcp.md).
The server is suitable for server/API proof when configured with the required
inference provider; simulator output is not real-inference evidence.

## Driving Limerick via MCP (`limerick-mcp`)

`.mcp.json` at the repo root registers `limerick-mcp` as a project-level MCP
server. When you start a Claude Code session here, the tools below are
available as `mcp__limerick__*`:

| Tool                         | Effect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `limerick_world_snapshot`    | Read clock, player location, weather, recent log.                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `limerick_map`               | Read the location graph plus the player's position.                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `limerick_npcs_here`         | List NPCs co-located with the player.                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `limerick_engine_state`      | Read the canonical deterministic engine state for QA validation — `active_scene`, `clock`, `weather`, `player`, `npcs`, `grapevine`. Assert the UI against this after each interaction to detect UI-vs-engine drift (#1331).                                                                                                                                                                                                                                                                             |
| `limerick_save_state`        | Read save-file / branch metadata.                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `limerick_turn`              | Read recent canonical exchanges, the newest 20 unseen retained world events in chronological order, and current scene state. Pass its lifetime-monotonic `event_cursor` back as `since`; overflow drops the oldest unseen events and advances the cursor to the coherent current total.                                                                                                                                                                                                                  |
| `limerick_submit_input`      | Send player input — movement, action, dialogue, system commands. Optional `addressed_to` array scopes dialogue.                                                                                                                                                                                                                                                                                                                                                                                          |
| `limerick_new_game`          | Start a fresh game on a new save branch.                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `limerick_save_game`         | Save the current branch.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `limerick_load_branch`       | Load a branch by integer id.                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `limerick_setup_status`      | Reads first-run setup state: `{implemented, complete, provider, model, base_url, has_api_key, has_env_key}`.                                                                                                                                                                                                                                                                                                                                                                                             |
| `limerick_setup_byok`        | Submits a BYOK provider config (`provider`, `api_key`, optional `base_url`/`model`). Persists to keychain + `limerick.toml`, rebuilds the inference worker, emits `setup-done`.                                                                                                                                                                                                                                                                                                                          |
| `limerick_latest_screenshot` | Read metadata for the most recent player-triggered screenshot (`path`, `taken_at`, `size_bytes`). Capture is initiated by pressing F2 in the live desktop window.                                                                                                                                                                                                                                                                                                                                        |
| `limerick_file_bug`          | File a bug report (`title`, optional `description`/`context`). Bundles a live screenshot + recent logs + game state into a GitHub issue on the configured repo (`dmooney/rundale` by default) and returns the issue URL. Auto-appends a "black box" diagnostic payload — raw LLM prompt/response history, the `get_engine_state` snapshot, and the last raw user intent (#1331). In dry-run / no-token mode writes the composed report to disk (`created:false`, `bundle_path` set). For auto-QA agents. |
| `tauri_invoke`               | Generic escape hatch — call any backend command (e.g. `editor_*`, `get_debug_snapshot`) by name.                                                                                                                                                                                                                                                                                                                                                                                                         |

The MCP server is a _bridge_: it speaks HTTP to a running Limerick backend on
`127.0.0.1:3030`. **Before using any `mcp__limerick__*` tool**, ensure a
backend is up:

```sh
# Headless web server (works in any sandbox; recommended in CI / on the web):
bash limerick/scripts/limerick-mcp-backend.sh start    # spawn + wait for /api/health
bash limerick/scripts/limerick-mcp-backend.sh status   # report pid + health
bash limerick/scripts/limerick-mcp-backend.sh stop     # graceful shutdown

# Desktop, when a display is available — drives the live window:
cargo run -p limerick-tauri -- --mcp-port 3030
```

If a tool call returns an MCP `isError: true` with `transport error: ...`,
the backend isn't running — call `limerick-mcp-backend.sh start` first.

Two distinct things get called "MCP bridge" (#1366 §6) — be precise:
the `limerick-mcp` **crate** is the stdio MCP server that exposes the
`mcp__limerick__*` tools and forwards them over HTTP, while
`limerick-tauri/src/mcp_bridge.rs` is the **embedded HTTP listener** inside the
desktop app that answers those forwarded calls when Tauri is the backend
(the web server answers them natively). For deeper context on both see
[limerick/crates/limerick-mcp/README.md](../../limerick/crates/limerick-mcp/README.md)
and [limerick/crates/limerick-tauri/src/mcp_bridge.rs](../../limerick/crates/limerick-tauri/src/mcp_bridge.rs).

## Driving Limerick via the `limerick` CLI client

The `limerick` binary (`limerick-client` crate) is a thin synchronous HTTP client for a
**running** Limerick server. Calls `POST /api/command` and returns the full response in one
round-trip — no WebSocket, no polling.

```sh
# Start the server first:
bash limerick/scripts/limerick-mcp-backend.sh start     # port 3030
# or: just web 3001                                  # port 3001

# Drive it:
limerick [--server http://localhost:3001] "look"      # single-shot
limerick --script testing/fixtures/test_walkthrough.txt  # batch
limerick                                              # interactive REPL
limerick --json "go to the church" | jq .kind         # raw JSON

# LIMERICK_SERVER env var sets the default URL:
LIMERICK_SERVER=http://localhost:3001 limerick "look"
```

Use `limerick --script` for proof transcripts requiring real NPC inference.
Use `just run-headless --script` for deterministic, fast harness-level testing.
