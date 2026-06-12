# parish-persistence — agent scope

SQLite save/load with WAL journal and branching saves for the Parish engine. Backend-agnostic leaf crate — manages save files with branching, WAL journal for crash resilience, path resolution for platform user-data directories, snapshot serialization, file locking, and the database schema. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-persistence                    # unit tests (database, journal, snapshot, lock, paths, picker)
cargo test -p parish-persistence -- --nocapture     # with stdout for debugging
```

## Local gotchas

- **Leaf-crate dependency rule (rule #1).** Depends only on `parish-types`, `parish-world`, `parish-npc`, `rusqlite`, `serde`, `chrono`, `tokio`. Never depend on `parish-core` or any runtime crate (tauri, axum, engine).
- **Resolve runtime paths from explicit config, not cwd (rule #9).** `resolve_user_data_dir(app_name)` checks `PARISH_USER_DATA_DIR` first, then platform-native roots. `resolve_project_saves_dir(app_name)` checks `PARISH_SAVES_DIR`. Both are called once at startup and stored on `AppState` — never from request handlers.
- **App name fallback chain.** Data-directory app name comes from `ModMeta::app_name()`, falling back to `ModMeta.name`, then `DEFAULT_APP_NAME` (`"Parish"`).
- **WAL concurrent access.** `Database::open()` enables `PRAGMA journal_mode=WAL` + `PRAGMA synchronous=NORMAL`. `AsyncDatabase` serialises all operations through `Arc<Mutex<Database>>` via `spawn_blocking`.
- **Poison recovery on database mutex.** `lock_recovered()` transparently recovers from a poisoned mutex (issue #82); without it a single panic while holding the lock cascades to every subsequent call.
- **`IntoParishDbError` is crate-local.** `parish-types` dropped its `rusqlite` dependency (issue #699); `database/` uses the local trait for `.db_err()?` shorthand.
- **Atomic sequence assignment.** `append_event` uses a single `INSERT ... SELECT COALESCE(MAX(sequence),0)+1` with a UNIQUE index on `(branch_id, after_snapshot_id, sequence)` to prevent duplicate journal sequences under concurrent appends.
- **Compaction scoped to `(branch_id, snapshot_id)`.** `clear_journal()` deletes only events for the exact pair. Lifecycle: save snapshot A → append events → save snapshot B → `clear_journal(A)` → `load_latest_snapshot` returns B.
- **Lock sidecar files are reference-counted.** `SaveFileLock` writes a PID to `<save_path>.lock`; removed on `Drop` only when the last `Arc` reference drops.
- **Unix-only `libc` dependency.** `lock/` uses `libc::getpid()` on Unix; Windows uses `std::process::id()` — keep conditional compilation correct.

## Module map

`lib.rs` crate root + re-exports + `IntoParishDbError` + `format_timestamp`, `database/` SQLite schema + `Database` + `AsyncDatabase` + CRUD, `journal.rs` `WorldEvent` enum + event types + replay, `journal_bridge.rs` `GameEvent`→`WorldEvent` conversion, `snapshot/` `GameSnapshot` + `ClockSnapshot` + `NpcSnapshot` serialization, `paths.rs` `resolve_user_data_dir(app_name)`, `picker.rs` `resolve_project_saves_dir(app_name)` + save slot grid, `lock/` cross-platform `SaveFileLock` with sidecar PID files.
