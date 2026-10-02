# limerick-persistence — agent scope

SQLite save/load with WAL journal and branching saves for the Limerick engine. Backend-agnostic leaf crate — manages save files with branching, WAL journal for crash resilience, path resolution for platform user-data directories, snapshot serialization, file locking, and the database schema. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p limerick-persistence                    # unit tests (database, journal, snapshot, lock, paths, picker)
cargo test -p limerick-persistence --test save_lock_processes  # cross-process lock lifecycle
just ios-sim-save-lock                                # same lock tests on the iOS Simulator (macOS + Xcode)
cargo test -p limerick-persistence -- --nocapture     # with stdout for debugging
```

## Local gotchas

- **Leaf-crate dependency rule ([module ownership](../../../docs/agent/engineering-rules.md#module-ownership)).** Depends only on `limerick-types`, `limerick-world`, `limerick-npc`, `rusqlite`, `serde`, `chrono`, `tokio`. Never depend on `limerick-core` or any runtime crate (tauri, axum, engine).
- **Resolve runtime paths from explicit config, not cwd ([runtime paths](../../../docs/agent/persistence-and-session-rules.md#runtime-paths)).** `resolve_user_data_dir(app_name)` checks `LIMERICK_USER_DATA_DIR` first, then platform-native roots. `resolve_project_saves_dir(app_name)` checks `LIMERICK_SAVES_DIR`. Both are called once at startup and stored on `AppState` — never from request handlers.
- **App name fallback chain.** Data-directory app name comes from `ModMeta::app_name()`, falling back to `ModMeta.name`, then `DEFAULT_APP_NAME` (`"Limerick"`).
- **WAL concurrent access.** `Database::open()` enables `PRAGMA journal_mode=WAL` + `PRAGMA synchronous=NORMAL`. `AsyncDatabase` serialises all operations through `Arc<Mutex<Database>>` via `spawn_blocking`.
- **Poison recovery on database mutex.** `lock_recovered()` transparently recovers from a poisoned mutex (issue #82); without it a single panic while holding the lock cascades to every subsequent call.
- **`IntoLimerickDbError` is crate-local.** `limerick-types` dropped its `rusqlite` dependency (issue #699); `database/` uses the local trait for `.db_err()?` shorthand.
- **Atomic sequence assignment.** `append_event` uses a single `INSERT ... SELECT COALESCE(MAX(sequence),0)+1` with a UNIQUE index on `(branch_id, after_snapshot_id, sequence)` to prevent duplicate journal sequences under concurrent appends.
- **Turn journal tables are opaque storage.** `requests` and `transcript_events` hold JSON written and validated by `limerick-core`'s `SqliteTurnJournal`; this crate owns only the schema, `Database::turn_journal_transaction` (one immediate transaction per journal call, rolled back on any error), and ordered reads. Rows are per branch; `transcript_events.sequence` is `AUTOINCREMENT` (save-wide, never reused). `migrate` adds the tables to older saves on open.
- **Compaction scoped to `(branch_id, snapshot_id)`.** `clear_journal()` deletes only events for the exact pair. Lifecycle: save snapshot A → append events → save snapshot B → `clear_journal(A)` → `load_latest_snapshot` returns B.
- **The save lock is a kernel lock (ADR-026).** `SaveFileLock` holds `File::try_lock` (`flock`/`LockFileEx`) on `<save_path>.lock`; the kernel frees it when the owner dies (force-quit, jetsam), so never decide ownership from the PID the file records (diagnostic only). Same-process acquisitions share one open file through a registry, because a second open file's lock conflicts with our own. On Unix the last guard removes the file only if the path still names its file (device + inode); on Windows the file is never removed (no stable file identity in std). Old owner directories at the same path are removed only when their recorded owner is dead. Re-run on the iOS Simulator with `just ios-sim-save-lock`.
- **Unix-only `libc` dependency.** `lock/` uses `libc::kill(pid, 0)` on Unix only to judge old lock directories' owners; keep conditional compilation correct.

## Module map

`lib.rs` crate root + re-exports + `IntoLimerickDbError` + `format_timestamp`, `database/` SQLite schema + `Database` + `AsyncDatabase` + CRUD + turn-journal tables (`turn_journal.rs`), `journal.rs` `WorldEvent` enum + event types + replay, `journal_bridge.rs` `GameEvent`→`WorldEvent` conversion, `snapshot/` `GameSnapshot` + `ClockSnapshot` + `NpcSnapshot` serialization, `paths.rs` `resolve_user_data_dir(app_name)`, `picker.rs` `resolve_project_saves_dir(app_name)` + save slot grid, `lock/` cross-platform `SaveFileLock` (kernel lock on a `.lock` sidecar).
