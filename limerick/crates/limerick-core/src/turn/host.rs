//! The turn engine of a runtime that fulfils inference in-process.
//!
//! The server, the Tauri app, and its MCP bridge submit player input through
//! one [`InProcessTurns`] per session: a [`TurnEngine`] over a
//! [`SessionStoreTurnJournal`], driven to completion with
//! [`drive_in_process`]. Every turn runs on a candidate and is committed or
//! discarded whole. Design: `docs/design/portable-turn-api.md` §5, §7.

use std::sync::Arc;

use tokio::sync::Mutex;

use super::engine::{LoadingHook, TurnEngine, TurnError, TurnInput, TurnRules, TurnStep};
use super::journal::SessionStoreTurnJournal;
use crate::game_loop::GameLoopContext;
use crate::game_loop::inference::InProcessInference;
use crate::session_store::{SessionStore, TaskJournalTarget};

/// What a runtime supplies with one submission.
pub struct InProcessSubmission {
    /// The player's input.
    pub input: TurnInput,
    /// Travel mode and arrival-reaction templates, read per turn because a
    /// mod reload can change them.
    pub rules: TurnRules,
    /// The session store a committed task batch is appended to.
    pub session_store: Arc<dyn SessionStore>,
    /// The save and branch within it.
    pub task_target: Option<TaskJournalTarget>,
    /// The runtime's loading indicator.
    pub loading: Option<LoadingHook>,
}

/// One session's turn engine, fulfilling inference in-process.
///
/// The runtime holds its `persistence_gate` across [`Self::submit`]; the
/// engine lock is taken inside it, before any state lock.
pub struct InProcessTurns {
    engine: Mutex<TurnEngine>,
    journal: Arc<SessionStoreTurnJournal>,
}

impl Default for InProcessTurns {
    fn default() -> Self {
        Self::new()
    }
}

impl InProcessTurns {
    /// A fresh engine with no request history.
    pub fn new() -> Self {
        let journal = Arc::new(SessionStoreTurnJournal::new());
        Self {
            engine: Mutex::new(TurnEngine::new(journal.clone(), TurnRules::default())),
            journal,
        }
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
            session_store,
            task_target,
            loading,
        } = submission;
        let mut engine = self.engine.lock().await;
        self.journal.bind(session_store, task_target);
        engine.set_rules(rules);
        engine.set_loading(loading);
        let inference = InProcessInference::from_ctx(live);
        let result = super::drive_in_process(&mut engine, live, input, &inference).await;
        engine.set_loading(None);
        result
    }
}
