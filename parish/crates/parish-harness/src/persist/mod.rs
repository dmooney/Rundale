//! Persistence: the harness's own SQLite DB (schema + sink). Heavy artifacts
//! live on disk; this stores the queryable telemetry.

pub mod schema;
pub mod sink;

pub use sink::{Db, RunSummary, TurnRecord, default_db_path};
