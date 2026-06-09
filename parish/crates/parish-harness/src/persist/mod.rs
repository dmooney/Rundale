//! Persistence: the harness's own SQLite DB (schema + sink). Heavy artifacts
//! live on disk; this stores the queryable telemetry.

pub mod queries;
pub mod schema;
pub mod sink;

pub use queries::{
    AbCompare, AxisDelta, AxisScoreDto, FindingDto, RunDetail, RunSummaryDto, TimelinePoint,
    TurnSummaryDto,
};
pub use sink::{Db, RunSummary, TurnRecord, default_db_path};
