# limerick-mcp — agent scope

Stdio JSON-RPC 2.0 bridge that lets LLM clients (Claude Code, Claude Desktop) drive a running Limerick/Tauri instance over HTTP. Registered in `.mcp.json` at the repo root. See root [`AGENTS.md`](../../../AGENTS.md) and [`README.md`](README.md) for transport rationale, architecture diagram, and full tool reference.

## Scoped commands

```sh
cargo test -p limerick-mcp                                    # unit (JSON-RPC, backend mock, tool dispatch)
cargo run  -p limerick-mcp -- --base-url http://127.0.0.1:3030  # attach to a running backend
```

Backend must be up on `127.0.0.1:3030` — run `bash limerick/scripts/limerick-mcp-backend.sh start`, `limerick-tauri --mcp-port 3030`, or `limerick-server --port 3030` first.

## Local gotchas

- **Bridge, not backend.** All mutations flow through HTTP to `limerick-tauri --mcp-port` or `limerick-server`. A missing backend produces `transport error`.
- **Two backends, one trait.** `LimerickHttpBackend` (HTTP `/api/*`) is the production impl. `GenericTauriBackend` (WebDriver / `tauri-driver`) is a stub returning `BackendError::Unimplemented`, gated behind the off-by-default `generic-tauri-backend` cargo feature. New backends require no MCP protocol changes — just implement `TauriBackend`.
- **Mode-parity inherited from `sync_routes`.** `LimerickHttpBackend` wraps `POST /api/command` and `GET /api/state`; wire-type changes there are breaking for every downstream consumer.
- **Stdio-only transport.** Line-delimited JSON-RPC 2.0 on stdin/stdout; logs to stderr. `serve()` in `jsonrpc/dispatch.rs` is generic over `AsyncRead`/`AsyncWrite` — an HTTP/SSE transport needs only a new `main.rs` wiring.
- **BYOK tools require Tauri, not headless.** `limerick_setup_status` / `limerick_setup_byok` rely on in-process `AppState` sharing; `limerick-server` does not yet expose the matching HTTP routes.

## Module map

`jsonrpc/` JSON-RPC 2.0 framing (`dispatch.rs`, `message.rs`) (transport layer), `mcp.rs` MCP handshake + tool registry (protocol layer), `backend.rs` `TauriBackend` trait + `LimerickHttpBackend` impl (adapter layer), `tools.rs` tool definitions and JSON Schema parameter shapes.
