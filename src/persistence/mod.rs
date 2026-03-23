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
pub mod snapshot;

pub use database::{AsyncDatabase, BranchInfo, Database, SnapshotInfo};
pub use journal::{WorldEvent, replay_journal};
pub use snapshot::{ClockSnapshot, GameSnapshot, NpcSnapshot};
