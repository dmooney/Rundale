//! Schema setup, WAL configuration, and migration helpers.

use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, params};

use crate::IntoParishDbError as _;
use parish_types::ParishError;

/// Acquires a lock on `mutex`, recovering transparently from poisoning.
///
/// If a previous thread panicked while holding the database lock,
/// `Mutex::lock()` will return a [`PoisonError`]. Without recovery, every
/// subsequent call would cascade a single failure into a total application
/// crash (issue #82). SQLite writes are transactional, so the connection
/// itself remains in a consistent state after a panic; we simply log a
/// warning and return the underlying guard so database access continues
/// to work.
pub(super) fn lock_recovered<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("database lock was poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

/// Creates tables if they don't exist and ensures the "main" branch exists.
pub(super) fn migrate(conn: &Connection) -> Result<(), ParishError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS branches (
            id INTEGER PRIMARY KEY,
            name TEXT UNIQUE NOT NULL,
            created_at TEXT NOT NULL,
            parent_branch_id INTEGER REFERENCES branches(id)
        );

        CREATE TABLE IF NOT EXISTS snapshots (
            id INTEGER PRIMARY KEY,
            branch_id INTEGER NOT NULL REFERENCES branches(id),
            game_time TEXT NOT NULL,
            real_time TEXT NOT NULL,
            world_state TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS journal_events (
            id INTEGER PRIMARY KEY,
            branch_id INTEGER NOT NULL,
            sequence INTEGER NOT NULL,
            after_snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
            event_type TEXT NOT NULL,
            event_data TEXT NOT NULL,
            game_time TEXT NOT NULL
        );

        DROP INDEX IF EXISTS idx_journal_branch_snap_seq;
        CREATE UNIQUE INDEX idx_journal_branch_snap_seq
            ON journal_events(branch_id, after_snapshot_id, sequence);",
    )
    .db_err()?;

    migrate_branch_parent_fk(conn)?;

    // Ensure the "main" branch exists
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM branches WHERE name = 'main'",
            [],
            |row| row.get(0),
        )
        .db_err()?;
    if !exists {
        conn.execute(
            "INSERT INTO branches (name, created_at, parent_branch_id) VALUES (?1, ?2, NULL)",
            params!["main", chrono::Utc::now().to_rfc3339()],
        )
        .db_err()?;
    }

    Ok(())
}

pub(super) fn migrate_branch_parent_fk(conn: &Connection) -> Result<(), ParishError> {
    if branch_parent_fk_present(conn)? {
        return Ok(());
    }

    let migration = conn.execute_batch(
        "PRAGMA foreign_keys=OFF;
             BEGIN IMMEDIATE;
             DROP TABLE IF EXISTS branches_new;
             CREATE TABLE branches_new (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                created_at TEXT NOT NULL,
                parent_branch_id INTEGER REFERENCES branches_new(id)
             );
             INSERT INTO branches_new (id, name, created_at, parent_branch_id)
             SELECT child.id,
                    child.name,
                    child.created_at,
                    CASE
                        WHEN child.parent_branch_id IS NULL THEN NULL
                        WHEN EXISTS (
                            SELECT 1 FROM branches AS parent
                            WHERE parent.id = child.parent_branch_id
                        ) THEN child.parent_branch_id
                        ELSE NULL
                    END
             FROM branches AS child;
             DROP TABLE branches;
             ALTER TABLE branches_new RENAME TO branches;
             COMMIT;",
    );

    if let Err(err) = migration {
        let _ = conn.execute_batch("ROLLBACK;");
        let _ = conn.execute_batch("PRAGMA foreign_keys=ON;");
        return Err(err).db_err();
    }

    conn.execute_batch("PRAGMA foreign_keys=ON;").db_err()?;

    Ok(())
}

pub(super) fn branch_parent_fk_present(conn: &Connection) -> Result<bool, ParishError> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_list(branches)").db_err()?;
    let mut rows = stmt.query([]).db_err()?;
    while let Some(row) = rows.next().db_err()? {
        let from: String = row.get(3).db_err()?;
        let table: String = row.get(2).db_err()?;
        if from == "parent_branch_id" && table == "branches" {
            return Ok(true);
        }
    }
    Ok(false)
}
