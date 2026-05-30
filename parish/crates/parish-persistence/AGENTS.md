# parish-persistence — agent scope

SQLite save/load with WAL journal and branching saves for the Parish engine. Backend-agnostic leaf crate — manages all persistent state: save files with branching (multiple save branches), WAL journal for crash resilience, path resolution for platform user-data directories, snapshot serialization, file locking, and the database schema. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-persistence                    # unit tests (database, journal, snapshot, lock, paths, picker)
cargo test -p parish-persistence -- --nocapture     # with stdout for debugging
```

## Local gotchas

- **Leaf-crate dependency rule.** Depends only on `parish-types`, `parish-world`, `parish-npc`, `rusqlite`, `serde`, `chrono`, `tokio`. Never take a dependency on `parish-core` or any runtime crate (tauri, axum, engine).
- **Resolve runtime paths from explicit config, not cwd (rule #9).** `resolve_user_data_dir(app_name)` checks `PARISH_USER_DATA_DIR` env var first, then platform-native roots. `resolve_project_saves_dir(app_name)` checks `PARISH_SAVES_DIR` env var. Both are called once at startup and stored on `AppState` — never from request handlers.
- **App name fallback chain.** The data-directory app name comes from `ModMeta::app_name()`. If that's absent it falls back to `ModMeta.name`, then `DEFAULT_APP_NAME` (`"Parish"`).
- **WAL journal mode requires careful concurrent access.** `Database::open()` enables `PRAGMA journal_mode=WAL` + `PRAGMA synchronous=NORMAL`. WAL permits concurrent reads during writes, but `AsyncDatabase` serialises all operations through `Arc<Mutex<Database>>` via `run_blocking` / `spawn_blocking`.
- **Poison recovery on database mutex.** `lock_recovered()` wraps `Mutex::lock()` to transparently recover from a poisoned mutex (issue #82). Without this, a single thread panic while holding the database lock would cascade into every subsequent call panicking.
- **`IntoParishDbError` is crate-local.** `parish-types` dropped its `rusqlite` dependency (issue #699), so `database.rs` uses the local `IntoParishDbError` trait for `.db_err()?` shorthand. This satisfies the orphan rule while keeping ergonomic error conversion.
- **Atomic sequence assignment prevents duplicate journal sequences.** `append_event` uses a single `INSERT ... SELECT COALESCE(MAX(sequence),0)+1` statement with a UNIQUE index on `(branch_id, after_snapshot_id, sequence)` as a second line of defence. Concurrent appends produce correct, non-overlapping sequences.
- **Compaction must be scoped to `(branch_id, snapshot_id)`.** `clear_journal()` deletes only events for the exact pair. Events tied to a different snapshot or a different branch on the same database survive the prune. The compaction lifecycle: save snapshot A → append events → save snapshot B → clear_journal(A) → load_latest_snapshot returns B.
- **Lock sidecar files are reference-counted.** `SaveFileLock` writes a PID to `<save_path>.lock` and tracks live owners via `Arc`. The lock file is removed on `Drop` only when the last reference drops.
- **Unix-only `libc` dependency.** `lock.rs` uses `libc::getpid()` on Unix targets. Windows uses `std::process::id()` — keep conditional compilation correct when adding new locking logic.

## Module map

`lib.rs` crate root + re-exports + `IntoParishDbError` trait + `format_timestamp` helper, `database.rs` SQLite schema + `Database` + `AsyncDatabase` + CRUD, `journal.rs` `WorldEvent` enum + event types + replay, `journal_bridge.rs` `GameEvent`→`WorldEvent` conversion for the event bus, `snapshot.rs` `GameSnapshot` + `ClockSnapshot` + `NpcSnapshot` serialization, `paths.rs` `resolve_user_data_dir(app_name)`, `picker.rs` `resolve_project_saves_dir(app_name)` + save slot grid, `lock.rs` cross-platform `SaveFileLock` with sidecar PID files.
