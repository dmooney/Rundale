//! The turn engine: one pipeline and one request lifecycle for every runtime.
//!
//! [`TurnEngine::submit`] accepts player input as a logical request, journals
//! it, and runs an execution attempt of the shared game-loop pipeline
//! (`game_loop::handle_game_input`) against an isolated candidate copy of the
//! live session state. Each model call the pipeline makes goes through the
//! [`HostYield`] seam: the attempt suspends and the call is returned to the
//! host as [`TurnStatus::AwaitingInference`]. The host fulfils it however it
//! can and calls [`TurnEngine::resume`]. When the pipeline finishes, the
//! engine journals the outcome and, for a successful attempt, installs the
//! candidate into live state and releases its events and emissions.
//!
//! Stop, failure, and interruption discard the candidate, so they have no
//! authoritative effect. Callbacks for an attempt that is no longer current,
//! a call that is not the awaited one, or a stale base revision are
//! [`TurnStatus::Ignored`] with no effect.
//!
//! Desktop hosts fulfil calls in-process with [`drive_in_process`]. The live
//! state must not be mutated by anything else while an attempt is open (the
//! runtimes' `persistence_gate` provides this); commit replaces it with the
//! candidate. Design: `docs/design/portable-turn-api.md` §3–§6.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{Mutex, mpsc, oneshot};

use super::BoxFuture;
use super::ids::{ExecutionAttemptId, InferenceCallId, LogicalRequestId, StateRevision};
use super::journal::{JournalError, TurnCommit, TurnJournal};
use super::lifecycle::{IgnoredReason, LifecycleError, RequestRecord, TerminalOutcome};
use super::projection::project_emissions;
use super::transcript::{EventBuilder, PendingEvent, TranscriptEvent, TranscriptEventKind};
use crate::config::{InferenceConfig, InferenceSubrole};
use crate::game_loop::inference::InProcessInference;
use crate::game_loop::{GameInputOutcome, GameLoopContext, TurnCandidate, handle_game_input};
use crate::game_mod::PronunciationEntry;
use crate::inference::{AnyClient, DeferredInferenceAudit, InferenceQueue};
use crate::ipc::GameConfig;
use crate::npc::LanguageSettings;
use crate::npc::reactions::ReactionTemplates;
use crate::turn_inference::{
    CallReport, InferenceCall, InferenceFailureKind, InferenceOutcome, RouteStatus, TurnInference,
};
use crate::world::transport::TransportMode;

/// The line recorded when restart recovery interrupts an open request.
pub const INTERRUPTED_MESSAGE: &str = "The previous response was interrupted; you can retry it.";

/// Player input submitted as a new logical request.
#[derive(Debug, Clone, Default)]
pub struct TurnInput {
    /// Host-chosen request id; the engine mints one when absent.
    pub request_id: Option<LogicalRequestId>,
    /// The player's text.
    pub text: String,
    /// Addressees chosen with the submission (UI chips).
    pub addressed_to: Vec<String>,
    /// Host draft id, echoed on the command event.
    pub draft_id: Option<String>,
}

/// A model call the engine is waiting for.
#[derive(Debug, Clone)]
pub struct PendingInference {
    /// Call identity; the resolution must name it.
    pub id: InferenceCallId,
    /// Owning request.
    pub request_id: LogicalRequestId,
    /// Owning attempt; the resolution must name it.
    pub attempt_id: ExecutionAttemptId,
    /// Authoritative revision the attempt runs against; the resolution must
    /// carry it.
    pub base_revision: StateRevision,
    /// What to run.
    pub call: InferenceCall,
    /// Whether the pipeline consumes the reply as a stream. The engine only
    /// ever uses the completed text, but a host that can stream should make
    /// the provider request it does today for this workload.
    pub streaming: bool,
}

/// The host's answer to a [`PendingInference`].
#[derive(Debug, Clone)]
pub struct InferenceResolution {
    /// The call being answered.
    pub call_id: InferenceCallId,
    /// The attempt the call belongs to.
    pub attempt_id: ExecutionAttemptId,
    /// The call's base revision.
    pub base_revision: StateRevision,
    /// The result.
    pub outcome: InferenceOutcome,
}

/// Where a request stands after an engine call.
#[derive(Debug, Clone)]
pub enum TurnStatus {
    /// The attempt is suspended until the host resolves this call.
    AwaitingInference(PendingInference),
    /// The attempt ended.
    Completed {
        /// How it ended.
        outcome: TerminalOutcome,
        /// The committed revision (only for `Succeeded`).
        revision: Option<StateRevision>,
    },
    /// A stale, late, or duplicate callback; nothing changed.
    Ignored(IgnoredReason),
}

/// The result of one engine call.
#[derive(Debug, Clone)]
pub struct TurnStep {
    /// The request the step belongs to (absent for an ignored callback that
    /// names no known attempt).
    pub request_id: Option<LogicalRequestId>,
    /// The attempt the step belongs to.
    pub attempt_id: Option<ExecutionAttemptId>,
    /// Transcript events journaled by this call, in sequence order.
    pub events: Vec<TranscriptEvent>,
    /// Wire emissions of a committed attempt, in order. They have already
    /// been flushed to the live context's emitter.
    pub emissions: Vec<(String, serde_json::Value)>,
    /// Where the request stands.
    pub status: TurnStatus,
}

impl TurnStep {
    fn ignored(
        request_id: Option<LogicalRequestId>,
        attempt_id: Option<ExecutionAttemptId>,
        reason: IgnoredReason,
    ) -> Self {
        Self {
            request_id,
            attempt_id,
            events: Vec::new(),
            emissions: Vec::new(),
            status: TurnStatus::Ignored(reason),
        }
    }
}

/// An engine call that could not be carried out. Nothing changed, except
/// where noted.
#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    /// Another request is still open.
    #[error("request {0} is still open")]
    RequestInProgress(LogicalRequestId),
    /// The request id is already known to this engine.
    #[error("request {0} was already submitted")]
    DuplicateRequest(LogicalRequestId),
    /// The request id is not known to this engine.
    #[error("request {0} is unknown")]
    UnknownRequest(LogicalRequestId),
    /// The lifecycle does not allow the transition.
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    /// A journal write failed. When a finished attempt could not be
    /// committed, the attempt has ended `Failed` (retryable) and its
    /// candidate was discarded.
    #[error(transparent)]
    Journal(#[from] JournalError),
    /// The finished candidate could not be collected; the attempt has ended
    /// `Failed` and its candidate was discarded.
    #[error("turn candidate failed: {0}")]
    Candidate(String),
}

/// Route availability the pipeline sees for each workload, as the host
/// reports it. Workloads the host does not list are [`RouteStatus::Live`].
#[derive(Debug, Clone, Default)]
pub struct InferenceRoutes(HashMap<InferenceSubrole, RouteStatus>);

impl InferenceRoutes {
    /// Every workload live.
    pub fn live() -> Self {
        Self::default()
    }

    /// Sets the status of one workload.
    pub fn with(mut self, subrole: InferenceSubrole, status: RouteStatus) -> Self {
        self.0.insert(subrole, status);
        self
    }

    /// The status of one workload.
    pub fn status(&self, subrole: InferenceSubrole) -> RouteStatus {
        self.0.get(&subrole).copied().unwrap_or(RouteStatus::Live)
    }

    /// Asks `inference` for the status of every workload.
    pub async fn snapshot(inference: &dyn TurnInference) -> Self {
        let mut routes = Self::default();
        for subrole in InferenceSubrole::ALL {
            routes.0.insert(subrole, inference.route(subrole).await);
        }
        routes
    }
}

/// Turn rules a runtime supplies once: how the player travels and how NPCs
/// greet an arrival.
#[derive(Debug, Clone)]
pub struct TurnRules {
    /// Travel mode for movement.
    pub transport: TransportMode,
    /// Arrival-reaction templates from the loaded mod.
    pub reaction_templates: ReactionTemplates,
}

/// A model call from a running attempt, handed to the engine.
struct YieldedCall {
    call: InferenceCall,
    streaming: bool,
    reply: oneshot::Sender<InferenceOutcome>,
}

/// The [`TurnInference`] of an engine attempt: every call suspends the
/// attempt and is returned to the host; the host's resolution completes it.
///
/// A host without streaming delivers the whole reply once, so a streaming
/// caller receives the completed text as a single chunk after the call
/// resolves. Nothing is forwarded for a failed call.
pub struct HostYield {
    calls: mpsc::UnboundedSender<YieldedCall>,
    routes: InferenceRoutes,
}

impl HostYield {
    fn interrupted(message: &str) -> InferenceOutcome {
        InferenceOutcome::Failed {
            kind: InferenceFailureKind::Interrupted,
            message: message.to_string(),
            report: CallReport::default(),
        }
    }
}

impl TurnInference for HostYield {
    fn route(&self, subrole: InferenceSubrole) -> BoxFuture<'_, RouteStatus> {
        let status = self.routes.status(subrole);
        Box::pin(async move { status })
    }

    fn complete_streaming(
        &self,
        call: InferenceCall,
        tokens: Option<mpsc::Sender<String>>,
    ) -> BoxFuture<'_, InferenceOutcome> {
        Box::pin(async move {
            let (reply, answer) = oneshot::channel();
            let streaming = tokens.is_some();
            if self
                .calls
                .send(YieldedCall {
                    call,
                    streaming,
                    reply,
                })
                .is_err()
            {
                return Self::interrupted("the turn engine released the attempt");
            }
            let outcome = answer
                .await
                .unwrap_or_else(|_| Self::interrupted("the attempt was stopped"));
            if let (Some(tokens), Some(text)) = (tokens, outcome.text()) {
                let _ = tokens.send(text.to_string()).await;
            }
            outcome
        })
    }
}

/// Everything one attempt owns: the candidate state it mutates and a
/// snapshot of the read-only runtime configuration.
///
/// Provider slots are empty: the attempt reaches inference only through its
/// [`HostYield`] seam.
struct AttemptEnv {
    candidate: TurnCandidate,
    config: Mutex<GameConfig>,
    inference_queue: Mutex<Option<InferenceQueue>>,
    client: Mutex<Option<AnyClient>>,
    cloud_client: Mutex<Option<AnyClient>>,
    inference_config: InferenceConfig,
    pronunciations: Vec<PronunciationEntry>,
    language: LanguageSettings,
    inference_failure_messages: Vec<String>,
    idle_messages: Vec<String>,
    inference: Arc<HostYield>,
    rules: TurnRules,
}

impl AttemptEnv {
    fn context(&self) -> GameLoopContext<'_> {
        GameLoopContext {
            world: &self.candidate.world,
            npc_manager: &self.candidate.npc_manager,
            config: &self.config,
            conversation: &self.candidate.conversation,
            inference_queue: &self.inference_queue,
            emitter: self.candidate.emitter(),
            inference_config: &self.inference_config,
            pronunciations: &self.pronunciations,
            client: &self.client,
            cloud_client: &self.cloud_client,
            language: self.language.clone(),
            inference_failure_messages: &self.inference_failure_messages,
            idle_messages: &self.idle_messages,
            inference_override: Some(self.inference.clone()),
        }
    }
}

type AttemptFuture = Pin<Box<dyn Future<Output = GameInputOutcome> + Send>>;

/// The call an attempt is suspended on.
struct AwaitedCall {
    id: InferenceCallId,
    subrole: InferenceSubrole,
    reply: oneshot::Sender<InferenceOutcome>,
}

/// The attempt currently executing.
struct RunningAttempt {
    request_id: LogicalRequestId,
    attempt_id: ExecutionAttemptId,
    env: Arc<AttemptEnv>,
    future: AttemptFuture,
    calls: mpsc::UnboundedReceiver<YieldedCall>,
    awaiting: Option<AwaitedCall>,
    /// Player location name when the attempt started (for `SceneChanged`).
    location_before: Option<String>,
    /// Ordinal of the attempt's next transcript event.
    next_ordinal: u32,
    /// How the most recent dialogue call ended: `None` when it completed.
    last_dialogue_failure: Option<Option<InferenceFailureKind>>,
}

impl RunningAttempt {
    fn builder(&self) -> EventBuilder {
        EventBuilder::new(
            self.request_id.clone(),
            Some(self.attempt_id.clone()),
            self.next_ordinal,
        )
    }
}

enum Next {
    Finished(GameInputOutcome),
    Call(YieldedCall),
}

/// Runs player turns as journaled requests. See the module docs.
pub struct TurnEngine {
    journal: Arc<dyn TurnJournal>,
    rules: TurnRules,
    routes: InferenceRoutes,
    revision: StateRevision,
    records: HashMap<LogicalRequestId, RequestRecord>,
    running: Option<RunningAttempt>,
}

impl TurnEngine {
    /// An engine journaling to `journal`, starting at revision 0.
    pub fn new(journal: Arc<dyn TurnJournal>, rules: TurnRules) -> Self {
        Self {
            journal,
            rules,
            routes: InferenceRoutes::live(),
            revision: StateRevision::default(),
            records: HashMap::new(),
            running: None,
        }
    }

    /// Sets the route availability attempts started from now on see.
    pub fn set_routes(&mut self, routes: InferenceRoutes) {
        self.routes = routes;
    }

    /// The current authoritative revision.
    pub fn revision(&self) -> StateRevision {
        self.revision
    }

    /// The engine's record of a request.
    pub fn request(&self, id: &LogicalRequestId) -> Option<&RequestRecord> {
        self.records.get(id)
    }

    /// The request that is still open, if any.
    pub fn open_request(&self) -> Option<&LogicalRequestId> {
        self.running
            .as_ref()
            .map(|running| &running.request_id)
            .or_else(|| {
                self.records
                    .values()
                    .find(|record| record.phase.is_open())
                    .map(|record| &record.id)
            })
    }

    /// Accepts `input` as a new logical request and starts its first
    /// attempt. Acceptance and the command event are journaled before any
    /// interpretation or inference.
    pub async fn submit(
        &mut self,
        live: &GameLoopContext<'_>,
        input: TurnInput,
    ) -> Result<TurnStep, TurnError> {
        if let Some(open) = self.open_request() {
            return Err(TurnError::RequestInProgress(open.clone()));
        }
        let id = input.request_id.unwrap_or_else(LogicalRequestId::fresh);
        if let Some(known) = self.records.get(&id) {
            if known.has_committed() {
                return Err(LifecycleError::AlreadyCommitted(id).into());
            }
            return Err(TurnError::DuplicateRequest(id));
        }
        let record = RequestRecord::accept(
            id.clone(),
            input.text.clone(),
            input.addressed_to,
            input.draft_id,
        );
        let mut command =
            EventBuilder::new(id.clone(), None, 0).event(TranscriptEventKind::PlayerCommand);
        command.item_id = Some(record.command_item());
        command.content = Some(input.text);
        if let Some(draft) = &record.draft_id {
            command
                .metadata
                .insert("draftID".to_string(), draft.clone());
        }
        let mut accepted = self.journal.accept(record.clone(), vec![command]).await?;
        self.records.insert(id.clone(), record);
        let mut step = match self.start_attempt(live, &id, false).await {
            Ok(step) => step,
            Err(error) => {
                // The request is accepted but never started; end it so it
                // can be retried instead of blocking new input.
                if let Some(record) = self.records.get_mut(&id)
                    && record.finish_uncommitted(TerminalOutcome::Failed).is_ok()
                {
                    let _ = self.journal.update(record.clone(), Vec::new()).await;
                }
                return Err(error);
            }
        };
        accepted.append(&mut step.events);
        step.events = accepted;
        Ok(step)
    }

    /// Continues the suspended attempt with the host's result for its
    /// awaited call. A resolution for any other attempt, call, or revision is
    /// ignored.
    pub async fn resume(
        &mut self,
        live: &GameLoopContext<'_>,
        resolution: InferenceResolution,
    ) -> Result<TurnStep, TurnError> {
        let Some(record) = self.record_of_attempt(&resolution.attempt_id) else {
            let reason = if self.running.is_some() {
                IgnoredReason::StaleAttempt
            } else {
                IgnoredReason::NoOpenRequest
            };
            return Ok(TurnStep::ignored(None, Some(resolution.attempt_id), reason));
        };
        let request_id = record.id.clone();
        if let Err(reason) = record.check_callback(
            &resolution.attempt_id,
            Some(&resolution.call_id),
            resolution.base_revision,
            self.revision,
        ) {
            return Ok(TurnStep::ignored(
                Some(request_id),
                Some(resolution.attempt_id),
                reason,
            ));
        }
        let Some(running) = self
            .running
            .as_mut()
            .filter(|running| running.attempt_id == resolution.attempt_id)
        else {
            return Ok(TurnStep::ignored(
                Some(request_id),
                Some(resolution.attempt_id),
                IgnoredReason::StaleAttempt,
            ));
        };
        let Some(awaited) = running
            .awaiting
            .take_if(|awaited| awaited.id == resolution.call_id)
        else {
            return Ok(TurnStep::ignored(
                Some(request_id),
                Some(resolution.attempt_id),
                IgnoredReason::StaleCall,
            ));
        };
        if awaited.subrole == InferenceSubrole::Dialogue {
            running.last_dialogue_failure = Some(match &resolution.outcome {
                InferenceOutcome::Completed { .. } => None,
                InferenceOutcome::Failed { kind, .. } => Some(*kind),
            });
        }
        // The attempt is suspended on exactly this call, so the receiver is
        // alive; a send failure would mean the attempt already ended.
        let _ = awaited.reply.send(resolution.outcome);
        self.advance(live).await
    }

    /// Stops the running attempt: its candidate is discarded and the request
    /// ends `Cancelled` (retryable). Stopping any other attempt is ignored.
    pub async fn stop(
        &mut self,
        _live: &GameLoopContext<'_>,
        attempt: &ExecutionAttemptId,
    ) -> Result<TurnStep, TurnError> {
        let Some(running) = self
            .running
            .as_ref()
            .filter(|running| &running.attempt_id == attempt)
        else {
            let (request_id, reason) = match self.record_of_attempt(attempt) {
                Some(record) if record.current_attempt().map(|a| &a.id) == Some(attempt) => {
                    (Some(record.id.clone()), IgnoredReason::AttemptTerminal)
                }
                Some(record) => (Some(record.id.clone()), IgnoredReason::StaleAttempt),
                None => (None, IgnoredReason::NoOpenRequest),
            };
            return Ok(TurnStep::ignored(request_id, Some(attempt.clone()), reason));
        };
        let request_id = running.request_id.clone();
        let mut record = self.records[&request_id].clone();
        record.finish_uncommitted(TerminalOutcome::Cancelled)?;
        let terminal = running
            .builder()
            .response_completed(TerminalOutcome::Cancelled, None);
        let events = self.journal.update(record.clone(), vec![terminal]).await?;
        // Dropping the attempt aborts its pipeline future and discards the
        // candidate; the awaited call's reply channel closes with it.
        self.running = None;
        self.records.insert(request_id.clone(), record);
        Ok(TurnStep {
            request_id: Some(request_id),
            attempt_id: Some(attempt.clone()),
            events,
            emissions: Vec::new(),
            status: TurnStatus::Completed {
                outcome: TerminalOutcome::Cancelled,
                revision: None,
            },
        })
    }

    /// Runs a failed, stopped, or interrupted request again as a new attempt.
    /// A `Progress` event marking the retry precedes every event of the new
    /// attempt.
    pub async fn retry(
        &mut self,
        live: &GameLoopContext<'_>,
        request: &LogicalRequestId,
    ) -> Result<TurnStep, TurnError> {
        let Some(record) = self.records.get(request) else {
            return Err(TurnError::UnknownRequest(request.clone()));
        };
        if record.has_committed() {
            return Err(LifecycleError::AlreadyCommitted(request.clone()).into());
        }
        if let Some(open) = self.open_request() {
            return Err(TurnError::RequestInProgress(open.clone()));
        }
        self.start_attempt(live, request, true).await
    }

    /// Applies restart recovery to the journal: every request that was
    /// accepted or executing when the process stopped ends `Interrupted`
    /// and is never re-run automatically. Returns the events journaled.
    pub async fn recover(&mut self) -> Result<Vec<TranscriptEvent>, TurnError> {
        if let Some(open) = self.running.as_ref() {
            return Err(TurnError::RequestInProgress(open.request_id.clone()));
        }
        let mut journaled = Vec::new();
        for mut record in self.journal.open_requests().await? {
            if record.recover() {
                let mut builder = match record.current_attempt() {
                    Some(attempt) => EventBuilder::new(
                        record.id.clone(),
                        Some(attempt.id.clone()),
                        u32::from(record.attempts.len() > 1),
                    ),
                    None => EventBuilder::new(record.id.clone(), None, 1),
                };
                let mut notice = builder.event(TranscriptEventKind::Narration);
                notice.content = Some(INTERRUPTED_MESSAGE.to_string());
                let terminal = builder.response_completed(TerminalOutcome::Interrupted, None);
                journaled.extend(
                    self.journal
                        .update(record.clone(), vec![notice, terminal])
                        .await?,
                );
            }
            self.records.insert(record.id.clone(), record);
        }
        Ok(journaled)
    }

    fn record_of_attempt(&self, attempt: &ExecutionAttemptId) -> Option<&RequestRecord> {
        self.records
            .values()
            .find(|record| record.attempts.iter().any(|a| &a.id == attempt))
    }

    async fn start_attempt(
        &mut self,
        live: &GameLoopContext<'_>,
        request_id: &LogicalRequestId,
        retry: bool,
    ) -> Result<TurnStep, TurnError> {
        let attempt_id = ExecutionAttemptId::fresh();
        let mut record = self.records[request_id].clone();
        record.begin_attempt(attempt_id.clone(), self.revision)?;
        let mut builder = EventBuilder::new(request_id.clone(), Some(attempt_id.clone()), 0);
        let mut started = Vec::new();
        if retry {
            let mut progress = builder.event(TranscriptEventKind::Progress);
            progress.item_id = Some(record.command_item());
            progress
                .metadata
                .insert("retry".to_string(), "true".to_string());
            started.push(progress);
        }
        let mut events = self.journal.update(record.clone(), started).await?;
        self.records.insert(request_id.clone(), record.clone());

        let (calls_tx, calls) = mpsc::unbounded_channel();
        let location_before = live
            .world
            .lock()
            .await
            .current_location_data()
            .map(|location| location.name.clone());
        let env = Arc::new(AttemptEnv {
            candidate: TurnCandidate::capture(live, Vec::new()).await,
            config: Mutex::new(live.config.lock().await.clone()),
            inference_queue: Mutex::new(None),
            client: Mutex::new(None),
            cloud_client: Mutex::new(None),
            inference_config: live.inference_config.clone(),
            pronunciations: live.pronunciations.to_vec(),
            language: live.language.clone(),
            inference_failure_messages: live.inference_failure_messages.to_vec(),
            idle_messages: live.idle_messages.to_vec(),
            inference: Arc::new(HostYield {
                calls: calls_tx,
                routes: self.routes.clone(),
            }),
            rules: self.rules.clone(),
        });
        let attempt_env = Arc::clone(&env);
        let text = record.original_text.clone();
        let addressed_to = record.addressed_to.clone();
        let future: AttemptFuture = Box::pin(async move {
            let ctx = attempt_env.context();
            handle_game_input(
                &ctx,
                text,
                addressed_to,
                &attempt_env.rules.transport,
                &attempt_env.rules.reaction_templates,
                || None,
            )
            .await
        });
        self.running = Some(RunningAttempt {
            request_id: request_id.clone(),
            attempt_id,
            env,
            future,
            calls,
            awaiting: None,
            location_before,
            next_ordinal: builder.next_ordinal(),
            last_dialogue_failure: None,
        });
        let mut step = self.advance(live).await?;
        events.append(&mut step.events);
        step.events = events;
        Ok(step)
    }

    /// Polls the running attempt until it asks for inference or finishes.
    async fn advance(&mut self, live: &GameLoopContext<'_>) -> Result<TurnStep, TurnError> {
        let running = self
            .running
            .as_mut()
            .expect("advance is only called with a running attempt");
        let next = tokio::select! {
            biased;
            outcome = running.future.as_mut() => Next::Finished(outcome),
            Some(call) = running.calls.recv() => Next::Call(call),
        };
        match next {
            Next::Call(YieldedCall {
                call,
                streaming,
                reply,
            }) => {
                let record = self
                    .records
                    .get_mut(&running.request_id)
                    .expect("a running attempt has a record");
                let id = record.next_call()?;
                let base_revision = record
                    .current_attempt()
                    .expect("a running request has an attempt")
                    .base_revision;
                running.awaiting = Some(AwaitedCall {
                    id: id.clone(),
                    subrole: call.subrole,
                    reply,
                });
                Ok(TurnStep {
                    request_id: Some(running.request_id.clone()),
                    attempt_id: Some(running.attempt_id.clone()),
                    events: Vec::new(),
                    emissions: Vec::new(),
                    status: TurnStatus::AwaitingInference(PendingInference {
                        id,
                        request_id: running.request_id.clone(),
                        attempt_id: running.attempt_id.clone(),
                        base_revision,
                        call,
                        streaming,
                    }),
                })
            }
            Next::Finished(outcome) => {
                let running = self.running.take().expect("checked above");
                self.finish(live, running, outcome).await
            }
        }
    }

    /// Commits or discards a finished attempt.
    async fn finish(
        &mut self,
        live: &GameLoopContext<'_>,
        running: RunningAttempt,
        outcome: GameInputOutcome,
    ) -> Result<TurnStep, TurnError> {
        let RunningAttempt {
            request_id,
            attempt_id,
            env,
            future,
            calls,
            location_before,
            next_ordinal,
            last_dialogue_failure,
            ..
        } = running;
        drop(future);
        drop(calls);
        let mut builder =
            EventBuilder::new(request_id.clone(), Some(attempt_id.clone()), next_ordinal);
        let finished = Arc::into_inner(env)
            .ok_or_else(|| "the attempt's state is still shared".to_string())
            .and_then(|env| env.candidate.finish().map_err(|error| error.to_string()));
        let finished = match finished {
            Ok(finished) => finished,
            Err(message) => {
                self.end_uncommitted(
                    &request_id,
                    &mut builder,
                    TerminalOutcome::Failed,
                    "candidate",
                    &message,
                )
                .await?;
                return Err(TurnError::Candidate(message));
            }
        };

        if let Some(message) = outcome.dialogue_failure {
            let (terminal, kind) = match last_dialogue_failure {
                Some(Some(InferenceFailureKind::Interrupted)) => {
                    (TerminalOutcome::Interrupted, "interrupted")
                }
                Some(Some(kind)) => (TerminalOutcome::Failed, failure_kind_name(kind)),
                Some(None) => (TerminalOutcome::Failed, "semantic_validation"),
                None => (TerminalOutcome::Failed, "dialogue"),
            };
            let events = self
                .end_uncommitted(&request_id, &mut builder, terminal, kind, &message)
                .await?;
            return Ok(TurnStep {
                request_id: Some(request_id),
                attempt_id: Some(attempt_id),
                events,
                emissions: Vec::new(),
                status: TurnStatus::Completed {
                    outcome: terminal,
                    revision: None,
                },
            });
        }

        // Every committed attempt changes authoritative state: at the least
        // it records the player's input in the conversation state.
        let revision = self.revision.next();
        let mut record = self.records[&request_id].clone();
        record.complete(revision)?;
        let mut events = project_emissions(
            finished.emissions(),
            &mut builder,
            location_before.as_deref(),
        );
        events.push(builder.response_completed(TerminalOutcome::Succeeded, Some(revision)));
        let commit = TurnCommit {
            record: record.clone(),
            events,
            task_mutations: outcome.task_mutations,
        };
        let events = match self.journal.commit(commit).await {
            Ok(events) => events,
            Err(error) => {
                // Nothing was written; the candidate is dropped uninstalled.
                // The attempt ends `Failed` so the request can be retried.
                let mut builder =
                    EventBuilder::new(request_id.clone(), Some(attempt_id), next_ordinal);
                let _ = self
                    .end_uncommitted(
                        &request_id,
                        &mut builder,
                        TerminalOutcome::Failed,
                        "journal",
                        &error.to_string(),
                    )
                    .await;
                return Err(error.into());
            }
        };
        self.revision = revision;
        self.records.insert(request_id.clone(), record);
        let emissions = finished.install(live).await.release(live).await;
        Ok(TurnStep {
            request_id: Some(request_id),
            attempt_id: Some(attempt_id),
            events,
            emissions,
            status: TurnStatus::Completed {
                outcome: TerminalOutcome::Succeeded,
                revision: Some(revision),
            },
        })
    }

    /// Ends the current attempt of `request` without committing. The record
    /// is updated in memory even when the journal write fails (recovery
    /// then interrupts the journaled copy).
    async fn end_uncommitted(
        &mut self,
        request: &LogicalRequestId,
        builder: &mut EventBuilder,
        outcome: TerminalOutcome,
        error_kind: &str,
        message: &str,
    ) -> Result<Vec<TranscriptEvent>, TurnError> {
        let record = self
            .records
            .get_mut(request)
            .expect("an ending attempt has a record");
        record.finish_uncommitted(outcome)?;
        let record = record.clone();
        let mut error = builder.event(TranscriptEventKind::Error);
        error.content = Some(message.to_string());
        error
            .metadata
            .insert("errorKind".to_string(), error_kind.to_string());
        let mut terminal = builder.response_completed(outcome, None);
        terminal
            .metadata
            .insert("errorKind".to_string(), error_kind.to_string());
        let events: Vec<PendingEvent> = vec![error, terminal];
        Ok(self.journal.update(record, events).await?)
    }
}

fn failure_kind_name(kind: InferenceFailureKind) -> &'static str {
    match kind {
        InferenceFailureKind::Transport => "transport",
        InferenceFailureKind::Protocol => "protocol",
        InferenceFailureKind::TimedOut => "timed_out",
        InferenceFailureKind::Interrupted => "interrupted",
    }
}

/// Runs one submission to completion on a desktop host, fulfilling every
/// model call in-process with `inference` (today's provider calls).
///
/// Route availability is taken from `inference` before the attempt starts.
/// Provider audit records are held back until the attempt commits and are
/// discarded otherwise. Returns every event journaled for the submission and
/// the final status.
pub async fn drive_in_process(
    engine: &mut TurnEngine,
    live: &GameLoopContext<'_>,
    input: TurnInput,
    inference: &InProcessInference<'_>,
) -> Result<TurnStep, TurnError> {
    engine.set_routes(InferenceRoutes::snapshot(inference).await);
    let audit = DeferredInferenceAudit::default();
    let host = inference.with_deferred_audit(audit.clone());
    let mut events = Vec::new();
    let mut step = engine.submit(live, input).await;
    loop {
        let current = match step {
            Ok(current) => current,
            Err(error) => {
                audit.discard().await;
                return Err(error);
            }
        };
        events.extend(current.events.iter().cloned());
        match &current.status {
            TurnStatus::AwaitingInference(pending) => {
                let outcome = if pending.streaming {
                    // Stream from the provider as today; the engine takes
                    // only the completed text, so the chunks are drained.
                    let (tokens, mut chunks) = mpsc::channel(crate::ipc::TOKEN_CHANNEL_CAPACITY);
                    let (outcome, ()) = tokio::join!(
                        host.complete_streaming(pending.call.clone(), Some(tokens)),
                        async { while chunks.recv().await.is_some() {} }
                    );
                    outcome
                } else {
                    host.complete(pending.call.clone()).await
                };
                step = engine
                    .resume(
                        live,
                        InferenceResolution {
                            call_id: pending.id.clone(),
                            attempt_id: pending.attempt_id.clone(),
                            base_revision: pending.base_revision,
                            outcome,
                        },
                    )
                    .await;
            }
            TurnStatus::Completed { outcome, .. } => {
                if *outcome == TerminalOutcome::Succeeded {
                    audit.commit().await;
                } else {
                    audit.discard().await;
                }
                return Ok(TurnStep { events, ..current });
            }
            TurnStatus::Ignored(_) => {
                audit.discard().await;
                return Ok(TurnStep { events, ..current });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn_inference::ResponseShape;

    fn reaction_call() -> InferenceCall {
        InferenceCall {
            subrole: InferenceSubrole::ArrivalReaction,
            system: None,
            prompt: "greet the newcomer".to_string(),
            response: ResponseShape::Text,
            correlation_id: None,
        }
    }

    #[tokio::test]
    async fn host_yield_hands_the_call_over_and_streams_the_whole_reply_once() {
        let (calls_tx, mut calls) = mpsc::unbounded_channel();
        let host = HostYield {
            calls: calls_tx,
            routes: InferenceRoutes::live()
                .with(InferenceSubrole::TravelEncounter, RouteStatus::Simulated),
        };
        assert_eq!(
            host.route(InferenceSubrole::TravelEncounter).await,
            RouteStatus::Simulated
        );
        assert_eq!(
            host.route(InferenceSubrole::Intent).await,
            RouteStatus::Live
        );

        let (tokens, mut chunks) = mpsc::channel(4);
        let host_side = async {
            let yielded = calls.recv().await.expect("the call is handed over");
            assert!(yielded.streaming);
            assert_eq!(yielded.call.prompt, "greet the newcomer");
            yielded
                .reply
                .send(InferenceOutcome::Completed {
                    text: "Ye're welcome.".to_string(),
                    report: CallReport::default(),
                })
                .unwrap();
        };
        let (outcome, ()) = tokio::join!(
            host.complete_streaming(reaction_call(), Some(tokens)),
            host_side
        );
        assert_eq!(outcome.text(), Some("Ye're welcome."));
        assert_eq!(chunks.recv().await.as_deref(), Some("Ye're welcome."));
        assert_eq!(
            chunks.recv().await,
            None,
            "one chunk, then the stream closes"
        );
    }

    #[tokio::test]
    async fn a_dropped_resolution_interrupts_the_call_and_streams_nothing() {
        let (calls_tx, mut calls) = mpsc::unbounded_channel();
        let host = HostYield {
            calls: calls_tx,
            routes: InferenceRoutes::live(),
        };
        let (tokens, mut chunks) = mpsc::channel(4);
        let host_side = async {
            // Stop: the engine drops the awaited reply.
            drop(calls.recv().await.expect("the call is handed over"));
        };
        let (outcome, ()) = tokio::join!(
            host.complete_streaming(reaction_call(), Some(tokens)),
            host_side
        );
        assert!(matches!(
            outcome,
            InferenceOutcome::Failed {
                kind: InferenceFailureKind::Interrupted,
                ..
            }
        ));
        assert_eq!(chunks.recv().await, None);
    }
}
