# parish-mcp — agent scope

Stdio JSON-RPC 2.0 bridge that lets LLM clients (Claude Code, Claude Desktop) drive a running Parish/Tauri instance over HTTP. Registered in `.mcp.json` at the repo root. See root [`AGENTS.md`](../../../AGENTS.md) and [`README.md`](README.md) for transport rationale, architecture diagram, and full tool reference.

## Scoped commands

```sh
cargo test -p parish-mcp                                    # unit (JSON-RPC, backend mock, tool dispatch)
cargo run  -p parish-mcp -- --base-url http://127.0.0.1:3030  # attach to a running backend
```

Backend must be up on `127.0.0.1:3030` — run `bash parish/scripts/parish-mcp-backend.sh start`, `parish-tauri --mcp-port 3030`, or `parish-server --port 3030` first.

## Local gotchas

- **Bridge, not backend.** All mutations flow through HTTP to `parish-tauri --mcp-port` or `parish-server`. A missing backend produces `transport error`.
- **Two backends, one trait.** `ParishHttpBackend` (HTTP `/api/*`) is the production impl. `GenericTauriBackend` (WebDriver / `tauri-driver`) is a stub returning `BackendError::Unimplemented`, gated behind the off-by-default `generic-tauri-backend` cargo feature. New backends require no MCP protocol changes — just implement `TauriBackend`.
- **Mode-parity inherited from `sync_routes`.** `ParishHttpBackend` wraps `POST /api/command` and `GET /api/state`; wire-type changes there are breaking for every downstream consumer.
- **Stdio-only transport.** Line-delimited JSON-RPC 2.0 on stdin/stdout; logs to stderr. `serve()` in `jsonrpc/dispatch.rs` is generic over `AsyncRead`/`AsyncWrite` — an HTTP/SSE transport needs only a new `main.rs` wiring.
- **BYOK tools require Tauri, not headless.** `parish_setup_status` / `parish_setup_byok` rely on in-process `AppState` sharing; `parish-server` does not yet expose the matching HTTP routes.

## Module map

`jsonrpc/` JSON-RPC 2.0 framing (`dispatch.rs`, `message.rs`) (transport layer), `mcp.rs` MCP handshake + tool registry (protocol layer), `backend.rs` `TauriBackend` trait + `ParishHttpBackend` impl (adapter layer), `tools.rs` tool definitions and JSON Schema parameter shapes.
