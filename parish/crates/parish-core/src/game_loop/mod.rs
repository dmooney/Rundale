//! Shared orchestration layer — game-loop functions extracted from all three
//! backends (#696 second slice).
//!
//! # Design
//!
//! The three Parish runtimes (axum web server, Tauri desktop, headless CLI)
//! previously duplicated long functions like `run_npc_turn`,
//! `handle_npc_conversation`, and `run_idle_banter` verbatim, with only their
//! event-emission calls differing.  This module resolves the duplication by:
//!
//! 1. Defining a [`GameLoopContext`] borrow struct that carries references to
//!    the shared Tokio-Mutex–wrapped game state that all runtimes need.
//! 2. Exposing free async functions — [`run_npc_turn`], [`handle_npc_conversation`],
//!    [`run_idle_banter`] — that take a [`GameLoopContext`] and a
//!    `&dyn EventEmitter` and operate identically across all runtimes.
//!
//! Each backend constructs a `GameLoopContext` by borrowing its own `AppState`
//! fields, then supplies its backend-specific [`EventEmitter`] implementation.
//!
//! # Architecture gate
//!
//! This module must remain backend-agnostic.  It does **not** import `axum`,
//! `tauri`, or any crate in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.  The
//! `architecture_fitness` test enforces this mechanically.
//!
//! # Headless CLI
//!
//! The headless CLI (`parish-cli`) uses a flat `App` struct with bare (non-Mutex)
//! fields, which cannot borrow directly into [`GameLoopContext`].  Migration of
//! `parish-cli` to the shared context is deferred to a follow-up slice — it
//! requires wrapping `App`'s fields in `Arc<Mutex<>>` which is a wider change.
//! In the meantime, `parish-cli` continues to use its own inline implementations.

pub mod context;
pub mod npc_turn;

pub use context::GameLoopContext;
pub use npc_turn::{TurnOutcome, handle_npc_conversation, run_idle_banter, run_npc_turn};
