# parish-server — agent scope

Axum HTTP/WebSocket entry point. One of three modes (Tauri, CLI, server) — must stay parity-equivalent. See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/`](../../../docs/agent/) for repo-wide rules.

## Scoped commands

```sh
cargo test -p parish-server                     # unit + integration
cargo run  -p parish-server -- --port 3001      # local web server (also: just web)
bash parish/scripts/parish-mcp-backend.sh start # boot for mcp__parish__* tools (port 3030)
just check                                      # full fmt+clippy+tests (workspace)
```

The crate ships **both** a library (`parish_server::run_server`) and a binary (`parish-server`). The binary is `src/main.rs`; everything else lives under the library surface so embedders (tests, future Tauri "embed the HTTP API" flows, etc.) can keep using `run_server` directly.

## Local gotchas

- **Cross-runtime orchestration belongs in `parish-core`** (rule #12). New game-loop / IPC handlers must live in `parish-core` parameterized over `EventEmitter`; this crate provides only the Axum/WS adapter. Copy-pasting orchestration from `parish-tauri` is forbidden (#687, #696).
- **Resolve runtime paths from explicit config** (rule #9). Never call `current_dir()` in handlers — use `AppState`-stored paths set at startup. Use `parish_persistence::picker::resolve_project_saves_dir`.
- **Per-visitor session isolation.** Each browser visitor gets its own session with persisted save state — auth + session lifecycle live in `auth/`, `cf_auth/`, `middleware/`, `session/`, `state/`.
- **Backend-agnostic dependency rule (enforced by architecture-fitness test).** Never let `axum`/`tower*` types leak into `parish-core` or leaf crates.

## Module map

`routes.rs` HTTP, `ws.rs` real-time channel, `sync_routes.rs` + `sync_types.rs` + `drain.rs` synchronous `POST /api/command` + `GET /api/state` endpoints for thin clients, `editor_routes.rs` mod editor surface, `session.rs`+`state.rs` lifecycle, `auth.rs`+`cf_auth.rs`+`middleware.rs` policy.

## Thin-client surface (`sync_routes.rs`)

`POST /api/command` and `GET /api/state` are the synchronous public API used by `parish-client`, the MCP bridge, and CI harnesses. They are part of the mode-parity contract — every gameplay action available over Tauri IPC or WebSocket must also be reachable via `POST /api/command`. The request body is `{ text, addressedTo?, timeoutMs?, includeState?, includeMap? }`; the response shape is defined in `sync_types::CommandResponse`. Treat any change to those types as a breaking API change for downstream consumers and update `parish-client`'s wire types (`parish/crates/parish-client/src/client.rs`) in the same PR.
