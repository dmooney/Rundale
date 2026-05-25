# parish-mcp — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Weak Tests | P2 | `src/tools.rs:1-491` | The curated MCP tool registry is manually kept in sync with backend command names and request shapes. Existing tests cover translators, but there is no parity test against the canonical server/Tauri IPC route registry. Add a wiring test so new gameplay commands do not silently miss MCP coverage. |
| TD-002 | Future Stub | P3 | `src/backend.rs:196-215` | `GenericTauriBackend` is an exported unimplemented placeholder for a future WebDriver/`tauri-driver` backend. It is intentionally real API surface, but it should stay visible in the debt ledger until implemented or explicitly moved behind a feature flag. |
| TD-003 | Complexity | P3 | `src/jsonrpc.rs:1-385` | JSON-RPC framing, request/response structs, protocol errors, async stdin/stdout loop, and tests all live in one module. If protocol support grows beyond line-delimited requests, split framing from dispatch/error types first. |

## In Progress

*(none)*

## Done

*(none)*

## Progress Log

- **2026-05-25**: Initialized the crate debt ledger and recorded TD-001 through TD-003 from the current source scan.
