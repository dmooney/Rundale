//! IPC type definitions and handler logic shared by all frontends.
//!
//! Contains the serializable types exchanged between the game engine and
//! any UI layer (Tauri desktop, axum web server, etc.), plus pure functions
//! that build those types from game state.

pub mod bug_report;
pub mod byok;
pub mod commands;
pub mod config;
pub mod demo;
pub mod editor;
pub mod engine_state;
pub mod event_emitter;
pub mod handlers;
pub mod state;
pub mod streaming;
pub mod turn;
pub mod types;

pub use bug_report::{
    BugContext, BugReportError, BugReportRequest, BugReportResult, BugReportState,
    DiagnosticPayload, GitHubBugConfig, LlmExchange, create_bug_report,
};
pub use commands::{
    CommandEffect, CommandResult, TextPresentation, handle_command, render_look_text,
};
pub use config::GameConfig;
pub use engine_state::{ENGINE_STATE_SCHEMA_VERSION, EngineState, build_engine_state};
pub use event_emitter::{CapturingEmitter, EventEmitter};
pub use handlers::*;
pub use state::{ConversationRuntimeState, SaveState, UiConfigSnapshot};
pub use streaming::{TOKEN_CHANNEL_CAPACITY, stream_npc_tokens};
pub use turn::{
    SubmitInputRequest, SubmitInputResult, TurnClock, TurnEvent, TurnExchange, TurnReadParams,
    TurnReadResult, build_submit_input_result, build_turn_read_result, conversation_cursor,
    events_since, recent_exchanges,
};
pub use types::*;
