//! One embedded game session: the shared engine's live state, its turn
//! engine over the save's journal, and the inference call the host owes it.
//!
//! The session is the mobile host of [`TurnEngine`]. It fulfils nothing
//! itself: every model call with a published Endpoint is handed to the Swift
//! host as a pending invocation, and the host answers with the Endpoint's
//! validated terminal output or a failure. Calls without an Endpoint (travel
//! encounters, arrival reactions) are routed as unavailable, so the pipeline
//! uses its canned fallbacks and never asks. The dialogue content guards are
//! off on this path (`DIALOGUE_CONTENT_GUARDS_FLAG`): the reply is committed
//! after structural checks only.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use limerick_core::config::{InferenceConfig, InferenceSubrole};
use limerick_core::endpoint_input::{EndpointInput, InvocationEnvelope};
use limerick_core::game_loop::world_pump::{AdvanceOptions, GossipMode, WeatherMode};
use limerick_core::game_loop::{GameLoopContext, advance_world, load_fresh_world_and_npcs};
use limerick_core::game_mod::{EndpointRole, GameMod};
use limerick_core::inference::{AnyClient, InferenceQueue};
use limerick_core::ipc::capitalize_first;
use limerick_core::ipc::{ConversationRuntimeState, EventEmitter, GameConfig};
use limerick_core::npc::manager::NpcManager;
use limerick_core::npc::{LanguageSettings, NpcId};
use limerick_core::persistence::{Database, GameSnapshot, SaveFileLock};
use limerick_core::portable_look::{render_look_text, render_scene};
use limerick_core::session_store::RecoveryBundle;
use limerick_core::turn::commands::advertised as advertised_commands;
use limerick_core::turn::{COMMANDS, LocalCommand};
use limerick_core::turn::{
    ExecutionAttemptId, InferenceCallId, InferenceResolution, InferenceRoutes, LogicalRequestId,
    PendingInference, RequestRecord, SqliteTurnJournal, StateRevision, TerminalOutcome,
    TranscriptEvent, TranscriptEventId, TranscriptEventKind, TurnEngine, TurnError, TurnInput,
    TurnRules, TurnStatus, TurnStep,
};
use limerick_core::turn_inference::{
    CallReport, FailureReason, InferenceFailureKind, InferenceOutcome, RouteStatus,
};
use limerick_core::world::WorldState;
use limerick_diagnostics::mobile_report::{
    self, ExchangeOutcome, ExchangeRecord, MobileReport, TranscriptLine,
};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::wire::{self, EventIndex, Provisional};

/// Most events one page (or a snapshot's tail) carries.
pub const MAX_PAGE: usize = 100;
/// Most requests a snapshot projects (the newest, plus any still open).
pub const MAX_SNAPSHOT_REQUESTS: usize = 100;
/// Most answered Endpoint calls a session keeps for a bug report.
pub const MAX_RECORDED_EXCHANGES: usize = 8;
/// Most journaled events a bug report reads its transcript from.
const BUG_REPORT_EVENTS: usize = 40;

/// Why an operation could not be carried out. Nothing changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpError {
    /// Stable machine-readable code.
    pub code: &'static str,
    /// Player- or developer-facing message.
    pub message: String,
}

impl OpError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn protocol(message: impl Into<String>) -> Self {
        Self::new("protocol_error", message)
    }

    fn storage(error: impl std::fmt::Display) -> Self {
        Self::new("storage_error", error.to_string())
    }
}

impl From<TurnError> for OpError {
    fn from(error: TurnError) -> Self {
        let code = match &error {
            TurnError::RequestInProgress(_) => "request_in_progress",
            TurnError::DuplicateRequest(_) => "duplicate_request",
            TurnError::UnknownRequest(_) => "unknown_request",
            TurnError::Lifecycle(_) => "rejected",
            TurnError::Journal(_) => "storage_error",
            TurnError::Candidate(_) => "internal_error",
        };
        Self::new(code, error.to_string())
    }
}

/// How the host asked to open the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Start a new game; the save must not exist.
    New,
    /// Continue the save, or start a new game there if it does not exist.
    Resume,
}

/// What the host supplies to open a session.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    /// The SQLite save the session plays in.
    pub save_path: PathBuf,
    /// The game mod directory (bundled with the app).
    pub mod_dir: PathBuf,
}

/// Discards the engine's wire emissions: the app renders the journaled
/// transcript events each operation returns, not the desktop wire events.
struct DiscardEmitter;

impl EventEmitter for DiscardEmitter {
    fn emit_event(&self, _name: &str, _payload: Value) {}
}

/// The shared engine's live state for one session.
struct Live {
    game_mod: GameMod,
    world: Mutex<WorldState>,
    npc_manager: Mutex<NpcManager>,
    config: Mutex<GameConfig>,
    conversation: Mutex<ConversationRuntimeState>,
    inference_queue: Mutex<Option<InferenceQueue>>,
    client: Mutex<Option<AnyClient>>,
    cloud_client: Mutex<Option<AnyClient>>,
    inference_config: InferenceConfig,
    language: LanguageSettings,
    emitter: Arc<dyn EventEmitter>,
}

impl Live {
    fn new(game_mod: GameMod, world: WorldState, npc_manager: NpcManager) -> Self {
        let language = LanguageSettings::new(
            game_mod.player_language().to_string(),
            game_mod.native_language().map(str::to_string),
        );
        Self {
            game_mod,
            world: Mutex::new(world),
            npc_manager: Mutex::new(npc_manager),
            config: Mutex::new(endpoint_game_config()),
            conversation: Mutex::new(ConversationRuntimeState::new()),
            inference_queue: Mutex::new(None),
            client: Mutex::new(None),
            cloud_client: Mutex::new(None),
            inference_config: InferenceConfig::default(),
            language,
            emitter: Arc::new(DiscardEmitter),
        }
    }

    fn ctx(&self) -> GameLoopContext<'_> {
        GameLoopContext {
            world: &self.world,
            npc_manager: &self.npc_manager,
            config: &self.config,
            conversation: &self.conversation,
            inference_queue: &self.inference_queue,
            emitter: Arc::clone(&self.emitter),
            inference_config: &self.inference_config,
            pronunciations: &self.game_mod.pronunciations,
            endpoints: &self.game_mod.endpoints,
            client: &self.client,
            cloud_client: &self.cloud_client,
            language: self.language.clone(),
            inference_failure_messages: &self.game_mod.loading.inference_failure_messages,
            idle_messages: &self.game_mod.loading.idle_messages,
            inference_override: None,
        }
    }

    fn rules(&self) -> TurnRules {
        TurnRules {
            transport: self.game_mod.transport.default_mode().clone(),
            reaction_templates: self.game_mod.reactions.clone(),
        }
    }
}

/// The runtime configuration of the Endpoint path: the default desktop
/// configuration with the dialogue content guards off.
pub fn endpoint_game_config() -> GameConfig {
    let mut config = GameConfig::default();
    config
        .flags
        .disable(limerick_core::npc::DIALOGUE_CONTENT_GUARDS_FLAG);
    config
}

/// Model calls the phone has no Endpoint for. The pipeline sees them as
/// unavailable and uses its canned fallbacks.
/// The mod's line for each reason a failed Endpoint call can give. Keys the
/// engine does not know are ignored.
fn failure_lines(game_mod: &GameMod) -> HashMap<FailureReason, String> {
    game_mod
        .loading
        .failure_lines
        .iter()
        .filter_map(|(key, line)| Some((FailureReason::from_key(key)?, line.clone())))
        .collect()
}

fn endpoint_routes() -> InferenceRoutes {
    InferenceRoutes::live()
        .with(InferenceSubrole::TravelEncounter, RouteStatus::Unavailable)
        .with(InferenceSubrole::ArrivalReaction, RouteStatus::Unavailable)
}

/// One open game session.
pub struct Session {
    runtime: tokio::runtime::Runtime,
    live: Live,
    engine: TurnEngine,
    journal: Arc<SqliteTurnJournal>,
    session_id: String,
    pending: Option<PendingInference>,
    index: EventIndex,
    /// Provisional rows of each attempt's delivered dialogue replies, in
    /// order, with their speaker label. The committed line takes over its
    /// reply's row, so the final text replaces the streamed text in place.
    streamed: HashMap<ExecutionAttemptId, Vec<(String, Option<String>)>>,
    /// The newest answered Endpoint calls, oldest first, for a bug report.
    /// In memory only: a relaunch starts with none.
    exchanges: VecDeque<ExchangeRecord>,
    /// When the awaited call was handed to the host.
    pending_since: Option<Instant>,
    cursor: u64,
    _lock: SaveFileLock,
}

impl Session {
    /// Opens (or creates) the save and restores the engine from its journal.
    /// A request an earlier process left open ends `Interrupted`.
    pub fn open(mode: OpenMode, options: OpenOptions) -> Result<Self, OpError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| OpError::new("internal_error", error.to_string()))?;
        let game_mod = GameMod::load(&options.mod_dir).map_err(|error| {
            OpError::new(
                "content_unavailable",
                format!("the game content could not be loaded: {error}"),
            )
        })?;
        let exists = options.save_path.exists();
        if mode == OpenMode::New && exists {
            return Err(OpError::new(
                "save_exists",
                "a new game cannot replace an existing save",
            ));
        }
        let lock = SaveFileLock::try_acquire(&options.save_path)
            .ok_or_else(|| OpError::new("save_locked", "the save is open in another session"))?;
        // A save this build cannot read is kept, byte for byte, beside the
        // new game that replaces it at the save path (ADR-025 §4).
        let mut refused = false;
        let restored = if exists {
            match restore_save(&options.save_path, &game_mod) {
                Err(error) if error.code == "save_incompatible" => {
                    set_aside(&options.save_path)?;
                    refused = true;
                    None
                }
                other => Some(other?),
            }
        } else {
            None
        };
        let (world, npc_manager, db, branch_id, fresh) = match restored {
            Some((world, npcs, db, branch)) => (world, npcs, db, branch, false),
            None => {
                let (world, npcs, db, branch) = create_save(&options.save_path, &game_mod)?;
                (world, npcs, db, branch, true)
            }
        };
        let session_id = session_identity(&db, branch_id)?;
        let journal = Arc::new(SqliteTurnJournal::from_database(db, branch_id));
        let live = Live::new(game_mod, world, npc_manager);

        let engine = runtime.block_on(async {
            let mut engine = TurnEngine::restore(
                Arc::clone(&journal) as Arc<dyn limerick_core::turn::TurnJournal>,
                live.rules(),
            )
            .await?;
            engine.recover().await?;
            engine.set_routes(endpoint_routes());
            engine.enable_local_commands();
            engine.set_failure_lines(failure_lines(&live.game_mod));
            if fresh {
                let opening = {
                    let world = live.world.lock().await;
                    let npcs = live.npc_manager.lock().await;
                    render_scene(&world, &npcs)
                };
                engine
                    .describe_scene(
                        TranscriptEventId::new(format!("opening:{branch_id}")),
                        &opening,
                    )
                    .await?;
                if refused {
                    engine
                        .narrate(
                            TranscriptEventId::new(format!("refused-save:{branch_id}")),
                            limerick_core::save_compat::INCOMPATIBLE_SAVE_AT_LAUNCH_MESSAGE,
                        )
                        .await?;
                }
            }
            Ok::<_, TurnError>(engine)
        })?;

        let mut session = Self {
            runtime,
            live,
            engine,
            journal,
            session_id,
            pending: None,
            index: EventIndex::default(),
            streamed: HashMap::new(),
            exchanges: VecDeque::new(),
            pending_since: None,
            cursor: 0,
            _lock: lock,
        };
        let history = session.journal.events().map_err(OpError::storage)?;
        session.observe(&history);
        Ok(session)
    }

    fn observe(&mut self, events: &[TranscriptEvent]) {
        self.index.observe(events);
        if let Some(last) = events.iter().map(|event| event.sequence.0).max() {
            self.cursor = self.cursor.max(last);
        }
    }

    async fn journal_requests(&self) -> Result<Vec<RequestRecord>, OpError> {
        use limerick_core::turn::TurnJournal as _;
        self.journal.requests().await.map_err(OpError::storage)
    }

    fn events_json(&self, events: &[TranscriptEvent]) -> Vec<Value> {
        events
            .iter()
            .map(|event| {
                let first_attempt = event
                    .event
                    .request_id
                    .as_ref()
                    .and_then(|id| self.engine.request(id))
                    .and_then(|record| record.attempts.first())
                    .map(|attempt| attempt.id.as_str());
                wire::event(&self.session_id, event, first_attempt)
            })
            .collect()
    }

    /// Advances the world's deterministic schedule (weather, NPC schedules,
    /// tiers) to the clock before the player acts, as the desktop per-turn
    /// pump does. Background simulation that needs inference waits for the
    /// background inference seam (#2025).
    fn pump_world(&self) {
        self.runtime.block_on(async {
            let mut world = self.live.world.lock().await;
            let mut npcs = self.live.npc_manager.lock().await;
            advance_world(
                &mut world,
                &mut npcs,
                &mut rand::rng(),
                AdvanceOptions {
                    weather: WeatherMode::Single,
                    run_banshee: false,
                    gossip: GossipMode::Skip,
                    run_tier4: false,
                },
            );
        });
    }

    /// Runs `step` on, resolving calls the phone has no Endpoint for as
    /// unavailable, until the request waits on the host or ends. Returns the
    /// operation result.
    fn settle(
        &mut self,
        first: Result<TurnStep, TurnError>,
        accepted: bool,
    ) -> Result<Value, OpError> {
        let mut step = first?;
        let mut events = std::mem::take(&mut step.events);
        loop {
            match &step.status {
                TurnStatus::AwaitingInference(pending) if pending.call.endpoint.is_none() => {
                    let resolution = InferenceResolution {
                        call_id: pending.id.clone(),
                        attempt_id: pending.attempt_id.clone(),
                        base_revision: pending.base_revision,
                        outcome: InferenceOutcome::Failed {
                            kind: InferenceFailureKind::Transport,
                            message: "no Endpoint serves this workload on the phone".to_string(),
                            report: CallReport::default(),
                        },
                    };
                    let ctx = self.live.ctx();
                    step = self
                        .runtime
                        .block_on(self.engine.resume(&ctx, resolution))?;
                    events.append(&mut step.events);
                }
                _ => break,
            }
        }
        let next = match &step.status {
            TurnStatus::AwaitingInference(pending) => Some(pending.clone()),
            // An ignored operation (a stale call, attempt, or revision)
            // changed nothing: the engine still awaits the same call.
            TurnStatus::Ignored(_) => self.pending.clone(),
            _ => None,
        };
        if next.as_ref().map(|p| &p.id) != self.pending.as_ref().map(|p| &p.id) {
            self.pending_since = next.as_ref().map(|_| Instant::now());
        }
        self.pending = next;
        self.observe(&events);
        Ok(self.result(&step, events, accepted))
    }

    /// Projects an operation's events, giving each committed dialogue line
    /// the provisional row its reply streamed into.
    fn live_events_json(&mut self, events: &[TranscriptEvent]) -> Vec<Value> {
        let mut projected = self.events_json(events);
        for (event, json) in events.iter().zip(projected.iter_mut()) {
            if event.event.kind != TranscriptEventKind::NpcDialogue {
                continue;
            }
            let Some(rows) = event
                .event
                .attempt_id
                .as_ref()
                .and_then(|attempt| self.streamed.get_mut(attempt))
            else {
                continue;
            };
            let speaker = event.event.speaker.as_deref();
            if let Some(position) = rows.iter().position(|(_, s)| s.as_deref() == speaker) {
                let (item, _) = rows.remove(position);
                json["transcriptItemID"] = Value::String(item);
            }
        }
        projected
    }

    fn result(&mut self, step: &TurnStep, events: Vec<TranscriptEvent>, accepted: bool) -> Value {
        debug_assert!(step.events.is_empty(), "the step's events were collected");
        let projected = self.live_events_json(&events);
        if !matches!(step.status, TurnStatus::AwaitingInference(_))
            && let Some(attempt) = &step.attempt_id
        {
            self.streamed.remove(attempt);
        }
        let (status, terminal) = match &step.status {
            TurnStatus::AwaitingInference(_) => ("awaiting_inference", None),
            TurnStatus::AwaitingClarification(_) => ("awaiting_clarification", None),
            TurnStatus::Completed { outcome, .. } => ("completed", Some(*outcome)),
            TurnStatus::Ignored(_) => ("ignored", None),
        };
        json!({
            "accepted": accepted,
            "logicalRequestID": step.request_id.as_ref().map(LogicalRequestId::as_str),
            "attemptID": step.attempt_id.as_ref().map(ExecutionAttemptId::as_str),
            "events": projected,
            "terminalOutcome": terminal.map(wire::outcome),
            "ignored": matches!(step.status, TurnStatus::Ignored(_)),
            "error": failure_message(step, &events),
            "status": status,
            "eventCursor": wire::raw(self.cursor),
        })
    }

    /// Submits player input as a new logical request.
    pub fn submit(
        &mut self,
        text: String,
        draft_id: Option<String>,
        request_id: Option<String>,
    ) -> Result<Value, OpError> {
        if text.trim().is_empty() {
            return Err(OpError::new("rejected", "there is nothing to submit"));
        }
        // The engine runs the phone's slash commands as local turns. Any
        // other `/` input is refused here, so it never reaches the intent
        // Endpoint and leaves no request behind.
        if text.trim_start().starts_with('/') && LocalCommand::parse(&text).is_none() {
            return Err(OpError::new(
                "command_unavailable",
                "That command isn't available here. Try /help.",
            ));
        }
        self.pump_world();
        let input = TurnInput {
            request_id: request_id.map(LogicalRequestId::new),
            text,
            addressed_to: Vec::new(),
            draft_id,
        };
        let ctx = self.live.ctx();
        let step = self.runtime.block_on(self.engine.submit(&ctx, input));
        self.settle(step, true)
    }

    /// Runs a failed, stopped, or interrupted request again.
    pub fn retry(&mut self, request_id: &str) -> Result<Value, OpError> {
        self.pump_world();
        let ctx = self.live.ctx();
        let id = LogicalRequestId::new(request_id);
        let step = self.runtime.block_on(self.engine.retry(&ctx, &id));
        self.settle(step, true)
    }

    /// Answers the question a request is parked on.
    pub fn answer_clarification(
        &mut self,
        request_id: &str,
        choice: &str,
    ) -> Result<Value, OpError> {
        self.pump_world();
        let ctx = self.live.ctx();
        let id = LogicalRequestId::new(request_id);
        let step = self
            .runtime
            .block_on(self.engine.answer_clarification(&ctx, &id, choice));
        self.settle(step, true)
    }

    /// Stops the open request's current attempt (a running attempt or a
    /// question waiting for an answer). With nothing open, reports so.
    pub fn stop(&mut self) -> Result<Value, OpError> {
        let attempt = self
            .engine
            .open_request()
            .and_then(|id| self.engine.request(id))
            .and_then(RequestRecord::current_attempt)
            .map(|attempt| attempt.id.clone());
        let Some(attempt) = attempt else {
            return Ok(json!({
                "accepted": false,
                "events": [],
                "ignored": true,
                "status": "ignored",
                "eventCursor": wire::raw(self.cursor),
            }));
        };
        let ctx = self.live.ctx();
        let step = self.runtime.block_on(self.engine.stop(&ctx, &attempt));
        self.settle(step, false)
    }

    /// The invocation the host owes the engine, or `null`.
    pub fn pending_endpoint(&self) -> Value {
        let Some(pending) = &self.pending else {
            return Value::Null;
        };
        let Some(endpoint) = &pending.call.endpoint else {
            return Value::Null;
        };
        let envelope = InvocationEnvelope {
            session_id: self.session_id.clone(),
            request_id: pending.request_id.clone(),
            attempt_id: pending.attempt_id.clone(),
            base_revision: pending.base_revision,
        };
        json!({
            "callID": pending.id.as_str(),
            "logicalRequestID": pending.request_id.as_str(),
            "attemptID": pending.attempt_id.as_str(),
            "baseRevision": wire::raw(pending.base_revision.0),
            // Whether this Endpoint version streams its reply (its
            // definition declares `inferenceConfig.streaming`): the host
            // uses the `/stream` route only when it does.
            "stream": self.streams(endpoint.reference.role),
            "endpoint": {
                "role": endpoint.reference.role.manifest_key(),
                "slug": endpoint.reference.slug,
                "version": endpoint.reference.version,
            },
            "input": endpoint.invocation(&envelope),
        })
    }

    fn streams(&self, role: EndpointRole) -> bool {
        let catalog = &self.live.game_mod.endpoints;
        let file = match role {
            EndpointRole::Dialogue => catalog.dialogue.as_ref(),
            EndpointRole::Intent => catalog.intent.as_ref(),
        };
        file.is_some_and(|file| file.definition.inference_config.streaming.is_some())
    }

    fn resolution(
        &self,
        call_id: &str,
        attempt_id: &str,
        base_revision: u64,
        outcome: InferenceOutcome,
    ) -> InferenceResolution {
        InferenceResolution {
            call_id: InferenceCallId::new(call_id),
            attempt_id: ExecutionAttemptId::new(attempt_id),
            base_revision: StateRevision(base_revision),
            outcome,
        }
    }

    fn report(&self) -> CallReport {
        let model = self
            .pending
            .as_ref()
            .and_then(|pending| pending.call.endpoint.as_ref())
            .map(|endpoint| {
                format!(
                    "endpoint:{}.v{}",
                    endpoint.reference.slug, endpoint.reference.version
                )
            })
            .unwrap_or_default();
        CallReport {
            model,
            ..CallReport::default()
        }
    }

    /// Resumes the attempt with an Endpoint's validated terminal output. A
    /// resolution for anything but the awaited call is ignored.
    pub fn resolve(
        &mut self,
        call_id: &str,
        attempt_id: &str,
        base_revision: u64,
        output: &Value,
    ) -> Result<Value, OpError> {
        if !output.is_object() {
            return Err(OpError::protocol(
                "`output` must be the Endpoint's JSON object",
            ));
        }
        let awaited = self.pending.as_ref().filter(|pending| {
            pending.id.as_str() == call_id
                && pending.attempt_id.as_str() == attempt_id
                && pending.base_revision.0 == base_revision
                && pending.call.subrole == InferenceSubrole::Dialogue
        });
        if let Some(pending) = awaited {
            let row = (Provisional::item_id(call_id), self.speaker_label(pending));
            self.streamed
                .entry(pending.attempt_id.clone())
                .or_default()
                .push(row);
        }
        self.record_exchange(
            call_id,
            attempt_id,
            base_revision,
            ExchangeOutcome::Completed {
                output: output.to_string(),
            },
        );
        let outcome = InferenceOutcome::Completed {
            text: output.to_string(),
            report: self.report(),
        };
        let resolution = self.resolution(call_id, attempt_id, base_revision, outcome);
        let ctx = self.live.ctx();
        let step = self.runtime.block_on(self.engine.resume(&ctx, resolution));
        self.settle(step, false)
    }

    /// Resumes the attempt with the host's failure for the awaited call.
    pub fn fail(
        &mut self,
        call_id: &str,
        attempt_id: &str,
        base_revision: u64,
        kind: InferenceFailureKind,
        message: String,
        reason: Option<FailureReason>,
    ) -> Result<Value, OpError> {
        let described = match reason {
            Some(reason) => format!("{}/{}", failure_kind_key(kind), reason.key()),
            None => failure_kind_key(kind).to_string(),
        };
        self.record_exchange(
            call_id,
            attempt_id,
            base_revision,
            ExchangeOutcome::Failed {
                kind: described,
                message: message.clone(),
            },
        );
        let outcome = InferenceOutcome::Failed {
            kind,
            message,
            report: CallReport {
                reason,
                ..self.report()
            },
        };
        let resolution = self.resolution(call_id, attempt_id, base_revision, outcome);
        let ctx = self.live.ctx();
        let step = self.runtime.block_on(self.engine.resume(&ctx, resolution));
        self.settle(step, false)
    }

    /// Keeps an answer to the awaited Endpoint call for a bug report. An
    /// answer to anything else is ignored by the engine and not recorded.
    fn record_exchange(
        &mut self,
        call_id: &str,
        attempt_id: &str,
        base_revision: u64,
        outcome: ExchangeOutcome,
    ) {
        let Some(pending) = self.pending.as_ref().filter(|pending| {
            pending.id.as_str() == call_id
                && pending.attempt_id.as_str() == attempt_id
                && pending.base_revision.0 == base_revision
        }) else {
            return;
        };
        let Some(endpoint) = &pending.call.endpoint else {
            return;
        };
        let asked = match &endpoint.input {
            EndpointInput::Dialogue(input) => format!(
                "{} at {}: {}",
                input.speaker.display_name, input.current_location.display_name, input.player_input
            ),
            EndpointInput::Intent(input) => input.player_input.clone(),
        };
        let record = ExchangeRecord {
            endpoint: format!(
                "{}.v{}",
                endpoint.reference.slug, endpoint.reference.version
            ),
            duration_ms: self
                .pending_since
                .map(|since| u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)),
            asked,
            outcome,
        };
        if self.exchanges.len() == MAX_RECORDED_EXCHANGES {
            self.exchanges.pop_front();
        }
        self.exchanges.push_back(record);
    }

    /// The bounded plain-text report the app files as a GitHub issue through
    /// `limerick-bug-report` (#2022). Reads only: nothing is journaled and no state
    /// changes.
    pub fn bug_report(&self, description: &str, build: Option<String>) -> Result<Value, OpError> {
        let (events, _) = self
            .journal
            .events_before(u64::MAX, BUG_REPORT_EVENTS)
            .map_err(OpError::storage)?;
        let transcript = events
            .iter()
            .filter_map(|event| {
                let text = event.event.content.as_deref()?.trim();
                if text.is_empty() {
                    return None;
                }
                let speaker = match event.event.kind {
                    TranscriptEventKind::PlayerCommand => {
                        return Some(TranscriptLine {
                            speaker: None,
                            from_player: true,
                            text: text.to_string(),
                        });
                    }
                    TranscriptEventKind::NpcDialogue | TranscriptEventKind::ActionResult => {
                        event.event.speaker.clone()
                    }
                    TranscriptEventKind::Narration
                    | TranscriptEventKind::SceneChanged
                    | TranscriptEventKind::ClarificationRequired
                    | TranscriptEventKind::Error => None,
                    _ => return None,
                };
                Some(TranscriptLine {
                    speaker,
                    from_player: false,
                    text: text.to_string(),
                })
            })
            .collect();
        let (scene, time_of_day, weather, present) = self.runtime.block_on(async {
            let world = self.live.world.lock().await;
            let npcs = self.live.npc_manager.lock().await;
            let mut people: Vec<_> = npcs.npcs_at(world.player_location);
            people.sort_by_key(|npc| npc.id.0);
            (
                world.current_location().name.clone(),
                world.clock.time_of_day().to_string(),
                world.weather.to_string(),
                people
                    .iter()
                    .map(|npc| capitalize_first(npcs.display_name(npc)))
                    .collect::<Vec<_>>(),
            )
        });
        let open_request = match (&self.pending, self.engine.open_request()) {
            (Some(pending), _) => Some(match &pending.call.endpoint {
                Some(endpoint) => format!(
                    "waiting on {}.v{}",
                    endpoint.reference.slug, endpoint.reference.version
                ),
                None => "waiting on a model call".to_string(),
            }),
            (None, Some(_)) => Some("waiting on the player's answer".to_string()),
            (None, None) => None,
        };
        let report = MobileReport {
            description: description.to_string(),
            build,
            contract_version: format!("{}.{}", wire::CONTRACT_VERSION.0, wire::CONTRACT_VERSION.1),
            scene,
            time_of_day,
            weather,
            present,
            open_request,
            transcript,
            exchanges: self.exchanges.iter().cloned().collect(),
        };
        let text = mobile_report::compose(&report, mobile_report::REPORT_BUDGET);
        Ok(json!({
            "text": text,
            "characters": text.chars().count(),
        }))
    }

    /// The label a dialogue call's committed line carries: the speaker as
    /// the player knows them (a description until introduced).
    fn speaker_label(&self, pending: &PendingInference) -> Option<String> {
        pending
            .call
            .endpoint
            .as_ref()
            .and_then(
                |endpoint| match (&endpoint.reference.role, &endpoint.input) {
                    (EndpointRole::Dialogue, EndpointInput::Dialogue(input)) => input
                        .speaker
                        .id
                        .strip_prefix("npc-")
                        .and_then(|id| id.parse::<u32>().ok()),
                    _ => None,
                },
            )
            .and_then(|id| {
                self.runtime.block_on(async {
                    let npcs = self.live.npc_manager.lock().await;
                    npcs.get(NpcId(id))
                        .map(|npc| capitalize_first(npcs.display_name(npc)))
                })
            })
    }

    /// Projects a streamed text delta of the awaited dialogue call as
    /// provisional transcript text. Frames for any other call are ignored;
    /// nothing is journaled and no state changes.
    pub fn frame(&self, call_id: &str, attempt_id: &str, sequence: u64, text: &str) -> Value {
        let ignored = || {
            json!({
                "accepted": false,
                "events": [],
                "ignored": true,
                "status": "ignored",
                "eventCursor": wire::raw(self.cursor),
            })
        };
        let Some(pending) = self.pending.as_ref().filter(|pending| {
            pending.id.as_str() == call_id
                && pending.attempt_id.as_str() == attempt_id
                && pending.call.subrole == InferenceSubrole::Dialogue
        }) else {
            return ignored();
        };
        if text.is_empty() {
            return ignored();
        }
        let speaker = self.speaker_label(pending);
        let event = Provisional {
            session_id: &self.session_id,
            request_id: pending.request_id.as_str(),
            attempt_id,
            call_id,
            speaker: speaker.as_deref(),
            cursor: self.cursor,
            stream_sequence: sequence,
            text,
        };
        json!({
            "accepted": false,
            "logicalRequestID": pending.request_id.as_str(),
            "attemptID": attempt_id,
            "events": [event.to_json()],
            "ignored": false,
            "status": "awaiting_inference",
            "eventCursor": wire::raw(self.cursor),
        })
    }

    /// At most `limit` events after `after`, oldest first.
    pub fn events_after(&self, after: u64, limit: usize) -> Result<Value, OpError> {
        let (events, more) = self
            .journal
            .events_after(after, limit.clamp(1, MAX_PAGE))
            .map_err(OpError::storage)?;
        let cursor = events.last().map_or(after, |event| event.sequence.0);
        Ok(json!({
            "events": self.events_json(&events),
            "cursor": wire::raw(cursor),
            "hasMore": more,
        }))
    }

    /// The newest `limit` events before `before`, oldest first. The returned
    /// cursor is the oldest of them, to page further back from.
    pub fn events_before(&self, before: u64, limit: usize) -> Result<Value, OpError> {
        let (events, older) = self
            .journal
            .events_before(before, limit.clamp(1, MAX_PAGE))
            .map_err(OpError::storage)?;
        let cursor = events.first().map_or(before, |event| event.sequence.0);
        Ok(json!({
            "events": self.events_json(&events),
            "cursor": wire::raw(cursor),
            "hasMore": older,
        }))
    }

    /// The authoritative presentation snapshot: read model, requests, and
    /// the newest events.
    pub fn snapshot(&self) -> Result<Value, OpError> {
        let (events, older) = self
            .journal
            .events_before(u64::MAX, MAX_PAGE)
            .map_err(OpError::storage)?;
        let records = self.runtime.block_on(self.journal_requests())?;
        let open = self.engine.open_request().cloned();
        let keep_from = records.len().saturating_sub(MAX_SNAPSHOT_REQUESTS);
        let requests: Vec<Value> = records
            .iter()
            .enumerate()
            .filter(|(position, record)| {
                *position >= keep_from || open.as_ref() == Some(&record.id)
            })
            .map(|(_, record)| wire::request(record, &self.index))
            .collect();
        let events = self.events_json(&events);
        Ok(json!({
            "contractVersion": wire::contract_version(),
            "sessionID": self.session_id,
            "stateRevision": wire::raw(self.engine.revision().0),
            "eventCursor": wire::raw(self.cursor),
            "readModel": self.read_model(),
            "requests": requests,
            "activeRequestID": open.as_ref().map(LogicalRequestId::as_str),
            "events": events,
            "hasOlderEvents": older,
            "pendingInference": self.pending.is_some(),
        }))
    }

    fn read_model(&self) -> Value {
        self.runtime.block_on(async {
            let world = self.live.world.lock().await;
            let npcs = self.live.npc_manager.lock().await;
            let location = world.current_location();
            let transport = self.live.game_mod.transport.default_mode();
            let mut people: Vec<_> = npcs.npcs_at(world.player_location);
            people.sort_by_key(|npc| npc.id.0);
            let mut everyone: Vec<_> = npcs.all_npcs().collect();
            everyone.sort_by_key(|npc| npc.id.0);
            json!({
                "scene": {
                    "id": world.player_location.0.to_string(),
                    "name": location.name,
                    "detail": render_look_text(
                        &world,
                        &npcs,
                        transport.speed_m_per_s,
                        &transport.label,
                        false,
                    ),
                },
                "nearbyPeople": people.iter().map(|npc| json!({
                    "id": format!("npc-{}", npc.id.0),
                    "displayName": npcs.display_name(npc),
                })).collect::<Vec<_>>(),
                "timeOfDay": world.clock.time_of_day().to_string(),
                "weather": world.weather.to_string(),
                "commands": advertised_commands().map(|command| json!({
                    "name": command.name,
                    "summary": command.summary,
                })).collect::<Vec<_>>(),
                // Completion: every command the phone runs, the words that
                // may follow each, and every NPC a name may complete to.
                "commandCompletions": COMMANDS.iter().map(|command| command.completion()).collect::<Vec<_>>(),
                "everyone": everyone.iter().map(|npc| json!({
                    "id": format!("npc-{}", npc.id.0),
                    "name": npc.name,
                })).collect::<Vec<_>>(),
            })
        })
    }
}

/// A new save at `path`: the mod's opening world, persisted as the main
/// branch's first snapshot.
fn create_save(
    path: &Path,
    game_mod: &GameMod,
) -> Result<(WorldState, NpcManager, Database, i64), OpError> {
    let (world, mut npcs) = load_fresh_world_and_npcs(Some(game_mod), &game_mod.mod_dir)
        .map_err(|error| OpError::new("content_unavailable", error))?;
    npcs.assign_tiers(&world, &[]);
    let db = Database::open(path).map_err(OpError::storage)?;
    let branch = db
        .find_branch("main")
        .map_err(OpError::storage)?
        .ok_or_else(|| OpError::storage("a new save has no main branch"))?;
    db.save_snapshot(branch.id, &GameSnapshot::capture(&world, &npcs))
        .map_err(OpError::storage)?;
    Ok((world, npcs, db, branch.id))
}

/// The save at `path`, checked against the loaded content and restored to
/// its main branch's latest state. A save that cannot be read is refused
/// and left untouched.
fn restore_save(
    path: &Path,
    game_mod: &GameMod,
) -> Result<(WorldState, NpcManager, Database, i64), OpError> {
    let identity = game_mod.content_identity();
    let inspection =
        limerick_core::save_compat::check_save(path, Some(&identity)).map_err(|error| {
            if limerick_core::save_compat::is_incompatible(&error) {
                OpError::new(
                    "save_incompatible",
                    limerick_core::save_compat::refusal_message(path, &error),
                )
            } else {
                OpError::storage(error)
            }
        })?;
    let db = Database::open_inspected(path, &inspection).map_err(OpError::storage)?;
    let branch = db
        .find_branch("main")
        .map_err(OpError::storage)?
        .ok_or_else(|| OpError::storage("the save has no main branch"))?;
    let data = db
        .load_recovery_data(branch.id)
        .map_err(OpError::storage)?
        .ok_or_else(|| OpError::storage("the save has no snapshot"))?;
    let (mut world, mut npcs) = load_fresh_world_and_npcs(Some(game_mod), &game_mod.mod_dir)
        .map_err(|error| OpError::new("content_unavailable", error))?;
    RecoveryBundle {
        snapshot_id: data.snapshot_id,
        snapshot: data.snapshot,
        journal: data.journal,
    }
    .restore(&mut world, &mut npcs);
    Ok((world, npcs, db, branch.id))
}

/// Moves a refused save (and its SQLite sidecars) aside unchanged:
/// `game.sqlite` becomes `game.refused-<unix seconds>.sqlite`.
fn set_aside(path: &Path) -> Result<PathBuf, OpError> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("save");
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("sqlite");
    let aside = path.with_file_name(format!("{stem}.refused-{stamp}.{extension}"));
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
        if sidecar.exists() {
            let moved = PathBuf::from(format!("{}{suffix}", aside.display()));
            std::fs::rename(&sidecar, moved).map_err(OpError::storage)?;
        }
    }
    std::fs::rename(path, &aside).map_err(OpError::storage)?;
    Ok(aside)
}

/// A stable identity for the save's session: its branch and the time its
/// first snapshot was written, so a new game in the same file is a new
/// session.
fn session_identity(db: &Database, branch_id: i64) -> Result<String, OpError> {
    let first = db
        .branch_log(branch_id)
        .map_err(OpError::storage)?
        .into_iter()
        .min_by_key(|snapshot| snapshot.id)
        .map(|snapshot| snapshot.real_time)
        .unwrap_or_default();
    Ok(format!("rundale:{branch_id}:{first}"))
}

/// The player-facing message of an attempt that ended failed or
/// interrupted, read from the operation's own `Error` event.
fn failure_message(step: &TurnStep, events: &[TranscriptEvent]) -> Option<String> {
    match step.status {
        TurnStatus::Completed {
            outcome: TerminalOutcome::Failed | TerminalOutcome::Interrupted,
            ..
        } => events
            .iter()
            .rev()
            .find(|event| event.event.kind == TranscriptEventKind::Error)
            .and_then(|event| event.event.content.clone()),
        _ => None,
    }
}

/// The FFI spelling of a failure kind, as the host sends it.
fn failure_kind_key(kind: InferenceFailureKind) -> &'static str {
    match kind {
        InferenceFailureKind::Transport => "transport",
        InferenceFailureKind::Protocol => "protocol",
        InferenceFailureKind::TimedOut => "timed_out",
        InferenceFailureKind::Interrupted => "interrupted",
    }
}
