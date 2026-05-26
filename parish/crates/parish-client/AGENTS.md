# parish-client — agent scope

Synchronous CLI client for the Parish game server. Binary name: `parish`. Thin HTTP client speaking to a running `parish-server` via `POST /api/command` and `GET /api/state`. Three modes: single-shot, script batch (`--script`), and interactive REPL. See root [`AGENTS.md`](../../../AGENTS.md) for non-negotiable rules.

## Scoped commands

```sh
cargo run  -p parish-client "look"                          # single-shot against localhost:3001
cargo run  -p parish-client --script testing/fixtures/...    # batch script mode
cargo run  -p parish-client                                  # interactive REPL
cargo run  -p parish-client --json "go to the church"        # raw JSON output
cargo test -p parish-client                                  # unit
```

Set `PARISH_SERVER=http://localhost:3030` to target a different backend (default: `http://localhost:3001`).

## Local gotchas

- **Wire types must stay in sync with `parish-server`.** The `CommandResponse` shape in `src/client.rs` must match `parish-server`'s `sync_types::CommandResponse` exactly. Change both in the same PR or the client deserializes silently into partial/missing fields (#771).
- **Binary-only crate (no lib).** This crate ships no library surface — only the `parish` binary at `src/main.rs`. Embedders should talk to `parish-server`'s library or use `parish-client` as a subprocess.
- **Synchronous per-call, not streaming.** Uses `reqwest` with JSON + cookies per request. No WebSocket, no event stream. For real-time pushes, use `parish-server`'s WS endpoints or Tauri IPC.
- **REPL builds history in memory only.** Session history is not persisted between invocations. Use `--script` for repeatable batch workflows.
- **`PARISH_SERVER` env var is the only configuration surface.** No config file, no CLI flag for the URL beyond `--server`. The env-var default matches `parish-server`'s default port.

## Module map

`main.rs` CLI entry (clap parser, three-mode dispatch), `client.rs` `ParishClient` HTTP wrapper, `render.rs` response→prose rendering, `repl.rs` script runner + interactive REPL loop, `session.rs` session token management.
