# parish-client — agent scope

Synchronous CLI client (`parish` binary) for a running `parish-server`. Speaks `POST /api/command` + `GET /api/state`. Three modes: single-shot, `--script` batch, interactive REPL. See root [`AGENTS.md`](../../../AGENTS.md).

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

- **Wire types must stay in sync with `parish-server` (#771).** `CommandResponse` in `src/client.rs` must match `parish-server`'s `sync_types::CommandResponse` exactly — change both in the same PR or deserialization silently drops fields.
- **Binary-only — no lib surface.** Only `src/main.rs`. Embedders should use `parish-server`'s library directly, not subprocess this binary.
- **Synchronous per-call, not streaming.** `reqwest` JSON + cookies; no WebSocket. For real-time pushes use `parish-server`'s WS endpoints or Tauri IPC.
- **REPL history is in-memory only.** Not persisted between invocations — use `--script` for repeatable workflows.
- **`PARISH_SERVER` env var is the only URL config** (default matches `parish-server`'s default port); `--server` is the CLI override.

## Module map

`main.rs` CLI entry (clap parser, three-mode dispatch), `client.rs` `ParishClient` HTTP wrapper, `render.rs` response→prose rendering, `repl.rs` script runner + interactive REPL loop, `session.rs` session token management.
