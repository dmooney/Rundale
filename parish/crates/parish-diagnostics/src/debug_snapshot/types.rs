//! Debug snapshot types — serializable DTOs for the debug UI.

use serde::Serialize;

use parish_inference::InferenceLogEntry;

/// A complete debug snapshot of all game state.
///
/// Built by [`build_debug_snapshot`] from live game state references.
/// All fields are owned strings/values so the snapshot can be freely
/// serialized and sent across IPC boundaries.
#[derive(Debug, Clone, Serialize)]
pub struct DebugSnapshot {
    /// Game clock and timing information.
    pub clock: ClockDebug,
    /// Dynamic weather state machine internals.
    pub weather: WeatherDebug,
    /// World graph and player position.
    pub world: WorldDebug,
    /// Full NPC state for every NPC.
    pub npcs: Vec<NpcDebug>,
    /// Tier assignment summary.
    pub tier_summary: TierSummary,
    /// Event bus + recent game events flowing through it.
    pub event_bus: EventBusDebug,
    /// Gossip network state.
    pub gossip: GossipDebug,
    /// Conversation log (player-NPC exchanges).
    pub conversations: ConversationsDebug,
    /// Recent debug events (schedule, tier, inference).
    pub events: Vec<DebugEvent>,
    /// Inference pipeline configuration.
    pub inference: InferenceDebug,
    /// Auth state for this session (web-server only; disabled on Tauri).
    pub auth: AuthDebug,
}

/// Auth state for debug display.
///
/// On the web server, reflects the current visitor's session + OAuth linkage.
/// On Tauri (single local user), `oauth_enabled` is always `false`.
#[derive(Debug, Clone, Serialize)]
pub struct AuthDebug {
    /// Whether the server has Google OAuth credentials configured.
    pub oauth_enabled: bool,
    /// Whether the current session is linked to an OAuth account.
    pub logged_in: bool,
    /// OAuth provider name when `logged_in` (currently always `"google"`).
    pub provider: Option<String>,
    /// Display name or stable id for the linked account.
    pub display_name: Option<String>,
    /// Current session id (the `parish_sid` cookie). `None` on Tauri.
    pub session_id: Option<String>,
}

impl AuthDebug {
    /// Returns an `AuthDebug` for contexts where OAuth is not applicable
    /// (e.g. the Tauri desktop app).
    pub fn disabled() -> Self {
        Self {
            oauth_enabled: false,
            logged_in: false,
            provider: None,
            display_name: None,
            session_id: None,
        }
    }
}

/// Game clock state for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct ClockDebug {
    /// Formatted game time (e.g. "08:30 1820-03-20").
    pub game_time: String,
    /// Time of day label (e.g. "Morning").
    pub time_of_day: String,
    /// Season label (e.g. "Spring").
    pub season: String,
    /// Festival name if today is a festival, or null.
    pub festival: Option<String>,
    /// Current weather.
    pub weather: String,
    /// Whether the clock is player-paused.
    pub paused: bool,
    /// Whether the clock is paused while waiting on an inference call.
    pub inference_paused: bool,
    /// Clock speed multiplier (game seconds per real second).
    pub speed_factor: f64,
    /// Named speed preset matching the current factor, if any (e.g. "Normal").
    pub speed_name: Option<String>,
    /// Full day-of-week name (e.g. "Monday").
    pub day_of_week: String,
    /// Schedule day type label (e.g. "Weekday", "Sunday", "Market Day").
    pub day_type: String,
    /// Origin game-time anchor (creation or last resume).
    pub start_game_time: String,
    /// Frozen game time captured when the clock was paused (valid while frozen).
    pub paused_game_time: String,
    /// Real-world elapsed seconds since the anchor (for drift diagnostics).
    pub real_elapsed_secs: f64,
}

/// Dynamic weather engine internals for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct WeatherDebug {
    /// Current weather label (e.g. "LightRain").
    pub current: String,
    /// Game time when the current weather state began.
    pub since: String,
    /// Game-hours the current state has persisted.
    pub duration_hours: f64,
    /// Minimum duration before a transition is allowed (game-hours).
    pub min_duration_hours: f64,
    /// Canonical game time of the last transition evaluation, if any.
    pub last_check_at: Option<String>,
}

/// World graph summary for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct WorldDebug {
    /// Player's current location name.
    pub player_location_name: String,
    /// Player's current location ID.
    pub player_location_id: u32,
    /// Total number of locations in the graph.
    pub location_count: usize,
    /// Number of locations the player has visited (fog-of-war reveal set).
    pub visited_count: usize,
    /// Names of all visited locations.
    pub visited_locations: Vec<String>,
    /// Edge traversal counts (player "worn path" footprints).
    pub edge_traversals: Vec<EdgeTraversalDebug>,
    /// Most recent player-facing text log lines (tail).
    pub text_log_tail: Vec<String>,
    /// Total number of lines currently in the text log.
    pub text_log_len: usize,
    /// Per-location debug info.
    pub locations: Vec<LocationDebug>,
    /// Player's name if they have introduced themselves, or null.
    pub player_name: Option<String>,
}

/// A single edge in the player "worn path" map.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeTraversalDebug {
    /// Name of the first endpoint (lower id).
    pub from_name: String,
    /// Name of the second endpoint (higher id).
    pub to_name: String,
    /// Times the player has walked along this edge.
    pub count: u32,
}

/// Per-location debug info.
#[derive(Debug, Clone, Serialize)]
pub struct LocationDebug {
    /// Location ID.
    pub id: u32,
    /// Location name.
    pub name: String,
    /// Whether indoor.
    pub indoor: bool,
    /// Whether public.
    pub public: bool,
    /// Number of connected locations.
    pub connection_count: usize,
    /// Names of NPCs currently present here.
    pub npcs_here: Vec<String>,
    /// Whether the player has visited this location.
    pub visited: bool,
    /// Outgoing graph edges from this location.
    pub edges: Vec<GraphEdgeDebug>,
}

/// A single outgoing edge in the world graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphEdgeDebug {
    /// Destination location id.
    pub target_id: u32,
    /// Destination location name.
    pub target_name: String,
    /// Prose path description (e.g. "a narrow boreen lined with hawthorn").
    pub path_description: String,
    /// Travel time in game-minutes on foot.
    pub walking_minutes: u16,
}

/// Full NPC state for deep-dive inspection.
#[derive(Debug, Clone, Serialize)]
pub struct NpcDebug {
    /// NPC ID.
    pub id: u32,
    /// Full name.
    pub name: String,
    /// Brief anonymous descriptor shown before introduction.
    pub brief_description: String,
    /// Whether the player has been introduced to this NPC.
    pub introduced: bool,
    /// Age in years.
    pub age: u8,
    /// Occupation.
    pub occupation: String,
    /// Personality description.
    pub personality: String,
    /// Current location name.
    pub location_name: String,
    /// Current location ID.
    pub location_id: u32,
    /// Home location name (if set).
    pub home_name: Option<String>,
    /// Workplace location name (if set).
    pub workplace_name: Option<String>,
    /// Current mood.
    pub mood: String,
    /// Whether the Tier 4 rules engine currently flags this NPC as ill.
    pub is_ill: bool,
    /// Current state description ("Present" or "InTransit -> Dest @HH:MM").
    pub state: String,
    /// Cognitive tier label ("Tier1", "Tier2", etc.).
    pub tier: String,
    /// All schedule variants with active/current indicators.
    pub schedule: Vec<ScheduleVariantDebug>,
    /// Relationships with other NPCs.
    pub relationships: Vec<RelationshipDebug>,
    /// Recent short-term memory entries.
    pub memories: Vec<MemoryDebug>,
    /// Importance-weighted long-term memories.
    pub long_term_memories: Vec<LongTermMemoryDebug>,
    /// Recent player emoji reactions directed at this NPC.
    pub reactions: Vec<ReactionDebug>,
    /// Deflated summary captured at the last tier drop, if any.
    pub deflated_summary: Option<DeflatedSummaryDebug>,
    /// Knowledge entries.
    pub knowledge: Vec<String>,
    /// Intelligence profile dimensions (each 1-5).
    pub intelligence: IntelligenceDebug,
    /// Last Tier 3 batch activity summary, if this NPC has received one.
    pub last_activity: Option<String>,
    /// Whether this NPC knows the player's name.
    pub knows_player_name: bool,
}

/// A long-term memory entry for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct LongTermMemoryDebug {
    /// Formatted game timestamp.
    pub timestamp: String,
    /// What happened.
    pub content: String,
    /// Importance score in [0.0, 1.0].
    pub importance: f32,
    /// Retrieval keywords.
    pub keywords: Vec<String>,
}

/// A player reaction entry for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct ReactionDebug {
    /// Formatted game timestamp.
    pub timestamp: String,
    /// Emoji used.
    pub emoji: String,
    /// Natural-language description (e.g. "looked angry").
    pub description: String,
    /// Truncated context — what the NPC said that was reacted to.
    pub context: String,
}

/// Summary captured when an NPC was deflated to a lower tier.
#[derive(Debug, Clone, Serialize)]
pub struct DeflatedSummaryDebug {
    /// Location name at the time of deflation.
    pub location_name: String,
    /// Mood at the time of deflation.
    pub mood: String,
    /// Short summaries of recent activity.
    pub recent_activity: Vec<String>,
    /// Notable relationship changes since last inflation.
    pub key_relationship_changes: Vec<String>,
}

/// Compact intelligence profile for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct IntelligenceDebug {
    /// Verbal — language fluency, vocabulary, eloquence (1-5).
    pub verbal: u8,
    /// Analytical — logic, reasoning, problem-solving (1-5).
    pub analytical: u8,
    /// Emotional — empathy, reading people, social awareness (1-5).
    pub emotional: u8,
    /// Practical — common sense, hands-on resourcefulness (1-5).
    pub practical: u8,
    /// Wisdom — life experience, judgment, foresight (1-5).
    pub wisdom: u8,
    /// Creative — imagination, wit, improvisation (1-5).
    pub creative: u8,
}

/// A single schedule entry for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct ScheduleEntryDebug {
    /// Start hour (0-23).
    pub start_hour: u8,
    /// End hour (0-23).
    pub end_hour: u8,
    /// Location name for this slot.
    pub location_name: String,
    /// Activity description.
    pub activity: String,
    /// Whether this is the currently active entry right now.
    pub is_current: bool,
}

/// A schedule variant for debug display (one variant = one season/day-type combination).
#[derive(Debug, Clone, Serialize)]
pub struct ScheduleVariantDebug {
    /// Season this variant applies to ("Spring", "Summer", etc.), or null for any season.
    pub season: Option<String>,
    /// Day type this variant applies to ("Weekday", "Sunday", "Market Day"), or null for any.
    pub day_type: Option<String>,
    /// Whether this variant is the one currently in use.
    pub is_active: bool,
    /// Schedule entries for this variant.
    pub entries: Vec<ScheduleEntryDebug>,
}

/// A relationship for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct RelationshipDebug {
    /// Name of the other NPC.
    pub target_name: String,
    /// Relationship kind (e.g. "friend", "family").
    pub kind: String,
    /// Strength from -1.0 to 1.0.
    pub strength: f64,
    /// Number of history entries.
    pub history_count: usize,
    /// Recent history entries (up to 10, newest first).
    pub history: Vec<RelationshipEventDebug>,
}

/// A single relationship history entry for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct RelationshipEventDebug {
    /// Formatted game timestamp.
    pub timestamp: String,
    /// Description of what happened.
    pub description: String,
}

/// A memory entry for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryDebug {
    /// Formatted game timestamp.
    pub timestamp: String,
    /// What happened.
    pub content: String,
    /// Location name where it happened.
    pub location_name: String,
}

/// Tier assignment summary.
#[derive(Debug, Clone, Serialize)]
pub struct TierSummary {
    /// Number of Tier 1 NPCs.
    pub tier1_count: usize,
    /// Number of Tier 2 NPCs.
    pub tier2_count: usize,
    /// Number of Tier 3 NPCs.
    pub tier3_count: usize,
    /// Number of Tier 4 NPCs.
    pub tier4_count: usize,
    /// Names of Tier 1 NPCs (at player's location).
    pub tier1_names: Vec<String>,
    /// Names of Tier 2 NPCs (nearby).
    pub tier2_names: Vec<String>,
    /// Names of Tier 3 NPCs (distant, batch-simulated).
    pub tier3_names: Vec<String>,
    /// Names of Tier 4 NPCs (rules-engine tick).
    pub tier4_names: Vec<String>,
    /// Whether a Tier 3 batch inference is currently in flight.
    pub tier3_in_flight: bool,
    /// Formatted game time of last Tier 2 schedule tick.
    pub last_tier2_tick: Option<String>,
    /// Formatted game time of last Tier 3 batch tick.
    pub last_tier3_tick: Option<String>,
    /// Formatted game time of last Tier 4 rules engine tick.
    pub last_tier4_tick: Option<String>,
    /// Number of NPCs the player has been introduced to.
    pub introduced_count: usize,
    /// Whether a Tier 2 background inference is currently in flight.
    pub tier2_in_flight: bool,
    /// Number of Tier 3 NPCs queued for the next batch dispatch.
    pub tier3_pending_count: usize,
    /// Last ~5 Tier 4 life-event descriptions (newest last).
    pub tier4_recent_events: Vec<String>,
}

/// Event bus + recent event stream for debug display.
#[derive(Debug, Clone, Serialize, Default)]
pub struct EventBusDebug {
    /// Number of active subscribers on the game event bus.
    pub subscriber_count: usize,
    /// Recent `GameEvent`s captured from the bus (newest last).
    pub recent_events: Vec<GameEventDebug>,
}

/// A single game event for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct GameEventDebug {
    /// Formatted game timestamp.
    pub timestamp: String,
    /// Event discriminant name (e.g. "WeatherChanged").
    pub kind: String,
    /// Human-readable event summary.
    pub summary: String,
}

/// Gossip network state for debug display.
#[derive(Debug, Clone, Serialize, Default)]
pub struct GossipDebug {
    /// Total number of gossip items in the network.
    pub item_count: usize,
    /// All gossip items (newest first, capped).
    pub items: Vec<GossipItemDebug>,
}

/// A single gossip item for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct GossipItemDebug {
    /// Unique gossip id.
    pub id: u32,
    /// Current (possibly distorted) content.
    pub content: String,
    /// Name of the original source NPC.
    pub source_name: String,
    /// How many times this item has been distorted (0 = original).
    pub distortion_level: u8,
    /// Names of NPCs who know this gossip.
    pub known_by: Vec<String>,
    /// Formatted game timestamp of creation.
    pub timestamp: String,
}

/// Conversation log state for debug display.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ConversationsDebug {
    /// Total number of stored exchanges.
    pub exchange_count: usize,
    /// All exchanges in chronological order.
    pub exchanges: Vec<ConversationExchangeDebug>,
}

/// A single conversation exchange for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct ConversationExchangeDebug {
    /// Formatted game timestamp.
    pub timestamp: String,
    /// Speaker NPC id.
    pub speaker_id: u32,
    /// Speaker display name.
    pub speaker_name: String,
    /// Location name where the exchange happened.
    pub location_name: String,
    /// What the player said.
    pub player_input: String,
    /// What the NPC replied.
    pub npc_dialogue: String,
}

/// A timestamped debug event for the event log.
#[derive(Debug, Clone, Serialize)]
pub struct DebugEvent {
    /// Formatted game timestamp.
    pub timestamp: String,
    /// Event category: "schedule", "tier", "movement", "encounter", "system".
    pub category: String,
    /// Human-readable description.
    pub message: String,
}

/// Per-role inference configuration shown in the debug panel.
///
/// Mirrors one entry per concrete [`parish_config::InferenceSubrole`]. Provider
/// names display as `(inherits base)` in the UI when `provider` is `None`.
#[derive(Debug, Clone, Serialize)]
pub struct InferenceCategoryDebug {
    /// Stable concrete workload name such as "dialogue" or "tier2-simulation".
    pub role: String,
    /// Provider override for this role; `None` means inherit base.
    pub provider: Option<String>,
    /// Model override for this role; `None` means inherit base model.
    pub model: Option<String>,
    /// Base URL override for this role; `None` means inherit base.
    pub base_url: Option<String>,
    /// Effective Gemini tuning profile for this role.
    pub thinking_level: parish_config::ThinkingLevel,
    pub max_output_tokens: u32,
    pub service_tier: parish_config::ServiceTier,
}

/// Inference pipeline configuration for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct InferenceDebug {
    /// Base provider name (e.g. "ollama").
    pub provider_name: String,
    /// Base model name.
    pub model_name: String,
    /// Base URL.
    pub base_url: String,
    /// Cloud provider name (if configured).
    pub cloud_provider: Option<String>,
    /// Cloud model name (if configured).
    pub cloud_model: Option<String>,
    /// Whether an inference queue is active.
    pub has_queue: bool,
    /// Current value of the reaction request ID counter (monotonic).
    pub reaction_req_id: u64,
    /// Whether improv mode is enabled.
    pub improv_enabled: bool,
    /// Recent inference call log entries (newest last).
    pub call_log: Vec<InferenceLogEntry>,
    /// Per-workload provider/model/url state (one entry per inference subrole).
    /// Each entry's `Option<String>` fields
    /// are `None` when the role inherits from the base config.
    pub categories: Vec<InferenceCategoryDebug>,
    /// List of provider display names that have an API key configured (or are local).
    pub configured_providers: Vec<String>,
    /// Cumulative count of Tier 2 JSON parse failures since process start.
    /// TODO #29 — operator-visible counter so silent off-screen sim drops
    /// (caught only in WARN logs pre-fix) get a number an operator can
    /// trend across a demo run. Per-location detail is still available
    /// via the `parish_npc::ticks=warn` log channel.
    pub tier2_parse_failures_total: u64,
}
