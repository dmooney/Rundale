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
    Addressing, DIALOGUE_MAX_OUTPUT_CHARS, DIALOGUE_ROLE, EndpointCall, INTENT_ROLE,
    InvocationEnvelope, dialogue_endpoint, intent_endpoint,
};
use limerick_core::game_loop::{GameLoopContext, InferenceSlots, rebuild_inference_worker};
use limerick_core::game_mod::{EndpointCatalog, EndpointRole, GameMod, NO_ENDPOINTS};
use limerick_core::inference::AnyClient;
use limerick_core::inference::file_log::InferenceFileLog;
use limerick_core::ipc::{CapturingEmitter, ConversationRuntimeState, EventEmitter, GameConfig};
use limerick_core::npc::NpcId;
use limerick_core::npc::manager::NpcManager;
use limerick_core::npc::types::NpcState;
use limerick_core::turn::{
    ExecutionAttemptId, LogicalRequestId, MemoryTurnJournal, PendingInference, StateRevision,
    TurnEngine, TurnInput, TurnRules, TurnStatus, TurnStep,
};
use limerick_core::turn_inference::{CallReport, InferenceOutcome};
use limerick_core::world::WorldState;
use limerick_core::world::transport::TransportMode;
use limerick_types::{ConversationExchange, GameClock, LocationId};
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

/// `rundale-intent` is a decision Endpoint answered by Jev on TypeSafe's own
/// API (design §4.6 and §5).
#[test]
fn the_intent_endpoint_is_a_jev_decision() {
    let game_mod = canonical_mod();
    let intent = game_mod.endpoints.intent.as_ref().expect("intent declared");
    assert!(intent.definition.is_decision());
    assert_eq!(
        intent.definition.provider_config.provider,
        limerick_mod::EndpointProvider::Typesafe
    );
    assert_eq!(intent.definition.provider_config.model, "jev-1.13.0");
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
        .submit(&live.ctx(&endpoints), says(PLAYER_LINE))
        .await
        .expect("submit");
    let intent = awaiting(&step).clone();
    assert_eq!(intent.call.subrole, InferenceSubrole::Intent);
    let call = intent.call.endpoint.as_ref().expect("intent Endpoint");
    assert_eq!(
        Some(&call.reference),
        endpoints.reference(EndpointRole::Intent)
    );
    let input = serde_json::to_value(&call.input).unwrap();
    assert_eq!(input["role"], INTENT_ROLE);
    assert_eq!(input["line"], PLAYER_LINE);
    let option = input["present"]
        .as_array()
        .unwrap()
        .iter()
        .find(|person| person["name"] == npc.as_str())
        .expect("the person here is offered")["optionId"]
        .as_str()
        .unwrap()
        .to_string();

    // Jev's decision: speech, spoken to the person here.
    let choice = |choice: &str| json!({"choice": choice, "confidence": 0.95, "probabilities": {choice: 0.95}});
    let reply = json!({
        "action": choice("talk"),
        "destination": choice("none"),
        "target": choice("none"),
        "recipient": choice(&option),
        "mentioned": {},
        "atmosphere": choice("none"),
    })
    .to_string();
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
    limerick_core::game_loop::handle_game_input(
        &live.ctx(&endpoints),
        PLAYER_LINE.to_string(),
        Vec::new(),
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
        witnesses: Default::default(),
    });

    let dialogue: EndpointCall = dialogue_endpoint(
        catalog,
        &world,
        &npcs,
        speaker,
        "ask Peig about the old church",
        Addressing::SpokenTo,
    )
    .expect("dialogue Endpoint");
    let dialogue = dialogue.invocation(&envelope());
    assert_matches_input_schema(&dialogue, catalog, EndpointRole::Dialogue);
    assert_fixture("example-engine-invocation.json", &dialogue);

    let (intent, _) = intent_endpoint(catalog, &world, &npcs, "Let's make for the Letter Office")
        .expect("intent Endpoint");
    let intent = intent.invocation(&envelope());
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
            witnesses: Default::default(),
        });
    }
    let long_input = "a".repeat(limerick_core::endpoint_input::MAX_PLAYER_INPUT_CHARS + 10);
    let invocation = dialogue_endpoint(
        catalog,
        &world,
        &npcs,
        speaker,
        &long_input,
        Addressing::SpokenTo,
    )
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

const VILLAGE: LocationId = LocationId(1);
const LETTER_OFFICE: LocationId = LocationId(2);
const COTTAGE: LocationId = LocationId(3);

impl Live {
    /// The id of the canonical NPC called `name`.
    async fn id_of(&self, name: &str) -> NpcId {
        self.npc_manager
            .lock()
            .await
            .all_npcs()
            .find(|npc| npc.name == name)
            .unwrap_or_else(|| panic!("{name} in the canonical world"))
            .id
    }

    /// Sets the game clock to `at` (UTC, RFC 3339), paused.
    async fn set_time(&self, at: &str) {
        let mut clock = GameClock::new(at.parse().unwrap());
        clock.pause();
        self.world.lock().await.clock = clock;
    }

    /// Puts the player at `player` and each named NPC at its place.
    async fn arrange(&self, player: LocationId, people: &[(&str, LocationId)]) {
        let mut ids = Vec::new();
        for (name, place) in people {
            ids.push((self.id_of(name).await, *place));
        }
        self.world.lock().await.player_location = player;
        let mut npcs = self.npc_manager.lock().await;
        for (id, place) in ids {
            npcs.get_mut(id)
                .unwrap()
                .set_location_and_state(place, NpcState::Present);
        }
    }
}

fn says(line: &str) -> TurnInput {
    TurnInput {
        request_id: None,
        text: line.to_string(),
        addressed_to: Vec::new(),
        draft_id: None,
    }
}

fn spoken_to(line: &str, people: &[&str]) -> TurnInput {
    TurnInput {
        addressed_to: people.iter().map(|name| name.to_string()).collect(),
        ..says(line)
    }
}

fn dialogue_reply(line: &str) -> String {
    json!({"dialogue": line}).to_string()
}

fn canonical_engine(live: &Live) -> TurnEngine {
    TurnEngine::new(
        Arc::new(MemoryTurnJournal::new()),
        TurnRules {
            transport: Live::transport(),
            reaction_templates: live.game_mod.reactions.clone(),
        },
    )
}

/// The structured dialogue input of an awaited dialogue call.
fn dialogue_snapshot(pending: &PendingInference) -> Value {
    assert_eq!(pending.call.subrole, InferenceSubrole::Dialogue);
    let call = pending.call.endpoint.as_ref().expect("dialogue Endpoint");
    serde_json::to_value(&call.input).unwrap()
}

/// Submits `input` on the production turn path, answers its intent call as
/// talk, and returns the first dialogue call.
async fn first_dialogue_call(
    engine: &mut TurnEngine,
    live: &Live,
    endpoints: &EndpointCatalog,
    input: TurnInput,
) -> PendingInference {
    let line = input.text.clone();
    let step = engine
        .submit(&live.ctx(endpoints), input)
        .await
        .expect("submit");
    let pending = awaiting(&step).clone();
    if pending.call.subrole == InferenceSubrole::Dialogue {
        return pending;
    }
    assert_eq!(pending.call.subrole, InferenceSubrole::Intent);
    let reply = json!({"intent": "talk", "target": null, "dialogue": line}).to_string();
    let step = resolve(engine, live, endpoints, &pending, completed(reply)).await;
    awaiting(&step).clone()
}

async fn resolve(
    engine: &mut TurnEngine,
    live: &Live,
    endpoints: &EndpointCatalog,
    pending: &PendingInference,
    outcome: InferenceOutcome,
) -> TurnStep {
    engine
        .resume(
            &live.ctx(endpoints),
            limerick_core::turn::InferenceResolution {
                call_id: pending.id.clone(),
                attempt_id: pending.attempt_id.clone(),
                base_revision: pending.base_revision,
                outcome,
            },
        )
        .await
        .expect("resume")
}

fn exchange_lines(snapshot: &Value) -> Vec<String> {
    snapshot["recentConversation"]
        .as_array()
        .unwrap()
        .iter()
        .map(|exchange| exchange["npc_dialogue"].as_str().unwrap().to_string())
        .collect()
}

fn ids(snapshot: &Value, key: &str) -> Vec<String> {
    snapshot[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|person| person["id"].as_str().unwrap().to_string())
        .collect()
}

/// Dynamic NPC context §8 row 23: a speaker's snapshot on the production
/// turn path sends nobody's live whereabouts, names the people in the room,
/// and carries the speaker's authored mood, the time, the weather and what
/// the speaker knows of the player.
#[tokio::test]
async fn a_speakers_snapshot_holds_only_what_the_speaker_could_know() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    live.set_time("1820-03-20T08:00:00Z").await;
    live.arrange(
        LETTER_OFFICE,
        &[
            ("Peig Hannigan", LETTER_OFFICE),
            ("Mícheál Connolly", COTTAGE),
            ("Róisín Connolly", COTTAGE),
        ],
    )
    .await;
    let peig = live.id_of("Peig Hannigan").await;
    let mut engine = canonical_engine(&live);

    let call = first_dialogue_call(
        &mut engine,
        &live,
        &endpoints,
        spoken_to("Good morning.", &["Peig Hannigan"]),
    )
    .await;
    let snapshot = dialogue_snapshot(&call);

    assert_eq!(snapshot["speaker"]["id"], format!("npc-{}", peig.0));
    assert_eq!(snapshot["speaker"]["mood"], "alert", "authored mood");
    assert_eq!(snapshot["addressing"], "spoken_to");
    assert_eq!(
        snapshot["time"],
        json!({"date": "1820-03-20", "clockTime": "08:00", "partOfDay": "Morning"})
    );
    let weather = live.world.lock().await.weather.to_string();
    assert_eq!(snapshot["weather"], weather.as_str());
    assert_eq!(snapshot["festival"], Value::Null);
    assert_eq!(
        snapshot["player"],
        json!({"knowsName": false, "name": null})
    );
    assert!(
        ids(&snapshot, "present").is_empty(),
        "Peig is alone in the Letter Office"
    );
    for person in snapshot["knownPeople"].as_array().unwrap() {
        let keys: Vec<&String> = person.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["displayName", "id", "role"], "{person}");
    }
    assert_eq!(ids(&snapshot, "knownPeople").len(), 3);
    for place in snapshot["knownPlaces"].as_array().unwrap() {
        let description = place["description"].as_str().unwrap();
        for name in ["Peig", "Mícheál", "Róisín"] {
            assert!(
                !description.contains(name),
                "{} names {name}: {description}",
                place["displayName"]
            );
        }
    }
    let text = snapshot.to_string();
    assert!(
        !text.contains("currentLocationName\":\"Connolly Cottage"),
        "no one's live location but the speaker's is sent: {text}"
    );

    // In the cottage, each Connolly is present for the other, and a speaker
    // who has learned the player's name is told it.
    live.arrange(
        COTTAGE,
        &[("Mícheál Connolly", COTTAGE), ("Róisín Connolly", COTTAGE)],
    )
    .await;
    let micheal = live.id_of("Mícheál Connolly").await;
    let roisin = live.id_of("Róisín Connolly").await;
    live.world.lock().await.player_name = Some("Cormac".to_string());
    live.npc_manager.lock().await.teach_player_name(roisin);
    let world = live.world.lock().await;
    let npcs = live.npc_manager.lock().await;
    let snapshot = |speaker| {
        serde_json::to_value(
            dialogue_endpoint(
                &endpoints,
                &world,
                &npcs,
                speaker,
                "Hello",
                Addressing::SpokenTo,
            )
            .unwrap()
            .input,
        )
        .unwrap()
    };
    let for_roisin = snapshot(roisin);
    assert_eq!(ids(&for_roisin, "present"), [format!("npc-{}", micheal.0)]);
    assert_eq!(
        for_roisin["player"],
        json!({"knowsName": true, "name": "Cormac"})
    );
    let for_micheal = snapshot(micheal);
    assert_eq!(ids(&for_micheal, "present"), [format!("npc-{}", roisin.0)]);
    assert_eq!(
        for_micheal["player"],
        json!({"knowsName": false, "name": null}),
        "Mícheál has not learned the player's name"
    );
}

/// Dynamic NPC context §8 row 24: an exchange with Peig in the village at
/// 08:00 is absent from Mícheál's snapshot there at 15:00, because he did
/// not witness it. Peig, who did, still has it.
#[tokio::test]
async fn a_speaker_is_sent_only_the_exchanges_they_witnessed() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    live.set_time("1820-03-20T08:00:00Z").await;
    live.arrange(
        VILLAGE,
        &[
            ("Peig Hannigan", VILLAGE),
            ("Mícheál Connolly", COTTAGE),
            ("Róisín Connolly", COTTAGE),
        ],
    )
    .await;
    let mut engine = canonical_engine(&live);
    let line = "Peig, I have a letter for Dublin.";
    let call = first_dialogue_call(
        &mut engine,
        &live,
        &endpoints,
        spoken_to(line, &["Peig Hannigan"]),
    )
    .await;
    let step = resolve(
        &mut engine,
        &live,
        &endpoints,
        &call,
        completed(dialogue_reply("Leave it with me, so.")),
    )
    .await;
    assert!(
        matches!(step.status, TurnStatus::Completed { .. }),
        "{:?}",
        step.status
    );
    {
        let world = live.world.lock().await;
        let peig = live.id_of("Peig Hannigan").await;
        let exchange = world.conversation_log.all().last().expect("committed");
        assert_eq!(exchange.player_input, line);
        assert_eq!(
            exchange.witnesses.iter().copied().collect::<Vec<_>>(),
            [peig],
            "only Peig was in the village to hear it"
        );
    }

    live.set_time("1820-03-20T15:00:00Z").await;
    live.arrange(
        VILLAGE,
        &[
            ("Peig Hannigan", LETTER_OFFICE),
            ("Mícheál Connolly", VILLAGE),
        ],
    )
    .await;
    let call = first_dialogue_call(
        &mut engine,
        &live,
        &endpoints,
        spoken_to("Good afternoon.", &["Mícheál Connolly"]),
    )
    .await;
    let for_micheal = dialogue_snapshot(&call);
    assert_eq!(for_micheal["time"]["clockTime"], "15:00");
    assert!(
        exchange_lines(&for_micheal).is_empty(),
        "Mícheál was not there at 08:00: {for_micheal}"
    );

    let world = live.world.lock().await;
    let npcs = live.npc_manager.lock().await;
    let peig = npcs
        .all_npcs()
        .find(|npc| npc.name == "Peig Hannigan")
        .unwrap()
        .id;
    let for_peig = serde_json::to_value(
        dialogue_endpoint(
            &endpoints,
            &world,
            &npcs,
            peig,
            "Hello again",
            Addressing::SpokenTo,
        )
        .unwrap()
        .input,
    )
    .unwrap();
    assert_eq!(exchange_lines(&for_peig), ["Leave it with me, so."]);
}

/// Dynamic NPC context §6: in a line with several speakers, each later
/// speaker's snapshot includes the replies already committed in that line,
/// which they witnessed.
#[tokio::test]
async fn a_later_speaker_in_a_line_sees_the_replies_already_given() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    live.set_time("1820-03-20T10:00:00Z").await;
    live.arrange(
        COTTAGE,
        &[("Mícheál Connolly", COTTAGE), ("Róisín Connolly", COTTAGE)],
    )
    .await;
    let mut engine = canonical_engine(&live);
    let first = first_dialogue_call(
        &mut engine,
        &live,
        &endpoints,
        spoken_to(
            "God bless all here.",
            &["Mícheál Connolly", "Róisín Connolly"],
        ),
    )
    .await;
    let first_snapshot = dialogue_snapshot(&first);
    assert!(exchange_lines(&first_snapshot).is_empty());
    let first_speaker = first_snapshot["speaker"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let step = resolve(
        &mut engine,
        &live,
        &endpoints,
        &first,
        completed(dialogue_reply("And you too, traveller.")),
    )
    .await;
    let second = dialogue_snapshot(awaiting(&step));
    assert_ne!(second["speaker"]["id"], first_speaker.as_str());
    // The first reply as committed to the attempt, after the engine's
    // dialogue guards.
    let seen = exchange_lines(&second);
    assert_eq!(seen.len(), 1, "{second}");
    assert!(seen[0].contains("And you too, traveller."), "{seen:?}");
    assert_eq!(
        second["recentConversation"][0]["player_input"],
        "God bless all here."
    );
}

/// Dynamic NPC context §8 row 34: on 1 May every speaker's snapshot names
/// Bealtaine; on other days `festival` is null.
#[tokio::test]
async fn the_snapshot_names_todays_festival() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    let peig = live.id_of("Peig Hannigan").await;
    for (at, festival) in [
        ("1820-05-01T09:00:00Z", json!("Bealtaine")),
        ("1820-05-02T09:00:00Z", Value::Null),
    ] {
        live.set_time(at).await;
        let world = live.world.lock().await;
        let npcs = live.npc_manager.lock().await;
        for speaker in npcs.all_npcs().map(|npc| npc.id).collect::<Vec<_>>() {
            let input = serde_json::to_value(
                dialogue_endpoint(
                    &endpoints,
                    &world,
                    &npcs,
                    speaker,
                    "What day is it?",
                    Addressing::SpokenTo,
                )
                .unwrap()
                .input,
            )
            .unwrap();
            assert_eq!(input["festival"], festival, "{at}, speaker {speaker:?}");
        }
        assert!(npcs.get(peig).is_some());
    }
}

// ── Recipient resolution (dynamic NPC context design §3 and §8) ─────────────
//
// Each test runs the canonical world on the production turn path. The
// scripted transport answers the `rundale-intent` decision from the call's
// own options, the way Jev would on that case, so these rows test what the
// engine does with an answer, not the model.

/// How the scripted Jev answers the recipient and mention questions.
struct Answer {
    /// A person's full name, or a fixed option ("all", "someone_else",
    /// "no_one"), or `ref:<phrase>` for a reference option.
    recipient: &'static str,
    probability: f64,
    /// Full names of people the line talks about.
    mentioned: &'static [&'static str],
}

const NO_ONE: Answer = Answer {
    recipient: "no_one",
    probability: 0.95,
    mentioned: &[],
};

fn to(name: &'static str) -> Answer {
    Answer {
        recipient: name,
        probability: 0.95,
        mentioned: &[],
    }
}

/// The decision reply for `answer`, using the option ids the call offered.
fn decision_for(input: &Value, answer: &Answer) -> String {
    let present = input["present"].as_array().unwrap();
    let id_of = |name: &str| {
        present
            .iter()
            .find(|person| person["name"] == name)
            .unwrap_or_else(|| panic!("{name} is not offered: {input}"))["optionId"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let recipient = match answer.recipient {
        fixed @ ("all" | "someone_else" | "no_one") => fixed.to_string(),
        reference if reference.starts_with("ref:") => input["references"]
            .as_array()
            .unwrap()
            .iter()
            .find(|option| option["phrase"] == reference[4..])
            .unwrap_or_else(|| panic!("{reference} is not offered: {input}"))["optionId"]
            .as_str()
            .unwrap()
            .to_string(),
        name => id_of(name),
    };
    let mentioned: serde_json::Map<String, Value> = answer
        .mentioned
        .iter()
        .map(|name| (id_of(name), json!(0.9)))
        .collect();
    let choice = |option: &str, p: f64| json!({"choice": option, "confidence": p, "probabilities": {option: p}});
    json!({
        "action": choice("talk", 0.99),
        "destination": choice("none", 0.9),
        "target": choice("none", 0.9),
        "recipient": choice(&recipient, answer.probability),
        "mentioned": mentioned,
        "atmosphere": choice("none", 0.99),
    })
    .to_string()
}

/// What one turn did.
struct Turn {
    step: TurnStep,
    /// Speakers and how each came to answer, in order.
    speakers: Vec<(String, String)>,
    /// Every journaled event's content.
    lines: Vec<String>,
    /// The intent call's input, if one was made.
    intent: Option<Value>,
}

/// Runs `step` to its end: the intent call answered with `answer`, every
/// dialogue call with a short line.
async fn run_turn(
    engine: &mut TurnEngine,
    live: &Live,
    endpoints: &EndpointCatalog,
    mut step: TurnStep,
    answer: &Answer,
) -> Turn {
    let mut speakers = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut intent = None;
    loop {
        lines.extend(step.events.iter().filter_map(|e| e.event.content.clone()));
        let TurnStatus::AwaitingInference(pending) = &step.status else {
            break;
        };
        let pending = pending.clone();
        let reply = match pending.call.subrole {
            InferenceSubrole::Intent => {
                let input =
                    serde_json::to_value(&pending.call.endpoint.as_ref().unwrap().input).unwrap();
                let reply = decision_for(&input, answer);
                intent = Some(input);
                reply
            }
            InferenceSubrole::Dialogue => {
                let input = dialogue_snapshot(&pending);
                speakers.push((
                    input["speaker"]["displayName"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    input["addressing"].as_str().unwrap().to_string(),
                ));
                dialogue_reply("Aye, indeed.")
            }
            other => panic!("unexpected workload {other:?}"),
        };
        step = resolve(engine, live, endpoints, &pending, completed(reply)).await;
    }
    Turn {
        step,
        speakers,
        lines,
        intent,
    }
}

async fn say(
    engine: &mut TurnEngine,
    live: &Live,
    endpoints: &EndpointCatalog,
    line: &str,
    answer: Answer,
) -> Turn {
    let step = engine
        .submit(&live.ctx(endpoints), says(line))
        .await
        .expect("submit");
    run_turn(engine, live, endpoints, step, &answer).await
}

async fn choose(
    engine: &mut TurnEngine,
    live: &Live,
    endpoints: &EndpointCatalog,
    turn: &Turn,
    choice: &str,
) -> Turn {
    let request = turn.step.request_id.clone().unwrap();
    let step = engine
        .answer_clarification(&live.ctx(endpoints), &request, choice)
        .await
        .expect("answer");
    run_turn(engine, live, endpoints, step, &NO_ONE).await
}

fn asked(turn: &Turn) -> &limerick_core::turn::ClarificationPrompt {
    match &turn.step.status {
        TurnStatus::AwaitingClarification(prompt) => prompt,
        other => panic!("expected a question, got {other:?}"),
    }
}

fn names(turn: &Turn) -> Vec<&str> {
    turn.speakers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect()
}

fn no_absence_line(turn: &Turn) {
    assert!(
        !turn
            .lines
            .iter()
            .any(|line| line.contains("not here") || line.contains("No one here answers")),
        "no line answers a name: {:?}",
        turn.lines
    );
}

/// Makes `name` the partner. The id is looked up first: holding the NPC
/// lock while `id_of` takes it again would deadlock.
async fn set_partner(live: &Live, name: &str) {
    let id = live.id_of(name).await;
    live.npc_manager.lock().await.set_partner(Some(id));
}

async fn introduce(live: &Live, name: &str) {
    let id = live.id_of(name).await;
    live.npc_manager.lock().await.mark_introduced(id);
}

const M: &str = "Mícheál Connolly";
const R: &str = "Róisín Connolly";
const P: &str = "Peig Hannigan";

/// Both Connollys at home, the player with them, Peig at the Letter Office.
async fn at_the_cottage(live: &Live) {
    live.arrange(COTTAGE, &[(M, COTTAGE), (R, COTTAGE), (P, LETTER_OFFICE)])
        .await;
}

async fn partner(live: &Live) -> Option<String> {
    let npcs = live.npc_manager.lock().await;
    npcs.partner()
        .and_then(|id| npcs.get(id))
        .map(|npc| npc.name.clone())
}

/// Rows 1 to 4: "Hello" asks who with both labels and All, and makes no
/// dialogue call; choosing Róisín sends the line to her without retyping
/// and makes her the partner, so "And how's the harvest?" goes to her
/// without a question; choosing All reaches both, and the next unaddressed
/// line asks.
#[tokio::test]
async fn rows_1_to_4_unaddressed_speech_asks_then_the_partner_answers() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    at_the_cottage(&live).await;
    let mut engine = canonical_engine(&live);

    let hello = say(&mut engine, &live, &endpoints, "Hello", NO_ONE).await;
    let prompt = asked(&hello);
    assert_eq!(prompt.question, "Who are you speaking to?");
    let labels: Vec<&str> = prompt.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels.len(), 3);
    assert_eq!(labels[2], "All");
    assert_eq!(
        prompt.choices[2].accessibility_label.as_deref(),
        Some("Everyone here")
    );
    assert!(hello.speakers.is_empty(), "no dialogue call until answered");

    let roisin = format!("choose-npc-{}", live.id_of(R).await.0);
    let chosen = choose(&mut engine, &live, &endpoints, &hello, &roisin).await;
    assert_eq!(chosen.speakers, [(R.to_string(), "spoken_to".to_string())]);
    assert_eq!(partner(&live).await.as_deref(), Some(R));

    let harvest = say(
        &mut engine,
        &live,
        &endpoints,
        "And how's the harvest?",
        NO_ONE,
    )
    .await;
    assert_eq!(names(&harvest), [R], "no question: the partner answers");

    let again = say(
        &mut engine,
        &live,
        &endpoints,
        "Grand",
        Answer {
            probability: 0.4,
            ..NO_ONE
        },
    )
    .await;
    let everyone = choose(&mut engine, &live, &endpoints, &again, "choose-all").await;
    assert_eq!(
        everyone.speakers,
        [
            (M.to_string(), "part_of_all".to_string()),
            (R.to_string(), "part_of_all".to_string())
        ]
    );
    assert_eq!(partner(&live).await, None, "All ends the partnership");
    let next = say(&mut engine, &live, &endpoints, "Well?", NO_ONE).await;
    assert_eq!(asked(&next).question, "Who are you speaking to?");
}

/// Row 5: Peig alone; the recipient question has a single option, so it is
/// not offered to Jev, and Peig answers directly.
#[tokio::test]
async fn row_5_one_person_present_answers_directly() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    live.arrange(
        LETTER_OFFICE,
        &[(P, LETTER_OFFICE), (M, COTTAGE), (R, COTTAGE)],
    )
    .await;
    let mut engine = canonical_engine(&live);
    let turn = say(&mut engine, &live, &endpoints, "Hello", to(P)).await;
    assert_eq!(names(&turn), [P]);
    let input = turn.intent.unwrap();
    for list in ["everyone", "someoneElse", "noOne", "mentionable"] {
        assert_eq!(input[list], json!([]), "{list}");
    }
}

/// Rows 6 and 18: an @tag or chip picks the recipient with no question and
/// no intent call; only that person answers, and a name in the line of
/// someone elsewhere gets no "not here" line.
#[tokio::test]
async fn rows_6_and_18_explicit_selection_skips_system_1() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    at_the_cottage(&live).await;
    let mut engine = canonical_engine(&live);
    let tagged = say(
        &mut engine,
        &live,
        &endpoints,
        "@Róisín Connolly any news?",
        NO_ONE,
    )
    .await;
    assert!(tagged.intent.is_none(), "no intent call");
    assert_eq!(names(&tagged), [R]);
    let step = engine
        .submit(&live.ctx(&endpoints), spoken_to("Any news?", &[M]))
        .await
        .unwrap();
    let chip = run_turn(&mut engine, &live, &endpoints, step, &NO_ONE).await;
    assert!(chip.intent.is_none(), "no intent call");
    assert_eq!(names(&chip), [M]);

    live.arrange(
        LETTER_OFFICE,
        &[(P, LETTER_OFFICE), (M, COTTAGE), (R, COTTAGE)],
    )
    .await;
    let ribbon = say(
        &mut engine,
        &live,
        &endpoints,
        "@Peig Hannigan The ribbon was woven by Róisín.",
        NO_ONE,
    )
    .await;
    assert_eq!(names(&ribbon), [P]);
    no_absence_line(&ribbon);
}

/// Rows 7 to 9: before any introduction, a line Jev reads as spoken to
/// Róisín reaches her. The call offers her full name, so it can.
#[tokio::test]
async fn rows_7_to_9_a_name_reaches_its_person_before_an_introduction() {
    for line in [
        "Róisín, what news is there from the village?",
        "ask Róisín about the village",
        "Roisin, what news is there from the village?",
    ] {
        let live = Live::canonical();
        let endpoints = live.game_mod.endpoints.clone();
        at_the_cottage(&live).await;
        let mut engine = canonical_engine(&live);
        let turn = say(&mut engine, &live, &endpoints, line, to(R)).await;
        assert_eq!(names(&turn), [R], "{line}");
        let input = turn.intent.unwrap();
        assert!(
            input["present"]
                .as_array()
                .unwrap()
                .iter()
                .any(|person| person["name"] == R && person["introduced"] == false),
            "{input}"
        );
    }
}

/// Rows 10 and 12: a line spoken to someone not present, or to a name that
/// fits no one, reaches both Connollys and ends the partnership; no system
/// line answers the name, and nobody elsewhere is in the call.
#[tokio::test]
async fn rows_10_and_12_someone_not_present_reaches_everyone_here() {
    for line in ["ask Seán about the road", "Peig, are you there?"] {
        let live = Live::canonical();
        let endpoints = live.game_mod.endpoints.clone();
        at_the_cottage(&live).await;
        set_partner(&live, R).await;
        let mut engine = canonical_engine(&live);
        let turn = say(&mut engine, &live, &endpoints, line, to("someone_else")).await;
        assert_eq!(names(&turn), [M, R], "{line}");
        no_absence_line(&turn);
        assert_eq!(partner(&live).await, None, "{line}");
        assert!(turn.intent.unwrap().get("elsewhere").is_none());
    }
}

/// Row 11: with Peig alone, "Mícheál, hello" goes to Peig, and neither the
/// intent call nor her snapshot says where Mícheál is.
#[tokio::test]
async fn row_11_naming_someone_elsewhere_reaches_the_person_present() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    live.arrange(
        LETTER_OFFICE,
        &[(P, LETTER_OFFICE), (M, COTTAGE), (R, COTTAGE)],
    )
    .await;
    {
        let mut npcs = live.npc_manager.lock().await;
        let id = npcs.all_npcs().find(|npc| npc.name == M).unwrap().id;
        npcs.mark_introduced(id);
    }
    let mut engine = canonical_engine(&live);
    let step = engine
        .submit(&live.ctx(&endpoints), says("Mícheál, hello"))
        .await
        .unwrap();
    let intent = awaiting(&step).clone();
    let input = serde_json::to_value(&intent.call.endpoint.as_ref().unwrap().input).unwrap();
    // Only the people present are offered; no one elsewhere, so the call
    // says nothing of where Mícheál is.
    assert!(input.get("elsewhere").is_none(), "{input}");
    let offered: Vec<&str> = input["present"]
        .as_array()
        .unwrap()
        .iter()
        .map(|person| person["name"].as_str().unwrap())
        .collect();
    assert_eq!(offered, [P]);
    let step = resolve(
        &mut engine,
        &live,
        &endpoints,
        &intent,
        completed(decision_for(&input, &to(P))),
    )
    .await;
    let snapshot = dialogue_snapshot(awaiting(&step));
    assert_eq!(snapshot["speaker"]["displayName"], P);
    for person in snapshot["knownPeople"].as_array().unwrap() {
        assert!(person.get("location").is_none(), "{person}");
    }
    let rest = run_turn(&mut engine, &live, &endpoints, step, &NO_ONE).await;
    assert_eq!(names(&rest), [P]);
    no_absence_line(&rest);
}

/// Rows 13 and 14: an exclamation naming a Mary elsewhere, or an ordinary
/// remark, is not an address: with no partner it asks, with a partner the
/// partner answers.
#[tokio::test]
async fn rows_13_and_14_a_remark_is_not_an_address() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    at_the_cottage(&live).await;
    {
        // The fixture's Mary Brennan, at the Letter Office.
        let mut npcs = live.npc_manager.lock().await;
        let mut mary = npcs.all_npcs().find(|npc| npc.name == P).unwrap().clone();
        mary.id = NpcId(40);
        mary.name = "Mary Brennan".to_string();
        npcs.add_npc(mary);
    }
    let mut engine = canonical_engine(&live);
    let line = "Mary, mother of God, look at the rain!";
    let first = say(&mut engine, &live, &endpoints, line, NO_ONE).await;
    assert_eq!(asked(&first).question, "Who are you speaking to?");
    no_absence_line(&first);
    set_partner(&live, M).await;
    let with_partner = say(&mut engine, &live, &endpoints, line, NO_ONE).await;
    assert_eq!(names(&with_partner), [M]);
    let fine_day = say(
        &mut engine,
        &live,
        &endpoints,
        "Well, it is a fine day.",
        NO_ONE,
    )
    .await;
    assert_eq!(names(&fine_day), [M]);
}

/// Rows 15 to 17: a person mentioned joins an unaddressed line after the
/// partner, never an addressed one; with no partner the line asks first,
/// and the person mentioned joins after the choice.
#[tokio::test]
async fn rows_15_to_17_mentions_join_unaddressed_lines() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    at_the_cottage(&live).await;
    set_partner(&live, R).await;
    let mut engine = canonical_engine(&live);

    let cow = say(
        &mut engine,
        &live,
        &endpoints,
        "Does Mícheál still keep the black cow?",
        Answer {
            mentioned: &[M],
            ..NO_ONE
        },
    )
    .await;
    assert_eq!(
        cow.speakers,
        [
            (R.to_string(), "spoken_to".to_string()),
            (M.to_string(), "joining_mentioned".to_string())
        ]
    );
    assert_eq!(partner(&live).await.as_deref(), Some(R), "Róisín stays");

    let well = say(
        &mut engine,
        &live,
        &endpoints,
        "Róisín, is Mícheál well?",
        Answer {
            mentioned: &[M],
            ..to(R)
        },
    )
    .await;
    assert_eq!(names(&well), [R], "an addressed line: nobody joins");

    live.npc_manager.lock().await.set_partner(None);
    let butter = say(
        &mut engine,
        &live,
        &endpoints,
        "I hear Róisín makes fine butter.",
        Answer {
            mentioned: &[R],
            ..NO_ONE
        },
    )
    .await;
    assert!(
        butter.intent.is_some(),
        "a first-person line still gets the call"
    );
    let micheal = format!("choose-npc-{}", live.id_of(M).await.0);
    let chosen = choose(&mut engine, &live, &endpoints, &butter, &micheal).await;
    assert_eq!(
        chosen.speakers,
        [
            (M.to_string(), "spoken_to".to_string()),
            (R.to_string(), "joining_mentioned".to_string())
        ]
    );
    assert_eq!(partner(&live).await.as_deref(), Some(M));
}

/// Row 19: a question pending; Róisín leaves before the answer. The game
/// says she is no longer here, sends nothing, and hands the line back.
#[tokio::test]
async fn row_19_a_chosen_person_who_left_returns_the_line() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    at_the_cottage(&live).await;
    introduce(&live, R).await;
    let mut engine = canonical_engine(&live);
    let hello = say(&mut engine, &live, &endpoints, "Hello", NO_ONE).await;
    live.arrange(COTTAGE, &[(R, VILLAGE)]).await;
    let roisin = format!("choose-npc-{}", live.id_of(R).await.0);
    let answered = choose(&mut engine, &live, &endpoints, &hello, &roisin).await;
    assert!(answered.speakers.is_empty());
    assert!(
        answered
            .lines
            .contains(&"Róisín Connolly is no longer here.".to_string())
    );
    let terminal = answered.step.events.last().unwrap();
    assert_eq!(
        terminal
            .event
            .metadata
            .get(limerick_core::turn::COMPOSER_TEXT_KEY)
            .map(String::as_str),
        Some("Hello")
    );
}

/// Row 20: the partner leaves; the next unaddressed line resolves afresh.
#[tokio::test]
async fn row_20_a_partner_who_left_no_longer_answers() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    live.arrange(COTTAGE, &[(P, COTTAGE), (M, COTTAGE), (R, COTTAGE)])
        .await;
    set_partner(&live, R).await;
    live.arrange(COTTAGE, &[(R, VILLAGE)]).await;
    let mut engine = canonical_engine(&live);
    let turn = say(&mut engine, &live, &endpoints, "Hello", NO_ONE).await;
    let prompt = asked(&turn);
    assert_eq!(prompt.choices.len(), 3, "Peig, Mícheál and All");
}

/// Row 21: unsure, or a failed call, with both Connollys present asks.
#[tokio::test]
async fn row_21_an_unsure_answer_asks() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    at_the_cottage(&live).await;
    set_partner(&live, R).await;
    let mut engine = canonical_engine(&live);
    let unsure = say(
        &mut engine,
        &live,
        &endpoints,
        "Good morning to you",
        Answer {
            probability: 0.6,
            ..to(R)
        },
    )
    .await;
    assert_eq!(asked(&unsure).question, "Who are you speaking to?");

    let step = engine
        .submit(&live.ctx(&endpoints), says("Good morning to you"))
        .await
        .unwrap();
    let pending = awaiting(&step).clone();
    let failed = resolve(
        &mut engine,
        &live,
        &endpoints,
        &pending,
        InferenceOutcome::Failed {
            kind: limerick_core::turn_inference::InferenceFailureKind::Protocol,
            message: "MODEL_ERROR".to_string(),
            report: CallReport::default(),
        },
    )
    .await;
    assert!(matches!(
        failed.status,
        TurnStatus::AwaitingClarification(_)
    ));
}

/// Row 22 (engine half): the partner is saved with the game.
#[tokio::test]
async fn row_22_the_partner_is_saved_with_the_game() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    at_the_cottage(&live).await;
    let mut engine = canonical_engine(&live);
    say(&mut engine, &live, &endpoints, "Good morning", to(R)).await;
    let snapshot = {
        let world = live.world.lock().await;
        let npcs = live.npc_manager.lock().await;
        limerick_core::persistence::GameSnapshot::capture(&world, &npcs)
    };
    assert_eq!(snapshot.conversation_partner, Some(live.id_of(R).await));
}

/// Rows 27 and 29: a word that fits both Connollys asks which; "talk to
/// Mícheál about Róisín" is spoken to Mícheál alone.
#[tokio::test]
async fn rows_27_and_29_references_ask_and_talk_to_scopes_the_line() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    at_the_cottage(&live).await;
    let mut engine = canonical_engine(&live);
    let which = say(
        &mut engine,
        &live,
        &endpoints,
        "ask Connolly about the household",
        to("ref:Connolly"),
    )
    .await;
    let prompt = asked(&which);
    assert_eq!(prompt.question, "Which Connolly do you mean?");
    assert_eq!(prompt.choices.len(), 2);
    assert!(which.speakers.is_empty());

    let talk = say(
        &mut engine,
        &live,
        &endpoints,
        "talk to Mícheál about Róisín",
        Answer {
            mentioned: &[R],
            ..to(M)
        },
    )
    .await;
    assert_eq!(names(&talk), [M]);
}

/// Row 30: three people present, a line spoken to two of them comes out
/// unsure, so the game asks, offering each person and All.
#[tokio::test]
async fn row_30_three_present_and_unsure_asks() {
    let live = Live::canonical();
    let endpoints = live.game_mod.endpoints.clone();
    live.arrange(VILLAGE, &[(P, VILLAGE), (M, VILLAGE), (R, VILLAGE)])
        .await;
    let mut engine = canonical_engine(&live);
    let turn = say(
        &mut engine,
        &live,
        &endpoints,
        "Mícheál and Róisín, will ye walk up the road with me?",
        Answer {
            probability: 0.5,
            ..to(M)
        },
    )
    .await;
    assert_eq!(asked(&turn).choices.len(), 4);
}

/// Rows 31 and 32: with nobody present, a name gets the idle message only,
/// and the call does not ask Jev who the line is for.
#[tokio::test]
async fn rows_31_and_32_nobody_present_gets_the_idle_message() {
    for (line, introduced) in [("Mícheál, hello", true), ("Hello?", false)] {
        let live = Live::canonical();
        let endpoints = live.game_mod.endpoints.clone();
        live.arrange(VILLAGE, &[(P, LETTER_OFFICE), (M, COTTAGE), (R, COTTAGE)])
            .await;
        if introduced {
            introduce(&live, M).await;
        }
        let mut engine = canonical_engine(&live);
        let turn = say(&mut engine, &live, &endpoints, line, NO_ONE).await;
        assert!(turn.speakers.is_empty(), "{line}");
        no_absence_line(&turn);
        let input = turn.intent.expect("the call still decides the action");
        assert_eq!(input["noOne"], json!([{"optionId": "no_one"}]), "{line}");
        assert_eq!(input["present"], json!([]));
        assert_eq!(input["everyone"], json!([]));
    }
}
