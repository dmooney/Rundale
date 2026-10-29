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

## Portable app session through MCP

The session-host backend drives the app's shared Swift controller/presenter,
Rust FFI, and Endpoint client from a terminal on macOS or Linux. It requires
Swift and Rust toolchains, not Xcode, a simulator, an iOS GUI, or the Apple
Firebase SDK. It is separate from the default desktop/server HTTP backend.
See [ADR-028](../adr/028-portable-session-mcp-driver.md).

Build once, then launch MCP with explicit absolute paths:

```sh
just linux-host --help
(cd limerick && cargo build -p limerick-mcp)
limerick/target/debug/limerick-mcp --backend session-host \
  --host-executable "$PWD/tools/linux-host/.build/debug/rundale-linux-host" \
  --mod-dir "$PWD/mods/rundale" --save-dir /absolute/path/to/session-save \
  --inference mock
```

`mock` is the default. `fixture` uses the deterministic presentation fixture;
`sse --sse-fixture /absolute/path/to/dialogue-v1.sse` replays recorded dialogue
with mock intent; `live` uses the deployed `limerick-prod` Endpoints. The child
is managed by MCP; do not connect another MCP implementation to it. Its
`--protocol` JSON-lines interface is private and versioned independently of MCP.

### Actions and asynchronous output

The selected backend advertises only its supported tools:

| Tool suffix (after `limerick_`) | Arguments                          | Behavior                                                                   |
| ------------------------------- | ---------------------------------- | -------------------------------------------------------------------------- |
| `new_game`                      | optional `replaceExisting` boolean | Opens a new game; refuses existing files unless replacement is explicit.   |
| `resume_session`                | none                               | Opens the existing save; missing/corrupt saves fail without resetting.     |
| `submit_input`                  | `text`                             | Dispatches the line and returns before inference finishes.                 |
| `answer_clarification`          | `questionID`, `choiceID`           | Answers the currently offered question; stale identifiers fail.            |
| `stop`                          | none                               | Requests the shared session's Stop action.                                 |
| `retry`                         | none                               | Retries when the shared presenter offers Retry.                            |
| `persist_session`               | none                               | Persists an idle session and reports failure rather than claiming success. |
| `session_presentation`          | none                               | Reads current presentation and bounded recent snapshots.                   |
| `close_session`                 | none                               | Persists and closes an idle session.                                       |

Starting MCP leaves the save unopened. Call new or resume explicitly. To replace
an open session, close it first. Overlapping turns and save/context operations
while busy are rejected. Inspection and Stop remain responsive. A dispatched control
can report `controlPending` before the controller starts work;
wait for both `isStreaming` and `controlPending` to clear before saving.

Each successful action returns Markdown in MCP `content` and a structured result
in `structuredContent`: `presentation`, `markdown`, and `history`. Presentation
includes available actions, header, transcript row states, composer draft,
clarification identifiers, failures, and explicit persistence status. Dispatch
is an acknowledgment, not a committed gameplay result: inspect until the turn
settles. Provisional and interrupted output must not be treated as committed.
History retains at most 32 recent snapshots and two megabytes of encoded history,
so it is a latest-state inspection aid, not a complete event log.

A save-directory kernel lock prevents competing managed hosts. EOF Stops an
active turn, saves committed state, and closes the engine. The MCP parent allows
10 seconds for shutdown, then terminates and reaps a stalled child. Protocol
failure, timeout, or child exit is reported; there is no silent restart. Restart
MCP and explicitly resume the last durable save after a crash. Never assume a
failed persistence request saved the presentation projection.

### Live credentials

Supply `RUNDALE_FIREBASE_API_KEY` and `RUNDALE_APPCHECK_DEBUG_TOKEN` as private
process environment values. On Claude Code cloud, use the environment's secret
configuration; in a local terminal, load private values without echoing them.
Do not put secrets in MCP arguments, checked-in configuration, shell tracing,
logs, frames, evidence, or game saves. The API key must belong to the configured
iOS Firebase app; a web key or `sfk_live_` consumer key does not prove this path.

The defaults are project `limerick-prod`, app
`1:877612517009:ios:586f98a2cc3e7d0c676130`, and bundle `com.rundale.mobile`.
`RUNDALE_FIREBASE_PROJECT_ID`, `RUNDALE_FIREBASE_APP_ID`, and
`RUNDALE_FIREBASE_BUNDLE_ID` override those identifiers when deliberately testing
another matching deployment. Live Endpoint routing remains the deployed Rundale
`limerick-demo` binding. Restricted Firebase API requests carry the iOS bundle
identifier header; the credential provider does not relax API-key restrictions.

Use the existing `endpoints/deploy/limerick-prod.sh appcheck-token create NAME`
command to register a temporary debug token. Name it for the terminal/cloud
session, capture its output privately, and delete it afterward with
`endpoints/deploy/limerick-prod.sh appcheck-token delete NAME`, including on failed
runs. Live testing and debug-token registration in `limerick-prod` are already
authorized. This is mobile-path authorization, not device attestation.

Credentials remain in memory. Each host process signs in anonymously once;
refresh preserves that authenticated user and rotates its refresh token.
Restarting the host creates a new anonymous identity while the local game save
remains independent. Credential failures fail closed rather than signing in
again to conceal a refresh failure.

### Verification and evidence

Run `just linux-host-test`, `cargo test -p limerick-mcp` from `limerick/`, and
`python3 tools/linux-host/test_mcp_session.py` after building both binaries.
Linux CI runs the host suites and cross-language MCP test with mock inference.
A live terminal playthrough on macOS or Linux must additionally show streamed
output, committed replies, and persist/close/resume continuity. Missing live
credentials leave live acceptance incomplete; unavailable Linux cloud access
alone does not block acceptance when Linux CI and live Mac-terminal validation pass.

Label every receipt with its actual host platform and inference mode. Host
frames prove neither app layout, accessibility, iOS lifecycle, device attestation,
simulator behavior, nor physical-iPhone acceptance. Do not label a Mac-terminal
run as an observed Linux run.
