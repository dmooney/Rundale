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
//! durability seam ([`journal`]), and the projection of committed wire
//! emissions onto transcript events ([`projection`]). The turn engine that
//! drives them is built on top. Design: `docs/design/portable-turn-api.md`.

use std::future::Future;
use std::pin::Pin;

pub mod ids;
pub mod journal;
pub mod lifecycle;
pub mod projection;
pub mod transcript;

/// Boxed future returned by lifecycle traits.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub use ids::{
    EventSequence, ExecutionAttemptId, InferenceCallId, LogicalRequestId, StateRevision,
    TranscriptEventId, TranscriptItemId,
};
pub use journal::{JournalError, MemoryTurnJournal, TurnCommit, TurnJournal};
pub use lifecycle::{
    ClarificationChoice, ClarificationPrompt, IgnoredReason, LifecycleError, RequestAttempt,
    RequestPhase, RequestRecord, TerminalOutcome,
};
pub use projection::{PRESENTATION_EMISSIONS, TRANSCRIPT_EMISSIONS, project_emissions};
pub use transcript::{EventBuilder, PendingEvent, TranscriptEvent, TranscriptEventKind};
