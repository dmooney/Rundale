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
- **`mcp__limerick__*` tools silently fail to register on a fresh worktree/clone if the `limerick-mcp` binary isn't built at session init.** `.mcp.json` spawns the server at _session start_ to read its tool list; a missing binary means no tools register, with no in-session reload to recover — yet `claude mcp list` still shows the server `✓ Connected` (it reconnects lazily, after the roster is frozen), so the failure is invisible. The old `SessionStart--build-mcp.sh` async-built in the background and lost the race. Fixed (commit 3144b606): `.mcp.json` `command` is now `limerick/scripts/limerick-mcp-launch.sh`, which builds-if-missing then `exec`s, plus `MCP_TIMEOUT=900000` in `.claude/settings.json` so the one-time cold compile doesn't trip the startup timeout. If Limerick tools are ever absent mid-session, you can't reload them — restart Claude Code in the repo. To drive the game, prefer `mcp__limerick__*` over raw curl / the `limerick` HTTP client; the MCP `limerick_file_bug` path is the only one that captures a screenshot to disk (needs a live frontend: `just run` / `just demo`, not the headless server).
- **`limerick_turn` is a bounded latest-state feed, not a pageable event log.** It returns the newest 20 unseen retained events in chronological order and advances to the coherent lifetime total; overflow intentionally drops the oldest unseen events (#1843).
- **Context replacement clears retained events, not the lifetime cursor.** Publish-time epochs reject queued prior-context events while a monotonic total lets reconnecting clients receive the replacement context's first event.

## Module map

`jsonrpc/` JSON-RPC 2.0 framing (`dispatch.rs`, `message.rs`) (transport layer), `mcp.rs` MCP handshake + tool registry (protocol layer), `backend.rs` `TauriBackend` trait + `LimerickHttpBackend` impl (adapter layer), `tools.rs` tool definitions and JSON Schema parameter shapes.

- **Launch generated Python fixtures through the interpreter.** Parallel fork/exec
  can inherit a sibling test's open write descriptor before close-on-exec takes
  effect; directly executing that script can fail with Linux `ETXTBSY`. Keep the
  production executable launch covered by the real session-host integration test.
