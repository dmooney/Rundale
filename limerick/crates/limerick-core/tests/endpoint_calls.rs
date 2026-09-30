//! Endpoint references on the engine's model calls (#2041, ADR-025 §5).
//!
//! Every test runs the canonical `mods/rundale` world, whose manifest
//! declares the dialogue and intent Endpoint definitions. The engine attaches
//! each role's reference and structured input to the call; the desktop
//! in-process host ignores them, so its provider requests are unchanged.

use std::path::PathBuf;
use std::sync::Arc;

use limerick_core::config::{InferenceConfig, InferenceSubrole};
use limerick_core::endpoint_input::{
    DIALOGUE_MAX_OUTPUT_CHARS, DIALOGUE_ROLE, EndpointCall, INTENT_ROLE, InvocationEnvelope,
    dialogue_endpoint, intent_endpoint,
};
use limerick_core::game_loop::{GameLoopContext, InferenceSlots, rebuild_inference_worker};
use limerick_core::game_mod::{EndpointCatalog, EndpointRole, GameMod, NO_ENDPOINTS};
use limerick_core::inference::AnyClient;
use limerick_core::inference::file_log::InferenceFileLog;
use limerick_core::ipc::{CapturingEmitter, ConversationRuntimeState, EventEmitter, GameConfig};
use limerick_core::npc::manager::NpcManager;
use limerick_core::turn::{
    ExecutionAttemptId, LogicalRequestId, MemoryTurnJournal, PendingInference, StateRevision,
    TurnEngine, TurnInput, TurnRules, TurnStatus, TurnStep,
};
use limerick_core::turn_inference::{CallReport, InferenceOutcome};
use limerick_core::world::WorldState;
use limerick_core::world::transport::TransportMode;
use limerick_types::ConversationExchange;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const PLAYER_LINE: &str = "Good day to you. Any news of the fair?";
const NPC_LINE: &str = "God save ye kindly. 'Tis a soft day for the time of year.";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn canonical_mod() -> GameMod {
    GameMod::load(&repo_root().join("mods/rundale")).expect("load mods/rundale")
}

/// Runtime-owned live state on the canonical world.
struct Live {
    game_mod: GameMod,
    world: Mutex<WorldState>,
    npc_manager: Mutex<NpcManager>,
    config: Mutex<GameConfig>,
    conversation: Mutex<ConversationRuntimeState>,
    inference_queue: Mutex<Option<limerick_core::inference::InferenceQueue>>,
    client: Mutex<Option<AnyClient>>,
    cloud_client: Mutex<Option<AnyClient>>,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    inference_config: InferenceConfig,
    emitter: Arc<CapturingEmitter>,
}

impl Live {
    fn canonical() -> Self {
        let game_mod = canonical_mod();
        let (mut world, npc_manager) =
            limerick_core::game_loop::load_fresh_world_and_npcs(Some(&game_mod), &game_mod.mod_dir)
                .expect("load the canonical world");
        world.clock.pause();
        // At the 07:00 start Peig has just left for the village road; begin in
        // the Letter Office, where she still is, so someone is here to talk to.
        world.player_location = limerick_core::world::LocationId(2);
        Self {
            game_mod,
            world: Mutex::new(world),
            npc_manager: Mutex::new(npc_manager),
            config: Mutex::new(GameConfig::default()),
            conversation: Mutex::new(ConversationRuntimeState::new()),
            inference_queue: Mutex::new(None),
            client: Mutex::new(None),
            cloud_client: Mutex::new(None),
            worker: Mutex::new(None),
            inference_config: InferenceConfig::default(),
            emitter: Arc::new(CapturingEmitter::new()),
        }
    }

    fn ctx<'a>(&'a self, endpoints: &'a EndpointCatalog) -> GameLoopContext<'a> {
        GameLoopContext {
            world: &self.world,
            npc_manager: &self.npc_manager,
            config: &self.config,
            conversation: &self.conversation,
            inference_queue: &self.inference_queue,
            emitter: self.emitter.clone() as Arc<dyn EventEmitter>,
            inference_config: &self.inference_config,
            pronunciations: &self.game_mod.pronunciations,
            endpoints,
            client: &self.client,
            cloud_client: &self.cloud_client,
            language: limerick_core::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
            inference_override: None,
        }
    }

    fn transport() -> TransportMode {
        TransportMode {
            id: "walking".to_string(),
            label: "on foot".to_string(),
            speed_m_per_s: 1.2,
        }
    }

    /// The name of the first NPC (by name) at the player's location.
    async fn npc_here(&self) -> String {
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

fn npc_reply() -> String {
    json!({
        "dialogue": NPC_LINE,
        "action": "sets down a creel of turf",
        "mood": "content",
        "language_hints": [],
        "assigned_task": null,
        "internal_thought": null
    })
    .to_string()
}

fn talk_to(npc: &str) -> TurnInput {
    TurnInput {
        request_id: None,
        text: PLAYER_LINE.to_string(),
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

fn completed(text: String) -> InferenceOutcome {
    InferenceOutcome::Completed {
        text,
        report: CallReport::default(),
    }
}

fn envelope() -> InvocationEnvelope {
    InvocationEnvelope {
        session_id: "fixture-session".to_string(),
        request_id: LogicalRequestId::new("fixture-logical-request"),
        attempt_id: ExecutionAttemptId::new("fixture-attempt"),
        base_revision: StateRevision(0),
    }
}

#[test]
fn the_intent_endpoint_instructions_are_the_engine_intent_prompt() {
    let game_mod = canonical_mod();
    let intent = game_mod.endpoints.intent.as_ref().expect("intent declared");
    assert_eq!(
        intent.definition.instructions,
        limerick_core::input::intent_system_prompt(),
        "the intent Endpoint must publish the engine's intent prompt verbatim; \
         regenerate mods/rundale/endpoints/rundale-intent.v1.json after changing it"
    );
}

/// The canonical turn through the portable engine: the intent and dialogue
/// calls handed to the host carry the mod's Endpoint references and inputs
/// built from the same world the prompts describe.
#[tokio::test]
async fn turn_engine_calls_carry_the_mod_endpoint_references_and_inputs() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    let rules = TurnRules {
        transport: Live::transport(),
        reaction_templates: live.game_mod.reactions.clone(),
    };
    let mut engine = TurnEngine::new(Arc::new(MemoryTurnJournal::new()), rules);
    let npc = live.npc_here().await;

    let step = engine
        .submit(&live.ctx(&endpoints), talk_to(&npc))
        .await
        .expect("submit");
    let intent = awaiting(&step).clone();
    assert_eq!(intent.call.subrole, InferenceSubrole::Intent);
    let call = intent.call.endpoint.as_ref().expect("intent Endpoint");
    assert_eq!(
        Some(&call.reference),
        endpoints.reference(EndpointRole::Intent)
    );
    assert_eq!(
        serde_json::to_value(&call.input).unwrap(),
        json!({"role": INTENT_ROLE, "playerInput": PLAYER_LINE})
    );

    let reply = json!({"intent": "talk", "target": null, "dialogue": PLAYER_LINE}).to_string();
    let step = engine
        .resume(
            &live.ctx(&endpoints),
            limerick_core::turn::InferenceResolution {
                call_id: intent.id.clone(),
                attempt_id: intent.attempt_id.clone(),
                base_revision: intent.base_revision,
                outcome: completed(reply),
            },
        )
        .await
        .expect("resume the intent call");
    let dialogue = awaiting(&step);
    assert_eq!(dialogue.call.subrole, InferenceSubrole::Dialogue);
    let call = dialogue.call.endpoint.as_ref().expect("dialogue Endpoint");
    assert_eq!(
        Some(&call.reference),
        endpoints.reference(EndpointRole::Dialogue)
    );
    let input = serde_json::to_value(&call.input).unwrap();
    assert_eq!(input["role"], DIALOGUE_ROLE);
    assert_eq!(input["playerInput"], PLAYER_LINE);
    assert_eq!(input["speaker"]["displayName"], npc.as_str());
    assert_eq!(input["maxOutputChars"], DIALOGUE_MAX_OUTPUT_CHARS);
    let names = |key: &str| -> Vec<String> {
        input[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["displayName"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        names("knownPeople"),
        ["Peig Hannigan", "Mícheál Connolly", "Róisín Connolly"]
    );
    assert_eq!(
        names("knownPlaces"),
        ["Kilteevan Village", "Letter Office", "Connolly Cottage"]
    );
    let world = live.world.lock().await;
    assert_eq!(
        input["currentLocation"]["displayName"],
        world.current_location().name.as_str()
    );
    let npcs = live.npc_manager.lock().await;
    let speaker = npcs
        .all_npcs()
        .find(|candidate| candidate.name == npc)
        .expect("speaker");
    let facts: Vec<&str> = input["authoredFacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|fact| fact["statement"].as_str().unwrap())
        .collect();
    assert_eq!(facts, speaker.knowledge);
}

#[tokio::test]
async fn calls_carry_no_endpoint_when_the_mod_declares_none() {
    let live = Live::canonical();
    let rules = TurnRules {
        transport: Live::transport(),
        reaction_templates: live.game_mod.reactions.clone(),
    };
    let mut engine = TurnEngine::new(Arc::new(MemoryTurnJournal::new()), rules);
    let npc = live.npc_here().await;
    let step = engine
        .submit(&live.ctx(&NO_ENDPOINTS), talk_to(&npc))
        .await
        .expect("submit");
    assert!(awaiting(&step).call.endpoint.is_none());
}

/// Answers intent requests as speech and dialogue requests with the NPC
/// reply, streamed when the request streams.
struct ScriptedProvider;

impl Respond for ScriptedProvider {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).expect("JSON request body");
        let text = if request_text(&body).contains("text adventure input parser") {
            json!({"intent": "talk", "target": null, "dialogue": PLAYER_LINE}).to_string()
        } else {
            npc_reply()
        };
        if body["stream"] == json!(true) {
            let chunk = json!({"choices": [{"delta": {"content": text}, "finish_reason": "stop"}]});
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(format!("data: {chunk}\n\ndata: [DONE]\n\n"))
        } else {
            ResponseTemplate::new(200).set_body_json(json!({
                "id": "c",
                "object": "chat.completion",
                "model": "test-model",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": text},
                    "finish_reason": "stop"
                }]
            }))
        }
    }
}

fn request_text(body: &Value) -> String {
    body["messages"]
        .as_array()
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| message["content"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Plays one desktop turn through the in-process provider path against a
/// recording provider and returns every request body it received.
async fn desktop_request_bodies(with_endpoints: bool) -> Vec<Value> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ScriptedProvider)
        .mount(&server)
        .await;
    let live = Live::canonical();
    let log = limerick_core::inference::new_inference_log();
    rebuild_inference_worker(
        "lmstudio",
        &format!("{}/v1", server.uri()),
        None,
        &live.inference_config,
        log.clone(),
        InferenceFileLog::disabled(),
        InferenceSlots {
            client: &live.client,
            worker_handle: &live.worker,
            inference_queue: &live.inference_queue,
        },
    )
    .await;
    let endpoints = if with_endpoints {
        live.game_mod.endpoints.clone()
    } else {
        EndpointCatalog::default()
    };
    let npc = live.npc_here().await;
    limerick_core::game_loop::handle_game_input(
        &live.ctx(&endpoints),
        PLAYER_LINE.to_string(),
        vec![npc],
        &Live::transport(),
        &live.game_mod.reactions,
        || None,
    )
    .await;
    let spoke = live
        .emitter
        .events()
        .iter()
        .any(|(_, payload)| payload.to_string().contains(NPC_LINE));
    assert!(spoke, "the turn must reach and apply the dialogue reply");
    server
        .received_requests()
        .await
        .expect("recorded requests")
        .iter()
        .map(|request| serde_json::from_slice(&request.body).expect("JSON body"))
        .collect()
}

/// Desktop in-process inference is unchanged: the provider receives the same
/// request bodies whether or not the calls carry Endpoint references.
#[tokio::test]
async fn desktop_request_bodies_are_unchanged_by_endpoint_references() {
    let without = desktop_request_bodies(false).await;
    let with = desktop_request_bodies(true).await;
    assert_eq!(
        without.len(),
        2,
        "one intent and one dialogue request: {without:#?}"
    );
    assert!(request_text(&without[0]).contains("text adventure input parser"));
    assert!(request_text(&without[1]).contains(PLAYER_LINE));
    assert_eq!(with, without);
}

fn fixture_path(name: &str) -> PathBuf {
    repo_root().join("mobile/endpoint").join(name)
}

/// Compares `value` with a checked-in fixture; `UPDATE_ENDPOINT_FIXTURES=1`
/// rewrites it.
fn assert_fixture(name: &str, value: &Value) {
    let path = fixture_path(name);
    let rendered = format!("{}\n", serde_json::to_string_pretty(value).unwrap());
    if std::env::var_os("UPDATE_ENDPOINT_FIXTURES").is_some() {
        std::fs::write(&path, &rendered).unwrap();
    }
    let expected = std::fs::read_to_string(&path).expect("read fixture");
    assert_eq!(
        serde_json::from_str::<Value>(&expected).unwrap(),
        *value,
        "{name} is stale; rerun with UPDATE_ENDPOINT_FIXTURES=1"
    );
}

/// Checks the invocation's top-level shape against the definition's input
/// schema: exactly the declared properties, all required ones present. The
/// Endpoints test suite validates the same fixtures with a full JSON Schema
/// validator.
fn assert_matches_input_schema(invocation: &Value, catalog: &EndpointCatalog, role: EndpointRole) {
    let file = match role {
        EndpointRole::Dialogue => catalog.dialogue.as_ref(),
        EndpointRole::Intent => catalog.intent.as_ref(),
    }
    .expect("declared");
    let schema = &file.definition.input_schema;
    let mut declared: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key.as_str().unwrap())
        .collect();
    let mut sent: Vec<&str> = invocation
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    declared.sort();
    required.sort();
    sent.sort();
    assert_eq!(sent, declared, "{role:?} invocation properties");
    assert_eq!(
        sent, required,
        "{role:?} invocation requires every property"
    );
}

/// The example invocations the Endpoints suite validates against the mod's
/// definitions are the engine's own output on the canonical world.
#[tokio::test]
async fn example_invocations_are_the_engine_output_on_the_canonical_world() {
    let live = Live::canonical();
    let catalog = &live.game_mod.endpoints;
    let mut world = live.world.lock().await;
    let npcs = live.npc_manager.lock().await;
    let speaker = npcs
        .all_npcs()
        .find(|npc| npc.name == "Peig Hannigan")
        .expect("Peig")
        .id;
    let here = world.player_location;
    world.conversation_log.add(ConversationExchange {
        timestamp: "1820-03-20T07:05:00Z".parse().unwrap(),
        speaker_id: speaker,
        speaker_name: "Peig Hannigan".to_string(),
        player_input: "Good morning to you.".to_string(),
        npc_dialogue: "And to you. The post is late again.".to_string(),
        location: here,
    });

    let dialogue: EndpointCall = dialogue_endpoint(
        catalog,
        &world,
        &npcs,
        speaker,
        "ask Peig about the old church",
    )
    .expect("dialogue Endpoint");
    let dialogue = dialogue.invocation(&envelope());
    assert_matches_input_schema(&dialogue, catalog, EndpointRole::Dialogue);
    assert_fixture("example-engine-invocation.json", &dialogue);

    let intent = intent_endpoint(catalog, "Let's make for the Letter Office")
        .expect("intent Endpoint")
        .invocation(&envelope());
    assert_matches_input_schema(&intent, catalog, EndpointRole::Intent);
    assert_fixture("example-intent-invocation.json", &intent);
}

/// Inputs stay inside the definition's bounds: over-long player input is cut
/// to the schema maximum, and exchanges missing a side are not sent.
#[tokio::test]
async fn dialogue_inputs_respect_the_definition_bounds() {
    let live = Live::canonical();
    let catalog = &live.game_mod.endpoints;
    let mut world = live.world.lock().await;
    let npcs = live.npc_manager.lock().await;
    let speaker = npcs
        .all_npcs()
        .find(|npc| npc.name == "Peig Hannigan")
        .expect("Peig")
        .id;
    let here = world.player_location;
    for (player_input, npc_dialogue) in [("", "Unprompted remark."), ("Hello.", "Good day.")] {
        world.conversation_log.add(ConversationExchange {
            timestamp: "1820-03-20T07:05:00Z".parse().unwrap(),
            speaker_id: speaker,
            speaker_name: "Peig Hannigan".to_string(),
            player_input: player_input.to_string(),
            npc_dialogue: npc_dialogue.to_string(),
            location: here,
        });
    }
    let long_input = "a".repeat(limerick_core::endpoint_input::MAX_PLAYER_INPUT_CHARS + 10);
    let invocation = dialogue_endpoint(catalog, &world, &npcs, speaker, &long_input)
        .expect("dialogue Endpoint")
        .invocation(&envelope());
    assert_eq!(
        invocation["playerInput"].as_str().unwrap().chars().count(),
        limerick_core::endpoint_input::MAX_PLAYER_INPUT_CHARS
    );
    let exchanges = invocation["recentConversation"].as_array().unwrap();
    assert_eq!(exchanges.len(), 1);
    assert_eq!(exchanges[0]["npc_dialogue"], "Good day.");
}
