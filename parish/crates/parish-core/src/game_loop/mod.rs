//! Shared orchestration layer — game-loop functions extracted from all three
//! backends (#696 slices 2-5).
//!
//! # Design
//!
//! The three Parish runtimes (axum web server, Tauri desktop, headless CLI)
//! previously duplicated long functions like `run_npc_turn`,
//! `handle_npc_conversation`, `run_idle_banter`, and `emit_npc_reactions`
//! verbatim, with only their event-emission calls differing.  This module
//! resolves the duplication by:
//!
//! 1. Defining a [`GameLoopContext`] borrow struct that carries references to
//!    the shared Tokio-Mutex–wrapped game state that all runtimes need.
//! 2. Exposing free async functions — [`run_npc_turn`], [`handle_npc_conversation`],
//!    [`run_idle_banter`] — that take a [`GameLoopContext`] and a
//!    `&dyn EventEmitter` and operate identically across all runtimes.
//! 3. Exposing [`emit_npc_reactions`] which fires background NPC reaction tasks
//!    accepting pre-resolved parameters (no `Arc<AppState>` dependency).
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
//! # What is extracted (slice 5 additions)
//!
//! - [`reactions::emit_npc_reactions`] — extracted from server and Tauri by
//!   accepting individual `Arc<Mutex<NpcManager>>` + `Arc<dyn EventEmitter>`
//!   parameters instead of the full `Arc<AppState>`.  Callers pre-resolve the
//!   reaction client, model, and `npc-llm-reactions` feature flag from their
//!   runtime config, then pass them.  This removes the `Arc<AppState>` coupling
//!   that blocked extraction in slice 3.
//!
//! # What remains per-runtime (not extracted, with rationale)
//!
//! - **`handle_system_command`**: mode-specific side effects (`Quit` exits the
//!   process/app, `ShowSpinner` drives a backend-specific animation, `ToggleMap`
//!   dumps a text map in CLI vs. emitting a UI event in GUI modes).  These
//!   require runtime-specific handles (`app.exit(0)`, `event_bus`, `stdout`)
//!   that cannot be represented through the `EventEmitter` trait without adding
//!   a richer side-effect protocol.
//! - **`rebuild_inference`**: depends on per-runtime `AppState` fields
//!   (`worker_handle`, `inference_log`, `inference_client`).  A shared version
//!   would require a new `InferenceManager` trait. Deferred.
//! - **`do_save_game` / `do_new_game`**: server uses `spawn_blocking +
//!   Database::open`; CLI uses `Arc<AsyncDatabase>` directly; Tauri uses a
//!   third variant.  The `SessionStore` trait exists but is not yet wired to
//!   CLI or Tauri's persistence paths.  Deferred.
//! - **`handle_movement` / `handle_game_input`**: already delegate heavily to
//!   `parish_core::game_session::apply_movement`.  The remaining per-runtime
//!   code handles travel-encounter enrichment with different lock patterns and
//!   emit patterns.  Could be extracted in a future slice with a richer
//!   `MovementContext` struct.
//!
//! # Headless CLI
//!
//! The headless CLI (`parish-cli`) uses a flat `App` struct with bare (non-Mutex)
//! fields, which cannot borrow directly into [`GameLoopContext`].  CLI wiring
//! of [`emit_npc_reactions`] is done by pre-extracting the reaction client from
//! `App` before calling the shared function — no `Arc<Mutex>` migration needed.
//! Full migration of `parish-cli` to the shared `GameLoopContext` (requiring
//! `Arc<Mutex<T>>` for each field) is deferred to a dedicated slice because
//! it would touch hundreds of call sites throughout the CLI codebase.

pub mod context;
pub mod input;
pub mod movement;
pub mod npc_turn;
pub mod reactions;

pub use context::GameLoopContext;
pub use input::{handle_game_input, handle_look};
pub use movement::handle_movement;
pub use npc_turn::{TurnOutcome, handle_npc_conversation, run_idle_banter, run_npc_turn};
pub use reactions::{PersistReactionFn, emit_npc_reactions, is_snippet_injection_char};
