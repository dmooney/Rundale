//! Portable Phase 2 game-session orchestration.
//!
//! This module is deliberately synchronous.  A host (Swift, a C ABI, or a
//! deterministic test) calls one method at a time on [`MobileSession`].
//! Inference is represented by an [`EndpointInvocation`] value and its
//! callbacks re-enter the same session through [`MobileSession::receive_frame`],
//! [`MobileSession::receive_candidate`], or [`MobileSession::receive_failure`].
//! No provider client, task runtime, or UI type belongs in this module.
//!
//! The session owns the one authoritative `WorldState` and `NpcManager` for a
//! Phase 2 game.  A successful candidate is applied to cloned domain values,
//! request state, and final semantic events before the complete durable value
//! replaces the live value.  This gives the mobile persistence adapter one
//! atomic value to write and makes a late callback harmless.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

use chrono::{DateTime, Utc};
use parish_npc::{
    DialogueGroundingSnapshot, DialogueValidationPolicy, NpcResponseParseDisposition,
    NpcStreamResponse,
};
use parish_persistence::GameSnapshot;
use parish_persistence::mobile::{
    MobileEventInput as PersistentEventInput, MobileRequestUpsert as PersistentRequestUpsert,
    MobileStore as PersistentMobileStore,
};
use parish_types::{ConversationExchange, Location, LocationId, NpcId, Weather};
use parish_world::WorldState;
use parish_world::graph::WorldGraph;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use parish_npc::manager::NpcManager;

/// Presentation contract shared with `mobile/RundaleKit`.
pub const PRESENTATION_CONTRACT_VERSION: PresentationContractVersion =
    PresentationContractVersion { major: 1, minor: 0 };

/// Version of the durable mobile save envelope.  This evolves independently
/// from the semantic event payload consumed by RundaleKit.
pub const MOBILE_SAVE_FORMAT_VERSION: MobileSaveFormatVersion =
    MobileSaveFormatVersion { major: 1, minor: 0 };

/// The maximum bytes retained for a single provisional stream item.
pub const MAX_PROVISIONAL_TEXT_BYTES: usize = 16 * 1024;
/// The maximum number of provisional events retained in memory.
pub const MAX_PROVISIONAL_EVENTS: usize = 128;
/// The maximum number of semantic events retained by a live session.
pub const MAX_SEMANTIC_EVENTS: usize = 2_048;
/// The maximum amount of recent dialogue sent to an Endpoint.
pub const MAX_RECENT_CONVERSATION: usize = 8;
/// The maximum number of authored people/places sent to an Endpoint.
pub const MAX_GROUNDING_ENTRIES: usize = 32;
/// The maximum accepted player command size.
pub const MAX_COMMAND_BYTES: usize = 4 * 1024;
/// The maximum native transport error text retained in a durable error event.
pub const MAX_FAILURE_MESSAGE_BYTES: usize = 4 * 1024;
/// The maximum number of completion records returned to a host.
pub const MAX_COMPLETIONS: usize = 32;

/// Version of the Swift/Rust semantic presentation contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationContractVersion {
    #[serde(rename = "major")]
    pub major: u16,
    #[serde(rename = "minor")]
    pub minor: u16,
}

/// Version of the durable mobile save envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobileSaveFormatVersion {
    pub major: u16,
    pub minor: u16,
}

/// Opaque string identity values.  The explicit `rawValue` shape is required
/// by the Swift `RawRepresentable` wrappers in RundaleKit.
macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name {
            pub raw_value: String,
        }

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self {
                    raw_value: value.into(),
                }
            }

            pub fn fresh() -> Self {
                Self::new(Uuid::new_v4().to_string())
            }

            pub fn as_str(&self) -> &str {
                &self.raw_value
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

string_id!(SessionId);
string_id!(LogicalRequestId);
string_id!(ExecutionAttemptId);
string_id!(SemanticEventId);
string_id!(TranscriptItemId);
string_id!(DraftId);

/// Monotonic event sequence.  Swift decodes this as `EventSequence(rawValue:)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventSequence {
    #[serde(rename = "rawValue")]
    pub raw_value: u64,
}

impl EventSequence {
    pub const fn new(value: u64) -> Self {
        Self { raw_value: value }
    }
}

/// Monotonic authoritative domain revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StateRevision {
    #[serde(rename = "rawValue")]
    pub raw_value: u64,
}

impl StateRevision {
    pub const fn new(value: u64) -> Self {
        Self { raw_value: value }
    }
}

/// Cursor used for gap-free event paging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventCursor {
    #[serde(rename = "rawValue")]
    pub raw_value: u64,
}

impl EventCursor {
    pub const fn new(value: u64) -> Self {
        Self { raw_value: value }
    }
}

/// Semantic event kinds used by `RundaleKit.SemanticEventKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticEventKind {
    SceneChanged,
    PlayerCommand,
    CommandInterpreted,
    Narration,
    NpcDialogue,
    ActionResult,
    ClarificationRequired,
    ClarificationSelected,
    Progress,
    Error,
    ResponseCompleted,
}

/// Whether a stream payload replaces or appends to the item text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamUpdate {
    Replace,
    Append,
}

/// Terminal request outcome used by the Swift presentation reducer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseTerminalOutcome {
    Succeeded,
    Cancelled,
    Interrupted,
    Failed,
}

/// Safe, bounded categories supplied by the native Endpoint transport when a
/// response cannot produce a terminal candidate.  The engine owns the mapping
/// from these categories to durable request outcomes; native code never sends
/// an arbitrary terminal state or provider error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointFailureKind {
    Transport,
    Protocol,
    MissingTerminal,
    Interrupted,
}

/// Compatibility name for hosts that model a transport failure separately
/// from Endpoint protocol failures.
pub type TransportFailureKind = EndpointFailureKind;

impl EndpointFailureKind {
    const fn terminal_outcome(self) -> ResponseTerminalOutcome {
        match self {
            Self::Interrupted => ResponseTerminalOutcome::Interrupted,
            Self::Transport | Self::Protocol | Self::MissingTerminal => {
                ResponseTerminalOutcome::Failed
            }
        }
    }

    const fn metadata_value(self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::Protocol => "protocol",
            Self::MissingTerminal => "missing_terminal",
            Self::Interrupted => "interrupted",
        }
    }
}

/// A bounded, presentation-only event.  It contains no provider response
/// object or mutable engine object; references are stable opaque IDs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticEvent {
    #[serde(rename = "contractVersion")]
    pub contract_version: PresentationContractVersion,
    #[serde(rename = "eventID")]
    pub event_id: SemanticEventId,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    pub sequence: EventSequence,
    #[serde(rename = "gameTime")]
    pub game_time: Option<DateTime<Utc>>,
    pub kind: SemanticEventKind,
    pub content: Option<String>,
    pub speaker: Option<String>,
    #[serde(rename = "logicalRequestID")]
    pub logical_request_id: Option<LogicalRequestId>,
    #[serde(rename = "attemptID")]
    pub attempt_id: Option<ExecutionAttemptId>,
    #[serde(rename = "transcriptItemID")]
    pub transcript_item_id: Option<TranscriptItemId>,
    pub provisional: bool,
    #[serde(rename = "streamSequence")]
    pub stream_sequence: Option<u64>,
    #[serde(rename = "streamUpdate")]
    pub stream_update: StreamUpdate,
    #[serde(rename = "terminalOutcome")]
    pub terminal_outcome: Option<ResponseTerminalOutcome>,
    pub accepted: bool,
    #[serde(rename = "sourceDraftID")]
    pub source_draft_id: Option<DraftId>,
    #[serde(rename = "stateRevision")]
    pub state_revision: Option<StateRevision>,
    pub clarification: Option<ClarificationPrompt>,
    pub metadata: BTreeMap<String, String>,
}

/// A finite choice set for future clarification UI.  Phase 2 only emits
/// deterministic capability metadata, but keeping this shape stable avoids a
/// second FFI contract when clarification is introduced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClarificationChoice {
    pub id: String,
    pub label: String,
    #[serde(rename = "entityID")]
    pub entity_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClarificationPrompt {
    pub question: String,
    pub choices: Vec<ClarificationChoice>,
}

/// Phase 2 request phases.  A request can have multiple attempts, but only
/// the current non-terminal attempt may deliver a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestPhase {
    Accepted,
    Executing,
    Validating,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl RequestPhase {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

/// One execution attempt, including its bounded stream bookkeeping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestAttempt {
    pub id: ExecutionAttemptId,
    pub original_text: String,
    pub phase: RequestPhase,
    pub terminal_outcome: Option<ResponseTerminalOutcome>,
    #[serde(rename = "provisionalItemIDs")]
    pub provisional_item_ids: Vec<TranscriptItemId>,
    pub started_at: EventSequence,
    #[serde(rename = "terminalEventID")]
    pub terminal_event_id: Option<SemanticEventId>,
    pub committed_state_revision: Option<StateRevision>,
    pub base_revision: StateRevision,
    pub last_stream_sequence: u64,
    pub provisional_text: String,
    #[serde(skip)]
    grounding: Option<GroundingSnapshot>,
}

/// Durable logical request record.  The logical request remains retryable
/// after an uncommitted failure/cancel/interruption, while a successful
/// outcome permanently settles it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestRecord {
    pub id: LogicalRequestId,
    pub original_text: String,
    #[serde(rename = "acceptedCommandItemID")]
    pub accepted_command_item_id: Option<TranscriptItemId>,
    pub attempts: Vec<RequestAttempt>,
    #[serde(rename = "currentAttemptID")]
    pub current_attempt_id: Option<ExecutionAttemptId>,
    pub phase: RequestPhase,
    pub terminal_outcome: Option<ResponseTerminalOutcome>,
    pub committed_state_revision: Option<StateRevision>,
}

impl RequestRecord {
    pub fn current_attempt(&self) -> Option<&RequestAttempt> {
        self.current_attempt_id
            .as_ref()
            .and_then(|id| self.attempts.iter().find(|attempt| &attempt.id == id))
    }

    pub fn current_attempt_mut(&mut self) -> Option<&mut RequestAttempt> {
        let id = self.current_attempt_id.clone()?;
        self.attempts.iter_mut().find(|attempt| attempt.id == id)
    }

    pub fn has_committed_gameplay(&self) -> bool {
        self.terminal_outcome == Some(ResponseTerminalOutcome::Succeeded)
    }
}

/// A stable scene projection.  The Swift reducer can use this directly for
/// its current header without scanning generated prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSummary {
    pub id: String,
    pub name: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NearbyPerson {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub location_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitSummary {
    pub id: String,
    pub direction: String,
    pub display_name: String,
    pub description: String,
    pub destination_id: Option<String>,
    pub playable: bool,
}

/// Read model kept at the same revision as the authoritative world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileReadModel {
    pub state_revision: StateRevision,
    pub scene: SceneSummary,
    pub nearby_people: Vec<NearbyPerson>,
    pub exits: Vec<ExitSummary>,
}

/// Full snapshot returned alongside an event cursor.  The event vector is a
/// bounded tail; `hasOlderEvents` tells the host to page from an earlier cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileSnapshot {
    pub contract_version: PresentationContractVersion,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    pub state_revision: StateRevision,
    pub event_cursor: EventCursor,
    pub read_model: MobileReadModel,
    pub requests: Vec<RequestRecord>,
    #[serde(rename = "activeRequestID")]
    pub active_request_id: Option<LogicalRequestId>,
    pub events: Vec<SemanticEvent>,
    pub has_older_events: bool,
}

/// A gap-free snapshot plus events after an optional cursor.  The session is
/// synchronous, so the cursor and read model cannot be separated by a commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsistentSnapshot {
    pub snapshot: MobileSnapshot,
    pub events_after: Vec<SemanticEvent>,
    pub next_cursor: EventCursor,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPage {
    pub events: Vec<SemanticEvent>,
    pub next_cursor: EventCursor,
    pub has_more: bool,
    pub has_older_events: bool,
}

/// A deterministic completion supplied by the Rust capability registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCompletion {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub insertion_text: String,
    pub entity_id: Option<String>,
}

/// Authored, versioned Phase 2 content identity.  Stable IDs remain separate
/// from mutable engine IDs and are included in Endpoint grounding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Phase2ContentDefinition {
    pub schema_version: u16,
    pub content_version: u16,
    pub content_id: String,
    pub location_id: String,
    pub engine_location_id: u32,
    pub location_name: String,
    pub opening_description: String,
    pub look_text: String,
    pub people_text: String,
    pub exits_text: String,
    pub npc_id: String,
    pub engine_npc_id: u32,
    pub npc_name: String,
    pub npc_role: String,
    pub npc_personality: Vec<String>,
    pub npc_known_places: Vec<GroundedPlace>,
    pub npc_facts: Vec<GroundedFact>,
    pub exits: Vec<ExitSummary>,
}

#[derive(Debug, Deserialize)]
struct Phase2ContentBundle {
    schema_version: u16,
    content_version: u16,
    content_id: String,
    locations: Vec<Phase2LocationContent>,
    npcs: Vec<Phase2NpcContent>,
}

#[derive(Debug, Deserialize)]
struct Phase2LocationContent {
    id: String,
    engine_location_id: u32,
    display_name: String,
    playable: bool,
    opening_description: String,
    commands: Phase2Commands,
    initial_npc_ids: Vec<String>,
    exits: Vec<Phase2ExitContent>,
}

#[derive(Debug, Deserialize)]
struct Phase2Commands {
    look: String,
    people: String,
    exits: String,
}

#[derive(Debug, Deserialize)]
struct Phase2ExitContent {
    id: String,
    direction: String,
    display_name: String,
    description: String,
    destination_id: Option<String>,
    playable: bool,
}

#[derive(Debug, Deserialize)]
struct Phase2NpcContent {
    id: String,
    engine_npc_id: u32,
    display_name: String,
    interactive: bool,
    initial_location_id: String,
    home_location_id: String,
    role: String,
    personality: Vec<String>,
    known_places: Vec<Phase2PlaceContent>,
    known_facts: Vec<Phase2FactContent>,
}

#[derive(Debug, Deserialize)]
struct Phase2PlaceContent {
    id: String,
    display_name: String,
    description: String,
    playable: bool,
}

#[derive(Debug, Deserialize)]
struct Phase2FactContent {
    id: String,
    statement: String,
    #[serde(default)]
    known_people: Vec<String>,
    #[serde(default)]
    known_places: Vec<String>,
    source: String,
}

impl Phase2ContentDefinition {
    /// The canonical one-location/one-NPC content used by the mobile slice.
    /// Scenic/non-playable places are grounding facts only. The authored JSON
    /// is the sole content source; this projection is the engine-facing view.
    pub fn canonical() -> Self {
        Self::try_canonical().expect("embedded Phase 2 content must be valid")
    }

    pub fn try_canonical() -> Result<Self, MobileError> {
        let bundle: Phase2ContentBundle = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../mobile/content/phase2-crossroads.json"
        )))
        .map_err(|error| MobileError::Content(format!("invalid Phase 2 content JSON: {error}")))?;
        Self::from_bundle(bundle)
    }

    pub fn content_version_key(&self) -> String {
        format!("phase2-{}", self.content_version)
    }

    /// Stable identity for the embedded authored content. The fingerprint is
    /// derived from the validated projection, so changing a fact or command
    /// cannot silently reuse an old save.
    pub fn content_fingerprint(&self) -> String {
        use std::hash::{Hash, Hasher};
        let bytes = serde_json::to_vec(self).expect("content projection is serializable");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn from_bundle(bundle: Phase2ContentBundle) -> Result<Self, MobileError> {
        if bundle.schema_version != 1 || bundle.content_version == 0 {
            return Err(MobileError::Content(
                "unsupported Phase 2 content schema/version".to_string(),
            ));
        }
        if bundle.locations.len() != 1 || bundle.npcs.len() != 1 {
            return Err(MobileError::Content(
                "Phase 2 content must contain exactly one location and one NPC".to_string(),
            ));
        }
        let location = bundle.locations.into_iter().next().expect("one location");
        let npc = bundle.npcs.into_iter().next().expect("one NPC");
        if !location.playable
            || location.id.is_empty()
            || npc.id.is_empty()
            || !npc.interactive
            || npc.initial_location_id != location.id
            || npc.home_location_id != location.id
            || location.initial_npc_ids != vec![npc.id.clone()]
            || location.engine_location_id == 0
            || npc.engine_npc_id == 0
            || npc.known_places.is_empty()
            || npc.known_facts.is_empty()
        {
            return Err(MobileError::Content(
                "Phase 2 content has invalid location/NPC references".to_string(),
            ));
        }
        let known_place_ids: std::collections::HashSet<&str> = npc
            .known_places
            .iter()
            .map(|place| place.id.as_str())
            .collect();
        if known_place_ids.len() != npc.known_places.len()
            || npc
                .known_places
                .iter()
                .filter(|place| place.playable)
                .count()
                != 1
            || npc.known_facts.iter().any(|fact| {
                fact.id.is_empty()
                    || fact.statement.is_empty()
                    || fact.source.is_empty()
                    || fact
                        .known_places
                        .iter()
                        .any(|place_id| !known_place_ids.contains(place_id.as_str()))
                    || fact
                        .known_people
                        .iter()
                        .any(|person_id| person_id != &npc.id)
            })
        {
            return Err(MobileError::Content(
                "Phase 2 content has invalid authored grounding".to_string(),
            ));
        }
        if location.exits.iter().any(|exit| exit.id.is_empty()) {
            return Err(MobileError::Content(
                "Phase 2 content has an exit without a stable ID".to_string(),
            ));
        }
        let known_places = npc
            .known_places
            .into_iter()
            .map(|place| GroundedPlace {
                id: place.id,
                display_name: place.display_name,
                description: place.description,
                playable: place.playable,
            })
            .collect();
        let exits = location
            .exits
            .into_iter()
            .map(|exit| ExitSummary {
                id: exit.id,
                direction: exit.direction,
                display_name: exit.display_name,
                description: exit.description,
                destination_id: exit.destination_id,
                playable: exit.playable,
            })
            .collect();
        Ok(Self {
            schema_version: bundle.schema_version,
            content_version: bundle.content_version,
            content_id: bundle.content_id,
            location_id: location.id,
            engine_location_id: location.engine_location_id,
            location_name: location.display_name,
            opening_description: location.opening_description,
            look_text: location.commands.look,
            people_text: location.commands.people,
            exits_text: location.commands.exits,
            npc_id: npc.id,
            engine_npc_id: npc.engine_npc_id,
            npc_name: npc.display_name,
            npc_role: npc.role,
            npc_personality: npc.personality,
            npc_known_places: known_places,
            npc_facts: npc
                .known_facts
                .into_iter()
                .map(|fact| GroundedFact {
                    id: fact.id,
                    statement: fact.statement,
                    source: fact.source,
                })
                .collect(),
            exits,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedPlace {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub playable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedFact {
    pub id: String,
    pub statement: String,
    pub source: String,
}

/// A bounded grounding person sent to Parish Endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedPerson {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub current_location_id: u32,
    pub current_location_name: String,
}

/// Endpoint request DTO.  It is intentionally serializable and contains no
/// credentials, provider selection, or callable transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointInvocation {
    pub contract_version: PresentationContractVersion,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(rename = "logicalRequestID")]
    pub logical_request_id: LogicalRequestId,
    #[serde(rename = "attemptID")]
    pub attempt_id: ExecutionAttemptId,
    pub base_revision: StateRevision,
    pub idempotency_key: String,
    pub role: String,
    pub player_input: String,
    pub speaker: GroundedPerson,
    pub current_location: GroundedPlace,
    pub known_people: Vec<GroundedPerson>,
    pub known_places: Vec<GroundedPlace>,
    pub authored_facts: Vec<GroundedFact>,
    pub recent_conversation: Vec<ConversationExchange>,
    pub max_output_chars: usize,
    pub max_stream_bytes: usize,
}

/// Candidate response returned by the platform after Endpoint validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointCandidate {
    pub attempt_id: ExecutionAttemptId,
    pub base_revision: StateRevision,
    pub dialogue: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// The platform must identify whether the response crossed the structured
    /// JSON boundary.  Recovery/raw text is always rejected by the canonical
    /// NPC validator.
    #[serde(default)]
    pub structured: bool,
}

/// One bounded stream frame from Parish Endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointFrame {
    pub attempt_id: ExecutionAttemptId,
    pub base_revision: StateRevision,
    pub sequence: u64,
    /// Preferred transport form is cumulative replacement. Append is still
    /// accepted for old Endpoint implementations but is bounded and
    /// normalized into a cumulative in-memory buffer.
    pub text: String,
    #[serde(default = "default_stream_update")]
    pub stream_update: StreamUpdate,
    #[serde(default)]
    pub done: bool,
}

fn default_stream_update() -> StreamUpdate {
    StreamUpdate::Replace
}

/// Return from a submission, retry, frame, candidate, or stop call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileOperationResult {
    pub accepted: bool,
    #[serde(rename = "logicalRequestID")]
    pub logical_request_id: Option<LogicalRequestId>,
    #[serde(rename = "attemptID")]
    pub attempt_id: Option<ExecutionAttemptId>,
    pub events: Vec<SemanticEvent>,
    pub endpoint_invocation: Option<EndpointInvocation>,
    pub terminal_outcome: Option<ResponseTerminalOutcome>,
    pub ignored: bool,
    pub error: Option<String>,
    pub event_cursor: EventCursor,
    pub state_revision: StateRevision,
}

/// Why a callback was ignored.  Ignored callbacks do not append events,
/// mutate requests, or change the state revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoredCallback {
    UnknownAttempt,
    ObsoleteAttempt,
    BaseRevisionChanged,
    DuplicateSequence,
    TerminalAttempt,
    RequestCommitted,
}

/// Errors returned by the synchronous session boundary.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum MobileError {
    #[error("session is already open")]
    AlreadyOpen,
    #[error("command is empty")]
    EmptyCommand,
    #[error("command exceeds the {MAX_COMMAND_BYTES}-byte limit")]
    CommandTooLarge,
    #[error("request {0} was not found")]
    RequestNotFound(String),
    #[error("request {0} has already committed")]
    RequestAlreadyCommitted(String),
    #[error("request {0} is not retryable")]
    RequestNotRetryable(String),
    #[error("there is already an active request")]
    RequestInProgress,
    #[error("attempt {0} is not current")]
    AttemptNotCurrent(String),
    #[error("candidate is empty")]
    CandidateEmpty,
    #[error("candidate is too large")]
    CandidateTooLarge,
    #[error("failure message exceeds the {MAX_FAILURE_MESSAGE_BYTES}-byte limit")]
    FailureMessageTooLarge,
    #[error("failure message contains control characters")]
    FailureMessageInvalid,
    #[error("storage error: {0}")]
    Storage(String),
    #[error("content error: {0}")]
    Content(String),
}

/// Serializable game/session state owned by the persistence boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileSave {
    pub format_version: MobileSaveFormatVersion,
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    pub state_revision: StateRevision,
    pub next_event_sequence: u64,
    pub game: GameSnapshot,
    pub requests: Vec<RequestRecord>,
    pub events: Vec<SemanticEvent>,
    pub has_older_events: bool,
}

/// Optional persistence seam for hosts that have a durable SQLite adapter.
/// The core only ever hands the adapter a complete [`MobileSave`]; an adapter
/// may write it atomically. `MemoryMobileStore` is the deterministic default.
pub trait MobileStore {
    fn load(&mut self, session_id: &SessionId) -> Result<Option<MobileSave>, MobileError>;
    fn save(&mut self, save: &MobileSave) -> Result<(), MobileError>;

    /// Read the durable semantic history without exposing the store itself to
    /// the host binding.  In-memory stores return their bounded save window;
    /// SQLite stores read the indexed event table, which retains events that
    /// have fallen out of the live snapshot tail.
    fn read_event_page(
        &self,
        after: Option<EventCursor>,
        limit: usize,
    ) -> Result<EventPage, MobileError>;
}

/// In-memory store used by headless tests and by callers that supply their own
/// explicit save bytes. It still exercises the same acceptance/commit seam.
#[derive(Debug, Clone, Default)]
pub struct MemoryMobileStore {
    save: Option<MobileSave>,
    pub fail_writes: bool,
}

impl MemoryMobileStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save_value(&self) -> Option<&MobileSave> {
        self.save.as_ref()
    }
}

impl MobileStore for MemoryMobileStore {
    fn load(&mut self, session_id: &SessionId) -> Result<Option<MobileSave>, MobileError> {
        Ok(self
            .save
            .as_ref()
            .filter(|save| &save.session_id == session_id)
            .cloned())
    }

    fn save(&mut self, save: &MobileSave) -> Result<(), MobileError> {
        if self.fail_writes {
            return Err(MobileError::Storage("injected write failure".to_string()));
        }
        self.save = Some(save.clone());
        Ok(())
    }

    fn read_event_page(
        &self,
        after: Option<EventCursor>,
        limit: usize,
    ) -> Result<EventPage, MobileError> {
        let after = after.map(|cursor| cursor.raw_value).unwrap_or(0);
        let limit = limit.clamp(1, MAX_SEMANTIC_EVENTS);
        let Some(save) = &self.save else {
            return Ok(EventPage {
                events: Vec::new(),
                next_cursor: EventCursor::new(after),
                has_more: false,
                has_older_events: false,
            });
        };
        let mut events: Vec<SemanticEvent> = save
            .events
            .iter()
            .filter(|event| event.sequence.raw_value > after)
            .take(limit + 1)
            .cloned()
            .collect();
        let has_more = events.len() > limit;
        events.truncate(limit);
        let next_cursor = events
            .last()
            .map(|event| EventCursor::new(event.sequence.raw_value))
            .unwrap_or_else(|| EventCursor::new(after));
        Ok(EventPage {
            events,
            next_cursor,
            has_more,
            has_older_events: save.has_older_events,
        })
    }
}

/// The part of a [`MobileSave`] kept as the SQLite state projection. Logical
/// requests and semantic events are also written to their indexed tables by
/// [`SqliteMobileStore`], so recovery never has to infer them from prose.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DurableMobileDomain {
    format_version: MobileSaveFormatVersion,
    session_id: SessionId,
    state_revision: StateRevision,
    next_event_sequence: u64,
    game: GameSnapshot,
    events: Vec<SemanticEvent>,
    has_older_events: bool,
}

/// Production mobile storage adapter over the transactional SQLite boundary
/// owned by `parish-persistence`.
pub struct SqliteMobileStore {
    inner: PersistentMobileStore,
    generation: u64,
}

impl std::fmt::Debug for SqliteMobileStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqliteMobileStore")
            .field("inner", &self.inner)
            .field("generation", &self.generation)
            .finish()
    }
}

impl SqliteMobileStore {
    /// Open an explicitly selected mobile save path. The persistence crate
    /// validates format/content identity and takes the save-file lock before
    /// any writable setup.
    pub fn open(
        path: &Path,
        content_version: &str,
        content_fingerprint: &str,
    ) -> Result<Self, MobileError> {
        let inner = PersistentMobileStore::open(path, content_version, content_fingerprint)
            .map_err(|error| MobileError::Storage(error.to_string()))?;
        let generation = inner
            .load()
            .map_err(|error| MobileError::Storage(error.to_string()))?
            .generation;
        Ok(Self { inner, generation })
    }

    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    /// Load the domain envelope without requiring the caller to know the
    /// session identity first. This is the resume entry point for a host that
    /// has only the explicit save path.
    pub fn load_any(&mut self) -> Result<Option<MobileSave>, MobileError> {
        let loaded = self
            .inner
            .load()
            .map_err(|error| MobileError::Storage(error.to_string()))?;
        self.generation = loaded.generation;
        let Some(domain) = loaded.current_domain else {
            return Ok(None);
        };
        self.decode_domain(domain).map(Some)
    }

    /// Read the complete durable semantic history through bounded SQLite
    /// pages. The returned cursor uses the semantic event sequence stored in
    /// each event JSON, so provisional in-memory stream frames do not distort
    /// the mobile presentation cursor.
    pub fn read_event_page(
        &self,
        after: Option<EventCursor>,
        limit: usize,
    ) -> Result<EventPage, MobileError> {
        let after = after.map(|cursor| cursor.raw_value).unwrap_or(0);
        let limit = limit.clamp(1, MAX_SEMANTIC_EVENTS);
        let mut storage_cursor = 0;
        let mut durable = Vec::with_capacity(limit + 1);
        loop {
            let page = self
                .inner
                .read_events(
                    storage_cursor,
                    parish_persistence::mobile::MAX_EVENT_PAGE_SIZE,
                )
                .map_err(|error| MobileError::Storage(error.to_string()))?;
            if page.is_empty() {
                break;
            }
            storage_cursor = page
                .last()
                .map(|event| event.sequence)
                .unwrap_or(storage_cursor);
            for event in page {
                let event: SemanticEvent = serde_json::from_value(event.json).map_err(|error| {
                    MobileError::Storage(format!("invalid durable semantic event: {error}"))
                })?;
                if event.sequence.raw_value > after {
                    durable.push(event);
                    if durable.len() > limit {
                        break;
                    }
                }
            }
            if durable.len() > limit {
                break;
            }
        }
        durable.sort_by_key(|event| event.sequence.raw_value);
        let has_more = durable.len() > limit;
        let events: Vec<SemanticEvent> = durable.into_iter().take(limit).collect();
        let next_cursor = events
            .last()
            .map(|event| EventCursor::new(event.sequence.raw_value))
            .unwrap_or_else(|| EventCursor::new(after));
        Ok(EventPage {
            events,
            next_cursor,
            has_more,
            has_older_events: false,
        })
    }

    fn decode_domain(&self, value: serde_json::Value) -> Result<MobileSave, MobileError> {
        let domain: DurableMobileDomain = serde_json::from_value(value)
            .map_err(|error| MobileError::Storage(format!("invalid mobile domain: {error}")))?;
        let requests = self
            .inner
            .load_requests()
            .map_err(|error| MobileError::Storage(error.to_string()))?
            .into_iter()
            .map(|request| {
                serde_json::from_value::<RequestRecord>(request.json).map_err(|error| {
                    MobileError::Storage(format!(
                        "invalid durable request {}: {error}",
                        request.logical_id
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(MobileSave {
            format_version: domain.format_version,
            session_id: domain.session_id,
            state_revision: domain.state_revision,
            next_event_sequence: domain.next_event_sequence,
            game: domain.game,
            requests,
            events: domain.events,
            has_older_events: domain.has_older_events,
        })
    }
}

impl MobileStore for SqliteMobileStore {
    fn load(&mut self, session_id: &SessionId) -> Result<Option<MobileSave>, MobileError> {
        let Some(save) = self.load_any()? else {
            return Ok(None);
        };
        if &save.session_id == session_id {
            Ok(Some(save))
        } else {
            Ok(None)
        }
    }

    fn save(&mut self, save: &MobileSave) -> Result<(), MobileError> {
        let domain = DurableMobileDomain {
            format_version: save.format_version,
            session_id: save.session_id.clone(),
            state_revision: save.state_revision,
            next_event_sequence: save.next_event_sequence,
            game: save.game.clone(),
            events: save.events.clone(),
            has_older_events: save.has_older_events,
        };
        let domain_json = serde_json::to_value(domain)
            .map_err(|error| MobileError::Storage(error.to_string()))?;
        let requests: Vec<PersistentRequestUpsert> = save
            .requests
            .iter()
            .map(|request| {
                serde_json::to_value(request)
                    .map(|json| PersistentRequestUpsert::new(request.id.raw_value.clone(), json))
                    .map_err(|error| MobileError::Storage(error.to_string()))
            })
            .collect::<Result<_, _>>()?;
        let events: Vec<PersistentEventInput> = save
            .events
            .iter()
            .map(|event| {
                serde_json::to_value(event)
                    .map(|json| PersistentEventInput::new(event.event_id.raw_value.clone(), json))
                    .map_err(|error| MobileError::Storage(error.to_string()))
            })
            .collect::<Result<_, _>>()?;
        self.inner
            .commit(self.generation, Some(domain_json), &requests, &events)
            .map_err(|error| MobileError::Storage(error.to_string()))
            .map(|commit| {
                self.generation = commit.generation;
            })
    }

    fn read_event_page(
        &self,
        after: Option<EventCursor>,
        limit: usize,
    ) -> Result<EventPage, MobileError> {
        SqliteMobileStore::read_event_page(self, after, limit)
    }
}

/// The single-writer Phase 2 runtime.  Methods take `&mut self`; callers that
/// need thread-safe FFI use a host mutex/actor and re-enter this same lane.
pub struct MobileSession {
    session_id: SessionId,
    content: Phase2ContentDefinition,
    world: WorldState,
    npcs: NpcManager,
    state_revision: StateRevision,
    next_event_sequence: u64,
    events: VecDeque<SemanticEvent>,
    has_older_events: bool,
    requests: Vec<RequestRecord>,
    active_request_id: Option<LogicalRequestId>,
    provisional_events: VecDeque<SemanticEvent>,
    store: Option<Box<dyn MobileStore + Send>>,
}

impl std::fmt::Debug for MobileSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MobileSession")
            .field("session_id", &self.session_id)
            .field("state_revision", &self.state_revision)
            .field("next_event_sequence", &self.next_event_sequence)
            .field("requests", &self.requests)
            .field("active_request_id", &self.active_request_id)
            .finish_non_exhaustive()
    }
}

impl MobileSession {
    /// Create a new Phase 2 game with a fresh opaque session identity.
    pub fn open_new() -> Result<Self, MobileError> {
        Self::open_new_with_store(None)
    }

    /// Create a session backed by the production transactional mobile save.
    pub fn open_new_sqlite(path: &Path) -> Result<Self, MobileError> {
        let content = Phase2ContentDefinition::try_canonical()?;
        let store = SqliteMobileStore::open(
            path,
            &content.content_version_key(),
            &content.content_fingerprint(),
        )?;
        Self::open_new_with_store(Some(Box::new(store)))
    }

    /// Resume a session from an explicit production save path. An empty,
    /// initialized mobile database has no game yet and returns `None`.
    pub fn open_resume_sqlite(path: &Path) -> Result<Option<Self>, MobileError> {
        let content = Phase2ContentDefinition::try_canonical()?;
        let mut store = SqliteMobileStore::open(
            path,
            &content.content_version_key(),
            &content.content_fingerprint(),
        )?;
        let Some(save) = store.load_any()? else {
            return Ok(None);
        };
        Self::open_resume_with_store(save, Some(Box::new(store))).map(Some)
    }

    /// Create a new game and persist every durable boundary through `store`.
    pub fn open_new_with_store(
        store: Option<Box<dyn MobileStore + Send>>,
    ) -> Result<Self, MobileError> {
        let content = Phase2ContentDefinition::try_canonical()?;
        let (world, npcs) = make_phase2_domain(&content)?;
        let session_id = SessionId::fresh();
        let mut session = Self {
            session_id,
            content,
            world,
            npcs,
            state_revision: StateRevision::new(0),
            next_event_sequence: 0,
            events: VecDeque::new(),
            has_older_events: false,
            requests: Vec::new(),
            active_request_id: None,
            provisional_events: VecDeque::new(),
            store,
        };
        let event = session.emit(
            SemanticEventKind::SceneChanged,
            Some(session.content.opening_description.clone()),
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            false,
            None,
            None,
            metadata([
                ("sceneID", session.content.location_id.clone()),
                ("sceneName", session.content.location_name.clone()),
                ("sceneDetail", session.content.look_text.clone()),
            ]),
        );
        // A new session has a complete initial save before it is published to
        // a caller, matching the persistence identity invariant.
        session.persist_current()?;
        debug_assert_eq!(event.sequence.raw_value, 1);
        Ok(session)
    }

    /// Resume an explicitly provided save.  An accepted non-terminal request
    /// is recovered as a durable interruption and never re-invokes inference.
    pub fn open_resume(save: MobileSave) -> Result<Self, MobileError> {
        Self::open_resume_with_store(save, None)
    }

    pub fn open_resume_with_store(
        save: MobileSave,
        store: Option<Box<dyn MobileStore + Send>>,
    ) -> Result<Self, MobileError> {
        if save.format_version.major != MOBILE_SAVE_FORMAT_VERSION.major
            || save.format_version.minor > MOBILE_SAVE_FORMAT_VERSION.minor
        {
            return Err(MobileError::Storage(
                "unsupported mobile save version".to_string(),
            ));
        }
        let content = Phase2ContentDefinition::try_canonical()?;
        let (mut world, mut npcs) = make_phase2_domain(&content)?;
        save.game.clone().restore(&mut world, &mut npcs);
        let mut session = Self {
            session_id: save.session_id,
            content,
            world,
            npcs,
            state_revision: save.state_revision,
            next_event_sequence: save.next_event_sequence.max(
                save.events
                    .last()
                    .map(|event| event.sequence.raw_value)
                    .unwrap_or(0),
            ),
            events: save.events.into_iter().collect(),
            has_older_events: save.has_older_events,
            requests: save.requests,
            active_request_id: None,
            provisional_events: VecDeque::new(),
            store,
        };
        session.recover_interrupted_requests()?;
        session.persist_current()?;
        Ok(session)
    }

    /// Restore using a store-owned session identity.
    pub fn open_resume_from_store(
        store: &mut dyn MobileStore,
        session_id: &SessionId,
    ) -> Result<Option<Self>, MobileError> {
        let Some(save) = store.load(session_id)? else {
            return Ok(None);
        };
        Self::open_resume(save).map(Some)
    }

    /// Resume while retaining ownership of the store for future acceptance
    /// and gameplay commits.
    pub fn open_resume_from_boxed_store(
        mut store: Box<dyn MobileStore + Send>,
        session_id: &SessionId,
    ) -> Result<Option<Self>, MobileError> {
        let Some(save) = store.load(session_id)? else {
            return Ok(None);
        };
        Self::open_resume_with_store(save, Some(store)).map(Some)
    }

    /// Export the complete serializable state for a storage adapter or test.
    pub fn save(&self) -> MobileSave {
        self.to_save()
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn state_revision(&self) -> StateRevision {
        self.state_revision
    }

    pub fn world(&self) -> &WorldState {
        &self.world
    }

    pub fn npcs(&self) -> &NpcManager {
        &self.npcs
    }

    pub fn content(&self) -> &Phase2ContentDefinition {
        &self.content
    }

    pub fn snapshot(&self) -> MobileSnapshot {
        MobileSnapshot {
            contract_version: PRESENTATION_CONTRACT_VERSION,
            session_id: self.session_id.clone(),
            state_revision: self.state_revision,
            event_cursor: EventCursor::new(self.next_event_sequence),
            read_model: self.read_model(),
            requests: self.requests.clone(),
            active_request_id: self.active_request_id.clone(),
            events: self.events.iter().cloned().collect(),
            has_older_events: self.has_older_events,
        }
    }

    pub fn consistent_snapshot(
        &self,
        after: Option<EventCursor>,
        limit: usize,
    ) -> ConsistentSnapshot {
        let page = self.read_events(after, limit);
        ConsistentSnapshot {
            snapshot: self.snapshot(),
            events_after: page.events,
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        }
    }

    /// Read semantic events after a cursor.  `None` means the retained tail;
    /// `Some(0)` means all retained events.  The cursor is lifetime-monotonic
    /// even when old events have been evicted.
    pub fn read_events(&self, after: Option<EventCursor>, limit: usize) -> EventPage {
        let limit = limit.clamp(1, MAX_SEMANTIC_EVENTS);
        let cursor = after.map(|cursor| cursor.raw_value).unwrap_or(0);
        let events: Vec<SemanticEvent> = self
            .events
            .iter()
            .filter(|event| event.sequence.raw_value > cursor)
            .take(limit)
            .cloned()
            .collect();
        let next_cursor = events
            .last()
            .map(|event| EventCursor::new(event.sequence.raw_value))
            .unwrap_or_else(|| EventCursor::new(cursor.min(self.next_event_sequence)));
        let has_more = self
            .events
            .iter()
            .any(|event| event.sequence.raw_value > next_cursor.raw_value);
        EventPage {
            events,
            next_cursor,
            has_more,
            has_older_events: self.has_older_events,
        }
    }

    /// Read durable semantic history through the store-owned event index.
    /// SQLite retains events that have fallen out of the bounded live tail;
    /// the in-memory fallback preserves the same bounded contract for tests.
    pub fn read_event_page(
        &self,
        after: Option<EventCursor>,
        limit: usize,
    ) -> Result<EventPage, MobileError> {
        let limit = limit.clamp(1, MAX_SEMANTIC_EVENTS);
        if let Some(store) = &self.store {
            return store.read_event_page(after, limit);
        }
        Ok(self.read_events(after, limit))
    }

    /// Fetch deterministic slash and NPC completion data from authored Rust
    /// capabilities, independent of network or Swift fixture state.
    pub fn completions(&self, prefix: &str) -> Vec<CapabilityCompletion> {
        let lower = prefix.to_lowercase();
        let mut values = Vec::new();
        for command in ["/look", "/people", "/exits", "/help"] {
            if lower.is_empty() || command.starts_with(&lower) {
                values.push(CapabilityCompletion {
                    id: command.trim_start_matches('/').to_string(),
                    kind: "slash_command".to_string(),
                    label: command.to_string(),
                    insertion_text: command.to_string(),
                    entity_id: None,
                });
            }
        }
        if lower.is_empty() || "@peig".starts_with(&lower) {
            values.push(CapabilityCompletion {
                id: self.content.npc_id.clone(),
                kind: "npc_reference".to_string(),
                label: self.content.npc_name.clone(),
                insertion_text: format!("@{}", self.content.npc_name),
                entity_id: Some(self.content.npc_id.clone()),
            });
        }
        values.truncate(MAX_COMPLETIONS);
        values
    }

    /// Submit a new logical request.  The command acceptance event is added
    /// before any Endpoint invocation is returned.
    pub fn submit(
        &mut self,
        logical_request_id: Option<LogicalRequestId>,
        text: impl Into<String>,
        draft_id: Option<DraftId>,
    ) -> Result<MobileOperationResult, MobileError> {
        let text = text.into();
        validate_command_text(&text)?;
        if self.active_request_id.is_some() {
            return Err(MobileError::RequestInProgress);
        }

        let request_id = logical_request_id.unwrap_or_else(LogicalRequestId::fresh);
        if let Some(existing) = self.requests.iter().find(|record| record.id == request_id) {
            if existing.has_committed_gameplay() {
                return Err(MobileError::RequestAlreadyCommitted(request_id.raw_value));
            }
            return Err(MobileError::RequestNotRetryable(request_id.raw_value));
        }

        let attempt_id = ExecutionAttemptId::fresh();
        let command_item_id = TranscriptItemId::new(format!("{}:command", request_id.raw_value));
        let base_revision = self.state_revision;
        let attempt = RequestAttempt {
            id: attempt_id.clone(),
            original_text: text.clone(),
            phase: RequestPhase::Accepted,
            terminal_outcome: None,
            provisional_item_ids: Vec::new(),
            started_at: EventSequence::new(self.next_event_sequence + 1),
            terminal_event_id: None,
            committed_state_revision: None,
            base_revision,
            last_stream_sequence: 0,
            provisional_text: String::new(),
            grounding: None,
        };
        let pre_acceptance_requests = self.requests.clone();
        let pre_acceptance_events = self.events.clone();
        let pre_acceptance_has_older_events = self.has_older_events;
        let pre_acceptance_next_sequence = self.next_event_sequence;
        let pre_acceptance_active = self.active_request_id.clone();
        let pre_acceptance_provisional = self.provisional_events.clone();
        let command_event = self.emit(
            SemanticEventKind::PlayerCommand,
            Some(text.clone()),
            None,
            Some(&request_id),
            Some(&attempt_id),
            Some(command_item_id.clone()),
            false,
            None,
            None,
            true,
            draft_id.clone(),
            None,
            metadata([("accepted", "true".to_string())]),
        );
        let mut record = RequestRecord {
            id: request_id.clone(),
            original_text: text.clone(),
            accepted_command_item_id: Some(command_item_id),
            attempts: vec![attempt],
            current_attempt_id: Some(attempt_id.clone()),
            phase: RequestPhase::Accepted,
            terminal_outcome: None,
            committed_state_revision: None,
        };
        // Acceptance is persisted before interpreting the command or handing
        // work to the platform.  If storage fails, no request is published.
        self.requests.push(record.clone());
        if let Err(error) = self.persist_current() {
            self.requests = pre_acceptance_requests;
            self.events = pre_acceptance_events;
            self.has_older_events = pre_acceptance_has_older_events;
            self.next_event_sequence = pre_acceptance_next_sequence;
            self.active_request_id = pre_acceptance_active;
            self.provisional_events = pre_acceptance_provisional;
            return Err(error);
        }

        let accepted_requests = self.requests.clone();
        let accepted_events = self.events.clone();
        let accepted_has_older_events = self.has_older_events;
        let accepted_next_sequence = self.next_event_sequence;
        let accepted_active = self.active_request_id.clone();
        let accepted_provisional = self.provisional_events.clone();

        if let Some(capability) = deterministic_capability(&text) {
            record.phase = RequestPhase::Completed;
            record.terminal_outcome = Some(ResponseTerminalOutcome::Succeeded);
            record.committed_state_revision = Some(self.state_revision);
            let (action_event, event) =
                self.commit_deterministic_command(&request_id, &attempt_id, draft_id, capability)?;
            if let Some(stored) = self.requests.iter_mut().find(|r| r.id == request_id) {
                *stored = record;
                stored
                    .current_attempt_mut()
                    .expect("new request has current attempt")
                    .phase = RequestPhase::Completed;
                let attempt = stored.current_attempt_mut().expect("attempt exists");
                attempt.phase = RequestPhase::Completed;
                attempt.terminal_outcome = Some(ResponseTerminalOutcome::Succeeded);
                attempt.terminal_event_id = Some(event.event_id.clone());
                attempt.committed_state_revision = Some(self.state_revision);
            }
            self.active_request_id = None;
            if let Err(error) = self.persist_current() {
                self.requests = accepted_requests;
                self.events = accepted_events;
                self.has_older_events = accepted_has_older_events;
                self.next_event_sequence = accepted_next_sequence;
                self.active_request_id = accepted_active;
                self.provisional_events = accepted_provisional;
                return Err(error);
            }
            return Ok(self.operation_result(
                true,
                Some(request_id),
                Some(attempt_id),
                vec![command_event, action_event, event],
                None,
                Some(ResponseTerminalOutcome::Succeeded),
                false,
                None,
            ));
        }

        let grounding = self.grounding_snapshot();
        if let Some(stored) = self.requests.iter_mut().find(|r| r.id == request_id) {
            stored.phase = RequestPhase::Executing;
            stored.current_attempt_mut().expect("current attempt").phase = RequestPhase::Executing;
            stored
                .current_attempt_mut()
                .expect("current attempt")
                .grounding = Some(grounding.clone());
        }
        self.active_request_id = Some(request_id.clone());
        let invocation = self.endpoint_invocation(&request_id, &attempt_id, &text, &grounding);
        if let Err(error) = self.persist_current() {
            self.requests = accepted_requests;
            self.events = accepted_events;
            self.has_older_events = accepted_has_older_events;
            self.next_event_sequence = accepted_next_sequence;
            self.active_request_id = accepted_active;
            self.provisional_events = accepted_provisional;
            return Err(error);
        }
        Ok(self.operation_result(
            true,
            Some(request_id),
            Some(attempt_id),
            vec![command_event],
            Some(invocation),
            None,
            false,
            None,
        ))
    }

    /// Retry an uncommitted logical request with a new attempt identity.
    pub fn retry(
        &mut self,
        logical_request_id: &LogicalRequestId,
    ) -> Result<MobileOperationResult, MobileError> {
        if self.active_request_id.is_some() {
            return Err(MobileError::RequestInProgress);
        }
        let request_index = self
            .requests
            .iter()
            .position(|record| &record.id == logical_request_id)
            .ok_or_else(|| MobileError::RequestNotFound(logical_request_id.raw_value.clone()))?;
        if self.requests[request_index].has_committed_gameplay() {
            return Err(MobileError::RequestAlreadyCommitted(
                logical_request_id.raw_value.clone(),
            ));
        }
        if !self.requests[request_index].phase.is_terminal() {
            return Err(MobileError::RequestNotRetryable(
                logical_request_id.raw_value.clone(),
            ));
        }

        let text = self.requests[request_index].original_text.clone();
        let attempt_id = ExecutionAttemptId::fresh();
        let grounding = self.grounding_snapshot();
        let attempt = RequestAttempt {
            id: attempt_id.clone(),
            original_text: text.clone(),
            phase: RequestPhase::Executing,
            terminal_outcome: None,
            provisional_item_ids: Vec::new(),
            started_at: EventSequence::new(self.next_event_sequence + 1),
            terminal_event_id: None,
            committed_state_revision: None,
            base_revision: self.state_revision,
            last_stream_sequence: 0,
            provisional_text: String::new(),
            grounding: Some(grounding.clone()),
        };
        let prior_requests = self.requests.clone();
        let prior_events = self.events.clone();
        let prior_has_older_events = self.has_older_events;
        let prior_next_sequence = self.next_event_sequence;
        let prior_active = self.active_request_id.clone();
        let prior_provisional = self.provisional_events.clone();
        self.requests[request_index].attempts.push(attempt);
        self.requests[request_index].current_attempt_id = Some(attempt_id.clone());
        self.requests[request_index].phase = RequestPhase::Executing;
        self.requests[request_index].terminal_outcome = None;
        self.requests[request_index].committed_state_revision = None;
        self.active_request_id = Some(logical_request_id.clone());
        let progress = self.emit(
            SemanticEventKind::Progress,
            Some("Retrying the request.".to_string()),
            None,
            Some(logical_request_id),
            Some(&attempt_id),
            None,
            false,
            None,
            None,
            true,
            None,
            None,
            metadata([("retry", "true".to_string())]),
        );
        let invocation =
            self.endpoint_invocation(logical_request_id, &attempt_id, &text, &grounding);
        if let Err(error) = self.persist_current() {
            self.requests = prior_requests;
            self.events = prior_events;
            self.has_older_events = prior_has_older_events;
            self.next_event_sequence = prior_next_sequence;
            self.active_request_id = prior_active;
            self.provisional_events = prior_provisional;
            return Err(error);
        }
        Ok(self.operation_result(
            true,
            Some(logical_request_id.clone()),
            Some(attempt_id),
            vec![progress],
            Some(invocation),
            None,
            false,
            None,
        ))
    }

    /// Cancel only the current attempt.  The cancellation terminal event is
    /// emitted once; a later candidate or frame is ignored.
    pub fn stop(
        &mut self,
        attempt_id: &ExecutionAttemptId,
    ) -> Result<MobileOperationResult, MobileError> {
        let Some(request_id) = self.active_request_id.clone() else {
            return Ok(self.operation_result(
                false,
                None,
                None,
                Vec::new(),
                None,
                None,
                false,
                None,
            ));
        };
        let Some(request) = self.requests.iter().find(|record| record.id == request_id) else {
            return Ok(self.operation_result(
                false,
                None,
                None,
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        };
        if request.current_attempt_id.as_ref() != Some(attempt_id) {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        }
        if request.has_committed_gameplay() {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                Some(ResponseTerminalOutcome::Succeeded),
                true,
                None,
            ));
        }
        let terminal = self.finish_uncommitted(
            &request_id,
            attempt_id,
            ResponseTerminalOutcome::Cancelled,
            Some("The response was stopped.".to_string()),
            None,
        )?;
        Ok(terminal)
    }

    /// Alias used by FFI bindings whose naming follows the Swift adapter.
    pub fn stop_current_attempt(
        &mut self,
        attempt_id: &ExecutionAttemptId,
    ) -> Result<MobileOperationResult, MobileError> {
        self.stop(attempt_id)
    }

    /// Compatibility wrapper for older hosts. New native transports should
    /// call [`Self::receive_failure`] so the base revision and safe failure
    /// category are correlated at the engine boundary.
    pub fn fail(
        &mut self,
        attempt_id: &ExecutionAttemptId,
        message: String,
    ) -> Result<MobileOperationResult, MobileError> {
        let base_revision = self
            .active_request_id
            .as_ref()
            .and_then(|request_id| self.requests.iter().find(|r| &r.id == request_id))
            .and_then(RequestRecord::current_attempt)
            .map(|attempt| attempt.base_revision)
            .unwrap_or(self.state_revision);
        self.receive_failure(
            attempt_id,
            base_revision,
            EndpointFailureKind::Transport,
            message,
        )
    }

    /// Mark the current uncommitted attempt as failed after a native Endpoint
    /// transport/protocol interruption. The attempt and state revision must
    /// still match the invocation that the native client received; a stale or
    /// late failure is ignored exactly like a stale candidate. The engine maps
    /// only the allow-listed categories to durable failed/interrupted states.
    pub fn receive_failure(
        &mut self,
        attempt_id: &ExecutionAttemptId,
        base_revision: StateRevision,
        failure_kind: EndpointFailureKind,
        message: String,
    ) -> Result<MobileOperationResult, MobileError> {
        let Some(request_id) = self.active_request_id.clone() else {
            return Ok(self.operation_result(
                false,
                None,
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        };
        let Some(request) = self.requests.iter().find(|record| record.id == request_id) else {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        };
        if request.current_attempt_id.as_ref() != Some(attempt_id) {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        }
        let Some(attempt) = request.current_attempt() else {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        };
        if request.has_committed_gameplay()
            || attempt.phase.is_terminal()
            || attempt.base_revision != base_revision
            || self.state_revision != base_revision
        {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                attempt.terminal_outcome,
                true,
                None,
            ));
        }
        if message.len() > MAX_FAILURE_MESSAGE_BYTES {
            return Err(MobileError::FailureMessageTooLarge);
        }
        if message.chars().any(char::is_control) {
            return Err(MobileError::FailureMessageInvalid);
        }
        let message = if message.trim().is_empty() {
            format!(
                "Endpoint response failed ({})",
                failure_kind.metadata_value()
            )
        } else {
            message
        };
        self.finish_uncommitted(
            &request_id,
            attempt_id,
            failure_kind.terminal_outcome(),
            Some(message),
            Some(failure_kind.metadata_value()),
        )
    }

    /// Apply one provisional stream frame on the serial lane.  Provisional
    /// text is retained only in memory and is never added to `ConversationLog`
    /// or the durable `GameSnapshot`.
    pub fn receive_frame(
        &mut self,
        frame: EndpointFrame,
    ) -> Result<MobileOperationResult, MobileError> {
        let Some(request_id) = self.active_request_id.clone() else {
            return Ok(self.operation_result(
                false,
                None,
                Some(frame.attempt_id),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        };
        let prior_requests = self.requests.clone();
        let prior_next_sequence = self.next_event_sequence;
        let prior_provisional_events = self.provisional_events.clone();
        let Some(request) = self
            .requests
            .iter_mut()
            .find(|record| record.id == request_id)
        else {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(frame.attempt_id),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        };
        if request.has_committed_gameplay()
            || request.current_attempt_id.as_ref() != Some(&frame.attempt_id)
        {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(frame.attempt_id),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        }
        let attempt = request.current_attempt_mut().expect("current attempt");
        if attempt.phase.is_terminal() || attempt.base_revision != frame.base_revision {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(frame.attempt_id),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        }
        if frame.sequence <= attempt.last_stream_sequence {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(frame.attempt_id),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        }
        let candidate_text = match frame.stream_update {
            StreamUpdate::Replace => frame.text,
            StreamUpdate::Append => {
                let mut combined = attempt.provisional_text.clone();
                combined.push_str(&frame.text);
                combined
            }
        };
        if candidate_text.len() > MAX_PROVISIONAL_TEXT_BYTES {
            return Err(MobileError::CandidateTooLarge);
        }
        attempt.last_stream_sequence = frame.sequence;
        attempt.provisional_text = candidate_text.clone();
        let item_id = TranscriptItemId::new(format!("{}:response", frame.attempt_id.raw_value));
        let event = self.emit(
            SemanticEventKind::NpcDialogue,
            Some(candidate_text),
            Some(self.content.npc_name.clone()),
            Some(&request_id),
            Some(&frame.attempt_id),
            Some(item_id),
            true,
            Some(frame.sequence),
            Some(StreamUpdate::Replace),
            false,
            None,
            None,
            metadata([("provisional", "true".to_string())]),
        );
        // Persist the monotonic sequence and request lifecycle, while the
        // actual provisional text remains transient.  This keeps a host's
        // event cursor valid across relaunch without putting stream bytes in
        // the durable save.  A storage failure rolls back the uncommitted
        // stream update, just like acceptance and commit boundaries.
        if let Err(error) = self.persist_current() {
            self.requests = prior_requests;
            self.next_event_sequence = prior_next_sequence;
            self.provisional_events = prior_provisional_events;
            return Err(error);
        }
        Ok(self.operation_result(
            true,
            Some(request_id),
            Some(frame.attempt_id),
            vec![event],
            None,
            None,
            false,
            None,
        ))
    }

    /// Re-enter the serial lane with a structured terminal candidate.
    pub fn receive_candidate(
        &mut self,
        candidate: EndpointCandidate,
    ) -> Result<MobileOperationResult, MobileError> {
        let Some(request_id) = self.active_request_id.clone() else {
            return Ok(self.operation_result(
                false,
                None,
                Some(candidate.attempt_id),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        };
        let request_index = self
            .requests
            .iter()
            .position(|record| record.id == request_id)
            .expect("active request exists");
        let request = &self.requests[request_index];
        let Some(attempt) = request.current_attempt() else {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(candidate.attempt_id),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        };
        if request.has_committed_gameplay()
            || attempt.id != candidate.attempt_id
            || attempt.phase.is_terminal()
            || attempt.base_revision != candidate.base_revision
            || self.state_revision != candidate.base_revision
        {
            return Ok(self.operation_result(
                false,
                Some(request_id),
                Some(candidate.attempt_id),
                Vec::new(),
                None,
                None,
                true,
                None,
            ));
        }
        validate_candidate_text(&candidate.dialogue)?;
        self.commit_dialogue(
            &request_id,
            &candidate.attempt_id,
            candidate.dialogue,
            candidate.structured,
            candidate.metadata,
        )
    }

    /// Take and clear the next pending invocation.  `submit`/`retry` also
    /// return this DTO directly; this accessor is useful for FFI polling.
    pub fn take_pending_invocation(&mut self) -> Option<EndpointInvocation> {
        // The request record is authoritative; constructing the DTO again is
        // deterministic and avoids a second mutable queue that could lose a
        // request across a save boundary.
        let request_id = self.active_request_id.clone()?;
        let request = self
            .requests
            .iter()
            .find(|record| record.id == request_id)?;
        let attempt = request.current_attempt()?;
        let grounding = attempt.grounding.as_ref()?;
        Some(self.endpoint_invocation(&request_id, &attempt.id, &request.original_text, grounding))
    }

    fn commit_deterministic_command(
        &mut self,
        request_id: &LogicalRequestId,
        attempt_id: &ExecutionAttemptId,
        draft_id: Option<DraftId>,
        capability: DeterministicCapability,
    ) -> Result<(SemanticEvent, SemanticEvent), MobileError> {
        let action_text = match capability {
            DeterministicCapability::Look => crate::portable_look::render_look_text(
                &self.world,
                &self.npcs,
                1.25,
                "on foot",
                false,
            ),
            _ => capability.content().to_string(),
        };
        let event = self.emit(
            SemanticEventKind::ActionResult,
            Some(action_text),
            None,
            Some(request_id),
            Some(attempt_id),
            Some(TranscriptItemId::new(format!(
                "{}:action",
                attempt_id.raw_value
            ))),
            false,
            None,
            None,
            true,
            draft_id,
            Some(self.state_revision),
            metadata([("capability", capability.name().to_string())]),
        );
        let terminal = self.emit(
            SemanticEventKind::ResponseCompleted,
            None,
            None,
            Some(request_id),
            Some(attempt_id),
            None,
            false,
            None,
            None,
            true,
            None,
            Some(self.state_revision),
            terminal_metadata(ResponseTerminalOutcome::Succeeded),
        );
        // The live batch must contain the result before its terminal event.
        // A consumer that observes the terminal cursor cannot replay an
        // omitted earlier result from a subsequent snapshot.
        Ok((event, terminal))
    }

    fn commit_dialogue(
        &mut self,
        request_id: &LogicalRequestId,
        attempt_id: &ExecutionAttemptId,
        dialogue: String,
        structured: bool,
        candidate_metadata: BTreeMap<String, String>,
    ) -> Result<MobileOperationResult, MobileError> {
        if dialogue.trim().is_empty() {
            return Err(MobileError::CandidateEmpty);
        }
        let player_input = self
            .requests
            .iter()
            .find(|record| &record.id == request_id)
            .map(|record| record.original_text.clone())
            .ok_or_else(|| MobileError::RequestNotFound(request_id.raw_value.clone()))?;
        let grounding = self
            .requests
            .iter()
            .find(|record| &record.id == request_id)
            .and_then(|record| record.current_attempt())
            .and_then(|attempt| attempt.grounding.clone())
            .unwrap_or_else(|| self.grounding_snapshot());
        let mut next_world = self.world.clone();
        let mut next_npcs = self.npcs.clone();
        let parsed = NpcStreamResponse {
            dialogue,
            metadata: None,
        };
        let parse_disposition = if structured {
            NpcResponseParseDisposition::FullJson
        } else {
            NpcResponseParseDisposition::RawText
        };
        let game_time = next_world.clock.now();
        let dialogue_grounding = grounding.as_dialogue_snapshot();
        let apply = crate::dialogue_apply::apply_npc_dialogue_turn_with_validation(
            &mut next_world,
            &mut next_npcs,
            NpcId(self.content.engine_npc_id),
            &parsed,
            parse_disposition,
            &dialogue_grounding,
            DialogueValidationPolicy::default(),
            &player_input,
            &player_input,
            game_time,
            LocationId(self.content.engine_location_id),
            &self.content.npc_name,
            &self.content.npc_name,
            None,
            &dialogue_grounding.known_person_names,
            &parish_npc::LanguageSettings::english_only(),
            &parish_config::FeatureFlags::default(),
        );
        if !apply.accepted_candidate {
            return self.finish_uncommitted(
                request_id,
                attempt_id,
                ResponseTerminalOutcome::Failed,
                Some("Peig could not make sense of that request.".to_string()),
                Some("semantic_validation"),
            );
        }
        let dialogue = apply.display_text;
        if dialogue.trim().is_empty() {
            return self.finish_uncommitted(
                request_id,
                attempt_id,
                ResponseTerminalOutcome::Failed,
                Some("Peig did not return a usable response.".to_string()),
                Some("semantic_validation"),
            );
        }
        let mut candidate_metadata = candidate_metadata;
        if !apply.guard_reasons.is_empty() {
            candidate_metadata.insert("guardReasons".to_string(), apply.guard_reasons.join(","));
        }

        let new_revision = StateRevision::new(self.state_revision.raw_value + 1);
        let response_sequence = self.next_event_sequence + 1;
        let terminal_sequence = response_sequence + 1;
        let response_event = self.make_event_at(
            EventSequence::new(response_sequence),
            SemanticEventKind::NpcDialogue,
            Some(dialogue),
            Some(self.content.npc_name.clone()),
            Some(request_id),
            Some(attempt_id),
            Some(TranscriptItemId::new(format!(
                "{}:response",
                attempt_id.raw_value
            ))),
            false,
            None,
            Some(StreamUpdate::Replace),
            true,
            None,
            Some(new_revision),
            candidate_metadata,
        );
        let terminal_event = self.make_event_at(
            EventSequence::new(terminal_sequence),
            SemanticEventKind::ResponseCompleted,
            None,
            None,
            Some(request_id),
            Some(attempt_id),
            None,
            false,
            None,
            None,
            true,
            None,
            Some(new_revision),
            terminal_metadata(ResponseTerminalOutcome::Succeeded),
        );

        let mut next_requests = self.requests.clone();
        let request = next_requests
            .iter_mut()
            .find(|record| &record.id == request_id)
            .ok_or_else(|| MobileError::RequestNotFound(request_id.raw_value.clone()))?;
        let attempt = request
            .attempts
            .iter_mut()
            .find(|attempt| &attempt.id == attempt_id)
            .ok_or_else(|| MobileError::AttemptNotCurrent(attempt_id.raw_value.clone()))?;
        attempt.phase = RequestPhase::Completed;
        attempt.terminal_outcome = Some(ResponseTerminalOutcome::Succeeded);
        attempt.terminal_event_id = Some(terminal_event.event_id.clone());
        attempt.committed_state_revision = Some(new_revision);
        request.phase = RequestPhase::Completed;
        request.terminal_outcome = Some(ResponseTerminalOutcome::Succeeded);
        request.committed_state_revision = Some(new_revision);
        request.current_attempt_id = Some(attempt_id.clone());

        // Build a complete save candidate before replacing any live state.
        let mut next_events = self.events.clone();
        next_events.push_back(response_event.clone());
        next_events.push_back(terminal_event.clone());
        let mut next_has_older_events = self.has_older_events;
        trim_events(&mut next_events, &mut next_has_older_events);
        let candidate_save = self.to_save_with(
            next_world.clone(),
            next_npcs.clone(),
            new_revision,
            next_requests.clone(),
            next_events.iter().cloned().collect(),
            terminal_sequence,
            next_has_older_events,
        );
        self.persist_candidate(&candidate_save)?;

        self.world = next_world;
        self.npcs = next_npcs;
        self.state_revision = new_revision;
        self.requests = next_requests;
        self.events = next_events;
        self.has_older_events = next_has_older_events;
        self.next_event_sequence = terminal_sequence;
        self.active_request_id = None;
        self.provisional_events.clear();
        Ok(self.operation_result(
            true,
            Some(request_id.clone()),
            Some(attempt_id.clone()),
            vec![response_event, terminal_event],
            None,
            Some(ResponseTerminalOutcome::Succeeded),
            false,
            None,
        ))
    }

    fn finish_uncommitted(
        &mut self,
        request_id: &LogicalRequestId,
        attempt_id: &ExecutionAttemptId,
        outcome: ResponseTerminalOutcome,
        message: Option<String>,
        failure_kind: Option<&str>,
    ) -> Result<MobileOperationResult, MobileError> {
        let phase = match outcome {
            ResponseTerminalOutcome::Cancelled => RequestPhase::Cancelled,
            ResponseTerminalOutcome::Interrupted => RequestPhase::Interrupted,
            ResponseTerminalOutcome::Failed => RequestPhase::Failed,
            ResponseTerminalOutcome::Succeeded => {
                return Err(MobileError::Storage(
                    "invalid uncommitted outcome".to_string(),
                ));
            }
        };
        let mut next_requests = self.requests.clone();
        let request = next_requests
            .iter_mut()
            .find(|record| &record.id == request_id)
            .ok_or_else(|| MobileError::RequestNotFound(request_id.raw_value.clone()))?;
        if request.has_committed_gameplay() {
            return Ok(self.operation_result(
                false,
                Some(request_id.clone()),
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                Some(ResponseTerminalOutcome::Succeeded),
                true,
                None,
            ));
        }
        let attempt = request
            .attempts
            .iter_mut()
            .find(|attempt| &attempt.id == attempt_id)
            .ok_or_else(|| MobileError::AttemptNotCurrent(attempt_id.raw_value.clone()))?;
        if attempt.phase.is_terminal() {
            return Ok(self.operation_result(
                false,
                Some(request_id.clone()),
                Some(attempt_id.clone()),
                Vec::new(),
                None,
                attempt.terminal_outcome,
                true,
                None,
            ));
        }
        attempt.phase = phase;
        attempt.terminal_outcome = Some(outcome);
        request.phase = phase;
        request.terminal_outcome = Some(outcome);
        request.current_attempt_id = Some(attempt_id.clone());
        let mut events = Vec::new();
        let mut next_event_sequence = self.next_event_sequence;
        if let Some(message) = message {
            let mut error_metadata = metadata([("outcome", outcome_string(outcome).to_string())]);
            if let Some(failure_kind) = failure_kind {
                error_metadata.insert("errorKind".to_string(), failure_kind.to_string());
            }
            next_event_sequence += 1;
            events.push(self.make_event_at(
                EventSequence::new(next_event_sequence),
                SemanticEventKind::Error,
                Some(message),
                None,
                Some(request_id),
                Some(attempt_id),
                None,
                false,
                None,
                None,
                true,
                None,
                None,
                error_metadata,
            ));
        }
        let mut terminal_metadata = terminal_metadata(outcome);
        if let Some(failure_kind) = failure_kind {
            terminal_metadata.insert("errorKind".to_string(), failure_kind.to_string());
        }
        next_event_sequence += 1;
        events.push(self.make_event_at(
            EventSequence::new(next_event_sequence),
            SemanticEventKind::ResponseCompleted,
            None,
            None,
            Some(request_id),
            Some(attempt_id),
            None,
            false,
            None,
            None,
            true,
            None,
            None,
            terminal_metadata,
        ));
        let mut next_events = self.events.clone();
        next_events.extend(events.iter().cloned());
        let mut next_has_older_events = self.has_older_events;
        trim_events(&mut next_events, &mut next_has_older_events);
        let candidate_save = self.to_save_with(
            self.world.clone(),
            self.npcs.clone(),
            self.state_revision,
            next_requests.clone(),
            next_events.iter().cloned().collect(),
            next_event_sequence,
            next_has_older_events,
        );
        self.persist_candidate(&candidate_save)?;
        self.requests = next_requests;
        self.events = next_events;
        self.has_older_events = next_has_older_events;
        self.next_event_sequence = next_event_sequence;
        self.active_request_id = None;
        self.provisional_events.clear();
        Ok(self.operation_result(
            true,
            Some(request_id.clone()),
            Some(attempt_id.clone()),
            events,
            None,
            Some(outcome),
            false,
            None,
        ))
    }

    fn recover_interrupted_requests(&mut self) -> Result<(), MobileError> {
        let active: Vec<(LogicalRequestId, ExecutionAttemptId)> = self
            .requests
            .iter()
            .filter(|request| !request.phase.is_terminal())
            .filter_map(|request| {
                request
                    .current_attempt()
                    .map(|attempt| (request.id.clone(), attempt.id.clone()))
            })
            .collect();
        for (request_id, attempt_id) in active {
            let _ = self.finish_uncommitted(
                &request_id,
                &attempt_id,
                ResponseTerminalOutcome::Interrupted,
                Some("The previous response was interrupted; you can retry it.".to_string()),
                Some("interrupted"),
            )?;
        }
        Ok(())
    }

    // Keep the operation result fields explicit so the protocol response remains auditable.
    #[allow(clippy::too_many_arguments)]
    fn operation_result(
        &self,
        accepted: bool,
        request_id: Option<LogicalRequestId>,
        attempt_id: Option<ExecutionAttemptId>,
        events: Vec<SemanticEvent>,
        endpoint_invocation: Option<EndpointInvocation>,
        terminal_outcome: Option<ResponseTerminalOutcome>,
        ignored: bool,
        error: Option<String>,
    ) -> MobileOperationResult {
        MobileOperationResult {
            accepted,
            logical_request_id: request_id,
            attempt_id,
            events,
            endpoint_invocation,
            terminal_outcome,
            ignored,
            error,
            event_cursor: EventCursor::new(self.next_event_sequence),
            state_revision: self.state_revision,
        }
    }

    fn read_model(&self) -> MobileReadModel {
        let location_id = LocationId(self.content.engine_location_id);
        let location_name = self.content.location_name.clone();
        let nearby_people = self
            .npcs
            .npcs_at(location_id)
            .into_iter()
            .map(|npc| NearbyPerson {
                id: self.content.npc_id.clone(),
                display_name: npc.name.clone(),
                role: npc.occupation.clone(),
                location_id: npc.location().0,
            })
            .collect();
        MobileReadModel {
            state_revision: self.state_revision,
            scene: SceneSummary {
                id: self.content.location_id.clone(),
                name: location_name,
                detail: Some(crate::portable_look::render_look_text(
                    &self.world,
                    &self.npcs,
                    1.25,
                    "on foot",
                    false,
                )),
            },
            nearby_people,
            exits: self.content.exits.clone(),
        }
    }

    fn grounding_snapshot(&self) -> GroundingSnapshot {
        let location_id = LocationId(self.content.engine_location_id);
        let current_name = self.content.location_name.clone();
        let recent_player_inputs: Vec<String> = self
            .world
            .conversation_log
            .recent_at(location_id, MAX_RECENT_CONVERSATION)
            .into_iter()
            .map(|exchange| exchange.player_input.clone())
            .collect();
        let mut dialogue = crate::dialogue_apply::dialogue_grounding_snapshot(
            &self.world,
            &self.npcs,
            NpcId(self.content.engine_npc_id),
        );
        // Phase 2 keeps scenic places in authored grounding without making
        // them playable world locations. The canonical snapshot supplies the
        // live calendar, weather/session, relationship, and safety facts;
        // these authored entries complete its location vocabulary.
        dialogue.current_location_name = current_name.clone();
        dialogue.known_location_names = self
            .content
            .npc_known_places
            .iter()
            .map(|place| place.display_name.clone())
            .collect();
        dialogue.location_facts = self
            .content
            .npc_known_places
            .iter()
            .map(|place| parish_npc::GroundedLocationFact {
                name: place.display_name.clone(),
                nearby_locations: Vec::new(),
                landmarks: vec![place.description.clone()],
            })
            .collect();
        dialogue.prior_player_inputs = recent_player_inputs.clone();
        dialogue.had_prior_exchange = !self
            .world
            .conversation_log
            .recent_at(location_id, 1)
            .is_empty();
        GroundingSnapshot {
            dialogue,
            known_people: vec![GroundedPerson {
                id: self.content.npc_id.clone(),
                display_name: self.content.npc_name.clone(),
                role: self.content.npc_role.clone(),
                current_location_id: self.content.engine_location_id,
                current_location_name: current_name.clone(),
            }],
            known_places: self.content.npc_known_places.clone(),
            authored_facts: self.content.npc_facts.clone(),
            current_location_name: current_name,
            speaker_name: self.content.npc_name.clone(),
            canonical_mood: self
                .npcs
                .get(NpcId(self.content.engine_npc_id))
                .map(|npc| npc.mood.clone())
                .unwrap_or_else(|| "watchful".to_string()),
            recent_player_inputs,
            had_prior_exchange: !self
                .world
                .conversation_log
                .recent_at(location_id, 1)
                .is_empty(),
        }
    }

    fn endpoint_invocation(
        &self,
        request_id: &LogicalRequestId,
        attempt_id: &ExecutionAttemptId,
        input: &str,
        grounding: &GroundingSnapshot,
    ) -> EndpointInvocation {
        let speaker = grounding
            .known_people
            .first()
            .cloned()
            .unwrap_or_else(|| GroundedPerson {
                id: self.content.npc_id.clone(),
                display_name: self.content.npc_name.clone(),
                role: self.content.npc_role.clone(),
                current_location_id: self.content.engine_location_id,
                current_location_name: self.content.location_name.clone(),
            });
        let recent_conversation = self
            .world
            .conversation_log
            .recent_at(
                LocationId(self.content.engine_location_id),
                MAX_RECENT_CONVERSATION,
            )
            .into_iter()
            .cloned()
            .collect();
        EndpointInvocation {
            contract_version: PRESENTATION_CONTRACT_VERSION,
            session_id: self.session_id.clone(),
            logical_request_id: request_id.clone(),
            attempt_id: attempt_id.clone(),
            base_revision: self.state_revision,
            idempotency_key: format!("{}:{}", request_id.raw_value, attempt_id.raw_value),
            role: "npc_dialogue".to_string(),
            player_input: input.to_string(),
            speaker,
            current_location: grounding
                .known_places
                .iter()
                .find(|place| place.playable)
                .cloned()
                .unwrap_or_else(|| GroundedPlace {
                    id: self.content.location_id.clone(),
                    display_name: self.content.location_name.clone(),
                    description: self.content.look_text.clone(),
                    playable: true,
                }),
            known_people: grounding
                .known_people
                .iter()
                .take(MAX_GROUNDING_ENTRIES)
                .cloned()
                .collect(),
            known_places: grounding
                .known_places
                .iter()
                .take(MAX_GROUNDING_ENTRIES)
                .cloned()
                .collect(),
            authored_facts: grounding
                .authored_facts
                .iter()
                .take(MAX_GROUNDING_ENTRIES)
                .cloned()
                .collect(),
            recent_conversation,
            max_output_chars: 8 * 1024,
            max_stream_bytes: MAX_PROVISIONAL_TEXT_BYTES,
        }
    }

    // Keep event fields explicit so each wire-level value is committed at this seam.
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        kind: SemanticEventKind,
        content: Option<String>,
        speaker: Option<String>,
        logical_request_id: Option<&LogicalRequestId>,
        attempt_id: Option<&ExecutionAttemptId>,
        transcript_item_id: Option<TranscriptItemId>,
        provisional: bool,
        stream_sequence: Option<u64>,
        stream_update: Option<StreamUpdate>,
        accepted: bool,
        source_draft_id: Option<DraftId>,
        state_revision: Option<StateRevision>,
        metadata: BTreeMap<String, String>,
    ) -> SemanticEvent {
        let event = self.make_event(
            kind,
            content,
            speaker,
            logical_request_id,
            attempt_id,
            transcript_item_id,
            provisional,
            stream_sequence,
            stream_update,
            accepted,
            source_draft_id,
            state_revision,
            metadata,
        );
        self.next_event_sequence += 1;
        if provisional {
            self.provisional_events.push_back(event.clone());
            while self.provisional_events.len() > MAX_PROVISIONAL_EVENTS {
                self.provisional_events.pop_front();
            }
        } else {
            self.events.push_back(event.clone());
            trim_events(&mut self.events, &mut self.has_older_events);
        }
        event
    }

    // Keep semantic event fields explicit so event construction stays auditable.
    // Keep snapshot components explicit so provisional data cannot enter persisted saves.
    #[allow(clippy::too_many_arguments)]
    fn make_event(
        &self,
        kind: SemanticEventKind,
        content: Option<String>,
        speaker: Option<String>,
        logical_request_id: Option<&LogicalRequestId>,
        attempt_id: Option<&ExecutionAttemptId>,
        transcript_item_id: Option<TranscriptItemId>,
        provisional: bool,
        stream_sequence: Option<u64>,
        stream_update: Option<StreamUpdate>,
        accepted: bool,
        source_draft_id: Option<DraftId>,
        state_revision: Option<StateRevision>,
        metadata: BTreeMap<String, String>,
    ) -> SemanticEvent {
        self.make_event_at(
            EventSequence::new(self.next_event_sequence + 1),
            kind,
            content,
            speaker,
            logical_request_id,
            attempt_id,
            transcript_item_id,
            provisional,
            stream_sequence,
            stream_update,
            accepted,
            source_draft_id,
            state_revision,
            metadata,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn make_event_at(
        &self,
        sequence: EventSequence,
        kind: SemanticEventKind,
        content: Option<String>,
        speaker: Option<String>,
        logical_request_id: Option<&LogicalRequestId>,
        attempt_id: Option<&ExecutionAttemptId>,
        transcript_item_id: Option<TranscriptItemId>,
        provisional: bool,
        stream_sequence: Option<u64>,
        stream_update: Option<StreamUpdate>,
        accepted: bool,
        source_draft_id: Option<DraftId>,
        state_revision: Option<StateRevision>,
        metadata: BTreeMap<String, String>,
    ) -> SemanticEvent {
        SemanticEvent {
            contract_version: PRESENTATION_CONTRACT_VERSION,
            event_id: SemanticEventId::fresh(),
            session_id: self.session_id.clone(),
            sequence,
            game_time: Some(self.world.clock.now()),
            kind,
            content,
            speaker,
            logical_request_id: logical_request_id.cloned(),
            attempt_id: attempt_id.cloned(),
            transcript_item_id,
            provisional,
            stream_sequence,
            stream_update: stream_update.unwrap_or(StreamUpdate::Replace),
            terminal_outcome: metadata
                .get("terminalOutcome")
                .and_then(|outcome| parse_outcome(outcome)),
            accepted,
            source_draft_id,
            state_revision,
            clarification: None,
            metadata,
        }
    }

    fn to_save(&self) -> MobileSave {
        self.to_save_with(
            self.world.clone(),
            self.npcs.clone(),
            self.state_revision,
            self.requests.clone(),
            self.events.iter().cloned().collect(),
            self.next_event_sequence,
            self.has_older_events,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn to_save_with(
        &self,
        world: WorldState,
        npcs: NpcManager,
        state_revision: StateRevision,
        requests: Vec<RequestRecord>,
        events: Vec<SemanticEvent>,
        next_event_sequence: u64,
        has_older_events: bool,
    ) -> MobileSave {
        let requests = durable_requests(requests);
        let events = events
            .into_iter()
            .filter(|event| !event.provisional)
            .collect();
        MobileSave {
            format_version: MOBILE_SAVE_FORMAT_VERSION,
            session_id: self.session_id.clone(),
            state_revision,
            next_event_sequence,
            game: GameSnapshot::capture(&world, &npcs),
            requests,
            events,
            has_older_events,
        }
    }

    fn persist_current(&mut self) -> Result<(), MobileError> {
        let save = self.to_save();
        self.persist_candidate(&save)
    }

    fn persist_candidate(&mut self, save: &MobileSave) -> Result<(), MobileError> {
        if let Some(store) = self.store.as_mut() {
            store.save(save)?;
        }
        Ok(())
    }
}

/// The compact grounding state retained by an attempt.  It is not serialized;
/// after restart the request is interrupted and must be retried with a fresh
/// grounding snapshot.
#[derive(Debug, Clone)]
struct GroundingSnapshot {
    dialogue: DialogueGroundingSnapshot,
    known_people: Vec<GroundedPerson>,
    known_places: Vec<GroundedPlace>,
    authored_facts: Vec<GroundedFact>,
    current_location_name: String,
    speaker_name: String,
    canonical_mood: String,
    recent_player_inputs: Vec<String>,
    had_prior_exchange: bool,
}

impl PartialEq for GroundingSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.known_people == other.known_people
            && self.known_places == other.known_places
            && self.authored_facts == other.authored_facts
            && self.current_location_name == other.current_location_name
            && self.speaker_name == other.speaker_name
            && self.canonical_mood == other.canonical_mood
            && self.recent_player_inputs == other.recent_player_inputs
            && self.had_prior_exchange == other.had_prior_exchange
    }
}

impl GroundingSnapshot {
    fn as_dialogue_snapshot(&self) -> DialogueGroundingSnapshot {
        self.dialogue.clone()
    }
}

#[derive(Debug, Clone, Copy)]
enum DeterministicCapability {
    Look,
    People,
    Exits,
    Help,
}

impl DeterministicCapability {
    const fn name(self) -> &'static str {
        match self {
            Self::Look => "look",
            Self::People => "people",
            Self::Exits => "exits",
            Self::Help => "help",
        }
    }

    const fn content(self) -> &'static str {
        match self {
            Self::Look => "Crossroads · stone wall · lane to the east",
            Self::People => "Nearby: Peig.",
            Self::Exits => "The alder lane runs east, but the way is closed tonight.",
            Self::Help => "/look · /people · /exits · /help. Address Peig in ordinary text.",
        }
    }
}

fn deterministic_capability(text: &str) -> Option<DeterministicCapability> {
    match text.trim().to_lowercase().as_str() {
        "/look" | "look" | "look around" => Some(DeterministicCapability::Look),
        "/people" | "people" | "/npcs" => Some(DeterministicCapability::People),
        "/exits" | "exits" => Some(DeterministicCapability::Exits),
        "/help" | "help" => Some(DeterministicCapability::Help),
        _ => None,
    }
}

fn validate_command_text(text: &str) -> Result<(), MobileError> {
    if text.trim().is_empty() {
        return Err(MobileError::EmptyCommand);
    }
    if text.len() > MAX_COMMAND_BYTES {
        return Err(MobileError::CommandTooLarge);
    }
    Ok(())
}

fn validate_candidate_text(text: &str) -> Result<(), MobileError> {
    if text.trim().is_empty() {
        return Err(MobileError::CandidateEmpty);
    }
    if text.len() > MAX_PROVISIONAL_TEXT_BYTES {
        return Err(MobileError::CandidateTooLarge);
    }
    Ok(())
}

fn make_phase2_domain(
    content: &Phase2ContentDefinition,
) -> Result<(WorldState, NpcManager), MobileError> {
    let location_id = LocationId(content.engine_location_id);
    let graph_json = serde_json::json!({
        "locations": [{
            "id": content.engine_location_id,
            "name": content.location_name,
            "description_template": content.look_text,
            "landmarks": content
                .npc_known_places
                .iter()
                .filter(|place| place.playable)
                .map(|place| place.description.clone())
                .collect::<Vec<_>>(),
            "indoor": false,
            "public": true,
            "connections": [],
            "lat": 53.618,
            "lon": -8.095,
            "associated_npcs": [content.engine_npc_id],
            "aliases": ["crossroads"],
            "geo_kind": "fictional"
        }]
    });
    let graph = WorldGraph::load_from_str(&graph_json.to_string())
        .map_err(|error| MobileError::Content(format!("invalid Phase 2 world graph: {error}")))?;
    if graph.location_count() != 1 || graph.get(location_id).is_none() {
        return Err(MobileError::Content(
            "Phase 2 world must contain exactly one location".to_string(),
        ));
    }
    let location = graph
        .get(location_id)
        .expect("validated Phase 2 graph location")
        .clone();
    let mut world = WorldState::new();
    world.graph = graph;
    world.player_location = location_id;
    world.locations.clear();
    world.locations.insert(
        location_id,
        Location {
            id: location_id,
            name: location.name,
            description: location.description_template,
            indoor: location.indoor,
            public: location.public,
            lat: location.lat,
            lon: location.lon,
        },
    );
    world.visited_locations.clear();
    world.visited_locations.insert(location_id);
    world.visited_order = vec![location_id];
    // Phase 2 opens at the authored evening crossroads scene. Pausing keeps
    // the fixture stable while still exercising the real clock/weather state.
    world.clock.advance(9 * 60);
    world.clock.pause();
    world.weather = Weather::LightRain;
    world
        .weather_engine
        .force(Weather::LightRain, world.clock.now());

    let npc_json = serde_json::json!({
        "npcs": [{
            "id": content.engine_npc_id,
            "name": content.npc_name,
            "brief_description": "a keen-eyed widow",
            "age": 58,
            "occupation": content.npc_role,
            "personality": content.npc_personality.join(", "),
            "pronouns": "she/her",
            "home": content.engine_location_id,
            "workplace": null,
            "mood": "watchful",
            "relationships": [],
            "knowledge": content
                .npc_facts
                .iter()
                .map(|fact| fact.statement.clone())
                .collect::<Vec<_>>()
        }]
    });
    let loaded = parish_npc::data::load_npcs_from_str(&npc_json.to_string())
        .map_err(|error| MobileError::Content(format!("invalid Phase 2 NPC data: {error}")))?;
    let mut npcs = NpcManager::new();
    for npc in loaded {
        npcs.add_npc(npc);
    }
    if npcs.all_npcs().count() != 1 {
        return Err(MobileError::Content(
            "Phase 2 world must contain exactly one NPC".to_string(),
        ));
    }
    Ok((world, npcs))
}

fn trim_events(events: &mut VecDeque<SemanticEvent>, has_older_events: &mut bool) {
    while events.len() > MAX_SEMANTIC_EVENTS {
        events.pop_front();
        *has_older_events = true;
    }
}

/// Strip stream-only state before handing a save to a persistence adapter.
///
/// Provisional text is useful while an Endpoint attempt is alive, but it is
/// not an authoritative transcript and must never survive a failure or
/// relaunch. The adapter receives an owned request vector, so this also keeps
/// the in-memory stream bookkeeping independent from the durable projection.
fn durable_requests(mut requests: Vec<RequestRecord>) -> Vec<RequestRecord> {
    for request in &mut requests {
        for attempt in &mut request.attempts {
            attempt.provisional_item_ids.clear();
            attempt.last_stream_sequence = 0;
            attempt.provisional_text.clear();
        }
    }
    requests
}

fn metadata<const N: usize>(pairs: [(&str, String); N]) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn terminal_metadata(outcome: ResponseTerminalOutcome) -> BTreeMap<String, String> {
    metadata([("terminalOutcome", outcome_string(outcome).to_string())])
}

fn outcome_string(outcome: ResponseTerminalOutcome) -> &'static str {
    match outcome {
        ResponseTerminalOutcome::Succeeded => "succeeded",
        ResponseTerminalOutcome::Cancelled => "cancelled",
        ResponseTerminalOutcome::Interrupted => "interrupted",
        ResponseTerminalOutcome::Failed => "failed",
    }
}

fn parse_outcome(value: &str) -> Option<ResponseTerminalOutcome> {
    match value {
        "succeeded" => Some(ResponseTerminalOutcome::Succeeded),
        "cancelled" => Some(ResponseTerminalOutcome::Cancelled),
        "interrupted" => Some(ResponseTerminalOutcome::Interrupted),
        "failed" => Some(ResponseTerminalOutcome::Failed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> MobileSession {
        MobileSession::open_new().expect("phase2 session")
    }

    #[test]
    fn new_session_is_exactly_one_location_and_one_npc() {
        let session = session();
        assert_eq!(session.content().engine_location_id, 1);
        assert_eq!(session.npcs().all_npcs().count(), 1);
        assert_eq!(session.npcs().get(NpcId(22)).unwrap().name, "Peig");
        assert_eq!(session.snapshot().read_model.nearby_people.len(), 1);
    }

    #[test]
    fn semantic_ids_and_fields_match_swift_shape() {
        let session = session();
        let json = serde_json::to_value(session.snapshot().events[0].clone()).unwrap();
        assert!(json["eventID"].is_string());
        assert!(json["sessionID"].is_string());
        assert_eq!(json["sequence"]["rawValue"], 1);
        assert_eq!(json["contractVersion"]["major"], 1);
        assert_eq!(json["contractVersion"]["minor"], 0);
        assert_eq!(json["streamUpdate"], "replace");
    }

    #[test]
    fn semantic_event_matches_rundale_kit_fixture_shape() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../mobile/RundaleKit/Tests/RundaleKitTests/Fixtures/scene-changed.json"
        )))
        .expect("scene changed fixture is valid JSON");
        let event = SemanticEvent {
            contract_version: PRESENTATION_CONTRACT_VERSION,
            event_id: SemanticEventId::new("fixture-event-scene-1"),
            session_id: SessionId::new("fixture-session"),
            sequence: EventSequence::new(1),
            game_time: None,
            kind: SemanticEventKind::SceneChanged,
            content: Some("The crossroads".to_string()),
            speaker: None,
            logical_request_id: None,
            attempt_id: None,
            transcript_item_id: Some(TranscriptItemId::new("opening:scene")),
            provisional: false,
            stream_sequence: None,
            stream_update: StreamUpdate::Replace,
            terminal_outcome: None,
            accepted: false,
            source_draft_id: None,
            state_revision: None,
            clarification: None,
            metadata: metadata([
                ("sceneID", "crossroads".to_string()),
                ("sceneName", "The crossroads".to_string()),
            ]),
        };
        assert_eq!(serde_json::to_value(event).unwrap(), fixture);
    }

    #[test]
    fn deterministic_commands_work_without_endpoint() {
        let mut session = session();
        let result = session
            .submit(None, "/look", Some(DraftId::new("draft-1")))
            .unwrap();
        assert!(result.endpoint_invocation.is_none());
        assert_eq!(
            result.terminal_outcome,
            Some(ResponseTerminalOutcome::Succeeded)
        );
        assert_eq!(session.snapshot().active_request_id, None);
        assert_eq!(
            result
                .events
                .iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
            vec![
                SemanticEventKind::PlayerCommand,
                SemanticEventKind::ActionResult,
                SemanticEventKind::ResponseCompleted,
            ]
        );
        assert!(
            result
                .events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
        assert!(
            session
                .read_events(Some(EventCursor::new(0)), 10)
                .events
                .iter()
                .any(|event| event.kind == SemanticEventKind::ActionResult)
        );
    }

    #[test]
    fn acceptance_precedes_endpoint_and_has_grounding() {
        let mut session = session();
        let result = session
            .submit(
                Some(LogicalRequestId::new("logical-1")),
                "ask Peig about the old church",
                None,
            )
            .unwrap();
        let invocation = result.endpoint_invocation.unwrap();
        assert_eq!(invocation.base_revision, StateRevision::new(0));
        assert_eq!(invocation.speaker.current_location_id, 1);
        assert!(
            invocation
                .known_places
                .iter()
                .any(|place| place.display_name == "the old church")
        );
        assert_eq!(
            session.snapshot().requests[0].phase,
            RequestPhase::Executing
        );
        assert_eq!(
            session.snapshot().events[1].kind,
            SemanticEventKind::PlayerCommand
        );
    }

    #[test]
    fn authored_scenic_place_is_grounded_but_invented_place_is_rejected() {
        let mut dialogue_session = session();
        let accepted = dialogue_session
            .submit(None, "ask Peig about the old church", None)
            .unwrap();
        let attempt = accepted.attempt_id.unwrap();
        let base = accepted.endpoint_invocation.unwrap().base_revision;
        let completed = dialogue_session
            .receive_candidate(EndpointCandidate {
                attempt_id: attempt,
                base_revision: base,
                dialogue: "The old church stands beyond the alder trees, where the path bends toward the hill.".to_string(),
                metadata: BTreeMap::new(),
                structured: true,
            })
            .unwrap();
        assert_eq!(
            completed.terminal_outcome,
            Some(ResponseTerminalOutcome::Succeeded)
        );
        assert_eq!(
            dialogue_session
                .world()
                .conversation_log
                .recent_at(LocationId(1), 5)
                .len(),
            1
        );

        let mut invented = session();
        let rejected = invented
            .submit(None, "ask Peig about the old castle", None)
            .unwrap();
        let attempt = rejected.attempt_id.unwrap();
        let base = rejected.endpoint_invocation.unwrap().base_revision;
        let failed = invented
            .receive_candidate(EndpointCandidate {
                attempt_id: attempt,
                base_revision: base,
                dialogue: "The old castle stands to the north beyond the river.".to_string(),
                metadata: BTreeMap::new(),
                structured: true,
            })
            .unwrap();
        assert_eq!(
            failed.terminal_outcome,
            Some(ResponseTerminalOutcome::Failed)
        );
        assert!(failed.events.iter().any(|event| {
            event.kind == SemanticEventKind::Error
                && event.metadata.get("errorKind") == Some(&"semantic_validation".to_string())
        }));
        assert!(
            invented
                .world()
                .conversation_log
                .recent_at(LocationId(1), 5)
                .is_empty()
        );
    }

    #[test]
    fn cumulative_frames_are_bounded_and_duplicate_frames_are_ignored() {
        let mut session = session();
        let result = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let attempt = result.attempt_id.unwrap();
        let base = result.endpoint_invocation.unwrap().base_revision;
        let first = session
            .receive_frame(EndpointFrame {
                attempt_id: attempt.clone(),
                base_revision: base,
                sequence: 1,
                text: "The wall".to_string(),
                stream_update: StreamUpdate::Replace,
                done: false,
            })
            .unwrap();
        assert!(first.accepted);
        let duplicate = session
            .receive_frame(EndpointFrame {
                attempt_id: attempt.clone(),
                base_revision: base,
                sequence: 1,
                text: "malicious late overwrite".to_string(),
                stream_update: StreamUpdate::Replace,
                done: false,
            })
            .unwrap();
        assert!(duplicate.ignored);
        let snapshot = session.snapshot();
        assert!(
            snapshot
                .events
                .iter()
                .all(|event| event.content.as_deref() != Some("malicious late overwrite"))
        );
    }

    #[test]
    fn stop_wins_and_late_candidate_cannot_commit() {
        let mut session = session();
        let result = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let request = result.logical_request_id.unwrap();
        let attempt = result.attempt_id.unwrap();
        let base = result.endpoint_invocation.unwrap().base_revision;
        let stopped = session.stop(&attempt).unwrap();
        assert_eq!(
            stopped.terminal_outcome,
            Some(ResponseTerminalOutcome::Cancelled)
        );
        let late = session
            .receive_candidate(EndpointCandidate {
                attempt_id: attempt,
                base_revision: base,
                dialogue: "The wall is low.".to_string(),
                metadata: BTreeMap::new(),
                structured: true,
            })
            .unwrap();
        assert!(late.ignored);
        assert!(
            session
                .world()
                .conversation_log
                .recent_at(LocationId(1), 5)
                .is_empty()
        );
        assert_eq!(
            session.snapshot().requests[0].terminal_outcome,
            Some(ResponseTerminalOutcome::Cancelled)
        );
        assert_eq!(session.snapshot().requests[0].id, request);
    }

    #[test]
    fn accepted_restart_becomes_interrupted_and_does_not_rerun() {
        let mut session = session();
        let result = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let save = session.save();
        let mut resumed = MobileSession::open_resume(save).unwrap();
        let record = &resumed.snapshot().requests[0];
        assert_eq!(record.phase, RequestPhase::Interrupted);
        assert_eq!(
            record.terminal_outcome,
            Some(ResponseTerminalOutcome::Interrupted)
        );
        assert!(resumed.take_pending_invocation().is_none());
        assert!(resumed.snapshot().events.iter().any(|event| {
            event.kind == SemanticEventKind::ResponseCompleted
                && event.terminal_outcome == Some(ResponseTerminalOutcome::Interrupted)
        }));
        assert_eq!(result.logical_request_id.unwrap(), record.id);
    }

    #[test]
    fn successful_candidate_commits_one_exchange_and_retry_is_rejected() {
        let mut session = session();
        let result = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let id = result.logical_request_id.unwrap();
        let attempt = result.attempt_id.unwrap();
        let base = result.endpoint_invocation.unwrap().base_revision;
        let completed = session
            .receive_candidate(EndpointCandidate {
                attempt_id: attempt,
                base_revision: base,
                dialogue: "A low stone wall borders the road here.".to_string(),
                metadata: BTreeMap::new(),
                structured: true,
            })
            .unwrap();
        assert_eq!(
            completed.terminal_outcome,
            Some(ResponseTerminalOutcome::Succeeded)
        );
        assert_eq!(
            session
                .world()
                .conversation_log
                .recent_at(LocationId(1), 5)
                .len(),
            1
        );
        assert!(matches!(
            session.retry(&id),
            Err(MobileError::RequestAlreadyCommitted(_))
        ));
        let late = session
            .receive_candidate(EndpointCandidate {
                attempt_id: ExecutionAttemptId::new("late"),
                base_revision: StateRevision::new(0),
                dialogue: "A second line.".to_string(),
                metadata: BTreeMap::new(),
                structured: true,
            })
            .unwrap();
        assert!(late.ignored);
        assert_eq!(
            session
                .world()
                .conversation_log
                .recent_at(LocationId(1), 5)
                .len(),
            1
        );
    }

    #[test]
    fn failed_attempt_can_retry_with_new_attempt_identity() {
        let mut session = session();
        let first = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let id = first.logical_request_id.unwrap();
        let attempt = first.attempt_id.unwrap();
        let failed = session
            .receive_candidate(EndpointCandidate {
                attempt_id: attempt.clone(),
                base_revision: StateRevision::new(0),
                dialogue: "not structured".to_string(),
                metadata: BTreeMap::new(),
                structured: false,
            })
            .unwrap();
        assert_eq!(
            failed.terminal_outcome,
            Some(ResponseTerminalOutcome::Failed)
        );
        let retry = session.retry(&id).unwrap();
        assert_ne!(retry.attempt_id, Some(attempt));
        assert!(retry.endpoint_invocation.is_some());
        assert_eq!(session.snapshot().requests[0].attempts.len(), 2);
    }

    #[test]
    fn correlated_endpoint_failure_is_terminal_and_retryable() {
        let mut session = session();
        let started = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let request_id = started.logical_request_id.clone().unwrap();
        let attempt_id = started.attempt_id.clone().unwrap();
        let base_revision = started.endpoint_invocation.unwrap().base_revision;

        let failed = session
            .receive_failure(
                &attempt_id,
                base_revision,
                EndpointFailureKind::MissingTerminal,
                "The Endpoint stream ended before its terminal frame.".to_string(),
            )
            .unwrap();
        assert_eq!(
            failed.terminal_outcome,
            Some(ResponseTerminalOutcome::Failed)
        );
        assert!(failed.events.iter().any(|event| {
            event.kind == SemanticEventKind::Error
                && event.metadata.get("errorKind") == Some(&"missing_terminal".to_string())
        }));
        assert!(failed.events.iter().any(|event| {
            event.kind == SemanticEventKind::ResponseCompleted
                && event.metadata.get("errorKind") == Some(&"missing_terminal".to_string())
        }));
        assert!(
            session
                .world()
                .conversation_log
                .recent_at(LocationId(1), 5)
                .is_empty()
        );
        assert_eq!(
            session.snapshot().requests[0].terminal_outcome,
            Some(ResponseTerminalOutcome::Failed)
        );

        let retry = session.retry(&request_id).unwrap();
        assert_ne!(retry.attempt_id, Some(attempt_id.clone()));
        let stale = session
            .receive_failure(
                &attempt_id,
                base_revision,
                EndpointFailureKind::Transport,
                "late transport failure".to_string(),
            )
            .unwrap();
        assert!(stale.ignored);
        assert_eq!(
            session.snapshot().requests[0].phase,
            RequestPhase::Executing
        );
        let retry_attempt = retry.attempt_id.unwrap();
        let interrupted = session
            .receive_failure(
                &retry_attempt,
                base_revision,
                EndpointFailureKind::Interrupted,
                "The native worker was interrupted.".to_string(),
            )
            .unwrap();
        assert_eq!(
            interrupted.terminal_outcome,
            Some(ResponseTerminalOutcome::Interrupted)
        );
    }

    #[test]
    fn endpoint_failure_requires_the_invocation_base_revision() {
        let mut session = session();
        let started = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let attempt_id = started.attempt_id.unwrap();
        let base_revision = started.endpoint_invocation.unwrap().base_revision;
        let too_large = "x".repeat(MAX_FAILURE_MESSAGE_BYTES + 1);
        assert_eq!(
            session.receive_failure(
                &attempt_id,
                base_revision,
                EndpointFailureKind::Transport,
                too_large,
            ),
            Err(MobileError::FailureMessageTooLarge)
        );
        assert_eq!(
            session.receive_failure(
                &attempt_id,
                base_revision,
                EndpointFailureKind::Transport,
                "line\nbreak".to_string(),
            ),
            Err(MobileError::FailureMessageInvalid)
        );
        assert_eq!(
            session.snapshot().requests[0].phase,
            RequestPhase::Executing
        );
        let ignored = session
            .receive_failure(
                &attempt_id,
                StateRevision::new(99),
                EndpointFailureKind::Protocol,
                "bad protocol".to_string(),
            )
            .unwrap();
        assert!(ignored.ignored);
        assert!(ignored.events.is_empty());
        assert_eq!(
            session.snapshot().requests[0].phase,
            RequestPhase::Executing
        );
    }

    #[test]
    fn sqlite_resume_drops_provisional_stream_text_and_keeps_final_dialogue() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stream-recovery.sqlite");
        let mut session = MobileSession::open_new_sqlite(&path).unwrap();
        let started = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let request_id = started.logical_request_id.clone().unwrap();
        let attempt_id = started.attempt_id.clone().unwrap();
        let base_revision = started.endpoint_invocation.unwrap().base_revision;
        let streamed = session
            .receive_frame(EndpointFrame {
                attempt_id: attempt_id.clone(),
                base_revision,
                sequence: 1,
                text: "PRIVATE PARTIAL TOKEN".to_string(),
                stream_update: StreamUpdate::Replace,
                done: false,
            })
            .unwrap();
        assert!(streamed.events[0].provisional);
        assert!(
            session
                .snapshot()
                .events
                .iter()
                .all(|event| !event.provisional)
        );
        session
            .receive_failure(
                &attempt_id,
                base_revision,
                EndpointFailureKind::Transport,
                "the worker stopped".to_string(),
            )
            .unwrap();
        drop(session);

        let mut resumed = MobileSession::open_resume_sqlite(&path).unwrap().unwrap();
        let persisted = resumed.save();
        assert!(persisted.events.iter().all(|event| !event.provisional));
        assert!(
            persisted
                .events
                .iter()
                .all(|event| event.content.as_deref() != Some("PRIVATE PARTIAL TOKEN"))
        );
        assert!(persisted.requests.iter().all(|request| {
            request
                .attempts
                .iter()
                .all(|attempt| attempt.provisional_text.is_empty())
        }));
        assert_eq!(
            persisted
                .events
                .iter()
                .filter_map(|event| event.terminal_outcome)
                .next_back(),
            Some(ResponseTerminalOutcome::Failed)
        );

        let retry = resumed.retry(&request_id).unwrap();
        let retry_attempt = retry.attempt_id.unwrap();
        let retry_base = retry.endpoint_invocation.unwrap().base_revision;
        let final_text = "A low stone wall borders the road here.";
        resumed
            .receive_candidate(EndpointCandidate {
                attempt_id: retry_attempt,
                base_revision: retry_base,
                dialogue: final_text.to_string(),
                metadata: BTreeMap::new(),
                structured: true,
            })
            .unwrap();
        drop(resumed);

        let final_session = MobileSession::open_resume_sqlite(&path).unwrap().unwrap();
        assert_eq!(
            final_session
                .world()
                .conversation_log
                .recent_at(LocationId(1), 5)
                .last()
                .map(|exchange| exchange.npc_dialogue.as_str()),
            Some(final_text)
        );
        drop(final_session);
        let content = Phase2ContentDefinition::try_canonical().unwrap();
        let store = SqliteMobileStore::open(
            &path,
            &content.content_version_key(),
            &content.content_fingerprint(),
        )
        .unwrap();
        let page = store
            .read_event_page(Some(EventCursor::new(0)), 128)
            .unwrap();
        assert!(page.events.iter().all(|event| !event.provisional));
        assert!(
            page.events
                .iter()
                .all(|event| event.content.as_deref() != Some("PRIVATE PARTIAL TOKEN"))
        );
        assert!(
            page.events
                .iter()
                .any(|event| event.content.as_deref() == Some(final_text))
        );
    }

    fn assert_strictly_increasing_event_sequences(events: &[SemanticEvent]) {
        assert!(
            events
                .windows(2)
                .all(|window| window[0].sequence < window[1].sequence),
            "event sequences were not strictly increasing: {:?}",
            events
                .iter()
                .map(|event| event.sequence.raw_value)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn sqlite_success_events_have_distinct_durable_sequences() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("success-sequences.sqlite");
        let mut session = MobileSession::open_new_sqlite(&path).unwrap();
        let started = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let attempt_id = started.attempt_id.unwrap();
        let base_revision = started.endpoint_invocation.unwrap().base_revision;
        session
            .receive_candidate(EndpointCandidate {
                attempt_id,
                base_revision,
                dialogue: "A low stone wall borders the road here.".to_string(),
                metadata: BTreeMap::new(),
                structured: true,
            })
            .unwrap();
        drop(session);

        let resumed = MobileSession::open_resume_sqlite(&path).unwrap().unwrap();
        let save = resumed.save();
        assert_strictly_increasing_event_sequences(&save.events);
        assert_eq!(
            save.next_event_sequence,
            save.events.last().unwrap().sequence.raw_value
        );
    }

    #[test]
    fn sqlite_failure_and_retry_events_remain_durable_and_monotonic() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retry-sequences.sqlite");
        let mut session = MobileSession::open_new_sqlite(&path).unwrap();
        let started = session
            .submit(None, "ask Peig about the wall", None)
            .unwrap();
        let request_id = started.logical_request_id.unwrap();
        let attempt_id = started.attempt_id.unwrap();
        let base_revision = started.endpoint_invocation.unwrap().base_revision;
        session
            .receive_failure(
                &attempt_id,
                base_revision,
                EndpointFailureKind::Transport,
                "the worker stopped".to_string(),
            )
            .unwrap();
        drop(session);

        let mut resumed = MobileSession::open_resume_sqlite(&path).unwrap().unwrap();
        let retry = resumed.retry(&request_id).unwrap();
        let retry_attempt_id = retry.attempt_id.unwrap();
        let retry_base_revision = retry.endpoint_invocation.unwrap().base_revision;
        resumed
            .receive_failure(
                &retry_attempt_id,
                retry_base_revision,
                EndpointFailureKind::Protocol,
                "the worker returned an invalid response".to_string(),
            )
            .unwrap();
        drop(resumed);

        let resumed = MobileSession::open_resume_sqlite(&path).unwrap().unwrap();
        let save = resumed.save();
        assert_strictly_increasing_event_sequences(&save.events);
        assert_eq!(
            save.next_event_sequence,
            save.events.last().unwrap().sequence.raw_value
        );
    }

    #[test]
    fn paged_events_are_monotonic_and_bounded() {
        let mut session = session();
        for _ in 0..12 {
            session.submit(None, "/help", None).unwrap();
        }
        let first = session.read_events(Some(EventCursor::new(0)), 2);
        assert_eq!(first.events.len(), 2);
        assert!(first.has_more);
        let second = session.read_events(Some(first.next_cursor), 100);
        assert!(second.events.first().unwrap().sequence > first.events.last().unwrap().sequence);
    }

    #[test]
    fn sqlite_store_round_trips_and_pages_durable_events() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("phase2.sqlite");
        let mut session = MobileSession::open_new_sqlite(&path).unwrap();
        session.submit(None, "/look", None).unwrap();
        drop(session);
        let content = Phase2ContentDefinition::try_canonical().unwrap();
        let store = SqliteMobileStore::open(
            &path,
            &content.content_version_key(),
            &content.content_fingerprint(),
        )
        .unwrap();
        drop(store);

        let resumed = MobileSession::open_resume_sqlite(&path).unwrap().unwrap();
        assert_eq!(resumed.npcs().all_npcs().count(), 1);
        assert_eq!(
            resumed
                .world()
                .conversation_log
                .recent_at(LocationId(1), 5)
                .len(),
            0
        );
        drop(resumed);
        let store = SqliteMobileStore::open(
            &path,
            &content.content_version_key(),
            &content.content_fingerprint(),
        )
        .unwrap();
        let page = store
            .read_event_page(Some(EventCursor::new(0)), 32)
            .unwrap();
        assert!(
            page.events
                .iter()
                .any(|event| event.kind == SemanticEventKind::SceneChanged)
        );
        assert!(
            page.events
                .iter()
                .any(|event| event.kind == SemanticEventKind::ActionResult)
        );
    }
}
