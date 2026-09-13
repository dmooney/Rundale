# limerick-tauri — agent scope

Desktop entry point + MCP bridge. Thin adapter — gameplay logic lives in `limerick-core`. See root [`AGENTS.md`](../../../AGENTS.md) and [`mcp_bridge.rs`](src/mcp_bridge.rs).

## Scoped commands

```sh
cargo run  -p limerick-tauri -- --mcp-port 3030    # live desktop window + MCP
cargo test -p limerick-tauri                       # unit
just run                                          # cargo tauri dev (full UI loop)
```

## Local gotchas

- **MCP bridge expects backend on 127.0.0.1:3030.** When starting the desktop, the `--mcp-port` flag also opens the bridge — `mcp__limerick__*` tools speak to it via HTTP. Without `--mcp-port`, MCP tools error with "transport error". **Driving the game via MCP** (time control via `/pause`+`/resume`, async-world event deltas, screenshot/focus gotchas, `submit_input` returns `null`): see [`docs/agent/driving-the-game-via-mcp.md`](../../../docs/agent/driving-the-game-via-mcp.md).
- **Tauri IPC types must match TS** (`limerick/apps/ui/src/lib/types.ts`). serde uses snake_case — drift breaks the frontend silently.
- **Cross-runtime orchestration belongs in `limerick-core`** (rule #12). Do not duplicate handlers from `limerick-server`. Wire via `EventEmitter`.
- **`commands/` and `editor_commands/` are typed adapters only.** Real work delegates into `limerick-core`; this layer only marshalls + emits events.
- **Onboarding wizard / BYOK setup** lives here (`limerick_setup_status`, `limerick_setup_byok`); state mutations persist to keychain + `limerick.toml`.

## Module map

`commands/` runtime IPC, `editor_commands/` Designer IPC, `events/` emission, `mcp_bridge.rs` HTTP→backend, `main`+`lib` startup wiring.
