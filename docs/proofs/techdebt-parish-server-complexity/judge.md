Evidence type: code extraction + test results
Verdict: sufficient
Technical debt: clear

The extracted helpers are pure relocations of inline blocks into named functions.
No behaviour changes: the original statement sequences, error paths, tracing calls,
and return values are preserved verbatim. All 168 unit tests and 63 integration
tests pass with no regressions. Clippy is clean with `-D warnings`.

Each extraction preserves the original function's public API:
- `run_server` pub fn unchanged
- `spawn_session_ticks` fn unchanged
- `purge_expired_disk_sessions` pub fn unchanged
- `load_branch` pub fn unchanged
- `idempotency_middleware` pub fn unchanged

No new feature flags, config values, or public API additions were introduced.
