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
//! | `advance_world` | [`world_pump`] | server, Tauri, CLI, harness |
//!
//! ## CLI structural note
//!
//! The headless CLI uses its own `handle_headless_new_game` because it
//! owns an `AsyncDatabase` directly and calls print helpers
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
//! `DbSessionStore` moved from `limerick-server` to `limerick-core::session_store`
//! so all three runtimes can instantiate it without a circular dependency.
//! Tauri and CLI pass `session_id = ""` (single-user flat saves layout).
//!
//! ## Architecture gate
//!
//! This module must remain backend-agnostic.  It does **not** import `axum`,
//! `tauri`, or any crate in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.  The
//! `architecture_fitness` test enforces this mechanically.

/// Canonical state mutations produced by one player-input or autonomous turn.
///
/// Task records are complete post-mutation values so persistence can append
/// them directly without observing the lossy broadcast event bus.
#[derive(Debug, Clone, Default)]
pub struct GameInputOutcome {
    /// Player-task post-states in the order they were applied.
    pub task_mutations: Vec<limerick_types::PlayerTask>,
    /// Player-visible recovery when an initiated NPC turn produced no
    /// canonical exchange (for example a length-terminated provider stream).
    pub dialogue_failure: Option<String>,
    /// The turn stopped before any dialogue to ask which person an explicit
    /// addressee meant. A turn engine parks the request on this question.
    pub clarification: Option<AddresseeClarification>,
}

impl GameInputOutcome {
    /// Creates an outcome containing one task mutation, when present.
    pub fn from_task(task: Option<limerick_types::PlayerTask>) -> Self {
        Self {
            task_mutations: task.into_iter().collect(),
            ..Self::default()
        }
    }
}

/// An explicit addressee matched several people present, so the turn asks
/// the player which one they meant instead of guessing.
#[derive(Debug, Clone)]
pub struct AddresseeClarification {
    /// The question and its choices (one per matching NPC).
    pub prompt: crate::turn::ClarificationPrompt,
    /// The intent the turn resolved before asking, so the answer's run needs
    /// no second intent call.
    pub intent: Option<crate::input::PlayerIntent>,
}

/// What an earlier run of the same request already settled: its intent and
/// the player's answers to clarifications. A run given this does not call
/// the intent model and resolves each answered reference to its choice.
#[derive(Debug, Clone, Default)]
pub struct SettledInput {
    /// The intent to use in place of parsing.
    pub intent: Option<crate::input::PlayerIntent>,
    /// Answered references and the NPC each one means.
    pub addressees: Vec<(String, crate::npc::NpcId)>,
}

pub mod context;
pub mod inference;
pub mod input;
pub mod movement;
pub mod npc_turn;
pub mod reactions;
pub mod save;
pub mod staged_turn;
pub mod system_command;
pub mod world_pump;

pub use context::GameLoopContext;
pub use inference::{InferenceSlots, rebuild_inference_worker};
pub use input::{handle_examine, handle_game_input, handle_game_input_settled, handle_look};
pub use movement::handle_movement;
pub use npc_turn::{
    ADDRESSEE_CLARIFICATION_FLAG, AUTONOMOUS_NPC_CHAIN_FLAG, NPC_ACTION_NARRATION_FLAG,
    TurnOutcome, handle_npc_conversation, handle_npc_conversation_settled, run_idle_banter,
    run_npc_turn,
};
pub use reactions::{
    PersistReactionFn, ReactionContextValidFn, emit_npc_reactions, is_snippet_injection_char,
    record_directional_reaction,
};
pub use save::{
    NewGameParams, SaveGameParams, do_new_game, do_save_game, load_fresh_world_and_npcs,
    render_branch_log_text, render_branches_text, resolve_named_branch,
};
pub use staged_turn::{
    FinishedCandidate, InstalledCandidate, StagedGameInputCommit, TurnCandidate,
    flush_staged_emissions, handle_staged_game_input, handle_staged_game_input_with_journal,
    input_may_mutate_tasks,
};
pub use system_command::{BoxFuture, SystemCommandHost, handle_system_command};
pub use world_pump::{
    AdvanceOptions, AdvanceReport, GossipMode, TIER4_SIMULATION_FLAG, WeatherMode, advance_world,
    budgeted_round_robin, build_tier2_groups, mint_tier2_gossip, tier4_simulation_enabled,
};
