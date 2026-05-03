//! Persistence layer — SQLite with write-ahead log.
//!
//! Three layers: real-time journal, periodic snapshots,
//! and named branches (git-like save model).
//! Uses SQLite in WAL mode via rusqlite.

pub mod database;
pub mod journal;
pub mod journal_bridge;
pub mod lock;
pub mod picker;
pub mod snapshot;

/// Extension trait for converting `rusqlite::Error` into
/// [`parish_types::ParishError::Database`].
///
/// `parish-types` no longer depends on `rusqlite` (issue #699). This crate-local
/// trait provides the ergonomic `.db_err()?` shorthand so that `database.rs`
/// does not need to spell out `.map_err(|e| ParishError::Database(e.to_string()))?`
/// at every call site.
///
/// Using a local trait satisfies the orphan rule: `IntoParishDbError` is defined
/// in this crate, so the `impl` is allowed even though both `rusqlite::Error`
/// and `ParishError` are external.
pub(crate) trait IntoParishDbError<T> {
    fn db_err(self) -> Result<T, parish_types::ParishError>;
}

impl<T> IntoParishDbError<T> for Result<T, rusqlite::Error> {
    fn db_err(self) -> Result<T, parish_types::ParishError> {
        self.map_err(|e| parish_types::ParishError::Database(e.to_string()))
    }
}

pub use database::{AsyncDatabase, BranchInfo, Database, SnapshotInfo};
pub use journal::{WorldEvent, replay_journal};
pub use lock::SaveFileLock;
pub use snapshot::{ClockSnapshot, GameSnapshot, NpcSnapshot};

/// Formats an RFC 3339 timestamp into a short, human-readable local-time string.
///
/// Example: `"2026-03-24T16:05:33.123+00:00"` → `"24 Mar 4:05 PM"`.
/// Falls back to the raw string if parsing fails.
pub fn format_timestamp(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|dt| {
            let local = dt.with_timezone(&chrono::Local);
            local.format("%-d %b %-I:%M %p").to_string()
        })
        .unwrap_or_else(|_| rfc3339.to_string())
}
