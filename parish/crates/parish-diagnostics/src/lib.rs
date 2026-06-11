//! Parish diagnostics — backend-agnostic introspection and bug-report support.
//!
//! Two responsibilities, both extracted from `parish-core` so the desktop,
//! web-server, and headless entry points share one implementation:
//!
//! - [`debug_snapshot`] — a serializable point-in-time aggregate of all
//!   inspectable game state ([`debug_snapshot::DebugSnapshot`]), built from live
//!   world/NPC/inference references. Consumed by the TUI debug panel and the
//!   Tauri/Svelte debug panel via IPC.
//! - [`bug_report`] — turns an in-app (or MCP-driven) bug report into a
//!   well-formed GitHub issue (or an offline disk bundle in dry-run mode),
//!   folding a world snapshot + a [`debug_snapshot::DebugSnapshot`] into the
//!   issue body.
//!
//! `parish-core` re-exports both modules under their historical paths
//! (`parish_core::debug_snapshot::*` and `parish_core::ipc::bug_report::*`) so
//! every existing consumer compiles without an import change.

pub mod bug_report;
pub mod debug_snapshot;
