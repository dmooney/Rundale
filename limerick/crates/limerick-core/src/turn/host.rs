//! The turn engine of a runtime that fulfils inference in-process.
//!
//! The server, the Tauri app, and its MCP bridge submit player input through
//! one [`InProcessTurns`] per session: a [`TurnEngine`] over the active
//! save's [`SqliteTurnJournal`], driven to completion with
//! [`drive_in_process`](super::drive_in_process). Every turn runs on a
//! candidate and is committed or discarded whole. Design:
//! `docs/design/portable-turn-api.md` §5–§7.
//!
//! The engine is opened on one save and branch. Opening restores the
//! journaled requests and the committed revision, then runs restart
//! recovery, so a request the previous process left accepted or executing
//! ends `Interrupted` and is never re-run. The runtime opens it wherever it
//! binds a save (launch, new game, load, fork); a submission for a different
//! save or branch than the open one reopens it first.

use std::sync::Arc;

use tokio::sync::Mutex;

use super::BoxFuture;
use super::engine::{LoadingHook, TurnEngine, TurnError, TurnInput, TurnRules, TurnStep};
use super::ids::StateRevision;
use super::journal::{JournalError, MemoryTurnJournal, TurnCommit, TurnJournal};
use super::lifecycle::RequestRecord;
use super::sqlite_journal::SqliteTurnJournal;
use super::transcript::{PendingEvent, TranscriptEvent};
use crate::game_loop::GameLoopContext;
use crate::game_loop::inference::InProcessInference;
use crate::session_store::TaskJournalTarget;

/// What a runtime supplies with one submission.
pub struct InProcessSubmission {
    /// The player's input.
    pub input: TurnInput,
    /// Travel mode and arrival-reaction templates, read per turn because a
    /// mod reload can change them.
    pub rules: TurnRules,
    /// The active save and branch, whose journal the turn is written to.
    /// `None` when no save is bound.
    pub task_target: Option<TaskJournalTarget>,
    /// The runtime's loading indicator.
    pub loading: Option<LoadingHook>,
}

/// One session's turn engine, fulfilling inference in-process.
///
/// The runtime holds its `persistence_gate` across [`Self::submit`] and
/// [`Self::open`]; the engine lock is taken inside them, before any state
/// lock.
pub struct InProcessTurns {
    turns: Mutex<Turns>,
}

/// An engine and the save it journals to.
struct Turns {
    engine: TurnEngine,
    /// The save and branch the engine is open on; `None` for the in-memory
    /// journal used while no save is bound.
    save: Option<TaskJournalTarget>,
}

impl Turns {
    /// An engine on an in-memory journal, for a runtime with no save bound.
    fn unsaved() -> Self {
        Self {
            engine: TurnEngine::new(
                Arc::new(UnsavedTurnJournal::default()),
                TurnRules::default(),
            ),
            save: None,
        }
    }

    /// Opens the engine on `save`'s journal and recovers requests the
    /// previous process left open. Returns the recovery events.
    async fn open(
        save: Option<TaskJournalTarget>,
    ) -> Result<(Self, Vec<TranscriptEvent>), TurnError> {
        let Some(save) = save else {
            return Ok((Self::unsaved(), Vec::new()));
        };
        let path = save.save_path.clone();
        let branch_id = save.branch_id;
        let journal =
            tokio::task::spawn_blocking(move || SqliteTurnJournal::open(&path, branch_id))
                .await
                .map_err(|error| JournalError::Storage(error.to_string()))??;
        let mut engine = TurnEngine::restore(Arc::new(journal), TurnRules::default()).await?;
        let recovered = engine.recover().await?;
        if !recovered.is_empty() {
            tracing::info!(
                save = %save.save_path.display(),
                branch_id,
                events = recovered.len(),
                "interrupted requests left open by the previous process"
            );
        }
        Ok((
            Self {
                engine,
                save: Some(save),
            },
            recovered,
        ))
    }
}

impl Default for InProcessTurns {
    fn default() -> Self {
        Self::new()
    }
}

impl InProcessTurns {
    /// An engine with no save bound yet.
    pub fn new() -> Self {
        Self {
            turns: Mutex::new(Turns::unsaved()),
        }
    }

    /// Opens the engine on the journal of `save` (the save and branch the
    /// runtime just bound: launch, new game, load, fork), replacing the
    /// current engine and any question still waiting for an answer.
    /// Restores the journaled requests and revision, then interrupts the
    /// requests a stopped process left open. Returns the events recovery
    /// journaled.
    ///
    /// On error the engine is left unopened, so the next submission retries
    /// the open and reports its failure as the turn's error.
    pub async fn open(
        &self,
        save: Option<TaskJournalTarget>,
    ) -> Result<Vec<TranscriptEvent>, TurnError> {
        let mut turns = self.turns.lock().await;
        match Turns::open(save).await {
            Ok((opened, recovered)) => {
                *turns = opened;
                Ok(recovered)
            }
            Err(error) => {
                *turns = Turns::unsaved();
                Err(error)
            }
        }
    }

    /// The authoritative revision of the open engine.
    pub async fn revision(&self) -> StateRevision {
        self.turns.lock().await.engine.revision()
    }

    /// Runs one submission to its end: committed, failed, or parked on a
    /// clarification. The player's echo and the before-turn world update are
    /// the caller's to release first; the turn's own output is released at
    /// commit.
    pub async fn submit(
        &self,
        live: &GameLoopContext<'_>,
        submission: InProcessSubmission,
    ) -> Result<TurnStep, TurnError> {
        let InProcessSubmission {
            input,
            rules,
            task_target,
            loading,
        } = submission;
        let mut turns = self.turns.lock().await;
        if turns.save != task_target {
            *turns = Turns::open(task_target).await?.0;
        }
        let engine = &mut turns.engine;
        engine.set_rules(rules);
        engine.set_loading(loading);
        let inference = InProcessInference::from_ctx(live);
        let result = super::drive_in_process(engine, live, input, &inference).await;
        engine.set_loading(None);
        result
    }
}

/// The journal of a runtime with no save bound: requests and events in
/// memory, and a committed task batch refused because it has nowhere
/// durable to go.
#[derive(Default)]
struct UnsavedTurnJournal {
    memory: MemoryTurnJournal,
}

impl TurnJournal for UnsavedTurnJournal {
    fn accept(
        &self,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        self.memory.accept(record, events)
    }

    fn update(
        &self,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        self.memory.update(record, events)
    }

    fn commit(
        &self,
        commit: TurnCommit,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        if !commit.task_mutations.is_empty() {
            return Box::pin(async {
                Err(JournalError::Storage(
                    "cannot journal player task without an active save and branch".to_string(),
                ))
            });
        }
        self.memory.commit(commit)
    }

    fn open_requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>> {
        self.memory.open_requests()
    }

    fn requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>> {
        self.memory.requests()
    }
}
