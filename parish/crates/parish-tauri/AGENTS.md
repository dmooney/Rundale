# parish-tauri — agent scope

Desktop entry point + MCP bridge. Thin adapter — gameplay logic lives in `parish-core`. See root [`AGENTS.md`](../../../AGENTS.md) and [`mcp_bridge.rs`](src/mcp_bridge.rs).

## Scoped commands

```sh
cargo run  -p parish-tauri -- --mcp-port 3030    # live desktop window + MCP
cargo test -p parish-tauri                       # unit
just run                                          # cargo tauri dev (full UI loop)
```

## Local gotchas

- **MCP bridge expects backend on 127.0.0.1:3030.** When starting the desktop, the `--mcp-port` flag also opens the bridge — `mcp__parish__*` tools speak to it via HTTP. Without `--mcp-port`, MCP tools error with "transport error".
- **Tauri IPC types must match TS** (`parish/apps/ui/src/lib/types.ts`). serde uses snake_case — drift breaks the frontend silently.
- **Cross-runtime orchestration belongs in `parish-core`** (rule #12). Do not duplicate handlers from `parish-server`. Wire via `EventEmitter`.
- **`commands/` and `editor_commands/` are typed adapters only.** Real work delegates into `parish-core`; this layer only marshalls + emits events.
- **Onboarding wizard / BYOK setup** lives here (`parish_setup_status`, `parish_setup_byok`); state mutations persist to keychain + `parish.toml`.

## Module map

`commands/` runtime IPC, `editor_commands/` Designer IPC, `events/` emission, `mcp_bridge.rs` HTTP→backend, `main`+`lib` startup wiring.
