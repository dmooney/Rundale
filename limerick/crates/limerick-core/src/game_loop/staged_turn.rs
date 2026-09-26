//! Atomic staging for player turns.
//!
//! [`TurnCandidate`] is the isolated copy of live state a turn runs against;
//! the turn engine (`crate::turn::TurnEngine`) runs every attempt on one.
//! [`handle_staged_game_input`] is the staged entry point the runtimes use
//! today for turns that may mutate durable task progress.

use std::future::Future;
use std::sync::Arc;

use limerick_types::GameEvent;
use tokio::sync::{Mutex, broadcast};

use super::{GameLoopContext, handle_game_input};
use crate::ipc::{CapturingEmitter, ConversationRuntimeState, EventEmitter};
use crate::npc::manager::NpcManager;
use crate::npc::reactions::ReactionTemplates;
use crate::session_store::{SessionStore, TaskJournalTarget, append_task_mutations};
use crate::world::WorldState;
use crate::world::transport::TransportMode;

/// Successfully committed staged turn.
#[derive(Debug)]
pub struct StagedGameInputCommit {
    /// Transport/UI emissions captured during the pending turn, in order.
    pub emissions: Vec<(String, serde_json::Value)>,
    /// Complete durable task post-states appended by the turn.
    pub task_mutations: Vec<limerick_types::PlayerTask>,
    /// Recovery result from a dialogue turn that produced no canonical reply.
    pub dialogue_failure: Option<String>,
}

/// Returns whether a free-form input must use whole-turn staging.
///
/// Explicit work requests, affirmative acceptance, and concrete first-step
/// follow-ups can assign a new task. Once any task is active, all free-form
/// inputs are staged because intent parsing may classify one as the physical
/// action that advances it.
pub fn input_may_mutate_tasks(world: &crate::world::WorldState, raw: &str) -> bool {
    crate::game_session::is_task_request_input(raw)
        || world.player_progress.active_tasks().next().is_some()
}

/// Runs a potential task-bearing turn against isolated state and emissions,
/// durably appends the task batch, then installs and publishes the candidate.
///
/// The runtime must hold its outer persistence/lifecycle gate for this entire
/// call. Every other live-state mutator must participate in that same gate.
#[allow(clippy::too_many_arguments)]
pub async fn handle_staged_game_input(
    live_ctx: &GameLoopContext<'_>,
    session_store: &dyn SessionStore,
    task_target: Option<&TaskJournalTarget>,
    prelude_emissions: Vec<(String, serde_json::Value)>,
    raw: String,
    addressed_to: Vec<String>,
    transport: &TransportMode,
    reaction_templates: &ReactionTemplates,
) -> Result<StagedGameInputCommit, crate::error::LimerickError> {
    handle_staged_game_input_with_journal(
        live_ctx,
        prelude_emissions,
        raw,
        addressed_to,
        transport,
        reaction_templates,
        move |tasks| async move {
            if tasks.is_empty() {
                return Ok(());
            }
            let target = task_target.ok_or_else(|| {
                crate::error::LimerickError::Database(
                    "cannot journal player task without an active save and branch".to_string(),
                )
            })?;
            append_task_mutations(session_store, target, &tasks).await?;
            Ok(())
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
/// Lower-level staged-turn seam for runtimes with a non-[`SessionStore`]
/// journal adapter (notably the synchronous real-loop test harness).
///
/// `journal` must atomically append the complete supplied task post-state
/// batch. All state, semantic events, transport output, and inference audit
/// records remain pending until it returns success.
pub async fn handle_staged_game_input_with_journal<F, Fut>(
    live_ctx: &GameLoopContext<'_>,
    prelude_emissions: Vec<(String, serde_json::Value)>,
    raw: String,
    addressed_to: Vec<String>,
    transport: &TransportMode,
    reaction_templates: &ReactionTemplates,
    journal: F,
) -> Result<StagedGameInputCommit, crate::error::LimerickError>
where
    F: FnOnce(Vec<limerick_types::PlayerTask>) -> Fut,
    Fut: Future<Output = Result<(), crate::error::LimerickError>>,
{
    let candidate = TurnCandidate::capture(live_ctx, prelude_emissions).await;
    let deferred_audit = crate::inference::DeferredInferenceAudit::default();
    let staged_inference_queue = {
        let live_queue = live_ctx.inference_queue.lock().await;
        Mutex::new(
            live_queue
                .as_ref()
                .map(|queue| queue.with_deferred_audit(deferred_audit.clone())),
        )
    };
    let staged_ctx = GameLoopContext {
        world: &candidate.world,
        npc_manager: &candidate.npc_manager,
        config: live_ctx.config,
        conversation: &candidate.conversation,
        inference_queue: &staged_inference_queue,
        emitter: candidate.emitter(),
        inference_config: live_ctx.inference_config,
        pronunciations: live_ctx.pronunciations,
        client: live_ctx.client,
        cloud_client: live_ctx.cloud_client,
        language: live_ctx.language.clone(),
        inference_failure_messages: live_ctx.inference_failure_messages,
        idle_messages: live_ctx.idle_messages,
        inference_override: live_ctx.inference_override.clone(),
    };

    // Loading indicators are outward effects too, so pending turns do not
    // spawn the live animation. Dialogue/token events remain captured.
    let outcome = handle_game_input(
        &staged_ctx,
        raw,
        addressed_to,
        transport,
        reaction_templates,
        || None,
    )
    .await;
    drop(staged_ctx);

    let finished = match candidate.finish() {
        Ok(finished) => finished,
        Err(error) => {
            deferred_audit.discard().await;
            return Err(error);
        }
    };

    // This is the only fallible step after the candidate turn finishes. The
    // caller's store appends the complete batch atomically.
    if let Err(error) = journal(outcome.task_mutations.clone()).await {
        deferred_audit.discard().await;
        return Err(error);
    }

    let installed = finished.install(live_ctx).await;

    // The provider call completed while the candidate was pending. Reveal its
    // debug-ring/JSONL audit record only after both the journal and canonical
    // install succeeded.
    deferred_audit.commit().await;

    let emissions = installed.release(live_ctx).await;

    Ok(StagedGameInputCommit {
        emissions,
        task_mutations: outcome.task_mutations,
        dialogue_failure: outcome.dialogue_failure,
    })
}

/// An isolated copy of the live world, NPCs, and conversation that one turn
/// runs against, with its transport output captured instead of emitted.
///
/// Nothing a turn does to a candidate is visible until it is installed:
/// state, semantic events, and wire emissions all stay pending, so a turn
/// that is stopped, fails, or cannot be journaled has no authoritative
/// effect.
pub struct TurnCandidate {
    /// Candidate world (fresh event bus; the live bus is transplanted back
    /// on install).
    pub world: Mutex<WorldState>,
    /// Candidate NPC manager.
    pub npc_manager: Mutex<NpcManager>,
    /// Candidate conversation state.
    pub conversation: Mutex<ConversationRuntimeState>,
    emitter: Arc<CapturingEmitter>,
    semantic_rx: broadcast::Receiver<GameEvent>,
}

impl TurnCandidate {
    /// Clones one coherent cut of `live` and records the turn as player
    /// activity. `prelude` emissions are captured first, ahead of anything
    /// the turn emits.
    pub async fn capture(
        live: &GameLoopContext<'_>,
        prelude: Vec<(String, serde_json::Value)>,
    ) -> Self {
        // Clone one coherent canonical cut while holding the same lock order
        // used by installation. The runtime persistence gate should already
        // exclude mutators; retaining all three guards here also prevents a
        // partially old/partially new candidate if a non-participating reader
        // or legacy adapter is still present.
        let (world, npc_manager, mut conversation) = {
            let live_world = live.world.lock().await;
            let live_npcs = live.npc_manager.lock().await;
            let live_conversation = live.conversation.lock().await;
            (
                live_world.clone_for_staged_turn(),
                live_npcs.clone(),
                live_conversation.clone(),
            )
        };
        let now = std::time::Instant::now();
        conversation.last_player_activity = now;
        conversation.last_spoken_at = now;
        let semantic_rx = world.event_bus.subscribe();
        let emitter = Arc::new(CapturingEmitter::new());
        for (name, payload) in prelude {
            emitter.emit_event(&name, payload);
        }
        Self {
            world: Mutex::new(world),
            npc_manager: Mutex::new(npc_manager),
            conversation: Mutex::new(conversation),
            emitter,
            semantic_rx,
        }
    }

    /// The emitter a candidate context must use.
    pub fn emitter(&self) -> Arc<dyn EventEmitter> {
        self.emitter.clone()
    }

    /// Wire emissions captured so far, in order (the turn keeps running).
    pub fn emissions(&self) -> Vec<(String, serde_json::Value)> {
        self.emitter.events()
    }

    /// Ends the turn: collects its semantic events and wire emissions.
    /// Fails when the semantic event buffer overflowed, because a partial
    /// event stream cannot be published.
    pub fn finish(mut self) -> Result<FinishedCandidate, crate::error::LimerickError> {
        let mut semantic_events = Vec::new();
        loop {
            match self.semantic_rx.try_recv() {
                Ok(event) => semantic_events.push(event),
                Err(broadcast::error::TryRecvError::Lagged(dropped)) => {
                    return Err(crate::error::LimerickError::Database(format!(
                        "pending turn semantic event buffer overflowed and dropped {dropped} event(s)"
                    )));
                }
                Err(broadcast::error::TryRecvError::Empty)
                | Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        Ok(FinishedCandidate {
            world: self.world.into_inner(),
            npc_manager: self.npc_manager.into_inner(),
            conversation: self.conversation.into_inner(),
            semantic_events,
            emissions: self.emitter.drain(),
        })
    }
}

/// A candidate whose turn has ended, ready to install.
pub struct FinishedCandidate {
    world: WorldState,
    npc_manager: NpcManager,
    conversation: ConversationRuntimeState,
    semantic_events: Vec<GameEvent>,
    emissions: Vec<(String, serde_json::Value)>,
}

impl FinishedCandidate {
    /// The wire emissions the turn produced, in order.
    pub fn emissions(&self) -> &[(String, serde_json::Value)] {
        &self.emissions
    }

    /// Replaces live state with the candidate under canonical lock order,
    /// transplanting the process-lifetime event bus so subscribers and the
    /// context epoch survive. Call only after the turn is durably journaled.
    pub async fn install(self, live: &GameLoopContext<'_>) -> InstalledCandidate {
        let mut world = self.world;
        {
            let mut live_world = live.world.lock().await;
            let mut live_npcs = live.npc_manager.lock().await;
            let mut live_conversation = live.conversation.lock().await;
            world.event_bus = std::mem::take(&mut live_world.event_bus);
            *live_world = world;
            *live_npcs = self.npc_manager;
            *live_conversation = self.conversation;
        }
        InstalledCandidate {
            semantic_events: self.semantic_events,
            emissions: self.emissions,
        }
    }
}

/// An installed candidate whose outward effects are still held back.
#[must_use = "an installed turn's events and emissions must be released"]
pub struct InstalledCandidate {
    semantic_events: Vec<GameEvent>,
    emissions: Vec<(String, serde_json::Value)>,
}

impl InstalledCandidate {
    /// Publishes the turn's semantic events on the live bus, then flushes its
    /// wire emissions to the live emitter. Returns the emissions.
    pub async fn release(self, live: &GameLoopContext<'_>) -> Vec<(String, serde_json::Value)> {
        {
            let live_world = live.world.lock().await;
            for event in self.semantic_events {
                live_world.event_bus.publish(event);
            }
        }
        flush_staged_emissions(live.emitter.as_ref(), self.emissions.clone());
        self.emissions
    }
}

/// Flushes a committed pending turn to its runtime transport.
pub fn flush_staged_emissions(
    emitter: &dyn EventEmitter,
    emissions: Vec<(String, serde_json::Value)>,
) {
    for (name, payload) in emissions {
        emitter.emit_event(&name, payload);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};

    use limerick_types::{GameEvent, NpcId};
    use serde_json::json;

    use super::*;
    use crate::config::InferenceConfig;
    use crate::inference::{InferenceQueue, InferenceRequest, InferenceResponse};
    use crate::ipc::{ConversationRuntimeState, GameConfig};
    use crate::npc::Npc;
    use crate::npc::manager::NpcManager;
    use crate::persistence::GameSnapshot;
    use crate::world::WorldState;

    #[derive(Default)]
    struct CommitAwareEmitter {
        journal_committed: Arc<AtomicBool>,
        events: StdMutex<Vec<(String, serde_json::Value)>>,
    }

    impl CommitAwareEmitter {
        fn new(journal_committed: Arc<AtomicBool>) -> Self {
            Self {
                journal_committed,
                events: StdMutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<(String, serde_json::Value)> {
            self.events.lock().unwrap().clone()
        }
    }

    impl EventEmitter for CommitAwareEmitter {
        fn emit_event(&self, name: &str, payload: serde_json::Value) {
            assert!(
                self.journal_committed.load(Ordering::Acquire),
                "pending transport output leaked before the task journal committed"
            );
            self.events
                .lock()
                .unwrap()
                .push((name.to_string(), payload));
        }
    }

    fn make_transport() -> TransportMode {
        TransportMode {
            id: "walking".to_string(),
            label: "on foot".to_string(),
            speed_m_per_s: 1.2,
        }
    }

    fn two_task_response_worker(
        mut requests: tokio::sync::mpsc::Receiver<InferenceRequest>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let responses = [
                json!({
                    "dialogue": "I need ye to dig over the potato patch here now.",
                    "action": "offers over a spade",
                    "mood": "busy",
                    "language_hints": [],
                    "assigned_task": "Dig over the potato patch.",
                    "internal_thought": null
                })
                .to_string(),
                json!({
                    "dialogue": "I need ye to fetch water from the well here now.",
                    "action": "sets down a pail",
                    "mood": "busy",
                    "language_hints": [],
                    "assigned_task": "Fetch water from the well.",
                    "internal_thought": null
                })
                .to_string(),
            ];
            for response in responses {
                let request = requests
                    .recv()
                    .await
                    .expect("the addressed two-NPC turn should issue two requests");
                if let Some(token_tx) = request.token_tx {
                    token_tx.send(response.clone()).await.unwrap();
                }
                request
                    .response_tx
                    .send(InferenceResponse {
                        id: request.id,
                        text: response,
                        error: None,
                    })
                    .unwrap();
            }
        })
    }

    #[tokio::test]
    async fn second_insert_failure_leaks_no_state_or_output_and_retry_commits_once() {
        let mut world_state = WorldState::new();
        world_state.text_log.push("unchanged sentinel".to_string());
        let player_location = world_state.player_location;
        let mut npc_manager_state = NpcManager::new();
        let mut first = Npc::new_test_npc();
        first.id = NpcId(1);
        first.name = "Brigid Doyle".to_string();
        first.set_location(player_location);
        let mut second = Npc::new_test_npc();
        second.id = NpcId(2);
        second.name = "Máire Kelly".to_string();
        second.set_location(player_location);
        npc_manager_state.add_npc(first);
        npc_manager_state.add_npc(second);

        let world = Mutex::new(world_state);
        let npc_manager = Mutex::new(npc_manager_state);
        let config = Mutex::new(GameConfig::default());
        let mut conversation_state = ConversationRuntimeState::new();
        conversation_state.last_player_input = Some("earlier turn".to_string());
        let conversation = Mutex::new(conversation_state);
        let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel::<InferenceRequest>(4);
        let (background_tx, _) = tokio::sync::mpsc::channel(1);
        let (batch_tx, _) = tokio::sync::mpsc::channel(1);
        let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
        let inference_queue = Mutex::new(Some(queue));
        let client = Mutex::new(None);
        let cloud_client = Mutex::new(None);
        let inference_config = InferenceConfig::default();
        let journal_committed = Arc::new(AtomicBool::new(false));
        let emitter = Arc::new(CommitAwareEmitter::new(Arc::clone(&journal_committed)));
        let ctx = GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter: Arc::clone(&emitter) as Arc<dyn EventEmitter>,
            inference_config: &inference_config,
            pronunciations: &[],
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
            inference_override: None,
        };
        let transport = make_transport();
        let reaction_templates = ReactionTemplates::default();
        let raw = "I'll take the work. What would you have me do first?".to_string();
        let addressed_to = vec!["Brigid Doyle".to_string(), "Máire Kelly".to_string()];
        let prelude = vec![(
            "text-log".to_string(),
            json!({"source": "player", "content": "> I'll take the work. What would you have me do first?"}),
        )];

        let before_snapshot = {
            let world = world.lock().await;
            let npcs = npc_manager.lock().await;
            GameSnapshot::capture(&world, &npcs)
        };
        let before_conversation = conversation.lock().await.clone();
        let mut live_semantic_rx = world.lock().await.event_bus.subscribe();

        let first_worker = two_task_response_worker(interactive_rx);
        let attempted_batch = Arc::new(StdMutex::new(Vec::new()));
        let attempted_batch_for_journal = Arc::clone(&attempted_batch);
        let error = handle_staged_game_input_with_journal(
            &ctx,
            prelude.clone(),
            raw.clone(),
            addressed_to.clone(),
            &transport,
            &reaction_templates,
            move |tasks| {
                *attempted_batch_for_journal.lock().unwrap() = tasks;
                async {
                    Err(crate::error::LimerickError::Database(
                        "injected second insert failure".to_string(),
                    ))
                }
            },
        )
        .await
        .expect_err("the injected second journal insert must fail the whole turn");
        first_worker.await.unwrap();
        assert!(error.to_string().contains("injected second insert failure"));
        assert_eq!(
            attempted_batch.lock().unwrap().len(),
            2,
            "the failure is injected into a real two-mutation turn"
        );
        assert!(
            emitter.events().is_empty(),
            "player speech, loading, dialogue, and stream output stay pending"
        );
        assert!(matches!(
            live_semantic_rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
        let after_failed_snapshot = {
            let world = world.lock().await;
            let npcs = npc_manager.lock().await;
            GameSnapshot::capture(&world, &npcs)
        };
        assert_eq!(after_failed_snapshot, before_snapshot);
        let after_failed_conversation = conversation.lock().await.clone();
        assert_eq!(
            after_failed_conversation.location,
            before_conversation.location
        );
        assert_eq!(
            after_failed_conversation.transcript,
            before_conversation.transcript
        );
        assert_eq!(
            after_failed_conversation.last_player_activity,
            before_conversation.last_player_activity
        );
        assert_eq!(
            after_failed_conversation.last_spoken_at,
            before_conversation.last_spoken_at
        );
        assert_eq!(
            after_failed_conversation.conversation_in_progress,
            before_conversation.conversation_in_progress
        );
        assert_eq!(
            after_failed_conversation.last_player_input,
            before_conversation.last_player_input
        );
        assert_eq!(
            after_failed_conversation.seen_openers_this_location,
            before_conversation.seen_openers_this_location
        );

        let (retry_tx, retry_rx) = tokio::sync::mpsc::channel::<InferenceRequest>(4);
        *inference_queue.lock().await = Some(InferenceQueue::new(
            retry_tx,
            tokio::sync::mpsc::channel(1).0,
            tokio::sync::mpsc::channel(1).0,
        ));
        let retry_worker = two_task_response_worker(retry_rx);
        let persisted = Arc::new(StdMutex::new(Vec::new()));
        let persisted_for_journal = Arc::clone(&persisted);
        let committed_for_journal = Arc::clone(&journal_committed);
        let commit = handle_staged_game_input_with_journal(
            &ctx,
            prelude.clone(),
            raw,
            addressed_to,
            &transport,
            &reaction_templates,
            move |tasks| {
                let persisted_for_journal = Arc::clone(&persisted_for_journal);
                let committed_for_journal = Arc::clone(&committed_for_journal);
                async move {
                    *persisted_for_journal.lock().unwrap() = tasks;
                    committed_for_journal.store(true, Ordering::Release);
                    Ok(())
                }
            },
        )
        .await
        .expect("the unchanged retry should commit");
        retry_worker.await.unwrap();

        assert_eq!(commit.task_mutations.len(), 2);
        assert_eq!(persisted.lock().unwrap().len(), 2);
        assert_eq!(world.lock().await.player_progress.active_tasks().count(), 2);
        let emitted = emitter.events();
        assert_eq!(
            emitted.first(),
            prelude.first(),
            "the pending player bubble remains first after commit"
        );
        assert_eq!(emitted, commit.emissions);
        let semantic_events =
            std::iter::from_fn(|| live_semantic_rx.try_recv().ok()).collect::<Vec<_>>();
        assert_eq!(
            semantic_events
                .iter()
                .filter(|event| matches!(event, GameEvent::PlayerTaskAssigned { .. }))
                .count(),
            2,
            "each committed task publishes its semantic event exactly once"
        );
    }

    #[test]
    fn task_staging_detector_is_shared_with_assignment_and_active_progress() {
        let mut world = WorldState::new();
        assert!(input_may_mutate_tasks(
            &world,
            "Do ye have work for me here now?"
        ));
        assert!(input_may_mutate_tasks(
            &world,
            "I'll take the work. What would you have me do first?"
        ));
        assert!(!input_may_mutate_tasks(&world, "I won't take the work."));
        assert!(!input_may_mutate_tasks(&world, "How is the weather?"));
        world
            .player_progress
            .assign_task(
                "Dig over the potato patch.",
                NpcId(1),
                world.player_location,
                world.clock.now(),
            )
            .unwrap();
        assert!(
            input_may_mutate_tasks(&world, "I dig over the potato patch"),
            "every free-form turn is staged while authoritative work is active"
        );
    }
}
