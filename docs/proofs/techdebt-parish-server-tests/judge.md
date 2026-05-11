Evidence type: test-output + clippy-output

## Verdict: sufficient

10 new tests exercise the four previously-untested handlers and middleware:
- TD-013: 2 tests (happy path + missing extension)
- TD-014: 2 tests (format + counter value)
- TD-015: 3 tests (no auth, extension path, cookie fallback)
- TD-016: 3 tests (within quota, exceeds quota, loopback bypass)

All 236 tests pass (173 unit + 63 integration across 15 suites). Clippy is
clean with `-D warnings`. The TODO.md has been updated to mark all four items
as done with a progress log entry.

## Technical debt: clear

Each test covers a single handler or middleware in isolation using the
established `tower::ServiceExt::oneshot` pattern for router-level tests and
direct function calls for pure-unit tests. No test infrastructure changes or
shared-state modifications were needed. The existing `admission_control.rs`
helper patterns (`make_global_state`, `default_game_config`) were followed for
consistency.
