# Proof Evidence — PR #933: parish-mcp stdio MCP server

Evidence type: gameplay transcript
Date: 2026-05-09
Branch: claude/mcp-tauri-server-0zdEN

## Requirement

PR #933 adds a new `parish-mcp` workspace crate that speaks Model Context
Protocol over stdio so an LLM client (Claude Code, Claude Desktop) can drive
a running Parish/Rundale instance. The new crate must:

1. Negotiate the MCP `initialize` handshake.
2. Advertise its tool registry via `tools/list`.
3. Route `tools/call` invocations through the `TauriBackend` trait into
   the underlying Parish HTTP API.
4. Surface backend transport / rejection errors as MCP `isError: true`
   tool results so the model can self-correct rather than abort.

## Unit / integration tests

```sh
cargo test -p parish-mcp
```

Result:

```
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The 25 tests cover:

- JSON-RPC framing (round-trip, notifications get no response, malformed
  JSON yields a parse-error response with `id: null`).
- HTTP backend behaviour against `wiremock` (GET vs POST heuristic, auth
  header forwarding, non-2xx surfaced as `BackendError::Rejected`,
  unimplemented stub).
- Tool argument validation and command translation (`parish_submit_input`
  rejects non-array `addressed_to`, `parish_load_branch` requires an
  integer id, `tauri_invoke` defaults `args` to `null`).
- MCP dispatch (`initialize` returns the negotiated `protocolVersion`,
  `tools/list` includes the curated set, `tools/call` routes through
  the backend, unknown tool → `-32602`, unknown method → `-32601`,
  backend rejection surfaces as `isError: true`).

## Architecture-fitness sensors

```sh
cargo test -p parish-core --test architecture_fitness
```

Result:

```
running 3 tests
test parish_cli_does_not_duplicate_parish_core_modules ... ok
test backend_agnostic_crates_do_not_pull_runtime_deps ... ok
test no_orphaned_source_files ... ok

test result: ok. 3 passed; 0 failed
```

Adding `parish-mcp` to the workspace did not violate the no-orphans rule
or pull a runtime dep into a backend-agnostic crate.

## End-to-end stdio MCP transcript

The clearest functional proof is to run the binary as a real MCP client
would and observe the JSON-RPC frames over stdio. We pipe three messages
— `initialize`, `tools/list`, `tools/call` — and read the responses.

Command:

```sh
printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","method":"initialize","id":1}' \
  '{"jsonrpc":"2.0","method":"tools/list","id":2}' \
  '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"parish_world_snapshot","arguments":{}},"id":3}' \
  | parish/target/debug/parish-mcp 2>/dev/null
```

Response 1 — `initialize` (formatted for readability):

```json
{
  "jsonrpc": "2.0",
  "result": {
    "capabilities": {"tools": {"listChanged": false}},
    "protocolVersion": "2025-06-18",
    "serverInfo": {"name": "parish-mcp", "version": "0.1.0"}
  },
  "id": 1
}
```

Response 2 — `tools/list` (showing the 9 curated tool names):

```
tauri_invoke
parish_world_snapshot
parish_map
parish_npcs_here
parish_save_state
parish_submit_input
parish_new_game
parish_save_game
parish_load_branch
```

Each tool descriptor carried a `description` string and a JSON-Schema
`inputSchema` (verified inline; full payload elided here for brevity).

Response 3 — `tools/call` for `parish_world_snapshot`, with no
`parish-server` running on `127.0.0.1:3030`:

```json
{
  "jsonrpc": "2.0",
  "result": {
    "content": [{
      "text": "transport error: error sending request for url (http://127.0.0.1:3030/api/world-snapshot)",
      "type": "text"
    }],
    "isError": true
  },
  "id": 3
}
```

This proves the four requirements above:

1. The server returned a valid `initialize` result with the negotiated
   protocol version and tool capability flag.
2. `tools/list` returned the curated registry, in the declared order.
3. `tools/call` routed through the backend — the URL it tried,
   `http://127.0.0.1:3030/api/world-snapshot`, is exactly what
   `ParishHttpBackend::command_to_path("get_world_snapshot")` produces,
   confirming the wiring-parity-aligned translation reaches the wire.
4. The transport error surfaced as a successful tool response with
   `isError: true` carrying the readable error text — the model can
   inspect this and try a different action rather than aborting the
   call.

## Lints

```sh
cargo clippy -p parish-mcp --all-targets -- -D warnings
```

Result: clean, no warnings.

## Summary

All gates green: 25 unit tests pass, architecture-fitness sensors
unchanged, clippy clean, end-to-end stdio handshake + `tools/list` +
`tools/call` round-trip verified. No partial-completion markers; the
production code path never reaches a placeholder macro. The future
WebDriver backend (`GenericTauriBackend`) intentionally returns
`BackendError::Unimplemented` as a typed, documented sentinel — see
its module-level doc-comment in `parish-mcp/src/backend.rs`.
