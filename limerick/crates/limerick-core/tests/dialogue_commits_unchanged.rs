//! Production-path proof that a model reply is committed exactly as written.
//!
//! Nothing inspects, rewrites or replaces what the model says (owner decision
//! 2026-10-23, `docs/design/npc-dialogue-prompt.md` Appendix A). The only check
//! is the structural response contract: complete JSON with non-empty dialogue.
//!
//! Each test runs the real shared pipeline (`game_loop::handle_game_input`
//! through [`TurnEngine`]) on the `mods/rundale` fixture world, with only the
//! completed provider output scripted. The reply names a person the world does
//! not have and gives them a trade and a place; it must reach the transcript,
//! the conversation log and the stream events unchanged on both hosts:
//!
//! - the phone path, where the host answers each suspended call with the
//!   Endpoint's terminal output (`TurnEngine::resume`), and
//! - the desktop path, where `drive_in_process` fulfils the call through the
//!   inference queue.

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
use limerick_core::turn::{
    InferenceResolution, MemoryTurnJournal, PendingInference, StateRevision, TerminalOutcome,
    TranscriptEvent, TranscriptEventKind, TurnEngine, TurnInput, TurnRules, TurnStatus, TurnStep,
    drive_in_process,
};
use limerick_core::turn_inference::{CallReport, InferenceOutcome};
use limerick_core::world::WorldState;
use limerick_core::world::transport::TransportMode;
use tokio::sync::Mutex;

/// Nobody by this name lives in the parish: the world has no such person.
const QUESTION: &str = "Do you know Tadhg Mulvihill?";
/// Ends in a bare capitalised word, as parsers once trimmed off as a stage
/// direction: the reply is committed with it.
const REPLY: &str = "Tadhg Mulvihill? Aye, the cartwright from beyond the bridge, a grand man \
                     altogether. Who sent you? Seamus";
const ACTION: &str = "tips his cap";

fn reply_json() -> String {
    serde_json::json!({
        "dialogue": REPLY,
        "action": ACTION,
        "mood": "friendly",
        "language_hints": [],
        "assigned_task": null,
        "internal_thought": null
    })
    .to_string()
}

/// The runtime-owned live state a host borrows into each engine call, built
/// from the default configuration both hosts now share.
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
    endpoints: limerick_core::game_mod::EndpointCatalog,
}

impl Live {
    fn rundale() -> Self {
        let mod_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testing/fixtures/mods/rundale-legacy");
        let game_mod = GameMod::load(&mod_dir).expect("load the fixture mod");
        let (mut world, npc_manager) =
            limerick_core::game_loop::load_fresh_world_and_npcs(Some(&game_mod), &mod_dir)
                .expect("load the rundale world");
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
            endpoints: limerick_core::game_mod::NO_ENDPOINTS.clone(),
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
            endpoints: &self.endpoints,
            client: &self.client,
            cloud_client: &self.cloud_client,
            language: limerick_core::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
            inference_override: None,
        }
    }

    fn engine(&self) -> TurnEngine {
        TurnEngine::new(
            Arc::new(MemoryTurnJournal::new()),
            TurnRules {
                transport: TransportMode {
                    id: "walking".to_string(),
                    label: "on foot".to_string(),
                    speed_m_per_s: 1.2,
                },
                reaction_templates: self.reaction_templates.clone(),
            },
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

fn talk_to(npc: &str) -> TurnInput {
    TurnInput {
        request_id: None,
        text: QUESTION.to_string(),
        addressed_to: vec![npc.to_string()],
        draft_id: None,
    }
}

fn awaiting(step: &TurnStep) -> &PendingInference {
    match &step.status {
        TurnStatus::AwaitingInference(pending) => pending,
        other => panic!("expected an inference request, got {other:?}"),
    }
}

/// What the player saw and what the game recorded, checked against [`REPLY`].
async fn assert_committed_unchanged(live: &Live, done: &TurnStep, npc: &str) {
    assert!(
        matches!(
            done.status,
            TurnStatus::Completed {
                outcome: TerminalOutcome::Succeeded,
                revision: Some(StateRevision(1))
            }
        ),
        "the reply must commit: {:?}",
        done.status
    );

    // The transcript carries the model's words, unchanged.
    let lines: Vec<&TranscriptEvent> = done
        .events
        .iter()
        .filter(|event| event.event.kind == TranscriptEventKind::NpcDialogue)
        .collect();
    assert_eq!(lines.len(), 1, "one committed exchange");
    assert_eq!(lines[0].event.content.as_deref(), Some(REPLY));

    // The conversation log, which feeds later prompts, holds the same words.
    let world = live.world.lock().await;
    let logged = world.conversation_log.recent_at(world.player_location, 4);
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0].player_input, QUESTION);
    assert_eq!(logged[0].npc_dialogue, REPLY);
    drop(world);

    // The stream events the desktop UI renders carry the same text, and the
    // model's accepted action is narrated.
    let emitted = live.emitter.events();
    let streamed: String = emitted
        .iter()
        .filter(|(name, _)| name == "stream-token")
        .filter_map(|(_, payload)| payload.get("token").and_then(serde_json::Value::as_str))
        .collect();
    assert_eq!(streamed, REPLY);
    let terminal = emitted
        .iter()
        .find(|(name, _)| name == "stream-turn-end")
        .map(|(_, payload)| payload)
        .expect("a terminal stream event");
    assert_eq!(
        terminal
            .get("final_text")
            .and_then(serde_json::Value::as_str),
        Some(REPLY)
    );
    assert!(
        !emitted.iter().any(|(name, _)| name == "dialogue-corrected"),
        "no correction event exists: the committed reply is the streamed reply"
    );
    assert!(
        emitted.iter().any(|(name, payload)| {
            name == "text-log"
                && payload
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| text.contains(ACTION))
        }),
        "the accepted action is narrated"
    );
    drop(emitted);

    // The speaker's mood took the model's accepted value.
    let npcs = live.npc_manager.lock().await;
    let speaker = npcs
        .all_npcs()
        .find(|candidate| candidate.name == npc)
        .expect("the speaker");
    assert_eq!(speaker.mood, "friendly");
}

/// The phone path: the host answers each suspended call with the Endpoint's
/// terminal output.
#[tokio::test]
async fn phone_path_commits_a_reply_naming_a_nonexistent_person_unchanged() {
    let live = Live::rundale();
    let mut engine = live.engine();
    let npc = live.first_npc_here().await;

    let step = engine.submit(&live.ctx(), talk_to(&npc)).await.unwrap();
    // An explicit recipient makes no intent call: the first call is the dialogue.
    let pending = awaiting(&step).clone();
    assert_eq!(pending.call.subrole, InferenceSubrole::Dialogue);
    assert!(
        pending.call.prompt.contains("Mulvihill"),
        "the player's question reaches the model"
    );

    let done = engine
        .resume(
            &live.ctx(),
            InferenceResolution {
                call_id: pending.id.clone(),
                attempt_id: pending.attempt_id.clone(),
                base_revision: pending.base_revision,
                outcome: InferenceOutcome::Completed {
                    text: reply_json(),
                    report: CallReport::default(),
                },
            },
        )
        .await
        .unwrap();

    assert_committed_unchanged(&live, &done, &npc).await;
}

/// An interactive-lane worker that answers every dialogue request with the
/// same scripted reply.
fn scripted_queue() -> (InferenceQueue, tokio::task::JoinHandle<usize>) {
    let (interactive_tx, mut interactive_rx) = tokio::sync::mpsc::channel::<InferenceRequest>(4);
    let (background_tx, _) = tokio::sync::mpsc::channel(1);
    let (batch_tx, _) = tokio::sync::mpsc::channel(1);
    let worker = tokio::spawn(async move {
        let mut served = 0;
        while let Some(request) = interactive_rx.recv().await {
            served += 1;
            if let Some(tokens) = request.token_tx {
                let _ = tokens.send(reply_json()).await;
            }
            let _ = request.response_tx.send(InferenceResponse {
                id: request.id,
                text: reply_json(),
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

/// The desktop path: `drive_in_process` fulfils the call through the
/// inference queue, as the Tauri and server hosts do.
#[tokio::test]
async fn desktop_path_commits_a_reply_naming_a_nonexistent_person_unchanged() {
    let live = Live::rundale();
    let (queue, worker) = scripted_queue();
    *live.inference_queue.lock().await = Some(queue);
    let mut engine = live.engine();
    let npc = live.first_npc_here().await;

    let ctx = live.ctx();
    let in_process = InProcessInference::from_ctx(&ctx);
    let done = drive_in_process(&mut engine, &ctx, talk_to(&npc), &in_process)
        .await
        .unwrap();

    assert_committed_unchanged(&live, &done, &npc).await;
    drop(ctx);
    *live.inference_queue.lock().await = None;
    assert_eq!(
        worker.await.unwrap(),
        1,
        "exactly one dialogue call: nothing retried or regenerated the reply"
    );
}

/// The one thing that still fails a turn is the structural contract: a reply
/// that is not complete JSON, or has no spoken dialogue, commits nothing.
#[tokio::test]
async fn an_incomplete_reply_still_commits_nothing() {
    for body in [
        r#"{"dialogue":"   ","action":"nods","mood":"friendly"}"#,
        r#"{"dialogue":"Tadhg Mulvihill is a grand man"#,
        "Tadhg Mulvihill is a grand man.",
    ] {
        let live = Live::rundale();
        let mut engine = live.engine();
        let npc = live.first_npc_here().await;
        let step = engine.submit(&live.ctx(), talk_to(&npc)).await.unwrap();
        let pending = awaiting(&step).clone();

        let done = engine
            .resume(
                &live.ctx(),
                InferenceResolution {
                    call_id: pending.id.clone(),
                    attempt_id: pending.attempt_id.clone(),
                    base_revision: pending.base_revision,
                    outcome: InferenceOutcome::Completed {
                        text: body.to_string(),
                        report: CallReport::default(),
                    },
                },
            )
            .await
            .unwrap();

        assert!(
            !matches!(
                done.status,
                TurnStatus::Completed {
                    outcome: TerminalOutcome::Succeeded,
                    ..
                }
            ),
            "{body}: a structurally incomplete reply must not commit"
        );
        let world = live.world.lock().await;
        assert!(
            world
                .conversation_log
                .recent_at(world.player_location, 4)
                .is_empty(),
            "{body}: nothing is recorded"
        );
    }
}
