//! Tier 1 and Tier 2 tick functions for NPC simulation.
//!
//! Tier 1 ticks run per player interaction (full LLM inference).
//! Tier 2 ticks run every 5 game-minutes for nearby NPCs (lighter inference).

use chrono::Utc;

use crate::inference::openai_client::OpenAiClient;
use crate::npc::memory::MemoryEntry;
use crate::npc::tier3::Tier3Update;
use crate::npc::tier4::Tier4Event;
use crate::npc::types::{NpcSummary, ScheduleEntry, Tier2Event, Tier2Response};
use crate::npc::{Npc, NpcId, NpcStreamResponse, build_tier1_context, build_tier1_system_prompt};
use crate::world::time::Season;
use crate::world::{LocationId, Weather, WorldState};

/// A lightweight snapshot of an NPC's state for Tier 2 inference.
///
/// Contains only the data needed to build Tier 2 prompts, allowing
/// the inference to run in a background task without borrowing from
/// the NpcManager.
#[derive(Debug, Clone)]
pub struct NpcSnapshot {
    /// NPC id.
    pub id: NpcId,
    /// NPC name.
    pub name: String,
    /// Occupation.
    pub occupation: String,
    /// Personality summary.
    pub personality: String,
    /// Current mood.
    pub mood: String,
    /// Relationship summaries with other NPCs at this location.
    pub relationship_context: String,
}

/// A group of NPC snapshots at a single location, for Tier 2 processing.
#[derive(Debug, Clone)]
pub struct Tier2Group {
    /// Location where these NPCs are gathered.
    pub location: LocationId,
    /// Location name for prompt context.
    pub location_name: String,
    /// Snapshots of NPCs at this location.
    pub npcs: Vec<NpcSnapshot>,
}

/// Builds an enhanced system prompt for Tier 1 interactions.
///
/// Extends the base system prompt with relationship summaries and
/// knowledge entries for richer, more contextual NPC dialogue.
pub fn build_enhanced_system_prompt(npc: &Npc, improv: bool) -> String {
    let mut prompt = build_tier1_system_prompt(npc, improv);

    // Add relationship context
    if !npc.relationships.is_empty() {
        prompt.push_str("\n\nRELATIONSHIPS:\n");
        for (target_id, rel) in &npc.relationships {
            let strength_desc = match rel.strength {
                s if s > 0.7 => "very close",
                s if s > 0.3 => "friendly",
                s if s > 0.0 => "acquainted",
                s if s > -0.3 => "cool",
                s if s > -0.7 => "strained",
                _ => "hostile",
            };
            prompt.push_str(&format!(
                "- NPC #{}: {} relationship, {} (strength {:.1})\n",
                target_id.0, rel.kind, strength_desc, rel.strength
            ));
        }
    }

    // Add knowledge
    if !npc.knowledge.is_empty() {
        prompt.push_str("\nTHINGS YOU KNOW:\n");
        for item in &npc.knowledge {
            prompt.push_str(&format!("- {}\n", item));
        }
    }

    prompt
}

/// Builds an enhanced context prompt for Tier 1 interactions.
///
/// Extends the base context with the NPC's recent memories and
/// information about other NPCs present at the same location.
pub fn build_enhanced_context(
    npc: &Npc,
    world: &WorldState,
    player_input: &str,
    other_npcs: &[&Npc],
) -> String {
    let mut context = build_tier1_context(npc, world, player_input);

    // Add other NPCs present
    if !other_npcs.is_empty() {
        context.push_str("\n\nAlso present here:");
        for other in other_npcs {
            context.push_str(&format!("\n- {} ({})", other.name, other.occupation));
        }
    }

    // Add recent memories
    let memory_ctx = npc.memory.context_string(5);
    if !memory_ctx.is_empty() {
        context.push_str("\n\nRecent memories:\n");
        context.push_str(&memory_ctx);
    }

    context
}

/// Processes a Tier 1 NPC response, updating mood and recording a memory.
///
/// Call this after receiving and parsing the LLM response for a Tier 1
/// interaction. Updates the NPC's mood from metadata and adds a memory
/// entry recording the interaction.
///
/// Returns a list of debug event strings (e.g. mood changes, memory commits).
pub fn apply_tier1_response(
    npc: &mut Npc,
    response: &NpcStreamResponse,
    player_input: &str,
    game_time: chrono::DateTime<Utc>,
) -> Vec<String> {
    let mut events = Vec::new();

    // Update mood from metadata
    if let Some(ref meta) = response.metadata
        && !meta.mood.is_empty()
        && meta.mood != npc.mood
    {
        events.push(format!("{} mood: {} -> {}", npc.name, npc.mood, meta.mood));
        npc.mood = meta.mood.clone();
    }

    // Record memory of the interaction
    let content = format!(
        "Spoke with a traveller who {}. Responded: {}",
        player_input,
        truncate_for_memory(&response.dialogue, 80)
    );
    events.push(format!(
        "{} remembers: {}",
        npc.name,
        truncate_for_memory(&content, 60)
    ));
    npc.memory.add(MemoryEntry {
        timestamp: game_time,
        content,
        participants: vec![npc.id],
        location: npc.location,
    });

    events
}

/// Builds the system prompt for a Tier 2 interaction between NPCs at a location.
pub fn build_tier2_prompt(group: &Tier2Group, time_desc: &str, weather: &str) -> String {
    let npc_descriptions: Vec<String> = group
        .npcs
        .iter()
        .map(|snap| format!("- {} ({}), mood: {}", snap.name, snap.occupation, snap.mood))
        .collect();

    format!(
        "You are simulating background interactions between characters in a small \
        Irish parish in 1820.\n\n\
        Location: {location}\n\
        Time: {time}\n\
        Weather: {weather}\n\n\
        Characters present:\n{characters}\n\n\
        Generate a brief (1-2 sentence) summary of what these characters are doing \
        and saying to each other. Include any mood changes or relationship shifts.\n\n\
        Respond with a JSON object:\n\
        {{\n\
          \"summary\": \"Brief description of the interaction\",\n\
          \"mood_changes\": [{{\"npc_id\": <id>, \"new_mood\": \"<mood>\"}}],\n\
          \"relationship_changes\": [{{\"from\": <id>, \"to\": <id>, \"delta\": <-0.1 to 0.1>}}]\n\
        }}",
        location = group.location_name,
        time = time_desc,
        weather = weather,
        characters = npc_descriptions.join("\n"),
    )
}

/// Runs Tier 2 inference for a group of NPCs at a location.
///
/// Uses `generate_json` for non-streaming structured output.
/// Returns a `Tier2Event` with the summary, mood changes, and relationship deltas.
pub async fn run_tier2_for_group(
    client: &OpenAiClient,
    model: &str,
    group: &Tier2Group,
    time_desc: &str,
    weather: &str,
) -> Option<Tier2Event> {
    if group.npcs.len() < 2 {
        // Solo NPC: generate a simple template event, no inference needed
        if let Some(snap) = group.npcs.first() {
            return Some(Tier2Event {
                location: group.location,
                summary: format!(
                    "{} goes about their business at {}.",
                    snap.name, group.location_name
                ),
                participants: vec![snap.id],
                mood_changes: Vec::new(),
                relationship_changes: Vec::new(),
            });
        }
        return None;
    }

    let prompt = build_tier2_prompt(group, time_desc, weather);
    let participant_ids: Vec<NpcId> = group.npcs.iter().map(|s| s.id).collect();

    match client
        .generate_json::<Tier2Response>(model, &prompt, None)
        .await
    {
        Ok(resp) => Some(Tier2Event {
            location: group.location,
            summary: resp.summary,
            participants: participant_ids,
            mood_changes: resp.mood_changes,
            relationship_changes: resp.relationship_changes,
        }),
        Err(e) => {
            tracing::warn!("Tier 2 inference failed at {}: {}", group.location_name, e);
            None
        }
    }
}

/// Applies a Tier 2 event's effects to the relevant NPCs.
///
/// Updates moods, adjusts relationship strengths, and records memories
/// for all participating NPCs.
///
/// Returns debug event strings describing what happened.
pub fn apply_tier2_event(
    event: &Tier2Event,
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    game_time: chrono::DateTime<Utc>,
) -> Vec<String> {
    let mut debug_events = Vec::new();

    // Apply mood changes
    for mc in &event.mood_changes {
        if let Some(npc) = npcs.get_mut(&mc.npc_id) {
            if npc.mood != mc.new_mood {
                debug_events.push(format!(
                    "{} mood: {} -> {}",
                    npc.name, npc.mood, mc.new_mood
                ));
            }
            npc.mood = mc.new_mood.clone();
        }
    }

    // Apply relationship changes
    for rc in &event.relationship_changes {
        if let Some(npc) = npcs.get_mut(&rc.from)
            && let Some(rel) = npc.relationships.get_mut(&rc.to)
        {
            rel.adjust_strength(rc.delta);
        }
    }

    // Record memory for all participants
    let memory_content = truncate_for_memory(&event.summary, 100);
    // Log the memory commit for all participants
    for &pid in &event.participants {
        if let Some(npc) = npcs.get(&pid) {
            debug_events.push(format!(
                "{} remembers: {}",
                npc.name,
                truncate_for_memory(&event.summary, 50)
            ));
        }
    }
    for &participant_id in &event.participants {
        if let Some(npc) = npcs.get_mut(&participant_id) {
            npc.memory.add(MemoryEntry {
                timestamp: game_time,
                content: memory_content.clone(),
                participants: event.participants.clone(),
                location: event.location,
            });
        }
    }

    debug_events
}

/// Truncates a string to a maximum length, adding "..." if truncated.
fn truncate_for_memory(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = crate::npc::floor_char_boundary(s, max_len.saturating_sub(3));
        format!("{}...", &s[..boundary])
    }
}

/// Builds a narrative context string when an NPC is inflated from Tier 3/4 to Tier 1/2.
///
/// Combines any available Tier 3 update summaries and Tier 4 event descriptions
/// into a synthetic memory entry, so the NPC has continuity when the player approaches.
pub fn inflate_npc_context(
    npc: &Npc,
    tier3_updates: &[Tier3Update],
    tier4_events: &[Tier4Event],
) -> String {
    let mut parts = Vec::new();

    // Include relevant Tier 3 summaries
    for update in tier3_updates {
        if update.npc_id == npc.id && !update.summary.is_empty() {
            parts.push(format!("Recently: {}", update.summary));
        }
    }

    // Include relevant Tier 4 events
    for event in tier4_events {
        match event {
            Tier4Event::Illness { npc_id } if *npc_id == npc.id => {
                parts.push("Was recently ill.".to_string());
            }
            Tier4Event::Recovery { npc_id } if *npc_id == npc.id => {
                parts.push("Recently recovered from illness.".to_string());
            }
            Tier4Event::SeasonalShift { npc_id, new_mood } if *npc_id == npc.id => {
                parts.push(format!("Mood shifted to {} with the season.", new_mood));
            }
            Tier4Event::RelationshipFormed { from, to } if *from == npc.id || *to == npc.id => {
                let other = if *from == npc.id { to } else { from };
                parts.push(format!("Formed a new connection with NPC #{}.", other.0));
            }
            Tier4Event::MoodShift { npc_id, new_mood } if *npc_id == npc.id => {
                parts.push(format!("Has been feeling {}.", new_mood));
            }
            _ => {}
        }
    }

    if parts.is_empty() {
        format!(
            "{} has been going about their usual routine as a {}.",
            npc.name, npc.occupation
        )
    } else {
        parts.join(" ")
    }
}

/// Deflates an NPC's state into a compact summary for Tier 3/4 processing.
///
/// Compacts short-term memory into a single activity string and extracts
/// key relationship information.
pub fn deflate_npc_state(npc: &Npc) -> NpcSummary {
    // Compact recent memories into activity summary
    let recent = npc.memory.recent(5);
    let recent_activity = if recent.is_empty() {
        format!("{} has been quiet.", npc.name)
    } else {
        recent
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    };

    // Extract key relationship descriptors
    let key_relationships: Vec<String> = npc
        .relationships
        .iter()
        .filter(|(_, rel)| rel.strength.abs() > 0.3)
        .map(|(id, rel)| {
            let strength_desc = if rel.strength > 0.5 {
                "close"
            } else if rel.strength > 0.0 {
                "friendly"
            } else if rel.strength > -0.5 {
                "strained"
            } else {
                "hostile"
            };
            format!("NPC #{}: {} ({})", id.0, rel.kind, strength_desc)
        })
        .collect();

    NpcSummary {
        npc_id: npc.id,
        location: npc.location,
        mood: npc.mood.clone(),
        recent_activity,
        key_relationships,
    }
}

/// Returns seasonal schedule overrides for an NPC.
///
/// Adjusts NPC schedules based on the season:
/// - Farmers: longer hours in summer (5am-9pm), shorter in winter (7am-4pm)
/// - Publicans: busier winter evenings (pub open from 4pm instead of 6pm)
/// - Teachers: school closed in summer (stays home)
///
/// Returns `None` if no override applies for this NPC/season combination.
pub fn seasonal_schedule_overrides(npc: &Npc, season: Season) -> Option<Vec<ScheduleEntry>> {
    let occupation = npc.occupation.to_lowercase();
    let home = npc.home.unwrap_or(npc.location);
    let workplace = npc.workplace.unwrap_or(npc.location);

    if occupation.contains("farmer") {
        let (work_start, work_end): (u8, u8) = match season {
            Season::Summer => (5, 21),
            Season::Spring | Season::Autumn => (6, 18),
            Season::Winter => (7, 16),
        };
        return Some(vec![
            ScheduleEntry {
                start_hour: 0,
                end_hour: work_start.saturating_sub(1),
                location: home,
                activity: "sleeping".to_string(),
            },
            ScheduleEntry {
                start_hour: work_start,
                end_hour: work_end,
                location: workplace,
                activity: crate::npc::tier4::seasonal_activity(season).to_string(),
            },
            ScheduleEntry {
                start_hour: work_end + 1,
                end_hour: 23,
                location: home,
                activity: "evening rest".to_string(),
            },
        ]);
    }

    if occupation.contains("publican") || occupation.contains("pub") {
        let open_hour: u8 = match season {
            Season::Winter => 16,
            Season::Autumn => 17,
            _ => 18,
        };
        return Some(vec![
            ScheduleEntry {
                start_hour: 0,
                end_hour: 7,
                location: home,
                activity: "sleeping".to_string(),
            },
            ScheduleEntry {
                start_hour: 8,
                end_hour: open_hour.saturating_sub(1),
                location: home,
                activity: "resting before opening".to_string(),
            },
            ScheduleEntry {
                start_hour: open_hour,
                end_hour: 23,
                location: workplace,
                activity: "tending bar".to_string(),
            },
        ]);
    }

    if (occupation.contains("teacher") || occupation.contains("school")) && season == Season::Summer
    {
        return Some(vec![
            ScheduleEntry {
                start_hour: 0,
                end_hour: 7,
                location: home,
                activity: "sleeping".to_string(),
            },
            ScheduleEntry {
                start_hour: 8,
                end_hour: 23,
                location: home,
                activity: "summer rest".to_string(),
            },
        ]);
    }

    None
}

/// Returns a weather-adjusted location for an NPC.
///
/// If the NPC is scheduled to be outdoors but the weather is bad (Rain, Storm,
/// or HeavyRain — note: `Weather` enum doesn't have HeavyRain, so just Rain/Storm),
/// overrides to the NPC's home or current location (nearest indoor location).
///
/// Returns `None` if no override is needed (weather is fine or location is indoor).
pub fn weather_location_override(
    npc: &Npc,
    _scheduled_location: LocationId,
    weather: Weather,
    is_indoor: bool,
) -> Option<LocationId> {
    let dominated_by_weather = matches!(weather, Weather::Rain | Weather::Storm);

    if dominated_by_weather && !is_indoor {
        // Prefer home, fall back to current location
        Some(npc.home.unwrap_or(npc.location))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::NpcMetadata;
    use crate::npc::memory::{LongTermMemory, ShortTermMemory};
    use crate::npc::types::{
        MoodChange, NpcState, Relationship, RelationshipChange, RelationshipKind,
    };
    use chrono::TimeZone;
    use std::collections::HashMap;

    fn make_test_npc(id: u32, name: &str, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: name.to_string(),
            age: 40,
            occupation: "Test".to_string(),
            personality: "Friendly".to_string(),
            location: LocationId(location),
            mood: "calm".to_string(),
            home: Some(LocationId(location)),
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::default(),
        }
    }

    #[test]
    fn test_enhanced_system_prompt_includes_relationships() {
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.8));
        npc.knowledge = vec!["Knows local history".to_string()];

        let prompt = build_enhanced_system_prompt(&npc, false);
        assert!(prompt.contains("RELATIONSHIPS:"));
        assert!(prompt.contains("very close"));
        assert!(prompt.contains("THINGS YOU KNOW:"));
        assert!(prompt.contains("Knows local history"));
    }

    #[test]
    fn test_enhanced_system_prompt_without_relationships() {
        let npc = make_test_npc(1, "Padraig", 2);
        let prompt = build_enhanced_system_prompt(&npc, false);
        assert!(!prompt.contains("RELATIONSHIPS:"));
        assert!(!prompt.contains("THINGS YOU KNOW:"));
    }

    #[test]
    fn test_enhanced_context_with_other_npcs() {
        let npc = make_test_npc(1, "Padraig", 1);
        let other = make_test_npc(2, "Tommy", 1);
        let world = WorldState::new();

        let context = build_enhanced_context(&npc, &world, "greets everyone", &[&other]);
        assert!(context.contains("Also present here:"));
        assert!(context.contains("Tommy (Test)"));
    }

    #[test]
    fn test_enhanced_context_with_memories() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.memory.add(MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "Saw a stranger at the crossroads".to_string(),
            participants: vec![NpcId(1)],
            location: LocationId(1),
        });
        let world = WorldState::new();

        let context = build_enhanced_context(&npc, &world, "says hello", &[]);
        assert!(context.contains("Recent memories:"));
        assert!(context.contains("Saw a stranger at the crossroads"));
    }

    #[test]
    fn test_apply_tier1_response_updates_mood() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        let response = NpcStreamResponse {
            dialogue: "Hello there!".to_string(),
            metadata: Some(NpcMetadata {
                action: "speaks".to_string(),
                mood: "cheerful".to_string(),
                internal_thought: None,
                irish_words: Vec::new(),
            }),
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        apply_tier1_response(&mut npc, &response, "says hello", game_time);

        assert_eq!(npc.mood, "cheerful");
        assert_eq!(npc.memory.len(), 1);
    }

    #[test]
    fn test_apply_tier1_response_no_metadata() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        let response = NpcStreamResponse {
            dialogue: "Hello there!".to_string(),
            metadata: None,
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        apply_tier1_response(&mut npc, &response, "waves", game_time);

        assert_eq!(npc.mood, "calm"); // unchanged
        assert_eq!(npc.memory.len(), 1); // memory still recorded
    }

    #[test]
    fn test_build_tier2_prompt() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            npcs: vec![
                NpcSnapshot {
                    id: NpcId(1),
                    name: "Padraig".to_string(),
                    occupation: "Publican".to_string(),
                    personality: "Warm".to_string(),
                    mood: "content".to_string(),
                    relationship_context: String::new(),
                },
                NpcSnapshot {
                    id: NpcId(5),
                    name: "Tommy".to_string(),
                    occupation: "Retired Farmer".to_string(),
                    personality: "Storyteller".to_string(),
                    mood: "reflective".to_string(),
                    relationship_context: String::new(),
                },
            ],
        };

        let prompt = build_tier2_prompt(&group, "Evening", "Overcast");
        assert!(prompt.contains("Darcy's Pub"));
        assert!(prompt.contains("Padraig (Publican)"));
        assert!(prompt.contains("Tommy (Retired Farmer)"));
        assert!(prompt.contains("Evening"));
        assert!(prompt.contains("Overcast"));
        assert!(prompt.contains("summary"));
    }

    #[test]
    fn test_apply_tier2_event() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc1 = make_test_npc(1, "Padraig", 2);
        npc1.relationships
            .insert(NpcId(5), Relationship::new(RelationshipKind::Friend, 0.5));
        npcs.insert(NpcId(1), npc1);
        npcs.insert(NpcId(5), make_test_npc(5, "Tommy", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Padraig and Tommy shared stories over a pint".to_string(),
            participants: vec![NpcId(1), NpcId(5)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(1),
                new_mood: "jovial".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: NpcId(1),
                to: NpcId(5),
                delta: 0.1,
            }],
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        apply_tier2_event(&event, &mut npcs, game_time);

        // Check mood updated
        assert_eq!(npcs.get(&NpcId(1)).unwrap().mood, "jovial");

        // Check relationship adjusted
        let rel = npcs
            .get(&NpcId(1))
            .unwrap()
            .relationships
            .get(&NpcId(5))
            .unwrap();
        assert!((rel.strength - 0.6).abs() < f64::EPSILON);

        // Check memories recorded for both
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 1);
        assert_eq!(npcs.get(&NpcId(5)).unwrap().memory.len(), 1);
    }

    #[test]
    fn test_truncate_for_memory() {
        assert_eq!(truncate_for_memory("short", 10), "short");
        let long = "a".repeat(100);
        let truncated = truncate_for_memory(&long, 20);
        assert!(truncated.len() <= 20);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_relationship_strength_descriptions() {
        let mut npc = make_test_npc(1, "Test", 1);

        // Test all strength tiers appear in the prompt
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Family, 0.9));
        npc.relationships
            .insert(NpcId(3), Relationship::new(RelationshipKind::Enemy, -0.8));

        let prompt = build_enhanced_system_prompt(&npc, false);
        assert!(prompt.contains("very close") || prompt.contains("hostile"));
    }

    #[test]
    fn test_inflate_npc_context_with_tier3_updates() {
        let npc = make_test_npc(1, "Padraig", 1);
        let updates = vec![Tier3Update {
            npc_id: NpcId(1),
            new_mood: Some("cheerful".to_string()),
            summary: "Served drinks and told stories all day".to_string(),
            relationship_changes: vec![],
        }];
        let context = inflate_npc_context(&npc, &updates, &[]);
        assert!(context.contains("Served drinks and told stories"));
    }

    #[test]
    fn test_inflate_npc_context_with_tier4_events() {
        let npc = make_test_npc(1, "Padraig", 1);
        let events = vec![
            Tier4Event::Illness { npc_id: NpcId(1) },
            Tier4Event::Recovery { npc_id: NpcId(1) },
        ];
        let context = inflate_npc_context(&npc, &[], &events);
        assert!(context.contains("ill"));
        assert!(context.contains("recovered"));
    }

    #[test]
    fn test_inflate_npc_context_empty() {
        let npc = make_test_npc(1, "Padraig", 1);
        let context = inflate_npc_context(&npc, &[], &[]);
        assert!(context.contains("Padraig"));
        assert!(context.contains("usual routine"));
    }

    #[test]
    fn test_inflate_npc_context_ignores_other_npcs() {
        let npc = make_test_npc(1, "Padraig", 1);
        let updates = vec![Tier3Update {
            npc_id: NpcId(99), // different NPC
            new_mood: None,
            summary: "Irrelevant".to_string(),
            relationship_changes: vec![],
        }];
        let context = inflate_npc_context(&npc, &updates, &[]);
        assert!(!context.contains("Irrelevant"));
        assert!(context.contains("usual routine"));
    }

    #[test]
    fn test_deflate_npc_state_with_memories() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.memory.add(MemoryEntry {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            content: "Spoke about the weather".to_string(),
            participants: vec![NpcId(1)],
            location: LocationId(1),
        });
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.8));

        let summary = deflate_npc_state(&npc);
        assert_eq!(summary.npc_id, NpcId(1));
        assert_eq!(summary.mood, "calm");
        assert!(summary.recent_activity.contains("Spoke about the weather"));
        assert_eq!(summary.key_relationships.len(), 1);
        assert!(summary.key_relationships[0].contains("close"));
    }

    #[test]
    fn test_deflate_npc_state_empty_memories() {
        let npc = make_test_npc(1, "Padraig", 1);
        let summary = deflate_npc_state(&npc);
        assert!(summary.recent_activity.contains("quiet"));
        assert!(summary.key_relationships.is_empty());
    }

    #[test]
    fn test_seasonal_schedule_overrides_farmer_summer() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.occupation = "Farmer".to_string();
        npc.home = Some(LocationId(1));
        npc.workplace = Some(LocationId(10));

        let overrides = seasonal_schedule_overrides(&npc, Season::Summer);
        assert!(overrides.is_some());
        let entries = overrides.unwrap();
        // Should have 3 entries: sleep, work, evening
        assert_eq!(entries.len(), 3);
        // Work entry should have longer summer hours
        let work = &entries[1];
        assert_eq!(work.start_hour, 5);
        assert_eq!(work.end_hour, 21);
        assert_eq!(work.location, LocationId(10));
    }

    #[test]
    fn test_seasonal_schedule_overrides_farmer_winter() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.occupation = "Farmer".to_string();
        npc.home = Some(LocationId(1));
        npc.workplace = Some(LocationId(10));

        let overrides = seasonal_schedule_overrides(&npc, Season::Winter);
        assert!(overrides.is_some());
        let entries = overrides.unwrap();
        let work = &entries[1];
        assert_eq!(work.start_hour, 7);
        assert_eq!(work.end_hour, 16);
    }

    #[test]
    fn test_seasonal_schedule_overrides_publican_winter() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.occupation = "Publican".to_string();
        npc.home = Some(LocationId(1));
        npc.workplace = Some(LocationId(2));

        let overrides = seasonal_schedule_overrides(&npc, Season::Winter);
        assert!(overrides.is_some());
        let entries = overrides.unwrap();
        // Pub opens earlier in winter
        let bar = entries
            .iter()
            .find(|e| e.activity == "tending bar")
            .unwrap();
        assert_eq!(bar.start_hour, 16);
    }

    #[test]
    fn test_seasonal_schedule_overrides_teacher_summer() {
        let mut npc = make_test_npc(1, "Siobhan", 1);
        npc.occupation = "Teacher".to_string();
        npc.home = Some(LocationId(1));
        npc.workplace = Some(LocationId(5));

        let overrides = seasonal_schedule_overrides(&npc, Season::Summer);
        assert!(overrides.is_some());
        let entries = overrides.unwrap();
        // Teacher stays home in summer
        assert!(entries.iter().all(|e| e.location == LocationId(1)));
    }

    #[test]
    fn test_seasonal_schedule_overrides_no_match() {
        let mut npc = make_test_npc(1, "Tommy", 1);
        npc.occupation = "Blacksmith".to_string();

        let overrides = seasonal_schedule_overrides(&npc, Season::Summer);
        assert!(overrides.is_none());
    }

    #[test]
    fn test_weather_location_override_rain_outdoor() {
        let npc = make_test_npc(1, "Padraig", 1);
        let result = weather_location_override(
            &npc,
            LocationId(5),
            Weather::Rain,
            false, // outdoor
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_weather_location_override_rain_indoor() {
        let npc = make_test_npc(1, "Padraig", 1);
        let result = weather_location_override(
            &npc,
            LocationId(5),
            Weather::Rain,
            true, // indoor
        );
        assert!(result.is_none()); // no override needed
    }

    #[test]
    fn test_weather_location_override_clear() {
        let npc = make_test_npc(1, "Padraig", 1);
        let result = weather_location_override(
            &npc,
            LocationId(5),
            Weather::Clear,
            false, // outdoor
        );
        assert!(result.is_none()); // no override for clear weather
    }

    #[test]
    fn test_weather_location_override_storm() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.home = Some(LocationId(3));
        let result = weather_location_override(&npc, LocationId(5), Weather::Storm, false);
        assert_eq!(result, Some(LocationId(3))); // goes home
    }
}
