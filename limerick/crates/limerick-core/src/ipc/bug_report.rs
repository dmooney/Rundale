//! Bug-report orchestration — re-export shim.
//!
//! The implementation was extracted into the backend-agnostic
//! `limerick-diagnostics` crate (alongside the debug-snapshot builders it folds
//! into the issue body). This module preserves the historical
//! `limerick_core::ipc::bug_report::...` path for every consumer (`limerick-tauri`,
//! `limerick-server`, `limerick-harness`, the MCP bridge) with zero import changes.
//!
//! The one piece that must live here is the [`WorldSnapshotFields`] impl for
//! [`WorldSnapshot`]: `limerick-diagnostics` defines the trait so it can read the
//! scalar world-state fields without depending on `limerick-core` (which would be
//! a dependency cycle), and `limerick-core` supplies the concrete impl so every
//! existing `BugReportState::from_snapshots(&world_snapshot, …)` call site still
//! compiles unchanged.

pub use limerick_diagnostics::bug_report::*;

use crate::ipc::types::WorldSnapshot;

impl limerick_diagnostics::bug_report::WorldSnapshotFields for WorldSnapshot {
    fn location_name(&self) -> &str {
        &self.location_name
    }
    fn time_label(&self) -> &str {
        &self.time_label
    }
    fn hour(&self) -> u8 {
        self.hour
    }
    fn minute(&self) -> u8 {
        self.minute
    }
    fn day_of_week(&self) -> &str {
        &self.day_of_week
    }
    fn weather(&self) -> &str {
        &self.weather
    }
    fn season(&self) -> &str {
        &self.season
    }
    fn festival(&self) -> Option<&str> {
        self.festival.as_deref()
    }
    fn paused(&self) -> bool {
        self.paused
    }
}
