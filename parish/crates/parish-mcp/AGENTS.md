# parish-mcp — agent scope

Model Context Protocol (MCP) server — stdio JSON-RPC 2.0 bridge that lets LLM clients (Claude Code, Claude Desktop) drive a running Parish/Tauri instance over HTTP. Registered in the repo root `.mcp.json`. See root [`AGENTS.md`](../../../AGENTS.md) for non-negotiable rules and [`README.md`](README.md) for transport rationale, architecture diagram, and full tool reference.

## Scoped commands

```sh
cargo test -p parish-mcp                                    # unit (JSON-RPC, backend mock, tool dispatch)
cargo run  -p parish-mcp -- --base-url http://127.0.0.1:3030  # attach to a running backend
```

The MCP server expects a backend on `127.0.0.1:3030` — call `bash parish/scripts/parish-mcp-backend.sh start` first, or run `parish-tauri --mcp-port 3030` / `parish-server --port 3030`.

## Local gotchas

- **Bridge, not backend.** `parish-mcp` never touches game state directly. All mutations flow through HTTP to a running backend (`parish-tauri --mcp-port` or `parish-server`). The backend must be up before any `mcp__parish__*` tool call or you get `transport error`.
- **Two backends, one trait.** `ParishHttpBackend` (HTTP `/api/*`) is the production impl; `GenericTauriBackend` (WebDriver / `tauri-driver`) is a stub wired through `BackendError::Unimplemented`, gated behind the off-by-default `generic-tauri-backend` cargo feature so the default build ships no always-`Unimplemented` backend. Adding a new backend requires no MCP protocol changes — just implement `TauriBackend`.
- **Mode-parity inherited from `sync_routes`.** Because `ParishHttpBackend` wraps `POST /api/command` and `GET /api/state`, it inherits the mode-parity contract — every gameplay action available over Tauri IPC or WebSocket must also be reachable through this layer. Wire-type changes are breaking for every downstream consumer.
- **Stdio-only transport.** The binary speaks line-delimited JSON-RPC 2.0 on stdin/stdout; logs go to stderr. The `serve()` function in `jsonrpc/dispatch.rs` is generic over `AsyncRead`/`AsyncWrite` so an HTTP/SSE transport can reuse it unchanged — only `main.rs` would need a second wiring path.
- **BYOK tools only work against a running Tauri desktop session, not headless server.** `parish_setup_status` / `parish_setup_byok` rely on in-process `AppState` sharing with the Tauri bridge — `parish-server` does not yet expose the matching HTTP routes (see README future work).

## Module map

`jsonrpc.rs` JSON-RPC 2.0 framing (transport layer), `mcp.rs` MCP handshake + tool registry (protocol layer), `backend.rs` `TauriBackend` trait + `ParishHttpBackend` impl (adapter layer), `tools.rs` tool definitions and JSON Schema parameter shapes.
