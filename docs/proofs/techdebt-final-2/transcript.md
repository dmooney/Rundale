Evidence type: gameplay transcript

## Summary

Final closure of the 206/208 techdebt sweep (a26110a). Audit turned up four loose ends; this bundle covers all of them.

### Changes

**parish-tauri TD-003 (Complexity, P2) — closure decomposition:**
- Extracted 8 helpers from the ~940-line `.setup()` closure into a new `crate::setup` module:
  - `init_screenshot_mode`
  - `bootstrap_inference_provider`
  - `init_inference_queue`
  - `init_persistence`
  - `spawn_event_bus_fanin`
  - `spawn_world_tick`
  - `spawn_inactivity_tick`
  - `spawn_debug_tick`
  - `spawn_autosave_tick`
- `lib.rs` shrinks 2170 -> 1203 lines. Behaviour byte-for-byte identical.
- Mirrors the `parish-server::session::spawn_session_ticks` pattern so the two runtimes are easy to compare side-by-side.

**parish-inference TD-015 (Weak Tests, P3) — Windows `taskkill` coverage:**
- Extracted pure helpers `taskkill_args(pid_arg) -> [&str; 4]` and `pid_string(pid)` from `OllamaProcess::stop`.
- Added 2 cross-platform unit tests pinning the `/F /T /PID <pid>` invariant: `taskkill_args_are_force_tree_kill_with_pid` and `taskkill_args_handle_u32_max_pid`.
- The `Command::new("taskkill")` invocation itself remains platform-locked, but is now a thin shim around tested data — no Command-mock abstraction required.

**parish-server TD-011 (Weak Tests, P1) — silent regression in a26110a:**
- `tests/ws_integration.rs` (added in a26110a) referenced `validate_ws_upgrade` and `WsValidation` symbols that were never actually extracted from `ws_handler`.
- Extracted both now and rewrote `ws_handler` to call through them. 8 integration tests pass.

**apps/ui + parish-core TODO.md bookkeeping (no code change):**
- TD-019 / TD-020 in apps/ui were mis-classified under "Follow-up: deferred" with bodies marked `Fixed`. Moved into `## Done`.
- parish-core Discovery note claimed Follow-up entries that did not exist; rewrote the note.

### Files modified

- `parish/crates/parish-tauri/src/lib.rs`
- `parish/crates/parish-tauri/src/setup.rs` (new file, 963 lines)
- `parish/crates/parish-tauri/TODO.md`
- `parish/crates/parish-inference/src/setup.rs`
- `parish/crates/parish-inference/TODO.md`
- `parish/crates/parish-server/src/ws.rs`
- `parish/apps/ui/TODO.md`
- `parish/crates/parish-core/TODO.md`

### Test output

```
cargo build --workspace
   Finished dev profile in 23.02s

cargo clippy --workspace --all-targets -- -D warnings
   No issues found

cargo test --workspace
   2457 passed, 17 ignored (60 suites, 7.51s)

cargo test -p parish-tauri
   76 passed (6 suites, 0.05s)

cargo test -p parish-inference
   253 passed, 7 ignored (3 suites, 0.94s)
   (includes 2 new taskkill_args tests)

cargo test -p parish-server --test ws_integration
   8 passed (1 suite, 0.01s)

cargo test -p parish-core --test architecture_fitness
   3 passed (1 suite, 0.09s)
```

### TODO reconciliation

Per-file audit script after the fixes:

```
apps/ui/TODO.md                              open=0 followup=0 done=30
crates/parish-cli/TODO.md                    open=0 followup=0 done=19
crates/parish-config/TODO.md                 open=0 followup=0 done=8
crates/parish-core/TODO.md                   open=0 followup=0 done=14
crates/parish-geo-tool/TODO.md               open=0 followup=0 done=11
crates/parish-inference/TODO.md              open=0 followup=0 done=23
crates/parish-input/TODO.md                  open=0 followup=0 done=8
crates/parish-npc-tool/TODO.md               open=0 followup=0 done=15
crates/parish-npc/TODO.md                    open=0 followup=0 done=16
crates/parish-palette/TODO.md                open=0 followup=0 done=7
crates/parish-persistence/TODO.md            open=0 followup=0 done=14
crates/parish-server/TODO.md                 open=0 followup=0 done=20
crates/parish-tauri/TODO.md                  open=0 followup=0 done=5
crates/parish-types/TODO.md                  open=0 followup=0 done=7
crates/parish-world/TODO.md                  open=0 followup=0 done=11
---
Total: open=0 followup=0 done=208
```

208/208. Zero deferred items.

### Behaviour impact

Pure refactor + bookkeeping. No gameplay logic, no UI state, no IPC payload, no observable API behaviour changed. The Tauri `.setup()` closure body was lifted verbatim into named helper functions; the inference `taskkill` invocation lifts an argv literal into a pure function then back to the same call site; the WebSocket validation lifts the same 20-line decision tree out of `ws_handler` so it can be tested directly.

Mode parity (CLAUDE.md rule #2): unaffected. Both `parish-server::session::spawn_session_ticks` and `parish-tauri::setup::spawn_*_tick` continue to exist; this PR only renames the inline Tauri code, not its call surface.

Architecture fitness (rule #1): green. `parish-core/tests/architecture_fitness.rs` passes — no orphaned files, no forbidden runtime imports in backend-agnostic crates, no leaf-crate logic duplicated in `parish-cli`.
