# limerick-server — agent scope

Axum HTTP/WebSocket entry point — one of three modes (Tauri, CLI, server), must stay parity-equivalent. See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/`](../../../docs/agent/) for repo-wide rules.

## Scoped commands

```sh
cargo test -p limerick-server                     # unit + integration
cargo run  -p limerick-server -- --port 3001      # local web server (also: just web)
bash limerick/scripts/limerick-mcp-backend.sh start # boot for mcp__limerick__* tools (port 3030)
just check                                      # full fmt+clippy+tests (workspace)
```

Ships both a library (`limerick_server::run_server`) and a binary (`src/main.rs`); embedders call `run_server` directly.

## Local gotchas

- **[Cross-runtime orchestration](../../../docs/agent/engineering-rules.md#cross-runtime-orchestration) belongs in `limerick-core`.** New game-loop / IPC handlers must live in `limerick-core` parameterized over `EventEmitter`; this crate provides only the Axum/WS adapter. Copy-pasting from `limerick-tauri` is forbidden (#687, #696).
- **Resolve runtime paths from config, not cwd ([runtime paths](../../../docs/agent/persistence-and-session-rules.md#runtime-paths)).** Never call `current_dir()` in handlers — use `AppState`-stored paths. Use `limerick_persistence::picker::resolve_project_saves_dir`.
- **Per-visitor session isolation.** Each visitor gets its own session with persisted save state; auth + lifecycle in `auth.rs`, `cf_auth.rs`, `middleware.rs`, `session.rs`, `session/`, `state.rs`.
- **No `axum`/`tower*` leakage (enforced).** These types must not appear in `limerick-core` or leaf crates.
- **The headless `limerick-server` isolates state per-visitor session (the `limerick_sid` cookie).** Each cookie-less HTTP request (e.g. a bare `curl` to `/api/command`, `/api/engine-state`, `/api/submit-bug-report`) gets a FRESH session → a FRESH `Arc<AppState>`, so mutations from one call (player moves, `last_player_input`, conversation transcript) are invisible to the next. When driving the live server for a multi-step audit/proof, share ONE session with a cookie jar: `JAR=$(mktemp); curl -c "$JAR" -b "$JAR" ...` on every call. Symptom of getting this wrong: `get_engine_state` always shows the starting scene and `limerick_file_bug`'s diagnostic payload reports `last user intent: _none_` even after you submitted turns. The in-process integration tests share one AppState and don't hit this.
- **`tracing_appender::non_blocking` returns a `WorkerGuard` that MUST be kept alive for the full process lifetime.** Binding it as `let (_guard, ...)` inside a setup function drops it on return, which shuts down the background writer thread — the log file is created but stays 0 bytes and all `tracing::info!/debug!` calls are silently discarded. Always return the guard and bind it in `main` as `let _log_guard = setup_tracing_and_otel();` so it lives until the process exits. (Fixed in limerick-server/src/main.rs — `setup_tracing_and_otel` now returns `WorkerGuard`.)
- **`AppState` locks in limerick-server are `MeteredMutex` (#1366 §2), not bare `tokio::sync::Mutex`.** The inherent `.lock().await` records wait-time/contention counters; `&state.field` still deref-coerces to `&Mutex<T>` for the shared `GameLoopContext`/`NewGameParams` borrows, and locks taken through that deref'd reference are deliberately unmetered. Read counters via `AppState::lock_metrics()`. New coordination fields should be `MeteredMutex::new("<LOCK_ORDER name>", …)`.
- **Hash-based UI CSP must watch generated HTML files, not only the `dist` directory.** A fresh Svelte build changed its inline bootstrap while Cargo reused a stale `limerick-server` CSP hash because directory-only `rerun-if-changed` did not fire recursively; emit one watch path per generated file from `build.rs`.
- **Cold session construction must persist before workers start.** Lock before SQLite, commit the active marker and registry row, and only then publish state or spawn inference/tick/log workers.
- **Live web-server inference proofs leave billable background ticks running per session.** Stop the isolated server promptly after collecting evidence (or explicitly evict its sessions); abandoned cookie sessions otherwise continue Tier-2/3 calls until normal session cleanup.
- **A tower-sessions cookie is an opaque store-record id, not the Limerick session UUID.** Persist the tower record (which contains `limerick_session_id`) across server restarts; treating the raw `limerick_sid` cookie as a UUID loses the browser's active save while appearing to preserve cookie isolation.
- **Server subprocess tests must isolate both user-data and user-config roots.** Setting only `LIMERICK_USER_DATA_DIR` still lets strict v2 startup read the developer's platform `limerick.toml`; give every spawned server a dedicated `LIMERICK_USER_CONFIG_DIR` so host configuration cannot determine the result.

## Module map

`routes.rs`+`routes/` HTTP, `ws.rs` real-time channel, `sync_routes.rs`+`sync_types.rs`+`drain.rs` synchronous `POST /api/command` + `GET /api/state`, `editor_routes.rs` mod editor surface, `session.rs`+`session/` lifecycle, `state.rs` app state, `lock_metrics.rs` `MeteredMutex` contention counters for the `AppState` locks (#1366 §2 — `AppState::lock_metrics()` snapshots them), `auth.rs`+`cf_auth.rs`+`middleware.rs` policy.

## Thin-client surface (`sync_routes.rs`)

**`POST /api/command` and `GET /api/state`** are the synchronous public API for `limerick-client`, the MCP bridge, and CI harnesses. Part of the mode-parity contract — every gameplay action available over Tauri IPC or WebSocket must be reachable here. Request: `{ text, addressedTo?, timeoutMs?, includeState?, includeMap? }`; response: `sync_types::CommandResponse`. Wire-type changes are breaking — update `limerick-client` wire types at `limerick/crates/limerick-client/src/client.rs` in the same PR.
