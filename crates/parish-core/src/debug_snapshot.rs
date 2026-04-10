//! Debug snapshot — serializable aggregate of all game state for debug UIs.
//!
//! Provides a single `DebugSnapshot` struct that captures a point-in-time
//! view of all inspectable game internals. Consumed by both the TUI debug
//! panel and the Tauri/Svelte debug panel via IPC.

use std::collections::VecDeque;

use chrono::{DateTime, Datelike, Timelike, Utc};
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
    /// World graph and player position.
    pub world: WorldDebug,
    /// Full NPC state for every NPC.
    pub npcs: Vec<NpcDebug>,
    /// Tier assignment summary.
    pub tier_summary: TierSummary,
    /// Recent debug events.
    pub events: Vec<DebugEvent>,
    /// Inference pipeline configuration.
    pub inference: InferenceDebug,
    /// Gossip network summary.
    pub gossip: GossipDebug,
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
    /// Whether the clock is paused.
    pub paused: bool,
    /// Clock speed multiplier.
    pub speed_factor: f64,
    /// Full day-of-week name (e.g. "Monday").
    pub day_of_week: String,
    /// Schedule day type label (e.g. "Weekday", "Sunday", "Market Day").
    pub day_type: String,
    /// Last ~5 weather transitions as (timestamp, weather_label) pairs.
    pub weather_recent: Vec<(DateTime<Utc>, String)>,
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
    /// Per-location debug info.
    pub locations: Vec<LocationDebug>,
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
}

/// Full NPC state for deep-dive inspection.
#[derive(Debug, Clone, Serialize)]
pub struct NpcDebug {
    /// NPC ID.
    pub id: u32,
    /// Full name.
    pub name: String,
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
    /// Current state description ("Present" or "InTransit -> Dest @HH:MM").
    pub state: String,
    /// Cognitive tier label ("Tier1", "Tier2", etc.).
    pub tier: String,
    /// All schedule variants with active/current indicators.
    pub schedule: Vec<ScheduleVariantDebug>,
    /// Relationships with other NPCs.
    pub relationships: Vec<RelationshipDebug>,
    /// Recent memory entries.
    pub memories: Vec<MemoryDebug>,
    /// Knowledge entries.
    pub knowledge: Vec<String>,
    /// Intelligence profile dimensions (each 1–5).
    pub intelligence: IntelligenceDebug,
    /// Last Tier 3 batch activity summary, if this NPC has received one.
    pub last_activity: Option<String>,
    /// Whether the NPC is currently ill.
    pub is_ill: bool,
    /// Most recent activity from the deflated summary, if any.
    pub deflated_summary: Option<String>,
    /// Number of long-term memory entries.
    pub long_term_memory_count: usize,
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
    /// Whether a Tier 3 batch inference is currently in flight.
    pub tier3_in_flight: bool,
    /// Formatted game time of last Tier 3 batch tick, or null if never run.
    pub last_tier3_tick: Option<String>,
    /// Whether a Tier 2 background inference is currently in flight.
    pub tier2_in_flight: bool,
    /// Formatted game time of last Tier 2 tick, or null if never run.
    pub last_tier2_tick: Option<String>,
    /// Number of Tier 3 NPCs queued for the next batch dispatch.
    pub tier3_pending_count: usize,
    /// Last ~5 Tier 4 life-event descriptions (newest last).
    pub tier4_recent_events: Vec<String>,
}

/// Gossip network summary for debug display.
#[derive(Debug, Clone, Serialize)]
pub struct GossipDebug {
    /// Total number of gossip items in circulation.
    pub rumor_count: usize,
    /// Number of NPCs who know at least one piece of gossip.
    pub recent_witnesses: usize,
    /// Last ~5 rumor texts.
    pub top_rumors: Vec<String>,
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
    /// Whether improv mode is enabled.
    pub improv_enabled: bool,
    /// Recent inference call log entries (newest last).
    pub call_log: Vec<InferenceLogEntry>,
}

/// Re-export from parish-inference so callers don't need a separate import.
pub use crate::inference::InferenceLogEntry;

/// Builds a complete debug snapshot from live game state.
///
/// Pure query function — reads but never mutates any state.
/// The `events` parameter is a ring buffer of recent debug events
/// maintained by the caller (TUI App or Tauri AppState).
pub fn build_debug_snapshot(
    world: &WorldState,
    npc_manager: &NpcManager,
    events: &VecDeque<DebugEvent>,
    inference: &InferenceDebug,
) -> DebugSnapshot {
    let clock = build_clock_debug(world);
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

    DebugSnapshot {
        clock,
        world: world_debug,
        npcs,
        tier_summary,
        events: event_list,
        inference: inference.clone(),
        gossip,
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
    // Build recent weather transitions from the weather engine history.
    // Take the last 5 entries for display.
    let weather_recent: Vec<(DateTime<Utc>, String)> = world
        .weather_engine
        .history()
        .iter()
        .rev()
        .take(5)
        .rev()
        .map(|(ts, w)| (*ts, w.to_string()))
        .collect();

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
        speed_factor: world.clock.speed_factor(),
        day_of_week,
        day_type: world.clock.day_type().to_string(),
        weather_recent,
    }
}

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
            locations.push(LocationDebug {
                id: loc_id.0,
                name: data.name.clone(),
                indoor: data.indoor,
                public: data.public,
                connection_count: data.connections.len(),
                npcs_here,
            });
        }
    }
    locations.sort_by_key(|l| l.id);

    WorldDebug {
        player_location_name: player_loc_name,
        player_location_id: world.player_location.0,
        location_count: world.graph.location_count(),
        locations,
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
                    RelationshipDebug {
                        target_name,
                        kind: rel.kind.to_string(),
                        strength: rel.strength,
                        history_count: rel.history.len(),
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

            let deflated_summary = npc
                .deflated_summary
                .as_ref()
                .and_then(|s| s.recent_activity.first().cloned());

            NpcDebug {
                id: npc.id.0,
                name: npc.name.clone(),
                age: npc.age,
                occupation: npc.occupation.clone(),
                personality: npc.personality.clone(),
                location_name: loc_name(npc.location, graph),
                location_id: npc.location.0,
                home_name: npc.home.map(|h| loc_name(h, graph)),
                workplace_name: npc.workplace.map(|w| loc_name(w, graph)),
                mood: npc.mood.clone(),
                state: state_str,
                tier,
                schedule,
                relationships,
                memories,
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
                is_ill: npc.is_ill,
                deflated_summary,
                long_term_memory_count: npc.long_term_memory.len(),
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
    let mut t4: usize = 0;

    for npc in npc_manager.all_npcs() {
        match npc_manager.tier_of(npc.id) {
            Some(CogTier::Tier1) => t1.push(npc.name.clone()),
            Some(CogTier::Tier2) => t2.push(npc.name.clone()),
            Some(CogTier::Tier3) | None => t3.push(npc.name.clone()),
            Some(CogTier::Tier4) => t4 += 1,
        }
    }

    let last_tier3_tick = npc_manager
        .last_tier3_game_time()
        .map(|t| t.format("%H:%M %Y-%m-%d").to_string());

    let last_tier2_tick = npc_manager
        .last_tier2_game_time()
        .map(|t| t.format("%H:%M %Y-%m-%d").to_string());

    let tier3_pending_count = t3.len();
    let tier4_recent_events: Vec<String> =
        npc_manager.recent_tier4_events().iter().cloned().collect();

    TierSummary {
        tier1_count: t1.len(),
        tier2_count: t2.len(),
        tier3_count: t3.len(),
        tier4_count: t4,
        tier1_names: t1,
        tier2_names: t2,
        tier3_names: t3,
        tier3_in_flight: npc_manager.tier3_in_flight(),
        last_tier3_tick,
        tier2_in_flight: npc_manager.tier2_in_flight(),
        last_tier2_tick,
        tier3_pending_count,
        tier4_recent_events,
    }
}

/// Builds the gossip network debug summary.
fn build_gossip_debug(world: &WorldState, npc_manager: &NpcManager) -> GossipDebug {
    let gossip = &world.gossip_network;
    let rumor_count = gossip.len();

    // Count NPCs who know at least one piece of gossip.
    let recent_witnesses = npc_manager
        .all_npcs()
        .filter(|npc| !gossip.known_by(npc.id).is_empty())
        .count();

    // Collect the last ~5 rumor texts (newest items are appended last by GossipNetwork).
    let top_rumors: Vec<String> = gossip
        .all_items()
        .iter()
        .rev()
        .take(5)
        .rev()
        .map(|item| item.content.clone())
        .collect();

    GossipDebug {
        rumor_count,
        recent_witnesses,
        top_rumors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::{Npc, NpcId};
    use std::collections::VecDeque;

    #[test]
    fn test_build_debug_snapshot_empty() {
        let world = WorldState::new();
        let npc_manager = NpcManager::new();
        let events = VecDeque::new();
        let inference = InferenceDebug {
            provider_name: "ollama".to_string(),
            model_name: "test-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            cloud_provider: None,
            cloud_model: None,
            has_queue: false,
            improv_enabled: false,
            call_log: vec![],
        };

        let snapshot = build_debug_snapshot(&world, &npc_manager, &events, &inference);

        assert!(snapshot.clock.game_time.contains("08:00"));
        assert_eq!(snapshot.clock.weather, "Clear");
        assert!(!snapshot.clock.paused);
        assert!(snapshot.npcs.is_empty());
        assert_eq!(snapshot.tier_summary.tier1_count, 0);
        assert_eq!(snapshot.inference.provider_name, "ollama");
    }

    #[test]
    fn test_build_debug_snapshot_with_npc() {
        let world = WorldState::new();
        let mut npc_manager = NpcManager::new();
        npc_manager.add_npc(Npc::new_test_npc());
        npc_manager.assign_tiers(&world, &[]);

        let events = VecDeque::new();
        let inference = InferenceDebug {
            provider_name: "ollama".to_string(),
            model_name: "test".to_string(),
            base_url: "http://localhost:11434".to_string(),
            cloud_provider: None,
            cloud_model: None,
            has_queue: true,
            improv_enabled: false,
            call_log: vec![],
        };

        let snapshot = build_debug_snapshot(&world, &npc_manager, &events, &inference);

        assert_eq!(snapshot.npcs.len(), 1);
        assert_eq!(snapshot.npcs[0].name, "Padraig O'Brien");
        assert_eq!(snapshot.npcs[0].mood, "content");
        assert_eq!(snapshot.npcs[0].state, "Present");
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
        let inference = InferenceDebug {
            provider_name: "test".to_string(),
            model_name: "test".to_string(),
            base_url: "http://localhost".to_string(),
            cloud_provider: None,
            cloud_model: None,
            has_queue: false,
            improv_enabled: false,
            call_log: vec![entry],
        };

        let snapshot = build_debug_snapshot(&world, &mgr, &events, &inference);
        assert_eq!(snapshot.inference.call_log.len(), 1);
        assert_eq!(snapshot.inference.call_log[0].request_id, 1);
        assert_eq!(snapshot.inference.call_log[0].duration_ms, 500);
    }

    #[test]
    fn test_events_included_in_snapshot() {
        let world = WorldState::new();
        let mgr = NpcManager::new();
        let mut events = VecDeque::new();
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
        let inference = InferenceDebug {
            provider_name: "test".to_string(),
            model_name: "test".to_string(),
            base_url: "http://localhost".to_string(),
            cloud_provider: None,
            cloud_model: None,
            has_queue: false,
            improv_enabled: false,
            call_log: vec![],
        };

        let snapshot = build_debug_snapshot(&world, &mgr, &events, &inference);
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
        assert_eq!(npcs[0].long_term_memory_count, 0);
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
    fn test_clock_debug_weather_recent_empty() {
        let world = WorldState::new();
        let clock = build_clock_debug(&world);
        // No transitions yet — history should be empty
        assert!(clock.weather_recent.is_empty());
    }

    #[test]
    fn test_gossip_debug_empty() {
        let world = WorldState::new();
        let mgr = NpcManager::new();
        let g = build_gossip_debug(&world, &mgr);
        assert_eq!(g.rumor_count, 0);
        assert_eq!(g.recent_witnesses, 0);
        assert!(g.top_rumors.is_empty());
    }

    #[test]
    fn test_gossip_debug_serializes_in_snapshot() {
        let world = WorldState::new();
        let mgr = NpcManager::new();
        let events = VecDeque::new();
        let inference = InferenceDebug {
            provider_name: "test".to_string(),
            model_name: "test".to_string(),
            base_url: "http://localhost".to_string(),
            cloud_provider: None,
            cloud_model: None,
            has_queue: false,
            improv_enabled: false,
            call_log: vec![],
        };
        let snapshot = build_debug_snapshot(&world, &mgr, &events, &inference);
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("gossip"));
        assert!(json.contains("rumor_count"));
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
        mgr.apply_tier4_events(&events, Utc::now());

        let summary = build_tier_summary(&mgr);
        assert_eq!(summary.tier4_recent_events.len(), 1);
        assert!(summary.tier4_recent_events[0].contains("ill"));
    }
}
