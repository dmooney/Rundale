//! Debug snapshot — serializable aggregate of all game state for debug UIs.
//!
//! Provides a single `DebugSnapshot` struct that captures a point-in-time
//! view of all inspectable game internals. Consumed by both the TUI debug
//! panel and the Tauri/Svelte debug panel via IPC.

use std::collections::VecDeque;

use chrono::{Datelike, Timelike};
use serde::Serialize;

use crate::npc::manager::NpcManager;
use crate::npc::types::{CogTier, NpcState};
use crate::world::WorldState;
use crate::world::graph::WorldGraph;
use crate::world::time::{DayType, Season};

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
    /// Conversation log (player–NPC exchanges).
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
    /// Game-hour cursor of the last transition evaluation, if any.
    pub last_check_hour: Option<i64>,
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

/// Structured emotion state, flattened for IPC / UI consumption.
#[derive(Debug, Clone, Serialize)]
pub struct EmotionDebug {
    /// Short descriptor (e.g. "grieving", "furious") derived from the
    /// dominant family + intensity.
    pub label: String,
    /// Top-3 leaf descriptors from `project_top_k` — richer than the
    /// label, useful for prompt-injection debugging.
    pub top_leaves: Vec<String>,
    /// Family intensities in `[0.0, 1.0]`, keyed by family name
    /// (lowercase: "joy", "sadness", …).
    pub families: std::collections::BTreeMap<String, f32>,
    /// Pleasure dimension of PAD, in `[-1.0, 1.0]`.
    pub pleasure: f32,
    /// Arousal dimension of PAD, in `[-1.0, 1.0]`.
    pub arousal: f32,
    /// Dominance dimension of PAD, in `[-1.0, 1.0]`.
    pub dominance: f32,
    /// Active behavioural gate names (e.g. "panic_truth",
    /// "withdraws_silent"). Empty when no gate fires.
    pub active_gates: Vec<String>,
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
    /// Structured emotion state — family intensities, PAD coordinates,
    /// and active behavioural gates. Populated from
    /// [`parish_types::EmotionState`] so frontends can render bars
    /// and tooltips without re-deriving from the mood string.
    pub emotion: EmotionDebug,
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
    /// Intelligence profile dimensions (each 1–5).
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
    /// Verbal — language fluency, vocabulary, eloquence (1–5).
    pub verbal: u8,
    /// Analytical — logic, reasoning, problem-solving (1–5).
    pub analytical: u8,
    /// Emotional — empathy, reading people, social awareness (1–5).
    pub emotional: u8,
    /// Practical — common sense, hands-on resourcefulness (1–5).
    pub practical: u8,
    /// Wisdom — life experience, judgment, foresight (1–5).
    pub wisdom: u8,
    /// Creative — imagination, wit, improvisation (1–5).
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
/// Mirrors one entry per [`crate::config::InferenceCategory`]. Provider
/// names display as `(inherits base)` in the UI when `provider` is `None`.
#[derive(Debug, Clone, Serialize)]
pub struct InferenceCategoryDebug {
    /// Lowercase role name: "dialogue", "simulation", "intent", "reaction".
    pub role: String,
    /// Provider override for this role; `None` means inherit base.
    pub provider: Option<String>,
    /// Model override for this role; `None` means inherit base model.
    pub model: Option<String>,
    /// Base URL override for this role; `None` means inherit base.
    pub base_url: Option<String>,
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
    /// Per-role provider/model/url state (always 4 entries: Dialogue,
    /// Simulation, Intent, Reaction). Each entry's `Option<String>` fields
    /// are `None` when the role inherits from the base config.
    pub categories: Vec<InferenceCategoryDebug>,
    /// List of provider display names that have an API key configured (or are local).
    pub configured_providers: Vec<String>,
}

/// Re-export from parish-inference so callers don't need a separate import.
pub use crate::inference::InferenceLogEntry;

/// Returns a list of provider display names that are ready to use
/// (either local providers, or cloud providers with an API key set).
pub fn build_configured_providers() -> Vec<String> {
    crate::config::Provider::ALL
        .iter()
        .filter(|p| p.is_configured_in_env())
        .map(|p| {
            crate::config::ProviderConfig {
                provider: p.clone(),
                base_url: String::new(),
                api_key: None,
                model: None,
            }
            .provider_display()
        })
        .collect()
}

/// Builds the per-role debug entries from a [`crate::ipc::config::GameConfig`].
///
/// Always returns 4 entries in [`crate::config::InferenceCategory::ALL`] order,
/// so the UI can render a stable table without conditional rows.
pub fn build_inference_categories(
    config: &crate::ipc::config::GameConfig,
) -> Vec<InferenceCategoryDebug> {
    use crate::config::InferenceCategory;
    use crate::ipc::config::GameConfig;
    InferenceCategory::ALL
        .iter()
        .map(|cat| {
            let idx = GameConfig::cat_idx(*cat);
            InferenceCategoryDebug {
                role: cat.name().to_string(),
                provider: config.category_provider[idx].clone(),
                model: config.category_model[idx].clone(),
                base_url: config.category_base_url[idx].clone(),
            }
        })
        .collect()
}

/// Builds a complete debug snapshot from live game state.
///
/// Pure query function — reads but never mutates any state.
/// The `events` parameter is a ring buffer of recent debug events
/// maintained by the caller (TUI App or Tauri AppState).
/// The `game_events` parameter is an optional ring buffer of recent
/// `GameEvent`s captured from the world event bus by the caller.
pub fn build_debug_snapshot(
    world: &WorldState,
    npc_manager: &NpcManager,
    events: &VecDeque<DebugEvent>,
    game_events: &VecDeque<crate::world::events::GameEvent>,
    inference: &InferenceDebug,
    auth: &AuthDebug,
) -> DebugSnapshot {
    let clock = build_clock_debug(world);
    let weather = build_weather_debug(world);
    let world_debug = build_world_debug(world, npc_manager);
    let current_hour = world.clock.now().hour() as u8;
    let current_season = world.clock.season();
    let current_day_type = world.clock.day_type();
    let npcs = build_npc_debug_list(
        npc_manager,
        &world.graph,
        current_hour,
        current_season,
        current_day_type,
    );
    let tier_summary = build_tier_summary(npc_manager);
    let gossip = build_gossip_debug(world, npc_manager);
    let event_list: Vec<DebugEvent> = events.iter().cloned().collect();
    let event_bus = build_event_bus_debug(world, game_events, npc_manager);
    let conversations = build_conversations_debug(world);

    DebugSnapshot {
        clock,
        weather,
        world: world_debug,
        npcs,
        tier_summary,
        event_bus,
        gossip,
        conversations,
        events: event_list,
        inference: inference.clone(),
        auth: auth.clone(),
    }
}

/// Builds a debug view of an NPC's structured emotion state.
fn build_emotion_debug(state: &parish_types::EmotionState) -> EmotionDebug {
    use parish_types::EmotionFamily;

    let f = &state.families;
    let families: std::collections::BTreeMap<String, f32> = EmotionFamily::ALL
        .iter()
        .map(|fam| (format!("{:?}", fam).to_lowercase(), f.get(*fam)))
        .collect();

    let top_leaves = parish_types::project_top_k(state, 3)
        .into_iter()
        .map(|l| l.word.to_string())
        .collect();

    let gates = state.gates();
    let mut active_gates = Vec::new();
    if gates.panic_truth {
        active_gates.push("panic_truth".to_string());
    }
    if gates.public_outburst {
        active_gates.push("public_outburst".to_string());
    }
    if gates.withdraws_silent {
        active_gates.push("withdraws_silent".to_string());
    }
    if gates.effusive {
        active_gates.push("effusive".to_string());
    }

    EmotionDebug {
        label: state.label().to_string(),
        top_leaves,
        families,
        pleasure: state.pleasure,
        arousal: state.arousal,
        dominance: state.dominance,
        active_gates,
    }
}

/// Builds clock debug info from world state.
fn build_clock_debug(world: &WorldState) -> ClockDebug {
    let now = world.clock.now();
    let day_of_week = match now.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    }
    .to_string();

    ClockDebug {
        game_time: format!(
            "{:02}:{:02} {}",
            now.hour(),
            now.minute(),
            now.format("%Y-%m-%d")
        ),
        time_of_day: world.clock.time_of_day().to_string(),
        season: world.clock.season().to_string(),
        festival: world.clock.check_festival().map(|f| f.to_string()),
        weather: world.weather.to_string(),
        paused: world.clock.is_paused(),
        inference_paused: world.clock.is_inference_paused(),
        speed_factor: world.clock.speed_factor(),
        speed_name: world.clock.current_speed().map(|s| s.to_string()),
        day_of_week,
        day_type: world.clock.day_type().to_string(),
        start_game_time: world
            .clock
            .start_game()
            .format("%H:%M %Y-%m-%d")
            .to_string(),
        paused_game_time: world
            .clock
            .paused_game_time()
            .format("%H:%M %Y-%m-%d")
            .to_string(),
        real_elapsed_secs: world.clock.real_elapsed_secs(),
    }
}

/// Builds weather engine debug info from world state.
fn build_weather_debug(world: &WorldState) -> WeatherDebug {
    let now = world.clock.now();
    WeatherDebug {
        current: world.weather_engine.current().to_string(),
        since: world
            .weather_engine
            .since()
            .format("%H:%M %Y-%m-%d")
            .to_string(),
        duration_hours: world.weather_engine.duration_hours(now),
        min_duration_hours: world.weather_engine.min_duration_hours(),
        last_check_hour: world.weather_engine.last_check_hour(),
    }
}

/// Builds event bus debug info from the captured game-event ring buffer.
fn build_event_bus_debug(
    world: &WorldState,
    game_events: &VecDeque<crate::world::events::GameEvent>,
    npc_manager: &NpcManager,
) -> EventBusDebug {
    use crate::world::events::GameEvent;

    let name_of = |id: crate::npc::NpcId| -> String {
        npc_manager
            .get(id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("NPC({})", id.0))
    };
    let loc_of = |id: crate::world::LocationId| -> String { loc_name(id, &world.graph) };

    let recent_events: Vec<GameEventDebug> = game_events
        .iter()
        .map(|e| {
            let timestamp = e.timestamp().format("%H:%M %Y-%m-%d").to_string();
            let kind = e.event_type().to_string();
            let summary = match e {
                GameEvent::DialogueOccurred {
                    npc_id, summary, ..
                } => format!("{}: {}", name_of(*npc_id), summary),
                GameEvent::MoodChanged {
                    npc_id, new_mood, ..
                } => format!("{} → {}", name_of(*npc_id), new_mood),
                GameEvent::RelationshipChanged {
                    npc_a,
                    npc_b,
                    delta,
                    ..
                } => format!("{} ↔ {} ({:+.2})", name_of(*npc_a), name_of(*npc_b), delta),
                GameEvent::NpcArrived {
                    npc_id, location, ..
                } => format!("{} arrived at {}", name_of(*npc_id), loc_of(*location)),
                GameEvent::NpcDeparted {
                    npc_id, location, ..
                } => format!("{} departed from {}", name_of(*npc_id), loc_of(*location)),
                GameEvent::WeatherChanged { new_weather, .. } => {
                    format!("Weather: {}", new_weather)
                }
                GameEvent::FestivalStarted { name, .. } => format!("Festival: {}", name),
                GameEvent::LifeEvent {
                    npc_id,
                    description,
                    ..
                } => format!("{}: {}", name_of(*npc_id), description),
                GameEvent::EmotionChanged {
                    npc_id,
                    family,
                    delta,
                    cause,
                    ..
                } => format!(
                    "{} {:?} {:+.2} ({})",
                    name_of(*npc_id),
                    family,
                    delta,
                    cause
                ),
            };
            GameEventDebug {
                timestamp,
                kind,
                summary,
            }
        })
        .collect();

    EventBusDebug {
        subscriber_count: world.event_bus.subscriber_count(),
        recent_events,
    }
}

/// Builds gossip network debug info.
fn build_gossip_debug(world: &WorldState, npc_manager: &NpcManager) -> GossipDebug {
    let name_of = |id: crate::npc::NpcId| -> String {
        npc_manager
            .get(id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("NPC({})", id.0))
    };

    let mut items: Vec<GossipItemDebug> = world
        .gossip_network
        .all_items()
        .iter()
        .map(|item| {
            let mut known_names: Vec<String> =
                item.known_by.iter().map(|id| name_of(*id)).collect();
            known_names.sort();
            GossipItemDebug {
                id: item.id,
                content: item.content.clone(),
                source_name: name_of(item.source),
                distortion_level: item.distortion_level,
                known_by: known_names,
                timestamp: item.timestamp.format("%H:%M %Y-%m-%d").to_string(),
            }
        })
        .collect();
    // Newest first
    items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    GossipDebug {
        item_count: world.gossip_network.len(),
        items,
    }
}

/// Builds the conversation log debug view.
fn build_conversations_debug(world: &WorldState) -> ConversationsDebug {
    let exchanges: Vec<ConversationExchangeDebug> = world
        .conversation_log
        .all()
        .map(|e| ConversationExchangeDebug {
            timestamp: e.timestamp.format("%H:%M %Y-%m-%d").to_string(),
            speaker_id: e.speaker_id.0,
            speaker_name: e.speaker_name.clone(),
            location_name: loc_name(e.location, &world.graph),
            player_input: e.player_input.clone(),
            npc_dialogue: e.npc_dialogue.clone(),
        })
        .collect();
    ConversationsDebug {
        exchange_count: world.conversation_log.len(),
        exchanges,
    }
}

/// Walking speed used to compute debug edge travel times (meters/second).
///
/// The real walking speed lives in each mod's transport config but is not
/// threaded through to the debug snapshot — we use a canonical fallback
/// that matches the default "on foot" preset so the panel can surface a
/// representative travel time per edge.
const DEBUG_WALKING_SPEED_M_PER_S: f64 = 1.25;

/// Maximum number of text-log lines included in the snapshot.
const TEXT_LOG_TAIL_LEN: usize = 50;

/// Builds world debug info including per-location NPC presence.
fn build_world_debug(world: &WorldState, npc_manager: &NpcManager) -> WorldDebug {
    let player_loc_name = world
        .graph
        .get(world.player_location)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| format!("Location({})", world.player_location.0));

    let mut locations: Vec<LocationDebug> = Vec::new();
    for loc_id in world.graph.location_ids() {
        if let Some(data) = world.graph.get(loc_id) {
            let npcs_here: Vec<String> = npc_manager
                .npcs_at(loc_id)
                .iter()
                .map(|n| n.name.clone())
                .collect();
            let edges: Vec<GraphEdgeDebug> = data
                .connections
                .iter()
                .map(|c| GraphEdgeDebug {
                    target_id: c.target.0,
                    target_name: loc_name(c.target, &world.graph),
                    path_description: c.path_description.clone(),
                    walking_minutes: world.graph.edge_travel_minutes(
                        loc_id,
                        c.target,
                        DEBUG_WALKING_SPEED_M_PER_S,
                    ),
                })
                .collect();
            locations.push(LocationDebug {
                id: loc_id.0,
                name: data.name.clone(),
                indoor: data.indoor,
                public: data.public,
                connection_count: data.connections.len(),
                npcs_here,
                visited: world.visited_locations.contains(&loc_id),
                edges,
            });
        }
    }
    locations.sort_by_key(|l| l.id);

    let mut visited_locations: Vec<String> = world
        .visited_locations
        .iter()
        .filter_map(|id| world.graph.get(*id).map(|d| d.name.clone()))
        .collect();
    visited_locations.sort();

    let mut edge_traversals: Vec<EdgeTraversalDebug> = world
        .edge_traversals
        .iter()
        .map(|((a, b), count)| EdgeTraversalDebug {
            from_name: loc_name(*a, &world.graph),
            to_name: loc_name(*b, &world.graph),
            count: *count,
        })
        .collect();
    edge_traversals.sort_by_key(|edge| std::cmp::Reverse(edge.count));

    let text_log_len = world.text_log.len();
    let text_log_tail: Vec<String> = world
        .text_log
        .iter()
        .rev()
        .take(TEXT_LOG_TAIL_LEN)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    WorldDebug {
        player_location_name: player_loc_name,
        player_location_id: world.player_location.0,
        location_count: world.graph.location_count(),
        visited_count: world.visited_locations.len(),
        visited_locations,
        edge_traversals,
        text_log_tail,
        text_log_len,
        locations,
        player_name: world.player_name.clone(),
    }
}

/// Resolves a location name from the world graph.
fn loc_name(id: crate::world::LocationId, graph: &WorldGraph) -> String {
    graph
        .get(id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| format!("Location({})", id.0))
}

/// Builds the NPC debug list with full deep-dive data.
fn build_npc_debug_list(
    npc_manager: &NpcManager,
    graph: &WorldGraph,
    current_hour: u8,
    current_season: Season,
    current_day_type: DayType,
) -> Vec<NpcDebug> {
    let mut npcs: Vec<NpcDebug> = npc_manager
        .all_npcs()
        .map(|npc| {
            let tier = npc_manager
                .tier_of(npc.id)
                .map(|t| format!("{:?}", t))
                .unwrap_or_else(|| "Unassigned".to_string());

            let state_str = match &npc.state {
                NpcState::Present => "Present".to_string(),
                NpcState::InTransit { to, arrives_at, .. } => {
                    let dest = loc_name(*to, graph);
                    format!(
                        "InTransit -> {} @{:02}:{:02}",
                        dest,
                        arrives_at.hour(),
                        arrives_at.minute()
                    )
                }
            };

            let schedule: Vec<ScheduleVariantDebug> = npc
                .schedule
                .as_ref()
                .map(|s| {
                    // Determine which variant is currently active
                    let active_entries = s.resolve(current_season, current_day_type);
                    s.variants
                        .iter()
                        .map(|v| {
                            let is_active =
                                active_entries.is_some_and(|ae| std::ptr::eq(ae, &v.entries[..]));
                            let entries = v
                                .entries
                                .iter()
                                .map(|e| {
                                    let is_current = is_active
                                        && current_hour >= e.start_hour
                                        && current_hour <= e.end_hour;
                                    ScheduleEntryDebug {
                                        start_hour: e.start_hour,
                                        end_hour: e.end_hour,
                                        location_name: loc_name(e.location, graph),
                                        activity: e.activity.clone(),
                                        is_current,
                                    }
                                })
                                .collect();
                            ScheduleVariantDebug {
                                season: v.season.map(|s| s.to_string()),
                                day_type: v.day_type.map(|d| d.to_string()),
                                is_active,
                                entries,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            let mut relationships: Vec<RelationshipDebug> = npc
                .relationships
                .iter()
                .map(|(target_id, rel)| {
                    let target_name = npc_manager
                        .get(*target_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| format!("NPC({})", target_id.0));
                    // Newest-first, cap at 10 entries
                    let mut history: Vec<RelationshipEventDebug> = rel
                        .history
                        .iter()
                        .rev()
                        .take(10)
                        .map(|e| RelationshipEventDebug {
                            timestamp: e.timestamp.format("%H:%M %Y-%m-%d").to_string(),
                            description: e.description.clone(),
                        })
                        .collect();
                    history.reverse(); // Back to oldest-first for display stability
                    RelationshipDebug {
                        target_name,
                        kind: rel.kind.to_string(),
                        strength: rel.strength,
                        history_count: rel.history.len(),
                        history,
                    }
                })
                .collect();
            relationships.sort_by(|a, b| {
                b.strength
                    .partial_cmp(&a.strength)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let memories: Vec<MemoryDebug> = npc
                .memory
                .recent(10)
                .iter()
                .map(|m| MemoryDebug {
                    timestamp: m.timestamp.format("%H:%M %Y-%m-%d").to_string(),
                    content: m.content.clone(),
                    location_name: loc_name(m.location, graph),
                })
                .collect();

            let long_term_memories: Vec<LongTermMemoryDebug> = npc
                .long_term_memory
                .all_entries()
                .iter()
                .map(|e| LongTermMemoryDebug {
                    timestamp: e.timestamp.format("%H:%M %Y-%m-%d").to_string(),
                    content: e.content.clone(),
                    importance: e.importance,
                    keywords: e.keywords.clone(),
                })
                .collect();

            let reactions: Vec<ReactionDebug> = npc
                .reaction_log
                .entries()
                .rev()
                .map(|r| ReactionDebug {
                    timestamp: r.timestamp.format("%H:%M %Y-%m-%d").to_string(),
                    emoji: r.emoji.clone(),
                    description: r.description.clone(),
                    context: r.context.clone(),
                })
                .collect();

            let deflated_summary = npc.deflated_summary.as_ref().map(|s| DeflatedSummaryDebug {
                location_name: loc_name(s.location, graph),
                mood: s.mood.clone(),
                recent_activity: s.recent_activity.clone(),
                key_relationship_changes: s.key_relationship_changes.clone(),
            });

            NpcDebug {
                id: npc.id.0,
                name: npc.name.clone(),
                brief_description: npc.brief_description.clone(),
                introduced: npc_manager.is_introduced(npc.id),
                age: npc.age,
                occupation: npc.occupation.clone(),
                personality: npc.personality.clone(),
                location_name: loc_name(npc.location, graph),
                location_id: npc.location.0,
                home_name: npc.home.map(|h| loc_name(h, graph)),
                workplace_name: npc.workplace.map(|w| loc_name(w, graph)),
                mood: npc.mood.clone(),
                emotion: build_emotion_debug(&npc.emotion),
                is_ill: npc.is_ill,
                state: state_str,
                tier,
                schedule,
                relationships,
                memories,
                long_term_memories,
                reactions,
                deflated_summary,
                knowledge: npc.knowledge.clone(),
                intelligence: IntelligenceDebug {
                    verbal: npc.intelligence.verbal,
                    analytical: npc.intelligence.analytical,
                    emotional: npc.intelligence.emotional,
                    practical: npc.intelligence.practical,
                    wisdom: npc.intelligence.wisdom,
                    creative: npc.intelligence.creative,
                },
                last_activity: npc.last_activity.clone(),
                knows_player_name: npc_manager.knows_player_name(npc.id),
            }
        })
        .collect();

    // Sort by tier (Tier1 first), then by name
    npcs.sort_by(|a, b| a.tier.cmp(&b.tier).then(a.name.cmp(&b.name)));
    npcs
}

/// Builds tier summary counts and name lists.
fn build_tier_summary(npc_manager: &NpcManager) -> TierSummary {
    let mut t1 = Vec::new();
    let mut t2 = Vec::new();
    let mut t3: Vec<String> = Vec::new();
    let mut t4: Vec<String> = Vec::new();

    for npc in npc_manager.all_npcs() {
        match npc_manager.tier_of(npc.id) {
            Some(CogTier::Tier1) => t1.push(npc.name.clone()),
            Some(CogTier::Tier2) => t2.push(npc.name.clone()),
            Some(CogTier::Tier3) | None => t3.push(npc.name.clone()),
            Some(CogTier::Tier4) => t4.push(npc.name.clone()),
        }
    }

    let fmt_tick = |t: chrono::DateTime<chrono::Utc>| t.format("%H:%M %Y-%m-%d").to_string();
    let last_tier2_tick = npc_manager.last_tier2_game_time().map(fmt_tick);
    let last_tier3_tick = npc_manager.last_tier3_game_time().map(fmt_tick);
    let last_tier4_tick = npc_manager.last_tier4_game_time().map(fmt_tick);

    let tier3_pending_count = t3.len();
    let tier4_recent_events: Vec<String> =
        npc_manager.recent_tier4_events().iter().cloned().collect();

    TierSummary {
        tier1_count: t1.len(),
        tier2_count: t2.len(),
        tier3_count: t3.len(),
        tier4_count: t4.len(),
        tier1_names: t1,
        tier2_names: t2,
        tier3_names: t3,
        tier4_names: t4,
        tier3_in_flight: npc_manager.tier3_in_flight(),
        last_tier2_tick,
        last_tier3_tick,
        last_tier4_tick,
        introduced_count: npc_manager.introduced_count(),
        tier2_in_flight: npc_manager.tier2_in_flight(),
        tier3_pending_count,
        tier4_recent_events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::{Npc, NpcId};
    use crate::world::events::GameEvent;
    use std::collections::VecDeque;

    /// Helper: build a minimal `InferenceDebug` for tests.
    fn test_inference() -> InferenceDebug {
        InferenceDebug {
            provider_name: "ollama".to_string(),
            model_name: "test-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            cloud_provider: None,
            cloud_model: None,
            has_queue: false,
            reaction_req_id: 100_000,
            improv_enabled: false,
            call_log: vec![],
            categories: vec![],
            configured_providers: vec![],
        }
    }

    #[test]
    fn test_build_debug_snapshot_empty() {
        let world = WorldState::new();
        let npc_manager = NpcManager::new();
        let events = VecDeque::new();
        let game_events: VecDeque<GameEvent> = VecDeque::new();
        let inference = test_inference();

        let snapshot = build_debug_snapshot(
            &world,
            &npc_manager,
            &events,
            &game_events,
            &inference,
            &AuthDebug::disabled(),
        );

        assert!(snapshot.clock.game_time.contains("08:00"));
        assert_eq!(snapshot.clock.weather, "Clear");
        assert!(!snapshot.clock.paused);
        assert!(!snapshot.clock.inference_paused);
        assert_eq!(snapshot.weather.current, "Clear");
        assert!(snapshot.npcs.is_empty());
        assert_eq!(snapshot.tier_summary.tier1_count, 0);
        assert_eq!(snapshot.inference.provider_name, "ollama");
        assert_eq!(snapshot.gossip.item_count, 0);
        assert_eq!(snapshot.conversations.exchange_count, 0);
    }

    #[test]
    fn test_build_debug_snapshot_with_npc() {
        let world = WorldState::new();
        let mut npc_manager = NpcManager::new();
        npc_manager.add_npc(Npc::new_test_npc());
        npc_manager.assign_tiers(&world, &[]);

        let events = VecDeque::new();
        let game_events: VecDeque<GameEvent> = VecDeque::new();
        let mut inference = test_inference();
        inference.has_queue = true;

        let snapshot = build_debug_snapshot(
            &world,
            &npc_manager,
            &events,
            &game_events,
            &inference,
            &AuthDebug::disabled(),
        );

        assert_eq!(snapshot.npcs.len(), 1);
        assert_eq!(snapshot.npcs[0].name, "Padraig O'Brien");
        assert_eq!(snapshot.npcs[0].mood, "content");
        assert_eq!(snapshot.npcs[0].state, "Present");
        assert!(!snapshot.npcs[0].introduced);
        assert_eq!(
            snapshot.npcs[0].brief_description,
            "an older man behind the bar"
        );
        // Intelligence matches new_test_npc: Intelligence::new(3, 3, 4, 4, 5, 4)
        let intel = &snapshot.npcs[0].intelligence;
        assert_eq!(intel.verbal, 3);
        assert_eq!(intel.analytical, 3);
        assert_eq!(intel.emotional, 4);
        assert_eq!(intel.practical, 4);
        assert_eq!(intel.wisdom, 5);
        assert_eq!(intel.creative, 4);
    }

    #[test]
    fn test_build_clock_debug() {
        let world = WorldState::new();
        let clock = build_clock_debug(&world);

        assert!(clock.game_time.contains("08:00"));
        assert_eq!(clock.time_of_day, "Morning");
        assert_eq!(clock.season, "Spring");
        assert_eq!(clock.weather, "Clear");
        assert!(!clock.paused);
    }

    #[test]
    fn test_build_tier_summary_empty() {
        let mgr = NpcManager::new();
        let summary = build_tier_summary(&mgr);
        assert_eq!(summary.tier1_count, 0);
        assert_eq!(summary.tier2_count, 0);
        assert_eq!(summary.tier3_count, 0);
        assert_eq!(summary.tier4_count, 0);
    }

    #[test]
    fn test_build_tier_summary_with_npcs() {
        let world = WorldState::new();
        let mut mgr = NpcManager::new();
        mgr.add_npc(Npc::new_test_npc());
        mgr.assign_tiers(&world, &[]);

        let summary = build_tier_summary(&mgr);
        // Test NPC is at LocationId(1) = player location = Tier1
        assert_eq!(summary.tier1_count, 1);
        assert!(summary.tier1_names.contains(&"Padraig O'Brien".to_string()));
    }

    #[test]
    fn test_build_world_debug() {
        let world = WorldState::new();
        let mgr = NpcManager::new();
        let w = build_world_debug(&world, &mgr);

        assert_eq!(w.player_location_id, 1);
        assert!(!w.player_location_name.is_empty());
    }

    #[test]
    fn test_debug_event_serialize() {
        let event = DebugEvent {
            timestamp: "08:00 1820-03-20".to_string(),
            category: "system".to_string(),
            message: "Game started".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Game started"));
        assert!(json.contains("system"));
    }

    #[test]
    fn test_inference_log_entry_serialize() {
        let entry = InferenceLogEntry {
            request_id: 42,
            timestamp: "14:32:05".to_string(),
            model: "qwen3:14b".to_string(),
            streaming: true,
            duration_ms: 1250,
            prompt_len: 500,
            response_len: 200,
            error: None,
            system_prompt: Some("You are helpful.".to_string()),
            prompt_text: "Hello world".to_string(),
            response_text: "Hi there!".to_string(),
            max_tokens: Some(300),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("qwen3:14b"));
        assert!(json.contains("1250"));
        assert!(json.contains("\"streaming\":true"));
    }

    #[test]
    fn test_inference_log_entry_with_error() {
        let entry = InferenceLogEntry {
            request_id: 7,
            timestamp: "09:00:00".to_string(),
            model: "test".to_string(),
            streaming: false,
            duration_ms: 30000,
            prompt_len: 100,
            response_len: 0,
            error: Some("timeout".to_string()),
            system_prompt: None,
            prompt_text: "test prompt".to_string(),
            response_text: String::new(),
            max_tokens: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("timeout"));
        assert!(json.contains("\"response_len\":0"));
    }

    #[test]
    fn test_call_log_included_in_snapshot() {
        let world = WorldState::new();
        let mgr = NpcManager::new();
        let events = VecDeque::new();
        let game_events: VecDeque<GameEvent> = VecDeque::new();
        let entry = InferenceLogEntry {
            request_id: 1,
            timestamp: "10:00:00".to_string(),
            model: "test-model".to_string(),
            streaming: true,
            duration_ms: 500,
            prompt_len: 100,
            response_len: 50,
            error: None,
            system_prompt: None,
            prompt_text: "test".to_string(),
            response_text: "response".to_string(),
            max_tokens: None,
        };
        let mut inference = test_inference();
        inference.call_log = vec![entry];

        let snapshot = build_debug_snapshot(
            &world,
            &mgr,
            &events,
            &game_events,
            &inference,
            &AuthDebug::disabled(),
        );
        assert_eq!(snapshot.inference.call_log.len(), 1);
        assert_eq!(snapshot.inference.call_log[0].request_id, 1);
        assert_eq!(snapshot.inference.call_log[0].duration_ms, 500);
    }

    #[test]
    fn test_events_included_in_snapshot() {
        let world = WorldState::new();
        let mgr = NpcManager::new();
        let mut events = VecDeque::new();
        let game_events: VecDeque<GameEvent> = VecDeque::new();
        events.push_back(DebugEvent {
            timestamp: "08:00".to_string(),
            category: "system".to_string(),
            message: "Test event".to_string(),
        });
        events.push_back(DebugEvent {
            timestamp: "08:05".to_string(),
            category: "schedule".to_string(),
            message: "NPC moved".to_string(),
        });
        let inference = test_inference();

        let snapshot = build_debug_snapshot(
            &world,
            &mgr,
            &events,
            &game_events,
            &inference,
            &AuthDebug::disabled(),
        );
        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.events[0].message, "Test event");
        assert_eq!(snapshot.events[1].category, "schedule");
    }

    #[test]
    fn test_npc_debug_relationships_sorted() {
        use crate::npc::types::{Relationship, RelationshipKind};

        let mut npc = Npc::new_test_npc();
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.8));
        npc.relationships
            .insert(NpcId(3), Relationship::new(RelationshipKind::Rival, -0.3));

        let mut mgr = NpcManager::new();
        mgr.add_npc(npc);

        let graph = WorldGraph::new();
        let npcs = build_npc_debug_list(&mgr, &graph, 10, Season::Spring, DayType::Weekday);
        assert_eq!(npcs.len(), 1);
        // Relationships should be sorted by strength descending
        assert_eq!(npcs[0].relationships.len(), 2);
        assert!(npcs[0].relationships[0].strength > npcs[0].relationships[1].strength);
    }

    #[test]
    fn test_npc_debug_new_fields() {
        let world = WorldState::new();
        let mut mgr = NpcManager::new();
        let npc = Npc::new_test_npc();
        mgr.add_npc(npc);
        mgr.assign_tiers(&world, &[]);

        let graph = WorldGraph::new();
        let npcs = build_npc_debug_list(&mgr, &graph, 10, Season::Spring, DayType::Weekday);
        assert_eq!(npcs.len(), 1);
        // New fields: is_ill should be false for a healthy NPC
        assert!(!npcs[0].is_ill);
        // No deflated summary on a fresh NPC
        assert!(npcs[0].deflated_summary.is_none());
        // Long-term memory starts empty
        assert!(npcs[0].long_term_memories.is_empty());
    }

    #[test]
    fn test_tier_summary_new_fields() {
        let mgr = NpcManager::new();
        let summary = build_tier_summary(&mgr);
        // New fields: defaults
        assert!(!summary.tier2_in_flight);
        assert!(summary.last_tier2_tick.is_none());
        assert_eq!(summary.tier3_pending_count, 0);
        assert!(summary.tier4_recent_events.is_empty());
    }

    #[test]
    fn test_gossip_debug_empty() {
        let world = WorldState::new();
        let mgr = NpcManager::new();
        let g = build_gossip_debug(&world, &mgr);
        assert_eq!(g.item_count, 0);
        assert!(g.items.is_empty());
    }

    #[test]
    fn test_gossip_debug_serializes_in_snapshot() {
        let world = WorldState::new();
        let mgr = NpcManager::new();
        let events = VecDeque::new();
        let game_events: VecDeque<GameEvent> = VecDeque::new();
        let inference = InferenceDebug {
            provider_name: "test".to_string(),
            model_name: "test".to_string(),
            base_url: "http://localhost".to_string(),
            cloud_provider: None,
            cloud_model: None,
            has_queue: false,
            reaction_req_id: 0,
            improv_enabled: false,
            call_log: vec![],
            categories: vec![],
            configured_providers: vec![],
        };
        let snapshot = build_debug_snapshot(
            &world,
            &mgr,
            &events,
            &game_events,
            &inference,
            &AuthDebug::disabled(),
        );
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("gossip"));
        assert!(json.contains("item_count"));
    }

    #[test]
    fn test_recent_tier4_events_in_tier_summary() {
        use crate::npc::tier4::Tier4Event;
        use chrono::Utc;

        let world = WorldState::new();
        let mut mgr = NpcManager::new();
        let npc = Npc::new_test_npc();
        let npc_id = npc.id;
        mgr.add_npc(npc);
        mgr.assign_tiers(&world, &[]);

        // Apply an Illness event — should populate ring buffer
        let events = vec![Tier4Event::Illness { npc_id }];
        mgr.apply_tier4_events(&events, Utc::now(), true);

        let summary = build_tier_summary(&mgr);
        assert_eq!(summary.tier4_recent_events.len(), 1);
        assert!(summary.tier4_recent_events[0].contains("ill"));
    }
}
