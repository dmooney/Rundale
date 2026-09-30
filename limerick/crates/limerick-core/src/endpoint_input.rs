//! Structured Endpoint inputs built by the engine (ADR-025 §5).
//!
//! When a model call has a published Endpoint (see
//! [`crate::game_mod::endpoints`]), the engine attaches the Endpoint reference
//! and the role's structured input to the [`InferenceCall`]. An Endpoint host
//! sends that input, and the Endpoint's published instructions do the
//! prompting; an in-process host ignores it and sends the call's rendered
//! prompt as before.
//!
//! A host builds the request body with [`EndpointCall::invocation`], which
//! adds the invocation envelope it owns (`contractVersion`, `sessionID`,
//! `logicalRequestID`, `attemptID`, `baseRevision`, `idempotencyKey`) to the
//! engine's input. The result matches the role's `inputSchema` in the mod's
//! definition file.
//!
//! [`InferenceCall`]: crate::turn_inference::InferenceCall

use serde::Serialize;
use serde_json::{Value, json};

use crate::game_mod::{EndpointCatalog, EndpointRef, EndpointRole};
use crate::npc::manager::NpcManager;
use crate::npc::{Npc, NpcId};
use crate::turn::{ExecutionAttemptId, LogicalRequestId, StateRevision};
use crate::world::WorldState;
use crate::world::description::render_description;
use limerick_types::{ConversationExchange, LocationId};

/// Most people, places, or authored facts sent with one dialogue call.
pub const MAX_GROUNDING_ENTRIES: usize = 32;
/// Most recent exchanges at the current location sent with one dialogue call.
pub const MAX_RECENT_CONVERSATION: usize = 8;
/// Longest player input, in characters, the definitions accept.
pub const MAX_PLAYER_INPUT_CHARS: usize = 4096;
/// Dialogue character budget fixed by the dialogue definition v1.
pub const DIALOGUE_MAX_OUTPUT_CHARS: usize = 8192;
/// Provisional stream budget, in bytes, fixed by the dialogue definition v1.
pub const DIALOGUE_MAX_STREAM_BYTES: usize = 16 * 1024;
/// The dialogue input's fixed `role` value.
pub const DIALOGUE_ROLE: &str = "npc_dialogue";
/// The intent input's fixed `role` value.
pub const INTENT_ROLE: &str = "player_intent";
/// Invocation contract version the v1 definitions accept.
pub const INVOCATION_CONTRACT_VERSION: (u32, u32) = (1, 0);

/// The identities a host adds to every invocation. They come from the
/// pending request, not from game state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationEnvelope {
    /// Opaque host session identity.
    pub session_id: String,
    /// The logical request; retries keep it.
    pub request_id: LogicalRequestId,
    /// The execution attempt.
    pub attempt_id: ExecutionAttemptId,
    /// The revision the attempt started from.
    pub base_revision: StateRevision,
}

/// The Endpoint a call executes and the input it sends.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EndpointCall {
    /// Published Endpoint identity (role, slug, version).
    pub reference: EndpointRef,
    /// Role-specific structured input.
    pub input: EndpointInput,
}

impl EndpointCall {
    /// The Endpoint request's `input` object: the envelope's identities,
    /// then the engine's input fields.
    pub fn invocation(&self, envelope: &InvocationEnvelope) -> Value {
        let (major, minor) = INVOCATION_CONTRACT_VERSION;
        let mut body = json!({
            "contractVersion": {"major": major, "minor": minor},
            "sessionID": envelope.session_id,
            "logicalRequestID": envelope.request_id.as_str(),
            "attemptID": envelope.attempt_id.as_str(),
            "baseRevision": {"rawValue": envelope.base_revision.0},
            "idempotencyKey": format!(
                "{}:{}",
                envelope.request_id.as_str(),
                envelope.attempt_id.as_str()
            ),
        });
        if let (Value::Object(body), Ok(Value::Object(input))) =
            (&mut body, serde_json::to_value(&self.input))
        {
            body.extend(input);
        }
        body
    }
}

/// A role's structured input. Serializes as the input object itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum EndpointInput {
    /// Input for the dialogue role (boxed: it carries the grounding lists).
    Dialogue(Box<DialogueEndpointInput>),
    /// Input for the intent role.
    Intent(IntentEndpointInput),
}

/// Input for the intent role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentEndpointInput {
    /// Always [`INTENT_ROLE`].
    pub role: String,
    /// The player input the local parser did not recognise.
    pub player_input: String,
}

/// A person the dialogue Endpoint may refer to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedPerson {
    /// Stable id (`npc-<engine id>`).
    pub id: String,
    /// Full name.
    pub display_name: String,
    /// Occupation.
    pub role: String,
    /// Engine id of the person's current location.
    pub current_location_id: u32,
    /// Name of the person's current location.
    pub current_location_name: String,
}

/// A place the dialogue Endpoint may refer to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedPlace {
    /// Stable id (`place-<engine id>`).
    pub id: String,
    /// Place name.
    pub display_name: String,
    /// The place's description rendered for the current time and weather.
    pub description: String,
    /// Whether the player can go there.
    pub playable: bool,
}

/// One authored fact the speaker knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedFact {
    /// Stable id (`npc-<engine id>-knowledge-<n>`).
    pub id: String,
    /// The fact.
    pub statement: String,
    /// Where the fact is authored.
    pub source: String,
}

/// Input for the dialogue role.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueEndpointInput {
    /// Always [`DIALOGUE_ROLE`].
    pub role: String,
    /// The player's words (or the autonomous prompt), bounded.
    pub player_input: String,
    /// The NPC who speaks.
    pub speaker: GroundedPerson,
    /// Where the conversation happens.
    pub current_location: GroundedPlace,
    /// Everyone in the world, by engine id.
    pub known_people: Vec<GroundedPerson>,
    /// Every place in the world, by engine id.
    pub known_places: Vec<GroundedPlace>,
    /// The speaker's authored knowledge.
    pub authored_facts: Vec<GroundedFact>,
    /// Recent exchanges at the current location, oldest first.
    pub recent_conversation: Vec<ConversationExchange>,
    /// Always [`DIALOGUE_MAX_OUTPUT_CHARS`].
    pub max_output_chars: usize,
    /// Always [`DIALOGUE_MAX_STREAM_BYTES`].
    pub max_stream_bytes: usize,
}

fn bounded_input(text: &str) -> String {
    text.chars().take(MAX_PLAYER_INPUT_CHARS).collect()
}

fn location_name(world: &WorldState, id: LocationId) -> String {
    world
        .graph
        .get(id)
        .map(|location| location.name.clone())
        .unwrap_or_else(|| world.current_location().name.clone())
}

fn grounded_person(world: &WorldState, npc: &Npc) -> GroundedPerson {
    let location = npc.location();
    GroundedPerson {
        id: format!("npc-{}", npc.id.0),
        display_name: npc.name.clone(),
        role: npc.occupation.clone(),
        current_location_id: location.0,
        current_location_name: location_name(world, location),
    }
}

fn grounded_place(
    world: &WorldState,
    npc_manager: &NpcManager,
    id: LocationId,
) -> Option<GroundedPlace> {
    let location = world.graph.get(id)?;
    let people: Vec<&str> = npc_manager
        .npcs_at(id)
        .into_iter()
        .map(|npc| npc_manager.display_name(npc))
        .collect();
    Some(GroundedPlace {
        id: format!("place-{}", id.0),
        display_name: location.name.clone(),
        description: render_description(
            location,
            world.clock.time_of_day(),
            &world.weather.to_string(),
            &people,
        ),
        playable: true,
    })
}

/// Builds the intent role's input for `raw_input`.
pub fn intent_input(raw_input: &str) -> IntentEndpointInput {
    IntentEndpointInput {
        role: INTENT_ROLE.to_string(),
        player_input: bounded_input(raw_input),
    }
}

/// Builds the dialogue role's input for `speaker_id` answering `player_input`
/// at the player's location. `None` when the speaker is unknown.
pub fn dialogue_input(
    world: &WorldState,
    npc_manager: &NpcManager,
    speaker_id: NpcId,
    player_input: &str,
) -> Option<DialogueEndpointInput> {
    let speaker = npc_manager.get(speaker_id)?;
    let here = world.player_location;

    let mut people: Vec<&Npc> = npc_manager.all_npcs().collect();
    people.sort_by_key(|npc| npc.id);
    let known_people = people
        .into_iter()
        .take(MAX_GROUNDING_ENTRIES)
        .map(|npc| grounded_person(world, npc))
        .collect();

    let mut place_ids = world.graph.location_ids();
    place_ids.sort();
    let known_places = place_ids
        .into_iter()
        .filter_map(|id| grounded_place(world, npc_manager, id))
        .take(MAX_GROUNDING_ENTRIES)
        .collect();

    let current_location = grounded_place(world, npc_manager, here).unwrap_or_else(|| {
        let location = world.current_location();
        GroundedPlace {
            id: format!("place-{}", here.0),
            display_name: location.name.clone(),
            description: location.description.clone(),
            playable: true,
        }
    });

    let authored_facts = speaker
        .knowledge
        .iter()
        .enumerate()
        .take(MAX_GROUNDING_ENTRIES)
        .map(|(index, statement)| GroundedFact {
            id: format!("npc-{}-knowledge-{}", speaker.id.0, index + 1),
            statement: statement.clone(),
            source: "npcs.json".to_string(),
        })
        .collect();

    // The definition requires both sides of an exchange; an exchange with an
    // empty side (an unprompted line) is not sent.
    let mut recent_conversation: Vec<ConversationExchange> = world
        .conversation_log
        .recent_at(here, MAX_RECENT_CONVERSATION)
        .into_iter()
        .filter(|exchange| {
            !exchange.player_input.trim().is_empty() && !exchange.npc_dialogue.trim().is_empty()
        })
        .cloned()
        .collect();
    recent_conversation.reverse();

    Some(DialogueEndpointInput {
        role: DIALOGUE_ROLE.to_string(),
        player_input: bounded_input(player_input),
        speaker: grounded_person(world, speaker),
        current_location,
        known_people,
        known_places,
        authored_facts,
        recent_conversation,
        max_output_chars: DIALOGUE_MAX_OUTPUT_CHARS,
        max_stream_bytes: DIALOGUE_MAX_STREAM_BYTES,
    })
}

/// The intent call's Endpoint, when the catalog declares the intent role.
pub fn intent_endpoint(catalog: &EndpointCatalog, raw_input: &str) -> Option<EndpointCall> {
    Some(EndpointCall {
        reference: catalog.reference(EndpointRole::Intent)?.clone(),
        input: EndpointInput::Intent(intent_input(raw_input)),
    })
}

/// The dialogue call's Endpoint, when the catalog declares the dialogue role
/// and the speaker exists.
pub fn dialogue_endpoint(
    catalog: &EndpointCatalog,
    world: &WorldState,
    npc_manager: &NpcManager,
    speaker_id: NpcId,
    player_input: &str,
) -> Option<EndpointCall> {
    let reference = catalog.reference(EndpointRole::Dialogue)?.clone();
    let input = dialogue_input(world, npc_manager, speaker_id, player_input)?;
    Some(EndpointCall {
        reference,
        input: EndpointInput::Dialogue(Box::new(input)),
    })
}
