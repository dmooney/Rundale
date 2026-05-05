//! Shared orchestration layer — game-loop functions extracted from all three
//! backends (#696 slices 1–8, complete).
//!
//! # Extraction summary
//!
//! All canonical game-loop logic lives in submodules of this crate.  Each
//! backend (axum server, Tauri desktop, headless CLI) constructs the
//! appropriate context and delegates to these functions.
//!
//! ## Extracted functions (all three backends delegate)
//!
//! | Function | Module | Backends |
//! |---|---|---|
//! | `run_npc_turn` | [`npc_turn`] | server, Tauri, CLI |
//! | `handle_npc_conversation` | [`npc_turn`] | server, Tauri, CLI |
//! | `run_idle_banter` | [`npc_turn`] | server, Tauri, CLI |
//! | `handle_game_input` | [`input`] | server, Tauri, CLI |
//! | `handle_movement` | [`movement`] | server, Tauri, CLI |
//! | `emit_npc_reactions` | [`reactions`] | server, Tauri, CLI |
//! | `rebuild_inference_worker` | [`inference`] | server, Tauri, CLI |
//! | `load_fresh_world_and_npcs` | [`save`] | server, Tauri, CLI |
//! | `do_save_game` | [`save`] | server, Tauri |
//! | `do_new_game` | [`save`] | server, Tauri |
//!
//! ## CLI structural note
//!
//! The headless CLI uses its own `handle_headless_new_game` because it
//! creates a new branch on an existing `AsyncDatabase` and calls print helpers
//! (`print_location_arrival`, `print_arrival_reactions`) that are not part of
//! the `EventEmitter` surface.  The save equivalent (`do_autosave_if_needed`)
//! similarly uses `AsyncDatabase` directly.  These diverge structurally, not
//! behaviourally, and both runtimes share `load_fresh_world_and_npcs`.
//!
//! ## SessionStore wiring (#696 slice 8)
//!
//! `Arc<dyn SessionStore>` is now wired into all three runtimes:
//!
//! - **Server** — `AppState::session_store` (existing, from #614).
//! - **Tauri** — `AppState::session_store` added in slice 8.
//! - **CLI** — `App::session_store` added in slice 8.
//!
//! `DbSessionStore` moved from `parish-server` to `parish-core::session_store`
//! so all three runtimes can instantiate it without a circular dependency.
//! Tauri and CLI pass `session_id = ""` (single-user flat saves layout).
//!
//! ## Architecture gate
//!
//! This module must remain backend-agnostic.  It does **not** import `axum`,
//! `tauri`, or any crate in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.  The
//! `architecture_fitness` test enforces this mechanically.

pub mod context;
pub mod inference;
pub mod input;
pub mod movement;
pub mod npc_turn;
pub mod reactions;
pub mod save;
pub mod system_command;

pub use context::GameLoopContext;
pub use inference::{InferenceSlots, rebuild_inference_worker};
pub use input::{handle_game_input, handle_look};
pub use movement::handle_movement;
pub use npc_turn::{TurnOutcome, handle_npc_conversation, run_idle_banter, run_npc_turn};
pub use reactions::{PersistReactionFn, emit_npc_reactions, is_snippet_injection_char};
pub use save::{NewGameParams, do_new_game, do_save_game, load_fresh_world_and_npcs};
pub use system_command::{BoxFuture, SystemCommandHost, handle_system_command};
