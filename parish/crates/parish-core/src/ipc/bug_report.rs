//! Bug-report orchestration — re-export shim.
//!
//! The implementation was extracted into the backend-agnostic
//! `parish-diagnostics` crate (alongside the debug-snapshot builders it folds
//! into the issue body). This module preserves the historical
//! `parish_core::ipc::bug_report::...` path for every consumer (`parish-tauri`,
//! `parish-server`, `parish-harness`, the MCP bridge) with zero import changes.
//!
//! The one piece that must live here is the [`WorldSnapshotFields`] impl for
//! [`WorldSnapshot`]: `parish-diagnostics` defines the trait so it can read the
//! scalar world-state fields without depending on `parish-core` (which would be
//! a dependency cycle), and `parish-core` supplies the concrete impl so every
//! existing `BugReportState::from_snapshots(&world_snapshot, …)` call site still
//! compiles unchanged.

pub use parish_diagnostics::bug_report::*;

use crate::ipc::types::WorldSnapshot;

impl parish_diagnostics::bug_report::WorldSnapshotFields for WorldSnapshot {
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
