//! Persistence layer — SQLite with write-ahead log.
//!
//! Three layers: real-time journal, periodic snapshots,
//! and named branches (git-like save model).
//! Uses SQLite in WAL mode via rusqlite.

pub mod active_identity;
pub mod database;
pub mod journal;
pub mod journal_bridge;
pub mod lock;
pub mod paths;
pub mod picker;
pub mod snapshot;

/// Extension trait for converting `rusqlite::Error` into
/// [`limerick_types::LimerickError::Database`].
///
/// `limerick-types` no longer depends on `rusqlite` (issue #699). This crate-local
/// trait provides the ergonomic `.db_err()?` shorthand so that `database.rs`
/// does not need to spell out `.map_err(|e| LimerickError::Database(e.to_string()))?`
/// at every call site.
///
/// Using a local trait satisfies the orphan rule: `IntoLimerickDbError` is defined
/// in this crate, so the `impl` is allowed even though both `rusqlite::Error`
/// and `LimerickError` are external.
pub(crate) trait IntoLimerickDbError<T> {
    fn db_err(self) -> Result<T, limerick_types::LimerickError>;
}

impl<T> IntoLimerickDbError<T> for Result<T, rusqlite::Error> {
    fn db_err(self) -> Result<T, limerick_types::LimerickError> {
        self.map_err(|e| limerick_types::LimerickError::Database(e.to_string()))
    }
}

pub use active_identity::{
    ActiveSaveIdentity, read_active_save_identity, read_active_save_identity_candidate,
    write_active_save_identity,
};
pub use database::{AsyncDatabase, BranchInfo, Database, RecoveryData, SnapshotInfo};
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
