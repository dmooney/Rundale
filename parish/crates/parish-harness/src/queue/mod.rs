//! Run queue — enqueue configs, claim jobs, mark done/failed.
//!
//! A thin layer on top of the `queue` table in `harness.db`. The worker
//! subcommand drives this in a polling loop; each worker atomically claims one
//! pending job at a time.
//!
//! The [`QueueBackend`] trait abstracts storage so a Postgres implementation
//! can slot in for multi-machine workers.

pub mod store;

pub use store::{QueueBackend, QueueCounts, QueueJob, QueueRow, QueueStore};
