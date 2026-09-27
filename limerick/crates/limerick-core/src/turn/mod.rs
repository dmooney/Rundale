//! The portable request lifecycle for player turns.
//!
//! One submitted input is a logical request. Each execution of it is an
//! attempt; a failed, stopped, or interrupted attempt may be retried with a
//! new attempt id, and a committed request never runs again. Every step is
//! journaled before the engine acts on it, and every transcript event carries
//! deterministic identities so late callbacks and redelivery have no effect.
//!
//! This module holds the pure pieces: identities ([`ids`]), the state
//! machine ([`lifecycle`]), transcript events ([`transcript`]), the
//! durability seam ([`journal`]) and its implementation in the save database
//! ([`sqlite_journal`]), and the projection of committed wire emissions onto
//! transcript events ([`projection`]). The turn engine
//! ([`engine`]) drives them over the shared game-loop pipeline, and
//! [`host`] is how in-process runtimes submit input through it. Design:
//! `docs/design/portable-turn-api.md`.

use std::future::Future;
use std::pin::Pin;

pub mod engine;
pub mod host;
pub mod ids;
pub mod journal;
#[cfg(test)]
mod journal_contract;
pub mod lifecycle;
pub mod projection;
pub mod sqlite_journal;
pub mod transcript;

/// Boxed future returned by lifecycle traits.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub use engine::{
    HostYield, INTERRUPTED_MESSAGE, InferenceResolution, InferenceRoutes, LoadingHook,
    PendingInference, TurnEngine, TurnError, TurnInput, TurnRules, TurnStatus, TurnStep,
    drive_in_process,
};
pub use host::{InProcessSubmission, InProcessTurns};
pub use ids::{
    EventSequence, ExecutionAttemptId, InferenceCallId, LogicalRequestId, StateRevision,
    TranscriptEventId, TranscriptItemId,
};
pub use journal::{JournalError, MemoryTurnJournal, TurnCommit, TurnJournal};
pub use lifecycle::{
    AddresseeSelection, ClarificationChoice, ClarificationPrompt, IgnoredReason, LifecycleError,
    RequestAttempt, RequestPhase, RequestRecord, TerminalOutcome,
};
pub use projection::{PRESENTATION_EMISSIONS, TRANSCRIPT_EMISSIONS, project_emissions};
pub use sqlite_journal::SqliteTurnJournal;
pub use transcript::{EventBuilder, PendingEvent, TranscriptEvent, TranscriptEventKind};
