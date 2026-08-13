//! Debug snapshot builders — construct DTOs from live game state.

use std::collections::VecDeque;

use chrono::{Datelike, Timelike};

use parish_npc::manager::NpcManager;
use parish_npc::types::{CogTier, NpcState};
use parish_world::WorldState;
use parish_world::graph::WorldGraph;
use parish_world::time::{DayType, Season};

use super::InferenceCategoryConfig;
use super::types::*;

/// Walking speed used to compute debug edge travel times (meters/second).
///
/// The real walking speed lives in each mod's transport config but is not
/// threaded through to the debug snapshot — we use a canonical fallback
/// that matches the default "on foot" preset so the panel can surface a
/// representative travel time per edge.
const DEBUG_WALKING_SPEED_M_PER_S: f64 = 1.25;

/// Maximum number of text-log lines included in the snapshot.
const TEXT_LOG_TAIL_LEN: usize = 50;

/// Returns a list of provider display names that are ready to use
/// (either local providers, or cloud providers with an API key set).
pub fn build_configured_providers() -> Vec<String> {
    use parish_config::registry;
    registry()
        .all()
        .iter()
        .filter(|p| p.is_configured_in_env())
        .map(|p| p.id().to_string())
        .collect()
}

/// Builds the per-role debug entries from an [`InferenceCategoryConfig`].
///
/// Always returns one entry per concrete inference workload so the UI exposes
/// the actual cap/thinking profile used by direct and queued call paths.
pub fn build_inference_categories(
    config: &impl InferenceCategoryConfig,
) -> Vec<InferenceCategoryDebug> {
    use parish_config::InferenceSubrole;
    InferenceSubrole::ALL
        .iter()
        .map(|subrole| {
            let category = subrole.category();
            let profile = config.subrole_profile(*subrole);
            InferenceCategoryDebug {
                role: subrole.name().to_string(),
                provider: config.category_provider(category),
                model: config.category_model(category),
                base_url: config.category_base_url(category),
                thinking_level: profile.thinking_level,
                max_output_tokens: profile.max_output_tokens,
                service_tier: profile.service_tier,
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
    game_events: &VecDeque<parish_world::events::GameEvent>,
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

/// Builds clock debug info from world state.
pub(crate) fn build_clock_debug(world: &WorldState) -> ClockDebug {
    let now = world.clock.now();
    let day_of_week = parish_types::time::weekday_name(now.weekday()).to_string();

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
pub(crate) fn build_weather_debug(world: &WorldState) -> WeatherDebug {
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
        last_check_at: world
            .weather_engine
            .last_check_at()
            .map(|at| at.format("%H:%M %Y-%m-%d").to_string()),
    }
}

/// Builds event bus debug info from the captured game-event ring buffer.
pub(crate) fn build_event_bus_debug(
    world: &WorldState,
    game_events: &VecDeque<parish_world::events::GameEvent>,
    npc_manager: &NpcManager,
) -> EventBusDebug {
    use parish_world::events::GameEvent;

    let name_of = |id: parish_npc::NpcId| -> String {
        npc_manager
            .get(id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("NPC({})", id.0))
    };
    let loc_of = |id: parish_world::LocationId| -> String { loc_name(id, &world.graph) };

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
                GameEvent::NpcActivity {
                    npc_id,
                    location,
                    activity,
                    ..
                } => format!("{} @ {}: {}", name_of(*npc_id), loc_of(*location), activity),
                GameEvent::GossipSpread {
                    source,
                    location,
                    content,
                    ..
                } => format!(
                    "Gossip [{} @ {}]: {}",
                    name_of(*source),
                    loc_of(*location),
                    content
                ),
                GameEvent::AddressedAbsentNpc { name, location, .. } => {
                    format!("Addressed absent: {} @ {}", name, loc_of(*location))
                }
                GameEvent::WeatherChanged { new_weather, .. } => {
                    format!("Weather: {}", new_weather)
                }
                GameEvent::FestivalStarted { name, .. } => format!("Festival: {}", name),
                GameEvent::LifeEvent {
                    npc_id,
                    description,
                    ..
                } => format!("{}: {}", name_of(*npc_id), description),
                GameEvent::PlayerMoved { from, to, .. } => {
                    format!("Player: {} → {}", loc_of(*from), loc_of(*to))
                }
                GameEvent::PlayerTaskAssigned { task, .. } => format!(
                    "Task #{} assigned by {} @ {}: {}",
                    task.id.0,
                    name_of(task.assigned_by),
                    loc_of(task.location),
                    task.description
                ),
                GameEvent::PlayerTaskProgressed { task, action, .. } => format!(
                    "Task #{} → {:?} @ {}: {} ({})",
                    task.id.0,
                    task.status,
                    loc_of(task.location),
                    task.description,
                    action
                ),
                GameEvent::NpcInteraction {
                    participants,
                    location,
                    summary,
                    ..
                } => {
                    let names = participants
                        .iter()
                        .map(|p| name_of(*p))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("@{} [{}]: {}", loc_of(*location), names, summary)
                }
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
pub(crate) fn build_gossip_debug(world: &WorldState, npc_manager: &NpcManager) -> GossipDebug {
    let name_of = |id: parish_npc::NpcId| -> String {
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
pub(crate) fn build_conversations_debug(world: &WorldState) -> ConversationsDebug {
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

/// Builds world debug info including per-location NPC presence.
pub(crate) fn build_world_debug(world: &WorldState, npc_manager: &NpcManager) -> WorldDebug {
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
pub(crate) fn loc_name(id: parish_world::LocationId, graph: &WorldGraph) -> String {
    graph
        .get(id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| format!("Location({})", id.0))
}

/// Builds the NPC debug list with full deep-dive data.
pub(crate) fn build_npc_debug_list(
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

            let state_str = match npc.state() {
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

            let schedule = build_npc_schedule_debug(
                npc,
                graph,
                current_hour,
                current_season,
                current_day_type,
            );
            let relationships = build_npc_relationship_debug(npc, npc_manager);
            let memories = build_npc_memory_debug(npc, graph);
            let long_term_memories = build_npc_long_term_memory_debug(npc);
            let reactions = build_npc_reaction_debug(npc);
            let deflated_summary = build_npc_deflated_summary_debug(npc, graph);

            NpcDebug {
                id: npc.id.0,
                name: npc.name.clone(),
                brief_description: npc.brief_description.clone(),
                introduced: npc_manager.is_introduced(npc.id),
                age: npc.age,
                occupation: npc.occupation.clone(),
                personality: npc.personality.clone(),
                location_name: loc_name(npc.location(), graph),
                location_id: npc.location().0,
                home_name: npc.home.map(|h| loc_name(h, graph)),
                workplace_name: npc.workplace.map(|w| loc_name(w, graph)),
                mood: npc.mood.clone(),
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

    npcs.sort_by(|a, b| a.tier.cmp(&b.tier).then(a.name.cmp(&b.name)));
    npcs
}

/// Builds schedule debug info for a single NPC.
pub(crate) fn build_npc_schedule_debug(
    npc: &parish_npc::Npc,
    graph: &WorldGraph,
    current_hour: u8,
    current_season: Season,
    current_day_type: DayType,
) -> Vec<ScheduleVariantDebug> {
    npc.schedule()
        .map(|s| {
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
        .unwrap_or_default()
}

/// Builds relationship debug info for a single NPC, sorted by strength descending.
pub(crate) fn build_npc_relationship_debug(
    npc: &parish_npc::Npc,
    npc_manager: &NpcManager,
) -> Vec<RelationshipDebug> {
    let mut relationships: Vec<RelationshipDebug> = npc
        .relationships
        .iter()
        .map(|(target_id, rel)| {
            let target_name = npc_manager
                .get(*target_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("NPC({})", target_id.0));
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
            history.reverse();
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
    relationships
}

/// Builds recent short-term memory debug entries for a single NPC.
pub(crate) fn build_npc_memory_debug(
    npc: &parish_npc::Npc,
    graph: &WorldGraph,
) -> Vec<MemoryDebug> {
    npc.memory
        .recent(10)
        .iter()
        .map(|m| MemoryDebug {
            timestamp: m.timestamp.format("%H:%M %Y-%m-%d").to_string(),
            content: m.content.clone(),
            location_name: loc_name(m.location, graph),
        })
        .collect()
}

/// Builds long-term memory debug entries for a single NPC.
pub(crate) fn build_npc_long_term_memory_debug(npc: &parish_npc::Npc) -> Vec<LongTermMemoryDebug> {
    npc.long_term_memory
        .all_entries()
        .iter()
        .map(|e| LongTermMemoryDebug {
            timestamp: e.timestamp.format("%H:%M %Y-%m-%d").to_string(),
            content: e.content.clone(),
            importance: e.importance,
            keywords: e.keywords.clone(),
        })
        .collect()
}

/// Builds reaction log debug entries for a single NPC.
pub(crate) fn build_npc_reaction_debug(npc: &parish_npc::Npc) -> Vec<ReactionDebug> {
    npc.reaction_log
        .entries()
        .rev()
        .map(|r| ReactionDebug {
            timestamp: r.timestamp.format("%H:%M %Y-%m-%d").to_string(),
            emoji: r.emoji.clone(),
            description: r.description.clone(),
            context: r.context.clone(),
        })
        .collect()
}

/// Builds the deflated summary debug entry for a single NPC, if present.
pub(crate) fn build_npc_deflated_summary_debug(
    npc: &parish_npc::Npc,
    graph: &WorldGraph,
) -> Option<DeflatedSummaryDebug> {
    npc.deflated_summary.as_ref().map(|s| DeflatedSummaryDebug {
        location_name: loc_name(s.location, graph),
        mood: s.mood.clone(),
        recent_activity: s.recent_activity.clone(),
        key_relationship_changes: s.key_relationship_changes.clone(),
    })
}

/// Builds tier summary counts and name lists.
pub(crate) fn build_tier_summary(npc_manager: &NpcManager) -> TierSummary {
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
