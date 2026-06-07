# parish-mcp — Technical Debt

## Open

| ID  | Category | Severity | Location | Description |
| --- | -------- | -------- | -------- | ----------- |

_(none — all open items resolved; see Done.)_

## In Progress

_(none)_

## Done

| ID     | Category    | Severity | Location             | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------ | ----------- | -------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Weak Tests  | P2       | `src/tools.rs:1-491` | The curated MCP tool registry is manually kept in sync with backend command names and request shapes. Existing tests cover translators, but there is no parity test against the canonical server/Tauri IPC route registry. Add a wiring test so new gameplay commands do not silently miss MCP coverage. (resolved 2026-06-06: parity test mcp_tool_commands_are_subset_of_bridge_routes at src/tools.rs:591-704 asserts every tool's derived HTTP path is a real bridge route) |
| TD-002 | Future Stub | P3       | `src/backend.rs`     | `GenericTauriBackend` was an unconditionally-exported `Unimplemented` placeholder. (resolved 2026-06-07, #1200: moved behind the off-by-default `generic-tauri-backend` cargo feature — the default public surface no longer ships an always-`Unimplemented` backend; it still returns the typed `BackendError::Unimplemented`, no panic macro introduced; default-build behaviour unchanged since the type was wired into no binary.)                                          |
| TD-003 | Complexity  | P3       | `src/jsonrpc/`       | `jsonrpc.rs` (385 LOC) held framing, value structs, protocol errors, the async serve loop, and tests in one module. (resolved 2026-06-07, #1200: split into `jsonrpc/{mod,message,dispatch}.rs` — `message` owns `Request`/`Response`/`RpcError`, `dispatch` owns `MethodHandler`/`ResponseWriter`/`write_response`/`serve`; `mod.rs` re-exports both flat so `jsonrpc::*` paths are unchanged.)                                                                                |

## Progress Log

- **2026-05-25**: Initialized the crate debt ledger and recorded TD-001 through TD-003 from the current source scan.
- **2026-06-04**: Audit — 3 Open items reviewed, 0 migrated to Done, 1 anchor corrected (TD-002 `src/backend.rs:196-215` → `199-212`).
- **2026-06-06**: Re-audit vs current code. Resolved->Done: TD-001. Still open: TD-002 (GenericTauriBackend stub), TD-003 (jsonrpc.rs single module). Tracking epic re-opened: #1200.
- **2026-06-07** (#1200 group A): Resolved->Done: TD-002 (GenericTauriBackend feature-gated behind `generic-tauri-backend`, default-off), TD-003 (jsonrpc.rs → `jsonrpc/{mod,message,dispatch}.rs`). No open items remain in this crate.

## Issue tracking

2026-06-04 audit: open items in this file are tracked under epic(s) #1200 (Workspace decomposition), #1202 (Test coverage & type-drift).
2026-06-06 re-audit: TD-002/TD-003 tracked under re-opened epic #1200 (Workspace decomposition).
