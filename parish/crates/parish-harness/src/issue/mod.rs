//! Issue filing for discrete harness findings.
//!
//! Wraps `parish_core::ipc::create_bug_report` to file deduped GitHub issues
//! for each finding discovered during a run. Dedup is by signature across all
//! runs — the same class of defect seen in 100 runs creates exactly one issue.

pub mod filer;

pub use filer::IssueFiler;
