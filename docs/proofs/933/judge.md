Verdict: sufficient
Technical debt: clear

PR #933 adds a new `parish-mcp` crate that exposes a Model Context
Protocol stdio server for driving Parish/Rundale (and, via the
`TauriBackend` trait seam, future generic Tauri apps).

The evidence in `evidence.md` covers all four requirements: handshake,
tools/list, tools/call routing, and error-surfacing semantics. The
end-to-end stdio transcript is the strongest signal — it exercises the
full path from JSON-RPC framing through MCP dispatch through the HTTP
backend's `command_to_path` translation, and confirms the surfaced
URL matches the wiring-parity translation rule.

The 25 unit tests are well-targeted (framing, backend, tools, MCP
dispatch) and the architecture-fitness sensors all pass — adding the
crate did not introduce orphan files, runtime-dep leaks into
backend-agnostic crates, or duplicate-module violations.

Debt status:
- The `GenericTauriBackend` is intentionally a stub that returns
  `BackendError::Unimplemented`. This is documented in both the module
  doc-comment and the type doc-comment, has a unit test pinning the
  behaviour, and is wired through the same trait so a real
  WebDriver/`tauri-driver` impl can land without API churn. This is
  forward-looking design, not unfinished work.
- The `agent-check` placeholder-debt scanner finds no leftover
  partial-completion macros or stale "unchanged" comment markers
  anywhere in the changed source files.
- Clippy clean with `-D warnings`.

Sufficient to ship.
