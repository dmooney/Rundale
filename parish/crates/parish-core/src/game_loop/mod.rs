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
//! # Extraction history — what was extracted and what remains
//!
//! ## Slices 1–5 (#696)
//!
//! Extracted: `run_npc_turn`, `handle_npc_conversation`, `run_idle_banter`,
//! `handle_game_input`, `handle_movement`, `emit_npc_reactions`,
//! `is_snippet_injection_char`.  Server and Tauri delegate to these via
//! `GameLoopContext`; the headless CLI was deferred (see below).
//!
//! ## Slice 6 (#696) — this slice
//!
//! **Extracted into [`inference`]:**
//! - [`rebuild_inference_worker`] — abort old worker, build new `AnyClient`,
//!   spawn new worker, install queue.  Server and Tauri delegate to this via
//!   [`InferenceSlots`]; each runtime still handles backend-specific side effects
//!   (server updates the trait-erased `inference_client` slot and emits a URL
//!   warning via the event bus; Tauri emits via `app.emit`).
//!
//! **Extracted into [`save`]:**
//! - [`load_fresh_world_and_npcs`] — pure world + NPC reload from game mod or
//!   legacy data files.  Both server and Tauri delegate to this.
//!
//! **Not extracted (confirmed non-extractable without larger AppState refactor):**
//!
//! - **`handle_system_command`**: all 16 `CommandEffect` variants have
//!   backend-specific side effects (`Quit` exits the process/app differently,
//!   `ShowSpinner` uses backend-specific animation, `ToggleMap` emits different
//!   event shapes, etc.).  A trait covering all variants would add more code
//!   than it removes.
//! - **`do_save_game`**: server and Tauri use different `AppState` concrete types
//!   and different `spawn_blocking + Database::open` call sites.  The shared
//!   [`SessionStore`] trait exists (#614) but is not yet wired into the command
//!   handler paths; threading `Arc<dyn SessionStore>` through every `AppState`
//!   variant is a future slice.
//!
//! ## Headless CLI deferral
//!
//! `parish-cli` uses a flat `App` struct with bare (non-Mutex) fields, which
//! cannot borrow directly into [`GameLoopContext`].  Migrating it requires
//! wrapping fields in `Arc<Mutex<T>>` — a wider change tracked for a future
//! slice.  The CLI continues to use its own inline implementations.
//!
//! [`SessionStore`]: crate::session_store::SessionStore

pub mod context;
pub mod inference;
pub mod input;
pub mod movement;
pub mod npc_turn;
pub mod reactions;
pub mod save;

pub use context::GameLoopContext;
pub use inference::{InferenceSlots, rebuild_inference_worker};
pub use input::{handle_game_input, handle_look};
pub use movement::handle_movement;
pub use npc_turn::{TurnOutcome, handle_npc_conversation, run_idle_banter, run_npc_turn};
pub use reactions::{PersistReactionFn, emit_npc_reactions, is_snippet_injection_char};
pub use save::load_fresh_world_and_npcs;
