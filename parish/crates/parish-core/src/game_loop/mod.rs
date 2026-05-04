//! Shared orchestration layer — game-loop functions extracted from all three
//! backends (#696 slices 2 and 3).
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
//! # What is and is not extracted (third slice rationale)
//!
//! Third slice (#696) aimed to extract `handle_movement`, `handle_game_input`,
//! `handle_system_command`, `emit_npc_reactions`, `rebuild_inference`,
//! `do_save_game`, and `do_new_game` from all three runtimes.  After reading
//! the actual call signatures and `AppState` layouts, the following were
//! confirmed non-extractable at this slice without restructuring `AppState`:
//!
//! - **`handle_system_command`**: mode-specific side effects (`Quit` exits the
//!   process/app, `ShowSpinner` drives a backend-specific animation, `ToggleMap`
//!   dumps a text map in CLI vs. emitting a UI event in GUI modes).
//! - **`rebuild_inference`**: depends on server's `BroadcastEventBus` /
//!   `InferenceClient` trait stack vs. Tauri's `app.emit` path.
//! - **`do_save_game` / `do_new_game`**: server uses `spawn_blocking +
//!   Database::open`; CLI uses `Arc<AsyncDatabase>` directly; Tauri uses a
//!   third variant. No shared `SessionStore` trait is in use at these call sites.
//! - **`emit_npc_reactions`**: spawns a background task that needs `Arc::clone`
//!   of the full `AppState`. State fields are `Mutex<T>` inside `Arc<AppState>`,
//!   not individually arc-wrapped, so there is no portable parameter form.
//! - **`handle_movement` / `handle_game_input`**: use `state.transport`,
//!   `state.game_mod`, `state.reaction_templates`, and backend-specific event
//!   patterns; no cost-free extraction exists without extending `GameLoopContext`
//!   significantly.
//!
//! What WAS extracted in slice 3:
//! - [`reactions::is_snippet_injection_char`] — shared injection-validation
//!   logic used by `react_to_message` on every runtime (#687 security parity).
//!
//! # Headless CLI
//!
//! The headless CLI (`parish-cli`) uses a flat `App` struct with bare (non-Mutex)
//! fields, which cannot borrow directly into [`GameLoopContext`].  Migration of
//! `parish-cli` to the shared context is deferred to a future slice — it
//! requires wrapping `App`'s fields in `Arc<Mutex<>>` which is a wider change.
//! In the meantime, `parish-cli` continues to use its own inline implementations.

pub mod context;
pub mod npc_turn;
pub mod reactions;

pub use context::GameLoopContext;
pub use npc_turn::{TurnOutcome, handle_npc_conversation, run_idle_banter, run_npc_turn};
pub use reactions::is_snippet_injection_char;
