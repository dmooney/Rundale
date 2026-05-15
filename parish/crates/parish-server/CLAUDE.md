# parish-server — agent scope

Axum HTTP/WebSocket entry point. One of three modes (Tauri, CLI, server) — must stay parity-equivalent. See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/`](../../../docs/agent/) for repo-wide rules.

## Scoped commands

```sh
cargo test -p parish-server                    # unit + integration
cargo run  -p parish-server -- --port 3030     # local web server
bash parish/scripts/parish-mcp-backend.sh start # boot for mcp__parish__* tools
just check                                      # full fmt+clippy+tests (workspace)
```

## Local gotchas

- **Cross-runtime orchestration belongs in `parish-core`** (rule #12). New game-loop / IPC handlers must live in `parish-core` parameterized over `EventEmitter`; this crate provides only the Axum/WS adapter. Copy-pasting orchestration from `parish-tauri` is forbidden (#687, #696).
- **Resolve runtime paths from explicit config** (rule #9). Never call `current_dir()` in handlers — use `AppState`-stored paths set at startup. Use `parish_persistence::picker::resolve_project_saves_dir`.
- **Per-visitor session isolation.** Each browser visitor gets its own session with persisted save state — auth + session lifecycle live in `auth/`, `cf_auth/`, `middleware/`, `session/`, `state/`.
- **Backend-agnostic dependency rule (enforced by architecture-fitness test).** Never let `axum`/`tower*` types leak into `parish-core` or leaf crates.

## Module map

`routes/` HTTP, `ws/` real-time channel, `editor_routes/` mod editor surface, `session/`+`state/` lifecycle, `auth/`+`cf_auth/`+`middleware/` policy.
