//! Atomic staging for player turns that may mutate durable task progress.

use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::{GameLoopContext, handle_game_input};
use crate::ipc::{CapturingEmitter, EventEmitter};
use crate::npc::reactions::ReactionTemplates;
use crate::session_store::{SessionStore, TaskJournalTarget, append_task_mutations};
use crate::world::transport::TransportMode;

/// Successfully committed staged turn.
#[derive(Debug)]
pub struct StagedGameInputCommit {
    /// Transport/UI emissions captured during the pending turn, in order.
    pub emissions: Vec<(String, serde_json::Value)>,
    /// Complete durable task post-states appended by the turn.
    pub task_mutations: Vec<parish_types::PlayerTask>,
}

/// Returns whether a free-form input must use whole-turn staging.
///
/// Explicit work requests can assign a new task. Once any task is active, all
/// free-form inputs are staged because intent parsing may classify one as the
/// physical action that advances it.
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
) -> Result<StagedGameInputCommit, crate::error::ParishError> {
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
                crate::error::ParishError::Database(
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
) -> Result<StagedGameInputCommit, crate::error::ParishError>
where
    F: FnOnce(Vec<parish_types::PlayerTask>) -> Fut,
    Fut: Future<Output = Result<(), crate::error::ParishError>>,
{
    // Clone one coherent canonical cut while holding the same lock order used
    // by installation below. The runtime persistence gate should already
    // exclude mutators; retaining all three guards here also prevents a
    // partially old/partially new candidate if a non-participating reader or
    // legacy adapter is still present.
    let (candidate_world, candidate_npcs, candidate_conversation) = {
        let live_world = live_ctx.world.lock().await;
        let live_npcs = live_ctx.npc_manager.lock().await;
        let live_conversation = live_ctx.conversation.lock().await;
        (
            live_world.clone_for_staged_turn(),
            live_npcs.clone(),
            live_conversation.clone(),
        )
    };
    let staged_world = Mutex::new(candidate_world);
    let staged_npcs = Mutex::new(candidate_npcs);
    let staged_conversation = Mutex::new(candidate_conversation);
    let deferred_audit = crate::inference::DeferredInferenceAudit::default();
    let staged_inference_queue = {
        let live_queue = live_ctx.inference_queue.lock().await;
        Mutex::new(
            live_queue
                .as_ref()
                .map(|queue| queue.with_deferred_audit(deferred_audit.clone())),
        )
    };
    {
        let mut conversation = staged_conversation.lock().await;
        let now = std::time::Instant::now();
        conversation.last_player_activity = now;
        conversation.last_spoken_at = now;
    }
    let mut semantic_rx = staged_world.lock().await.event_bus.subscribe();
    let capturing = Arc::new(CapturingEmitter::new());
    for (name, payload) in prelude_emissions {
        capturing.emit_event(&name, payload);
    }
    let staged_emitter: Arc<dyn EventEmitter> = capturing.clone();
    let staged_ctx = GameLoopContext {
        world: &staged_world,
        npc_manager: &staged_npcs,
        config: live_ctx.config,
        conversation: &staged_conversation,
        inference_queue: &staged_inference_queue,
        emitter: staged_emitter,
        inference_config: live_ctx.inference_config,
        pronunciations: live_ctx.pronunciations,
        client: live_ctx.client,
        cloud_client: live_ctx.cloud_client,
        language: live_ctx.language.clone(),
        inference_failure_messages: live_ctx.inference_failure_messages,
        idle_messages: live_ctx.idle_messages,
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

    let mut semantic_events = Vec::new();
    loop {
        match semantic_rx.try_recv() {
            Ok(event) => semantic_events.push(event),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(dropped)) => {
                return Err(crate::error::ParishError::Database(format!(
                    "pending turn semantic event buffer overflowed and dropped {dropped} event(s)"
                )));
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
            | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
        }
    }
    let emissions = capturing.drain();

    // This is the only fallible step after the candidate turn finishes. The
    // caller's store appends the complete batch atomically.
    if let Err(error) = journal(outcome.task_mutations.clone()).await {
        deferred_audit.discard().await;
        return Err(error);
    }

    // Install the candidate under canonical lock order, transplanting the
    // process-lifetime bus so subscribers and the context epoch survive.
    let mut candidate_world = staged_world.into_inner();
    let candidate_npcs = staged_npcs.into_inner();
    let candidate_conversation = staged_conversation.into_inner();
    {
        let mut live_world = live_ctx.world.lock().await;
        let mut live_npcs = live_ctx.npc_manager.lock().await;
        let mut live_conversation = live_ctx.conversation.lock().await;
        candidate_world.event_bus = std::mem::take(&mut live_world.event_bus);
        *live_world = candidate_world;
        *live_npcs = candidate_npcs;
        *live_conversation = candidate_conversation;
    }

    // The provider call completed while the candidate was pending. Reveal its
    // debug-ring/JSONL audit record only after both the journal and canonical
    // install succeeded.
    deferred_audit.commit().await;

    // Publish semantic events only after canonical state is installed and the
    // durable task batch has committed.
    let live_world = live_ctx.world.lock().await;
    for event in semantic_events {
        live_world.event_bus.publish(event);
    }
    drop(live_world);

    // Transport events are buffered alongside the candidate and become
    // visible only after both durable commit and canonical state install.
    flush_staged_emissions(live_ctx.emitter.as_ref(), emissions.clone());

    Ok(StagedGameInputCommit {
        emissions,
        task_mutations: outcome.task_mutations,
    })
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

    use parish_types::{GameEvent, NpcId};
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
        };
        let transport = make_transport();
        let reaction_templates = ReactionTemplates::default();
        let raw = "Do each of ye have work for me here now?".to_string();
        let addressed_to = vec!["Brigid Doyle".to_string(), "Máire Kelly".to_string()];
        let prelude = vec![(
            "text-log".to_string(),
            json!({"source": "player", "content": "> Could each of ye give me work?"}),
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
                    Err(crate::error::ParishError::Database(
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
