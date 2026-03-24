//! Persistence layer — SQLite with write-ahead log.
//!
//! Three layers: real-time journal, periodic snapshots,
//! and named branches (git-like save model).
//! Uses SQLite in WAL mode via rusqlite.

// TODO: Database init and migrations
// TODO: Journal (append-only event log)
// TODO: Snapshot (periodic full-state compaction)
// TODO: Branch management (fork, load, list)
// TODO: Autosave on quit
