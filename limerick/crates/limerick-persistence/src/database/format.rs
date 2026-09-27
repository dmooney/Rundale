//! Save format version and the read-only check that runs before a save is
//! opened for writing.
//!
//! A save records its format version in SQLite's `PRAGMA user_version`.
//! Every change to what a save stores (tables, columns, or the JSON shape of
//! a snapshot, world event, request, or transcript event) bumps
//! [`SAVE_FORMAT_VERSION`]; `limerick-core`'s `save_format_shape` test fails
//! when the shape changes without a bump. Formats:
//!
//! | Version | Written by | Stored |
//! |---------|------------|--------|
//! | 1 | before #2037 | branches, snapshots, world-event journal; no marker |
//! | 2 | #2037 | adds `requests` and `transcript_events`; no marker |
//! | 3 | #2038 | stamps `user_version`; snapshots record their content identity |
//!
//! Unmarked saves are detected as version 1 or 2 by their tables. Opening an
//! older save migrates it forward and stamps the current version. A save
//! stamped with a newer version than this build knows opens unchanged (its
//! stamp is never lowered) as long as this build can read its authoritative
//! state: forward compatibility is judged by what can be read, not by the
//! number (ADR-025 §4).
//!
//! [`inspect`] is the only gate. It reads the file without writing to it
//! (see `read_only_connection`) and refuses it
//! with [`LimerickError::SaveIncompatible`] only when authoritative state
//! cannot be read: the file is not a save database, a table this build reads
//! lacks a column, a branch's latest snapshot or the world events replayed
//! over it do not parse, or a turn request does not parse. Because nothing
//! has written to the file at that point, a refused save stays
//! byte-identical. Transcript events are presentation, not authoritative
//! state: an event of a kind this build does not know never blocks opening.

use std::path::Path;

use rusqlite::config::DbConfig;
use rusqlite::{Connection, ErrorCode, OpenFlags};

use super::{branches, journal};
use crate::IntoLimerickDbError as _;
use limerick_types::{ContentIdentity, LimerickError};

/// The save format this build writes.
pub const SAVE_FORMAT_VERSION: u32 = 3;

/// Tables this build reads and the columns it reads from each.
const TABLES: &[(&str, &[&str])] = &[
    (
        "branches",
        &["id", "name", "created_at", "parent_branch_id"],
    ),
    (
        "snapshots",
        &["id", "branch_id", "game_time", "real_time", "world_state"],
    ),
    (
        "journal_events",
        &[
            "id",
            "branch_id",
            "sequence",
            "after_snapshot_id",
            "event_type",
            "event_data",
            "game_time",
        ],
    ),
    (
        "requests",
        &[
            "request_id",
            "branch_id",
            "phase",
            "is_open",
            "committed_revision",
            "record",
        ],
    ),
    (
        "transcript_events",
        &[
            "sequence",
            "event_id",
            "branch_id",
            "request_id",
            "kind",
            "event",
        ],
    ),
];

/// Tables of format 1. Only `branches` identifies a save: a migration may
/// still add the others to a save from before they existed.
const BASE_TABLES: &[&str] = &["branches", "snapshots", "journal_events"];

/// Tables added in format 2.
const TURN_JOURNAL_TABLES: &[&str] = &["requests", "transcript_events"];

/// What [`inspect`] learned about a readable save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveInspection {
    /// The save's format: its stamp, or the detected format of an unmarked
    /// save; `0` for a new or empty file.
    pub format_version: u32,
    /// The value of `PRAGMA user_version` (`0` when unmarked).
    pub stamped_version: u32,
    /// Every branch, with the content identity of its latest snapshot.
    pub branches: Vec<InspectedBranch>,
}

impl SaveInspection {
    /// Whether opening must migrate the schema and stamp the current format.
    pub fn needs_migration(&self) -> bool {
        self.stamped_version < SAVE_FORMAT_VERSION
    }
}

/// One branch of an inspected save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedBranch {
    /// Branch row id.
    pub id: i64,
    /// Branch name.
    pub name: String,
    /// Content the branch's latest snapshot was captured against, when it
    /// recorded one (format 3 and later).
    pub content: Option<ContentIdentity>,
}

/// Reads `path` without writing to it and refuses it when its authoritative
/// state cannot be read. Request records are checked to be JSON; see
/// [`inspect_with`] to check their type.
pub fn inspect(path: &Path) -> Result<SaveInspection, LimerickError> {
    inspect_with(path, &|record| {
        serde_json::from_str::<serde_json::Value>(record)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

/// [`inspect`], checking every turn request record with `read_request`,
/// which returns why a record cannot be read.
pub fn inspect_with(
    path: &Path,
    read_request: &dyn Fn(&str) -> Result<(), String>,
) -> Result<SaveInspection, LimerickError> {
    let is_empty = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len() == 0,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(error.into()),
    };
    if is_empty {
        return Ok(SaveInspection {
            format_version: 0,
            stamped_version: 0,
            branches: Vec::new(),
        });
    }
    let conn = read_only_connection(path)?;
    inspect_connection(&conn, read_request)
}

/// A connection that never writes to the save file.
///
/// `SQLITE_OPEN_READ_ONLY` cannot open a WAL-mode save whose `-shm` file
/// does not exist yet (the normal state of a closed save), so the file is
/// opened read-write without `CREATE`, with `query_only` set so no statement
/// can write, and with the checkpoint on close disabled so closing never
/// copies WAL frames into the file. Only the `-wal`/`-shm` sidecars may be
/// created.
fn read_only_connection(path: &Path) -> Result<Connection, LimerickError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(unreadable_or_db)?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .db_err()?;
    conn.pragma_update(None, "query_only", true).db_err()?;
    Ok(conn)
}

/// The checks of [`inspect_with`] over an open connection.
fn inspect_connection(
    conn: &Connection,
    read_request: &dyn Fn(&str) -> Result<(), String>,
) -> Result<SaveInspection, LimerickError> {
    let stamped_version: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(unreadable_or_db)?;
    let tables = table_names(conn)?;
    let has = |table: &str| tables.iter().any(|name| name == table);

    if tables.is_empty() {
        return Ok(SaveInspection {
            format_version: stamped_version,
            stamped_version,
            branches: Vec::new(),
        });
    }
    if !has("branches") {
        return Err(refuse(
            "it is not a save database (no `branches` table)".into(),
        ));
    }
    let has_turn_journal = TURN_JOURNAL_TABLES.iter().all(|table| has(table));
    if stamped_version >= SAVE_FORMAT_VERSION
        && let Some(missing) = BASE_TABLES
            .iter()
            .chain(TURN_JOURNAL_TABLES)
            .find(|table| !has(table))
    {
        return Err(refuse(format!(
            "it is stamped format {stamped_version} but has no `{missing}` table"
        )));
    }
    for (table, columns) in TABLES {
        if !has(table) {
            continue;
        }
        let present = column_names(conn, table)?;
        if let Some(missing) = columns
            .iter()
            .find(|column| !present.iter().any(|name| name == *column))
        {
            return Err(refuse(format!("table `{table}` has no `{missing}` column")));
        }
    }

    let format_version = match stamped_version {
        0 if has_turn_journal => 2,
        0 => 1,
        stamped => stamped,
    };

    let mut inspected = Vec::new();
    let has_state = has("snapshots") && has("journal_events");
    for branch in branches::list_branches(conn)? {
        if !has_state {
            inspected.push(InspectedBranch {
                id: branch.id,
                name: branch.name,
                content: None,
            });
            continue;
        }
        let recovery = match journal::load_recovery_data(conn, branch.id) {
            Ok(recovery) => recovery,
            Err(LimerickError::Serialization(error)) => {
                return Err(refuse(format!(
                    "the saved state of branch '{}' cannot be read: {error}",
                    branch.name
                )));
            }
            Err(error) => return Err(error),
        };
        inspected.push(InspectedBranch {
            id: branch.id,
            name: branch.name,
            content: recovery.and_then(|recovery| recovery.snapshot.content),
        });
    }

    if has("requests") {
        let mut stmt = conn
            .prepare("SELECT request_id, record FROM requests ORDER BY rowid")
            .db_err()?;
        let mut rows = stmt.query([]).db_err()?;
        while let Some(row) = rows.next().db_err()? {
            let request_id: String = row.get(0).db_err()?;
            let record: String = row.get(1).db_err()?;
            read_request(&record).map_err(|error| {
                refuse(format!("turn request {request_id} cannot be read: {error}"))
            })?;
        }
    }

    Ok(SaveInspection {
        format_version,
        stamped_version,
        branches: inspected,
    })
}

fn table_names(conn: &Connection) -> Result<Vec<String>, LimerickError> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )
        .map_err(unreadable_or_db)?;
    stmt.query_map([], |row| row.get(0))
        .map_err(unreadable_or_db)?
        .collect::<Result<_, _>>()
        .map_err(unreadable_or_db)
}

fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, LimerickError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .db_err()?;
    stmt.query_map([], |row| row.get(1))
        .db_err()?
        .collect::<Result<_, _>>()
        .db_err()
}

fn refuse(reason: String) -> LimerickError {
    LimerickError::SaveIncompatible(reason)
}

/// A file that is not a SQLite database, or a corrupt one, is refused; any
/// other SQLite error (busy, I/O) is an ordinary database error.
fn unreadable_or_db(error: rusqlite::Error) -> LimerickError {
    match error.sqlite_error_code() {
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => {
            refuse(format!("it is not a readable save database ({error})"))
        }
        _ => LimerickError::Database(error.to_string()),
    }
}

/// Stamps the current format on a save that was just migrated to it.
pub(super) fn stamp(conn: &Connection) -> Result<(), LimerickError> {
    conn.pragma_update(None, "user_version", SAVE_FORMAT_VERSION)
        .db_err()
}
