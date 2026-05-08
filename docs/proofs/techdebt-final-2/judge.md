Verdict: sufficient
Technical debt: clear

This PR closes the final 2/208 deferred items from the techdebt sweep
(a26110a) plus two follow-on bugs surfaced during the audit:

1. **parish-tauri TD-003** — extracted ~940-line `.setup()` closure into 8
   named helpers in a new `setup` module. `lib.rs` shrinks 2170 -> 1203
   lines; behaviour byte-for-byte identical. Pattern mirrors the
   `parish-server::session::spawn_session_ticks` decomposition that landed
   in PR #925 (TD-006), so the two runtimes are now structurally symmetric.

2. **parish-inference TD-015** — extracted pure `taskkill_args` and
   `pid_string` helpers from `OllamaProcess::stop`, with 2 cross-platform
   unit tests pinning the `/F /T /PID <pid>` invariant. The platform-locked
   `Command::new("taskkill")` invocation itself stays as a 5-line shim
   around tested data, so no Command-mock abstraction was required.

3. **parish-server TD-011** — discovered while running CI: the
   `tests/ws_integration.rs` file added in a26110a referenced
   `validate_ws_upgrade` / `WsValidation` symbols that were never actually
   extracted from `ws_handler`. Extracted now; 8 integration tests pass.

4. **apps/ui + parish-core TODO.md bookkeeping** — TD-019/TD-020 in apps/ui
   were mis-classified under "Follow-up: deferred" with bodies marked
   "Fixed"; parish-core Discovery note claimed Follow-up entries that did
   not exist. Reconciled in-place.

Evidence: workspace clippy clean with `-D warnings`; full `cargo test
--workspace` reports 2457 passed, 17 ignored across 60 suites; 8/8
ws_integration tests; 76/76 parish-tauri; 253/253 parish-inference;
3/3 parish-core architecture_fitness.

Per-file TODO audit after the fixes shows 208 done, 0 open, 0 deferred
follow-up across all 15 TODO.md files. The user's `feedback_no_deferral`
memory is upheld: zero items left as "out of scope" or "net-negative".
