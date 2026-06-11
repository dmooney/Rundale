//! Parish chronicle writers — the on-disk record-keeping subsystem.
//!
//! This crate owns the three `GameEvent`-bus subscribers that persist the
//! player-visible history of a parish to disk:
//!
//! * [`character_log`] — per-NPC and player markdown logs (profile section +
//!   append-only journal) under `<user-data-dir>/<app>/logs/<branch>/`.
//! * [`location_log`] — per-location markdown logs, mirroring the character
//!   log structure and reusing its profile/journal helpers.
//! * [`chat_transcript`] — a persistent JSONL transcript of the player-visible
//!   chat stream, correlated to inference-log lines via `parish.request_id`.
//!
//! ## Why a separate crate
//!
//! These writers are pure orchestration leaves: they depend only on the
//! backend-agnostic logic crates (`parish-npc`, `parish-world`, `parish-types`,
//! `parish-persistence`, `parish-inference`) and never on a runtime backend
//! (`tauri` / `axum` / `tower*`). Extracting them out of `parish-core` keeps
//! the composition crate thin and lets the architecture-fitness sensor cover
//! them directly (rule #1, #12).
//!
//! `parish-core` re-exports all three modules at their historical
//! `parish_core::{character_log, location_log, chat_transcript}` paths, so
//! every entry-point call site (server, Tauri, engine) compiles unchanged.
//! The branch-switch subscriber-rebind call sites stay in their crates and
//! reach the managers through those re-exports.

pub mod character_log;
pub mod chat_transcript;
pub mod location_log;
