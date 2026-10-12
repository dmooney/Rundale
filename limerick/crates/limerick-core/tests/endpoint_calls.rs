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
