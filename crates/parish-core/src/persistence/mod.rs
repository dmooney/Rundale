//! Persistence layer — SQLite with write-ahead log.
//!
//! Three layers: real-time journal, periodic snapshots,
//! and named branches (git-like save model).
//! Uses SQLite in WAL mode via rusqlite.
//!
//! # Architecture
//!
//! - **Journal**: Every state mutation is appended as a [`WorldEvent`].
//!   This is the crash recovery net.
//! - **Snapshot**: Periodic full compaction of [`GameSnapshot`] to the
//!   database. This is the "clean" save point.
//! - **Branch**: A named reference (snapshot + journal tail). Fork copies
//!   the current snapshot; load switches to a different branch.
//!
//! All rusqlite calls are wrapped in `tokio::task::spawn_blocking` via
//! [`AsyncDatabase`] to avoid blocking the async runtime.

pub mod database;
pub mod journal;
pub mod journal_bridge;
pub mod picker;
pub mod snapshot;

pub use database::{AsyncDatabase, BranchInfo, Database, SnapshotInfo};
pub use journal::{WorldEvent, replay_journal};
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
