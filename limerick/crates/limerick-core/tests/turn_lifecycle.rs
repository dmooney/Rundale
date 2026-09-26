//! Request-lifecycle integration tests for the turn engine.
//!
//! Every test runs the real shared pipeline (`game_loop::handle_game_input`)
//! on the `mods/rundale` world through [`TurnEngine`], with a scripted host
//! that answers each suspended inference call. Behaviour is ported from the
//! `ios-port` oracle listed in `docs/design/portable-turn-api.md` §2.4.

use std::path::PathBuf;
use std::sync::Arc;

use limerick_core::config::{InferenceConfig, InferenceSubrole};
use limerick_core::game_loop::GameLoopContext;
use limerick_core::game_loop::inference::InProcessInference;
use limerick_core::game_mod::GameMod;
use limerick_core::inference::{AnyClient, InferenceQueue, InferenceRequest, InferenceResponse};
use limerick_core::ipc::{CapturingEmitter, ConversationRuntimeState, EventEmitter, GameConfig};
use limerick_core::npc::manager::NpcManager;
use limerick_core::npc::reactions::ReactionTemplates;
use limerick_core::persistence::GameSnapshot;
use limerick_core::turn::{
    ExecutionAttemptId, IgnoredReason, InferenceResolution, LifecycleError, LogicalRequestId,
    MemoryTurnJournal, PendingInference, RequestPhase, StateRevision, TerminalOutcome,
    TranscriptEvent, TranscriptEventKind, TurnEngine, TurnError, TurnInput, TurnRules, TurnStatus,
    TurnStep, drive_in_process,
};
use limerick_core::turn_inference::{CallReport, InferenceFailureKind, InferenceOutcome};
use limerick_core::world::transport::TransportMode;
use limerick_core::world::{LocationId, WorldState};
use tokio::sync::Mutex;

const NPC_LINE: &str = "God save ye kindly. 'Tis a soft day for the time of year.";
const ENCOUNTER_LINE: &str = "A drover passes with two heifers and lifts his hat.";
const REACTION_LINE: &str = "Ye're welcome in, stranger.";

fn rundale_mod_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale")
}

/// The runtime-owned live state a host borrows into each engine call.
struct Live {
    world: Mutex<WorldState>,
    npc_manager: Mutex<NpcManager>,
    config: Mutex<GameConfig>,
    conversation: Mutex<ConversationRuntimeState>,
    inference_queue: Mutex<Option<InferenceQueue>>,
    client: Mutex<Option<AnyClient>>,
    cloud_client: Mutex<Option<AnyClient>>,
    inference_config: InferenceConfig,
    emitter: Arc<CapturingEmitter>,
    reaction_templates: ReactionTemplates,
}

impl Live {
    fn rundale() -> Self {
        let mod_dir = rundale_mod_dir();
        let game_mod = GameMod::load(&mod_dir).expect("load mods/rundale");
        let (mut world, npc_manager) =
            limerick_core::game_loop::load_fresh_world_and_npcs(Some(&game_mod), &mod_dir)
                .expect("load the rundale world");
        // A paused clock keeps game time, and so schedules and encounter
        // rolls, independent of wall-clock time.
        world.clock.pause();
        Self {
            world: Mutex::new(world),
            npc_manager: Mutex::new(npc_manager),
            config: Mutex::new(GameConfig::default()),
            conversation: Mutex::new(ConversationRuntimeState::new()),
            inference_queue: Mutex::new(None),
            client: Mutex::new(None),
            cloud_client: Mutex::new(None),
            inference_config: InferenceConfig::default(),
            emitter: Arc::new(CapturingEmitter::new()),
            reaction_templates: game_mod.reactions.clone(),
        }
    }

    fn ctx(&self) -> GameLoopContext<'_> {
        GameLoopContext {
            world: &self.world,
            npc_manager: &self.npc_manager,
            config: &self.config,
            conversation: &self.conversation,
            inference_queue: &self.inference_queue,
            emitter: self.emitter.clone() as Arc<dyn EventEmitter>,
            inference_config: &self.inference_config,
            pronunciations: &[],
            client: &self.client,
            cloud_client: &self.cloud_client,
            language: limerick_core::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
            inference_override: None,
        }
    }

    fn rules(&self) -> TurnRules {
        TurnRules {
            transport: TransportMode {
                id: "walking".to_string(),
                label: "on foot".to_string(),
                speed_m_per_s: 1.2,
            },
            reaction_templates: self.reaction_templates.clone(),
        }
    }

    /// Everything a turn may change, for before/after comparison.
    async fn fingerprint(&self) -> String {
        let world = self.world.lock().await;
        let npcs = self.npc_manager.lock().await;
        let conversation = self.conversation.lock().await;
        let snapshot = serde_json::to_string(&GameSnapshot::capture(&world, &npcs)).unwrap();
        format!(
            "{snapshot}|{:?}|{:?}|{:?}|{}",
            conversation.transcript,
            conversation.last_player_input,
            conversation.last_player_activity,
            world.text_log.len()
        )
    }

    async fn first_npc_here(&self) -> String {
        let world = self.world.lock().await;
        let npcs = self.npc_manager.lock().await;
        let mut here: Vec<String> = npcs
            .npcs_at(world.player_location)
            .iter()
            .map(|npc| npc.name.clone())
            .collect();
        here.sort();
        here.into_iter()
            .next()
            .expect("an NPC at the start location")
    }
}

fn engine(live: &Live) -> (TurnEngine, Arc<MemoryTurnJournal>) {
    let journal = Arc::new(MemoryTurnJournal::new());
    (TurnEngine::new(journal.clone(), live.rules()), journal)
}

fn completed(text: &str) -> InferenceOutcome {
    InferenceOutcome::Completed {
        text: text.to_string(),
        report: CallReport::default(),
    }
}

fn failed(kind: InferenceFailureKind) -> InferenceOutcome {
    InferenceOutcome::Failed {
        kind,
        message: "scripted failure".to_string(),
        report: CallReport::default(),
    }
}

fn npc_reply() -> String {
    serde_json::json!({
        "dialogue": NPC_LINE,
        "action": "sets down a creel of turf",
        "mood": "content",
        "language_hints": [],
        "assigned_task": null,
        "internal_thought": null
    })
    .to_string()
}

/// The scripted host: a canned answer for every workload.
fn scripted(pending: &PendingInference, intent_target: &str) -> InferenceOutcome {
    match pending.call.subrole {
        // An empty target classifies the input as speech to whoever is
        // addressed; otherwise as travel to the target.
        InferenceSubrole::Intent if intent_target.is_empty() => completed(
            &serde_json::json!({"intent": "talk", "target": null, "dialogue": pending.call.prompt})
                .to_string(),
        ),
        InferenceSubrole::Intent => completed(
            &serde_json::json!({"intent": "move", "target": intent_target, "dialogue": null})
                .to_string(),
        ),
        InferenceSubrole::Dialogue => completed(&npc_reply()),
        InferenceSubrole::TravelEncounter => completed(ENCOUNTER_LINE),
        InferenceSubrole::ArrivalReaction => completed(REACTION_LINE),
        other => panic!("unexpected in-turn workload {other:?}"),
    }
}

fn resolution(pending: &PendingInference, outcome: InferenceOutcome) -> InferenceResolution {
    InferenceResolution {
        call_id: pending.id.clone(),
        attempt_id: pending.attempt_id.clone(),
        base_revision: pending.base_revision,
        outcome,
    }
}

fn awaiting(step: &TurnStep) -> &PendingInference {
    match &step.status {
        TurnStatus::AwaitingInference(pending) => pending,
        other => panic!("expected an inference request, got {other:?}"),
    }
}

/// Answers every call with the scripted host until the attempt ends.
/// Returns the final step, all events, and the workloads called.
async fn run_scripted(
    engine: &mut TurnEngine,
    live: &Live,
    mut step: TurnStep,
    intent_target: &str,
) -> (TurnStep, Vec<TranscriptEvent>, Vec<InferenceSubrole>) {
    let mut events = step.events.clone();
    let mut calls = Vec::new();
    while let TurnStatus::AwaitingInference(pending) = &step.status {
        calls.push(pending.call.subrole);
        assert_eq!(
            pending.streaming,
            pending.call.subrole == InferenceSubrole::ArrivalReaction,
            "only arrival reactions are consumed as a stream"
        );
        let outcome = scripted(pending, intent_target);
        step = engine
            .resume(&live.ctx(), resolution(pending, outcome))
            .await
            .expect("resume");
        events.extend(step.events.iter().cloned());
    }
    (step, events, calls)
}

/// Answers the intent call (as speech) and returns the step suspended on
/// the dialogue call.
async fn to_dialogue(engine: &mut TurnEngine, live: &Live, step: TurnStep) -> TurnStep {
    let pending = awaiting(&step).clone();
    assert_eq!(pending.call.subrole, InferenceSubrole::Intent);
    let next = engine
        .resume(&live.ctx(), resolution(&pending, scripted(&pending, "")))
        .await
        .expect("resume the intent call");
    assert_eq!(awaiting(&next).call.subrole, InferenceSubrole::Dialogue);
    next
}

fn talk_to(npc: &str, text: &str) -> TurnInput {
    TurnInput {
        request_id: None,
        text: text.to_string(),
        addressed_to: vec![npc.to_string()],
        draft_id: None,
    }
}

fn kinds(events: &[TranscriptEvent]) -> Vec<TranscriptEventKind> {
    events
        .iter()
        .map(|event| event.event.kind.clone())
        .collect()
}

fn assert_strictly_increasing(events: &[TranscriptEvent]) {
    for pair in events.windows(2) {
        assert!(
            pair[0].sequence < pair[1].sequence,
            "sequences must strictly increase: {:?}",
            events.iter().map(|e| e.sequence).collect::<Vec<_>>()
        );
    }
}

/// The engine-side contract the RundaleKit reducer relies on (design §2.4):
/// no event of an attempt follows its terminal event, only a succeeded
/// terminal carries a revision, sequences increase, and the command item is
/// stable across attempts.
fn assert_reducer_contract(events: &[TranscriptEvent], request: &LogicalRequestId) {
    assert_strictly_increasing(events);
    let mut ended: Vec<ExecutionAttemptId> = Vec::new();
    for event in events {
        let event = &event.event;
        if let Some(attempt) = &event.attempt_id {
            assert!(
                !ended.contains(attempt),
                "an event of attempt {attempt} follows its terminal event: {:?}",
                event.kind
            );
        }
        if event.kind == TranscriptEventKind::ResponseCompleted {
            assert_eq!(
                event.state_revision.is_some(),
                event.terminal_outcome == Some(TerminalOutcome::Succeeded),
                "only a succeeded terminal carries a committed revision"
            );
            ended.extend(event.attempt_id.clone());
        }
        if event.kind == TranscriptEventKind::PlayerCommand {
            assert_eq!(event.item_id, Some(request.command_item()));
        }
    }
}

// Oracle: `acceptance_precedes_endpoint_and_has_grounding` and
// `successful_candidate_commits_one_exchange_and_retry_is_rejected`.
#[tokio::test]
async fn dialogue_is_accepted_before_inference_and_commits_one_exchange() {
    let live = Live::rundale();
    let (mut engine, journal) = engine(&live);
    let npc = live.first_npc_here().await;
    let before = live.fingerprint().await;

    let step = engine
        .submit(
            &live.ctx(),
            TurnInput {
                request_id: Some(LogicalRequestId::new("logical-1")),
                draft_id: Some("draft-7".to_string()),
                ..talk_to(&npc, "Good day to you. Any news of the fair?")
            },
        )
        .await
        .unwrap();
    // Acceptance and the command are journaled before interpretation.
    assert_eq!(
        kinds(&step.events),
        vec![TranscriptEventKind::PlayerCommand]
    );
    assert_eq!(awaiting(&step).call.subrole, InferenceSubrole::Intent);
    assert_eq!(journal.events().len(), 1);
    let step = to_dialogue(&mut engine, &live, step).await;
    let pending = awaiting(&step).clone();
    assert_eq!(pending.base_revision, StateRevision(0));
    assert_eq!(pending.request_id, LogicalRequestId::new("logical-1"));
    assert!(
        pending.call.prompt.contains("fair"),
        "the call carries the rendered prompt"
    );

    // Nothing is visible or authoritative while the attempt is suspended.
    assert!(step.events.is_empty());
    let command = &journal.events()[0].event;
    assert_eq!(
        command.content.as_deref(),
        Some("Good day to you. Any news of the fair?")
    );
    assert_eq!(
        command.metadata.get("draftID").map(String::as_str),
        Some("draft-7")
    );
    let record = journal.request(&pending.request_id).unwrap();
    assert_eq!(record.phase, RequestPhase::Executing);
    assert_eq!(live.fingerprint().await, before);
    assert!(live.emitter.is_empty());

    let done = engine
        .resume(&live.ctx(), resolution(&pending, completed(&npc_reply())))
        .await
        .unwrap();
    assert!(matches!(
        done.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Succeeded,
            revision: Some(StateRevision(1))
        }
    ));
    assert_eq!(engine.revision(), StateRevision(1));
    let lines: Vec<_> = done
        .events
        .iter()
        .filter(|event| event.event.kind == TranscriptEventKind::NpcDialogue)
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "one committed exchange: {:?}",
        kinds(&done.events)
    );
    // An NPC the player has not met is labelled by description.
    assert!(
        lines[0]
            .event
            .speaker
            .as_deref()
            .is_some_and(|s| !s.is_empty())
    );
    assert!(
        lines[0]
            .event
            .content
            .as_deref()
            .unwrap()
            .contains("soft day")
    );

    // Commit installed the candidate and released its emissions.
    assert_ne!(live.fingerprint().await, before);
    assert_eq!(
        live.conversation.lock().await.last_player_input.as_deref(),
        Some("Good day to you. Any news of the fair?")
    );
    assert_eq!(live.emitter.events(), done.emissions);
    assert!(
        done.emissions
            .iter()
            .any(|(name, _)| name == "stream-turn-end")
    );
    assert!(
        journal
            .request(&pending.request_id)
            .unwrap()
            .has_committed()
    );
    assert_eq!(journal.task_batches().len(), 1);

    // A committed request never runs again; a late callback changes nothing.
    let after = live.fingerprint().await;
    assert!(matches!(
        engine.retry(&live.ctx(), &pending.request_id).await,
        Err(TurnError::Lifecycle(LifecycleError::AlreadyCommitted(_)))
    ));
    let late = engine
        .resume(&live.ctx(), resolution(&pending, completed(&npc_reply())))
        .await
        .unwrap();
    assert!(matches!(
        late.status,
        TurnStatus::Ignored(IgnoredReason::AttemptTerminal)
    ));
    assert_eq!(live.fingerprint().await, after);
    assert_reducer_contract(&journal.events(), &pending.request_id);
}

// Oracle: `stop_wins_and_late_candidate_cannot_commit`.
#[tokio::test]
async fn stop_discards_the_attempt_and_a_late_result_cannot_commit() {
    let live = Live::rundale();
    let (mut engine, journal) = engine(&live);
    let npc = live.first_npc_here().await;
    let before = live.fingerprint().await;

    let step = engine
        .submit(&live.ctx(), talk_to(&npc, "Tell me about the wall."))
        .await
        .unwrap();
    let step = to_dialogue(&mut engine, &live, step).await;
    let pending = awaiting(&step).clone();
    let stopped = engine.stop(&live.ctx(), &pending.attempt_id).await.unwrap();
    assert!(matches!(
        stopped.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Cancelled,
            revision: None
        }
    ));
    assert_eq!(
        kinds(&stopped.events),
        vec![TranscriptEventKind::ResponseCompleted]
    );

    let late = engine
        .resume(&live.ctx(), resolution(&pending, completed(&npc_reply())))
        .await
        .unwrap();
    assert!(matches!(
        late.status,
        TurnStatus::Ignored(IgnoredReason::AttemptTerminal)
    ));
    let again = engine.stop(&live.ctx(), &pending.attempt_id).await.unwrap();
    assert!(matches!(
        again.status,
        TurnStatus::Ignored(IgnoredReason::AttemptTerminal)
    ));

    // Pre-inference mutations (recorded input, activity) were candidate-only.
    assert_eq!(live.fingerprint().await, before);
    assert!(live.emitter.is_empty());
    let record = journal.request(&pending.request_id).unwrap();
    assert_eq!(record.terminal_outcome, Some(TerminalOutcome::Cancelled));
    assert_eq!(engine.revision(), StateRevision(0));
    assert_reducer_contract(&journal.events(), &pending.request_id);
}

// Oracle: `failed_attempt_can_retry_with_new_attempt_identity`,
// `correlated_endpoint_failure_is_terminal_and_retryable`, and RundaleKit
// `testRetryCreatesCurrentAttemptBeforeRejectingOldAttemptEvents`.
#[tokio::test]
async fn a_failed_dialogue_commits_nothing_and_retries_as_a_new_attempt() {
    let live = Live::rundale();
    let (mut engine, journal) = engine(&live);
    let npc = live.first_npc_here().await;
    let before = live.fingerprint().await;

    let step = engine
        .submit(&live.ctx(), talk_to(&npc, "Is there work going here?"))
        .await
        .unwrap();
    let step = to_dialogue(&mut engine, &live, step).await;
    let first = awaiting(&step).clone();
    let failed_step = engine
        .resume(
            &live.ctx(),
            resolution(&first, failed(InferenceFailureKind::Transport)),
        )
        .await
        .unwrap();
    assert!(matches!(
        failed_step.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Failed,
            revision: None
        }
    ));
    let error = failed_step
        .events
        .iter()
        .find(|event| event.event.kind == TranscriptEventKind::Error)
        .expect("a player-safe error event");
    assert_eq!(
        error.event.metadata.get("errorKind").map(String::as_str),
        Some("transport")
    );
    assert!(failed_step.emissions.is_empty());
    assert_eq!(
        live.fingerprint().await,
        before,
        "failure has no authoritative effect"
    );
    assert!(live.emitter.is_empty());

    let retry = engine.retry(&live.ctx(), &first.request_id).await.unwrap();
    assert_eq!(
        kinds(&retry.events),
        vec![TranscriptEventKind::Progress],
        "the retry marker precedes every event of the new attempt"
    );
    let second = awaiting(&retry).clone();
    assert_eq!(
        second.call.subrole,
        InferenceSubrole::Intent,
        "the retry re-runs the whole turn"
    );
    assert_ne!(second.attempt_id, first.attempt_id);
    assert_eq!(second.request_id, first.request_id);
    assert_eq!(
        retry.events[0].event.attempt_id.as_ref(),
        Some(&second.attempt_id)
    );

    // A late failure from the old attempt is stale and leaves the retry open.
    let stale = engine
        .resume(
            &live.ctx(),
            resolution(&first, failed(InferenceFailureKind::Transport)),
        )
        .await
        .unwrap();
    assert!(matches!(
        stale.status,
        TurnStatus::Ignored(IgnoredReason::StaleAttempt)
    ));
    assert_eq!(
        engine.request(&first.request_id).unwrap().phase,
        RequestPhase::Executing
    );

    let (done, _, _) = run_scripted(&mut engine, &live, retry, "").await;
    assert!(matches!(
        done.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Succeeded,
            ..
        }
    ));
    let record = journal.request(&first.request_id).unwrap();
    assert_eq!(record.attempts.len(), 2);
    assert!(record.has_committed());
    assert_eq!(
        live.conversation
            .lock()
            .await
            .transcript
            .iter()
            .filter(|line| line.text.contains("Is there work going here?"))
            .count(),
        1,
        "the retried line is recorded once"
    );
    assert_reducer_contract(&journal.events(), &first.request_id);
}

// Oracle: `correlated_endpoint_failure_is_terminal_and_retryable` (worker
// interruption) and `authored_phase3_fact_is_grounded_but_invented_place_is_rejected`
// (a reply the canonical apply rejects fails the attempt).
#[tokio::test]
async fn an_interrupted_call_ends_interrupted_and_a_rejected_reply_fails() {
    let live = Live::rundale();
    let (mut engine, journal) = engine(&live);
    let npc = live.first_npc_here().await;

    let step = engine
        .submit(&live.ctx(), talk_to(&npc, "Tell me about the wall."))
        .await
        .unwrap();
    let step = to_dialogue(&mut engine, &live, step).await;
    let pending = awaiting(&step).clone();
    let interrupted = engine
        .resume(
            &live.ctx(),
            resolution(&pending, failed(InferenceFailureKind::Interrupted)),
        )
        .await
        .unwrap();
    assert!(matches!(
        interrupted.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Interrupted,
            revision: None
        }
    ));

    let retry = engine
        .retry(&live.ctx(), &pending.request_id)
        .await
        .unwrap();
    let retry = to_dialogue(&mut engine, &live, retry).await;
    let second = awaiting(&retry).clone();
    let rejected = engine
        .resume(&live.ctx(), resolution(&second, completed("")))
        .await
        .unwrap();
    assert!(matches!(
        rejected.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Failed,
            revision: None
        }
    ));
    let error = rejected
        .events
        .iter()
        .find(|event| event.event.kind == TranscriptEventKind::Error)
        .unwrap();
    assert_eq!(
        error.event.metadata.get("errorKind").map(String::as_str),
        Some("semantic_validation")
    );
    assert_eq!(engine.revision(), StateRevision(0));
    assert_reducer_contract(&journal.events(), &pending.request_id);
}

// Design §4.3 failure policy: a failed autonomous-chain line stops the
// chain but the attempt still commits what was said.
#[tokio::test]
async fn a_failed_chain_line_stops_the_chain_and_the_turn_still_commits() {
    let live = Live::rundale();
    live.config
        .lock()
        .await
        .flags
        .enable(limerick_core::game_loop::npc_turn::AUTONOMOUS_NPC_CHAIN_FLAG);
    let npc = live.first_npc_here().await;
    {
        // A bystander who knows the speaker is motivated to chime in.
        let world = live.world.lock().await;
        let mut npcs = live.npc_manager.lock().await;
        let speaker = npcs
            .find_by_name(&npc, world.player_location)
            .expect("speaker")
            .id;
        let mut here: Vec<_> = npcs
            .npcs_at(world.player_location)
            .iter()
            .map(|other| other.id)
            .filter(|id| *id != speaker)
            .collect();
        here.sort();
        npcs.get_mut(here[0]).unwrap().relationships.insert(
            speaker,
            limerick_core::npc::types::Relationship::new(
                limerick_core::npc::types::RelationshipKind::Friend,
                0.8,
            ),
        );
    }
    let (mut engine, journal) = engine(&live);

    let step = engine
        .submit(&live.ctx(), talk_to(&npc, "Good day to you all."))
        .await
        .unwrap();
    let step = to_dialogue(&mut engine, &live, step).await;
    let addressed = awaiting(&step).clone();
    let chained = engine
        .resume(&live.ctx(), resolution(&addressed, completed(&npc_reply())))
        .await
        .unwrap();
    let bystander = awaiting(&chained).clone();
    assert_eq!(bystander.call.subrole, InferenceSubrole::Dialogue);
    assert_ne!(bystander.id, addressed.id);
    let done = engine
        .resume(
            &live.ctx(),
            resolution(&bystander, failed(InferenceFailureKind::Transport)),
        )
        .await
        .unwrap();
    assert!(
        matches!(
            done.status,
            TurnStatus::Completed {
                outcome: TerminalOutcome::Succeeded,
                revision: Some(StateRevision(1))
            }
        ),
        "{:?}",
        done.status
    );
    let lines: Vec<_> = done
        .events
        .iter()
        .filter(|event| event.event.kind == TranscriptEventKind::NpcDialogue)
        .collect();
    assert_eq!(lines.len(), 1, "{:?}", kinds(&done.events));
    assert_reducer_contract(&journal.events(), &addressed.request_id);
}

// Design §7 consequence 1: when one of several addressed speakers fails,
// the whole attempt fails; the earlier speaker's line is not committed, so a
// retry cannot duplicate it.
#[tokio::test]
async fn a_failed_second_addressee_fails_the_whole_turn() {
    let live = Live::rundale();
    let (mut engine, journal) = engine(&live);
    let names: Vec<String> = {
        let world = live.world.lock().await;
        let npcs = live.npc_manager.lock().await;
        let mut here: Vec<String> = npcs
            .npcs_at(world.player_location)
            .iter()
            .map(|npc| npc.name.clone())
            .collect();
        here.sort();
        here.truncate(2);
        here
    };
    let before = live.fingerprint().await;
    let step = engine
        .submit(
            &live.ctx(),
            TurnInput {
                addressed_to: names,
                ..talk_to("", "Good day to the pair of you.")
            },
        )
        .await
        .unwrap();
    let step = to_dialogue(&mut engine, &live, step).await;
    let first = awaiting(&step).clone();
    let second_step = engine
        .resume(&live.ctx(), resolution(&first, completed(&npc_reply())))
        .await
        .unwrap();
    let second = awaiting(&second_step).clone();
    assert_eq!(second.call.subrole, InferenceSubrole::Dialogue);
    let done = engine
        .resume(
            &live.ctx(),
            resolution(&second, failed(InferenceFailureKind::TimedOut)),
        )
        .await
        .unwrap();
    assert!(
        matches!(
            done.status,
            TurnStatus::Completed {
                outcome: TerminalOutcome::Failed,
                revision: None
            }
        ),
        "{:?}",
        done.status
    );
    assert_eq!(live.fingerprint().await, before);
    assert!(
        journal
            .events()
            .iter()
            .all(|event| event.event.kind != TranscriptEventKind::NpcDialogue)
    );
    assert_reducer_contract(&journal.events(), &first.request_id);
}

// Oracle: `endpoint_failure_requires_the_invocation_base_revision`.
#[tokio::test]
async fn callbacks_must_name_the_awaited_attempt_call_and_revision() {
    let live = Live::rundale();
    let (mut engine, _journal) = engine(&live);
    let npc = live.first_npc_here().await;
    let before = live.fingerprint().await;

    let step = engine
        .submit(&live.ctx(), talk_to(&npc, "Tell me about the wall."))
        .await
        .unwrap();
    let step = to_dialogue(&mut engine, &live, step).await;
    let pending = awaiting(&step).clone();

    let wrong_call = InferenceResolution {
        call_id: pending.attempt_id.call(9),
        ..resolution(&pending, completed(&npc_reply()))
    };
    let wrong_revision = InferenceResolution {
        base_revision: StateRevision(99),
        ..resolution(&pending, completed(&npc_reply()))
    };
    let wrong_attempt = InferenceResolution {
        attempt_id: ExecutionAttemptId::new("someone-else"),
        ..resolution(&pending, completed(&npc_reply()))
    };
    for (bad, reason) in [
        (wrong_call, IgnoredReason::StaleCall),
        (wrong_revision, IgnoredReason::StaleRevision),
        (wrong_attempt, IgnoredReason::StaleAttempt),
    ] {
        let ignored = engine.resume(&live.ctx(), bad).await.unwrap();
        assert!(
            matches!(ignored.status, TurnStatus::Ignored(r) if r == reason),
            "expected {reason:?}, got {:?}",
            ignored.status
        );
        assert!(ignored.events.is_empty());
    }
    assert_eq!(live.fingerprint().await, before);

    let done = engine
        .resume(&live.ctx(), resolution(&pending, completed(&npc_reply())))
        .await
        .unwrap();
    assert!(matches!(
        done.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Succeeded,
            ..
        }
    ));
    let duplicate = engine
        .resume(&live.ctx(), resolution(&pending, completed(&npc_reply())))
        .await
        .unwrap();
    assert!(matches!(
        duplicate.status,
        TurnStatus::Ignored(IgnoredReason::AttemptTerminal)
    ));
}

// Oracle: `accepted_restart_becomes_interrupted_and_does_not_rerun` and
// RundaleKit `testRestoredAdapterSuppressesOpeningReplayAndInterruptsActiveAttempt`.
#[tokio::test]
async fn restart_recovery_interrupts_the_open_request_without_rerunning_it() {
    let live = Live::rundale();
    let journal = Arc::new(MemoryTurnJournal::new());
    let npc = live.first_npc_here().await;
    let before = live.fingerprint().await;

    let pending = {
        let mut crashed = TurnEngine::new(journal.clone(), live.rules());
        let step = crashed
            .submit(&live.ctx(), talk_to(&npc, "Tell me about the wall."))
            .await
            .unwrap();
        awaiting(&step).clone()
        // The process stops here: the engine and its attempt are dropped.
    };

    let mut restarted = TurnEngine::new(journal.clone(), live.rules());
    let recovered = restarted.recover().await.unwrap();
    assert_eq!(
        kinds(&recovered),
        vec![
            TranscriptEventKind::Narration,
            TranscriptEventKind::ResponseCompleted
        ]
    );
    assert_eq!(
        recovered[0].event.content.as_deref(),
        Some(limerick_core::turn::INTERRUPTED_MESSAGE)
    );
    assert_eq!(
        recovered[1].event.terminal_outcome,
        Some(TerminalOutcome::Interrupted)
    );
    let record = journal.request(&pending.request_id).unwrap();
    assert_eq!(record.phase, RequestPhase::Interrupted);
    assert!(restarted.open_request().is_none(), "nothing re-runs");
    assert!(
        restarted.recover().await.unwrap().is_empty(),
        "recovery is idempotent"
    );
    assert_eq!(live.fingerprint().await, before);

    // The old attempt's result arriving after restart is ignored.
    let late = restarted
        .resume(&live.ctx(), resolution(&pending, completed(&npc_reply())))
        .await
        .unwrap();
    assert!(matches!(
        late.status,
        TurnStatus::Ignored(IgnoredReason::AttemptTerminal)
    ));

    // The player can retry it explicitly.
    let retry = restarted
        .retry(&live.ctx(), &pending.request_id)
        .await
        .unwrap();
    let (done, _, _) = run_scripted(&mut restarted, &live, retry, "").await;
    assert!(matches!(
        done.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Succeeded,
            ..
        }
    ));
    assert_reducer_contract(&journal.events(), &pending.request_id);
}

#[tokio::test]
async fn a_failed_journal_commit_installs_nothing_and_the_retry_commits_once() {
    let live = Live::rundale();
    let (mut engine, journal) = engine(&live);
    let npc = live.first_npc_here().await;
    let before = live.fingerprint().await;

    let step = engine
        .submit(&live.ctx(), talk_to(&npc, "Tell me about the wall."))
        .await
        .unwrap();
    let step = to_dialogue(&mut engine, &live, step).await;
    let pending = awaiting(&step).clone();
    journal.fail_next_writes(1);
    let error = engine
        .resume(&live.ctx(), resolution(&pending, completed(&npc_reply())))
        .await
        .unwrap_err();
    assert!(matches!(error, TurnError::Journal(_)), "{error}");
    assert_eq!(live.fingerprint().await, before);
    assert!(live.emitter.is_empty());
    assert_eq!(engine.revision(), StateRevision(0));
    assert_eq!(
        engine.request(&pending.request_id).unwrap().phase,
        RequestPhase::Failed
    );
    assert!(journal.task_batches().is_empty());

    let retry = engine
        .retry(&live.ctx(), &pending.request_id)
        .await
        .unwrap();
    let (done, _, _) = run_scripted(&mut engine, &live, retry, "").await;
    assert!(matches!(
        done.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Succeeded,
            revision: Some(StateRevision(1))
        }
    ));
    assert_eq!(journal.task_batches().len(), 1);
}

#[tokio::test]
async fn only_one_request_is_open_and_request_ids_are_never_reused() {
    let live = Live::rundale();
    let (mut engine, _journal) = engine(&live);
    let npc = live.first_npc_here().await;
    let id = LogicalRequestId::new("r1");
    let step = engine
        .submit(
            &live.ctx(),
            TurnInput {
                request_id: Some(id.clone()),
                ..talk_to(&npc, "Tell me about the wall.")
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        engine
            .submit(&live.ctx(), talk_to(&npc, "And the weather?"))
            .await,
        Err(TurnError::RequestInProgress(open)) if open == id
    ));
    let (_, _, _) = run_scripted(&mut engine, &live, step, "").await;
    assert!(matches!(
        engine
            .submit(
                &live.ctx(),
                TurnInput {
                    request_id: Some(id.clone()),
                    ..talk_to(&npc, "Tell me about the wall.")
                },
            )
            .await,
        Err(TurnError::Lifecycle(LifecycleError::AlreadyCommitted(_)))
    ));
}

/// Prepares a journey from the start that deterministically meets a
/// greeter and rolls a travel encounter, and returns the destination name.
///
/// Arrival greetings are enabled (`npc-arrival-greetings` is default-off).
/// The destination is an indoor neighbour holding only NPCs who work there:
/// an unmet NPC at an indoor workplace always reacts with a model-written
/// introduction, so no dice decide whether a reaction call happens. The
/// clock is then advanced to the first minute whose encounter roll fires;
/// the roll is seeded by game time and route, so it is deterministic.
async fn prepare_eventful_journey(live: &Live, rules: &TurnRules) -> String {
    use limerick_core::config::ReactionConfig;
    use limerick_core::npc::reactions::reaction_threshold;

    live.config
        .lock()
        .await
        .flags
        .enable(limerick_core::game_session::NPC_ARRIVAL_GREETINGS_FLAG);
    let mut world = live.world.lock().await;
    let mut npcs = live.npc_manager.lock().await;
    let start = world.player_location;
    let mut neighbours: Vec<LocationId> = world
        .graph
        .neighbors(start)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    neighbours.sort();
    let (destination, greeters) = neighbours
        .into_iter()
        .filter_map(|id| {
            let data = world.graph.get(id)?;
            if !data.indoor {
                return None;
            }
            let greeters: Vec<_> = npcs
                .all_npcs()
                .filter(|npc| npc.workplace == Some(id))
                .map(|npc| npc.id)
                .collect();
            (!greeters.is_empty()).then_some((id, greeters))
        })
        .next()
        .expect("an indoor neighbour of the start with a resident worker");
    let strays: Vec<_> = npcs
        .npcs_at(destination)
        .iter()
        .map(|npc| npc.id)
        .filter(|id| !greeters.contains(id))
        .collect();
    for id in strays {
        npcs.get_mut(id).unwrap().set_location(start);
    }
    for id in &greeters {
        npcs.get_mut(*id).unwrap().set_location(destination);
    }
    let name = world.graph.get(destination).unwrap().name.clone();
    let flags = live.config.lock().await.flags.clone();
    for _ in 0..(24 * 60) {
        let mut scratch_world = world.clone_for_staged_turn();
        let mut scratch_npcs = npcs.clone();
        let effects = limerick_core::game_session::apply_movement(
            &mut scratch_world,
            &mut scratch_npcs,
            &rules.reaction_templates,
            &name,
            &rules.transport,
            &flags,
        );
        let destination_data = scratch_world.graph.get(destination).unwrap();
        let tod = scratch_world.clock.time_of_day();
        let all_guaranteed = greeters.iter().all(|id| {
            let npc = scratch_npcs.get(*id).unwrap();
            reaction_threshold(npc, destination_data, tod, &ReactionConfig::default()) >= 1.0
        });
        if effects.world_changed
            && all_guaranteed
            && limerick_core::game_session::roll_travel_encounter(&scratch_world, &effects)
                .is_some()
        {
            assert!(effects.arrival_reactions.iter().all(|r| r.use_llm));
            return name;
        }
        world.clock.advance(1);
    }
    panic!("no minute of the day rolls a travel encounter to {name}");
}

// Oracle: `phase3_natural_travel_commits_once_and_updates_scene_and_schedule`.
#[tokio::test]
async fn full_travel_suspends_for_encounter_and_arrival_reactions_and_commits_once() {
    let live = Live::rundale();
    let rules = live.rules();
    let destination = prepare_eventful_journey(&live, &rules).await;
    let (mut engine, journal) = engine(&live);
    let start = live.world.lock().await.player_location;
    let before = live.fingerprint().await;

    let step = engine
        .submit(
            &live.ctx(),
            TurnInput {
                text: format!("Let us be off, walking on toward {destination}"),
                ..TurnInput::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(live.fingerprint().await, before);
    let (done, events, calls) = run_scripted(&mut engine, &live, step, &destination).await;
    assert!(
        matches!(
            done.status,
            TurnStatus::Completed {
                outcome: TerminalOutcome::Succeeded,
                revision: Some(StateRevision(1))
            }
        ),
        "{:?}",
        done.status
    );
    assert_eq!(calls[0], InferenceSubrole::Intent, "{calls:?}");
    assert_eq!(calls[1], InferenceSubrole::TravelEncounter, "{calls:?}");
    assert!(
        calls[2..]
            .iter()
            .all(|subrole| *subrole == InferenceSubrole::ArrivalReaction)
            && calls.len() > 2,
        "{calls:?}"
    );

    let world = live.world.lock().await;
    assert_ne!(world.player_location, start);
    assert_eq!(
        world.current_location_data().map(|l| l.name.clone()),
        Some(destination.clone())
    );
    drop(world);
    let scenes: Vec<_> = events
        .iter()
        .filter(|event| event.event.kind == TranscriptEventKind::SceneChanged)
        .collect();
    assert_eq!(scenes.len(), 1, "{:?}", kinds(&events));
    assert_eq!(
        scenes[0].event.content.as_deref(),
        Some(destination.as_str())
    );
    assert!(
        events.iter().any(|event| event
            .event
            .content
            .as_deref()
            .is_some_and(|text| text.contains(ENCOUNTER_LINE))),
        "the enriched encounter line is committed"
    );
    let reactions = events
        .iter()
        .filter(|event| {
            event
                .event
                .content
                .as_deref()
                .is_some_and(|text| text.contains(REACTION_LINE))
        })
        .count();
    assert_eq!(reactions, calls.len() - 2, "one line per arrival reaction");
    assert_eq!(journal.task_batches().len(), 1);
    assert_reducer_contract(&journal.events(), done.request_id.as_ref().unwrap());
}

// Stopping mid-journey (after the encounter call) leaves the player where
// they were.
#[tokio::test]
async fn stopping_a_journey_mid_way_leaves_the_player_where_they_were() {
    let live = Live::rundale();
    let rules = live.rules();
    let destination = prepare_eventful_journey(&live, &rules).await;
    let (mut engine, _journal) = engine(&live);
    let before = live.fingerprint().await;

    let mut step = engine
        .submit(
            &live.ctx(),
            TurnInput {
                text: format!("go to {destination}"),
                ..TurnInput::default()
            },
        )
        .await
        .unwrap();
    loop {
        let pending = awaiting(&step).clone();
        if pending.call.subrole == InferenceSubrole::ArrivalReaction {
            let stopped = engine.stop(&live.ctx(), &pending.attempt_id).await.unwrap();
            assert!(matches!(
                stopped.status,
                TurnStatus::Completed {
                    outcome: TerminalOutcome::Cancelled,
                    ..
                }
            ));
            break;
        }
        let outcome = scripted(&pending, &destination);
        step = engine
            .resume(&live.ctx(), resolution(&pending, outcome))
            .await
            .unwrap();
    }
    assert_eq!(live.fingerprint().await, before);
    assert!(live.emitter.is_empty());
}

/// An interactive-lane worker that answers every dialogue request with the
/// scripted NPC reply.
fn scripted_queue() -> (InferenceQueue, tokio::task::JoinHandle<usize>) {
    let (interactive_tx, mut interactive_rx) = tokio::sync::mpsc::channel::<InferenceRequest>(4);
    let (background_tx, _) = tokio::sync::mpsc::channel(1);
    let (batch_tx, _) = tokio::sync::mpsc::channel(1);
    let worker = tokio::spawn(async move {
        let mut served = 0;
        while let Some(request) = interactive_rx.recv().await {
            served += 1;
            if let Some(tokens) = request.token_tx {
                let _ = tokens.send(npc_reply()).await;
            }
            let _ = request.response_tx.send(InferenceResponse {
                id: request.id,
                text: npc_reply(),
                error: None,
            });
        }
        served
    });
    (
        InferenceQueue::new(interactive_tx, background_tx, batch_tx),
        worker,
    )
}

#[tokio::test]
async fn drive_in_process_fulfils_calls_through_the_desktop_adapter() {
    let live = Live::rundale();
    let (queue, worker) = scripted_queue();
    *live.inference_queue.lock().await = Some(queue);
    let (mut engine, journal) = engine(&live);
    let npc = live.first_npc_here().await;

    let ctx = live.ctx();
    let in_process = InProcessInference::from_ctx(&ctx);
    let done = drive_in_process(
        &mut engine,
        &ctx,
        talk_to(&npc, "Good day to you. Any news of the fair?"),
        &in_process,
    )
    .await
    .unwrap();
    assert!(matches!(
        done.status,
        TurnStatus::Completed {
            outcome: TerminalOutcome::Succeeded,
            revision: Some(StateRevision(1))
        }
    ));
    assert_eq!(
        kinds(&done.events).first(),
        Some(&TranscriptEventKind::PlayerCommand)
    );
    assert!(
        done.events
            .iter()
            .any(|event| event.event.kind == TranscriptEventKind::NpcDialogue
                && event.event.content.as_deref() == Some(NPC_LINE))
    );
    assert_eq!(done.events, journal.events());
    drop(ctx);
    *live.inference_queue.lock().await = None;
    assert_eq!(
        worker.await.unwrap(),
        1,
        "one provider request, via the queue"
    );
}

#[tokio::test]
async fn an_unavailable_dialogue_route_uses_the_offline_fallback_without_a_call() {
    let live = Live::rundale();
    let (mut engine, _journal) = engine(&live);
    engine.set_routes(
        limerick_core::turn::InferenceRoutes::live()
            .with(
                InferenceSubrole::Intent,
                limerick_core::turn_inference::RouteStatus::Unavailable,
            )
            .with(
                InferenceSubrole::Dialogue,
                limerick_core::turn_inference::RouteStatus::Unavailable,
            ),
    );
    let npc = live.first_npc_here().await;
    let done = engine
        .submit(&live.ctx(), talk_to(&npc, "Tell me about the wall."))
        .await
        .unwrap();
    assert!(
        matches!(done.status, TurnStatus::Completed { .. }),
        "no call is yielded: {:?}",
        done.status
    );
}

/// Per-turn candidate cost on the full Rundale world. Run with
/// `cargo test --release -p limerick-core --test turn_lifecycle -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "measurement; run explicitly in release mode"]
async fn measure_candidate_capture_cost_on_rundale() {
    async fn sample(live: &Live, label: &str) {
        let ctx = live.ctx();
        let runs = 200;
        let mut samples = Vec::with_capacity(runs);
        for _ in 0..runs {
            let start = std::time::Instant::now();
            let candidate =
                limerick_core::game_loop::TurnCandidate::capture(&ctx, Vec::new()).await;
            samples.push(start.elapsed());
            drop(candidate);
        }
        samples.sort();
        let world = live.world.lock().await;
        let npcs = live.npc_manager.lock().await;
        println!(
            "candidate capture, mods/rundale {label} ({} locations, {} NPCs, {} text-log lines, {runs} runs): median {:?}, p95 {:?}, max {:?}",
            world.graph.location_ids().len(),
            npcs.all_npcs().count(),
            world.text_log.len(),
            samples[runs / 2],
            samples[runs * 95 / 100],
            samples[runs - 1]
        );
    }

    let live = Live::rundale();
    sample(&live, "fresh").await;
    {
        let mut world = live.world.lock().await;
        for turn in 0..1_000 {
            world.log(format!(
                "Turn {turn}: the rain comes on soft over the bog road while the forge rings out \
                 and someone calls across the green about the price of oats at the fair."
            ));
        }
    }
    sample(&live, "with a full text log").await;
}
