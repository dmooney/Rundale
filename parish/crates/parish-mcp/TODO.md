# parish-mcp — Technical Debt

## Open

| ID     | Category    | Severity | Location                 | Description                                                                                                                                                                                                                                                    |
| ------ | ----------- | -------- | ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-002 | Future Stub | P3       | `src/backend.rs:199-212` | `GenericTauriBackend` is an exported unimplemented placeholder for a future WebDriver/`tauri-driver` backend. It is intentionally real API surface, but it should stay visible in the debt ledger until implemented or explicitly moved behind a feature flag. |
| TD-003 | Complexity  | P3       | `src/jsonrpc.rs:1-385`   | JSON-RPC framing, request/response structs, protocol errors, async stdin/stdout loop, and tests all live in one module. If protocol support grows beyond line-delimited requests, split framing from dispatch/error types first.                               |

## In Progress

_(none)_

## Done

| ID     | Category   | Severity | Location             | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------ | ---------- | -------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-001 | Weak Tests | P2       | `src/tools.rs:1-491` | The curated MCP tool registry is manually kept in sync with backend command names and request shapes. Existing tests cover translators, but there is no parity test against the canonical server/Tauri IPC route registry. Add a wiring test so new gameplay commands do not silently miss MCP coverage. (resolved 2026-06-06: parity test mcp_tool_commands_are_subset_of_bridge_routes at src/tools.rs:591-704 asserts every tool's derived HTTP path is a real bridge route) |

## Progress Log

- **2026-05-25**: Initialized the crate debt ledger and recorded TD-001 through TD-003 from the current source scan.
- **2026-06-04**: Audit — 3 Open items reviewed, 0 migrated to Done, 1 anchor corrected (TD-002 `src/backend.rs:196-215` → `199-212`).
- **2026-06-06**: Re-audit vs current code. Resolved->Done: TD-001. Still open: TD-002 (GenericTauriBackend stub), TD-003 (jsonrpc.rs single module). Tracking epic re-opened: #1200.

## Issue tracking

2026-06-04 audit: open items in this file are tracked under epic(s) #1200 (Workspace decomposition), #1202 (Test coverage & type-drift).
2026-06-06 re-audit: TD-002/TD-003 tracked under re-opened epic #1200 (Workspace decomposition).
