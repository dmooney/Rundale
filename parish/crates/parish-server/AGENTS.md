# parish-server — agent scope

Axum HTTP/WebSocket entry point — one of three modes (Tauri, CLI, server), must stay parity-equivalent. See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/`](../../../docs/agent/) for repo-wide rules.

## Scoped commands

```sh
cargo test -p parish-server                     # unit + integration
cargo run  -p parish-server -- --port 3001      # local web server (also: just web)
bash parish/scripts/parish-mcp-backend.sh start # boot for mcp__parish__* tools (port 3030)
just check                                      # full fmt+clippy+tests (workspace)
```

Ships both a library (`parish_server::run_server`) and a binary (`src/main.rs`); embedders call `run_server` directly.

## Local gotchas

- **Cross-runtime orchestration belongs in `parish-core` (rule #12).** New game-loop / IPC handlers must live in `parish-core` parameterized over `EventEmitter`; this crate provides only the Axum/WS adapter. Copy-pasting from `parish-tauri` is forbidden (#687, #696).
- **Resolve runtime paths from config, not cwd (rule #9).** Never call `current_dir()` in handlers — use `AppState`-stored paths. Use `parish_persistence::picker::resolve_project_saves_dir`.
- **Per-visitor session isolation.** Each visitor gets its own session with persisted save state; auth + lifecycle in `auth.rs`, `cf_auth.rs`, `middleware.rs`, `session.rs`, `session/`, `state.rs`.
- **No `axum`/`tower*` leakage (enforced).** These types must not appear in `parish-core` or leaf crates.

## Module map

`routes.rs`+`routes/` HTTP, `ws.rs` real-time channel, `sync_routes.rs`+`sync_types.rs`+`drain.rs` synchronous `POST /api/command` + `GET /api/state`, `editor_routes.rs` mod editor surface, `session.rs`+`session/` lifecycle, `state.rs` app state, `lock_metrics.rs` `MeteredMutex` contention counters for the `AppState` locks (#1366 §2 — `AppState::lock_metrics()` snapshots them), `auth.rs`+`cf_auth.rs`+`middleware.rs` policy.

## Thin-client surface (`sync_routes.rs`)

**`POST /api/command` and `GET /api/state`** are the synchronous public API for `parish-client`, the MCP bridge, and CI harnesses. Part of the mode-parity contract — every gameplay action available over Tauri IPC or WebSocket must be reachable here. Request: `{ text, addressedTo?, timeoutMs?, includeState?, includeMap? }`; response: `sync_types::CommandResponse`. Wire-type changes are breaking — update `parish-client` wire types at `parish/crates/parish-client/src/client.rs` in the same PR.
