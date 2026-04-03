//! Tier 1 and Tier 2 tick functions for NPC simulation.
//!
//! Tier 1 ticks run per player interaction (full LLM inference).
//! Tier 2 ticks run every 5 game-minutes for nearby NPCs (lighter inference).

use chrono::Utc;

use crate::config::{NpcConfig, RelationshipLabelConfig};
use crate::inference::openai_client::OpenAiClient;
use crate::npc::memory::MemoryEntry;
use crate::npc::types::{Tier2Event, Tier2Response};
use crate::npc::{Npc, NpcId, NpcStreamResponse, build_tier1_context, build_tier1_system_prompt};
use crate::world::{LocationId, WorldState};

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
    /// Compact intelligence tag for prompt injection (e.g. `INT[V3 A4 E2 P5 W4 C3]`).
    pub intelligence_tag: String,
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

/// Returns a descriptive label for a relationship strength value using the given config thresholds.
pub fn relationship_label_with_config(
    strength: f64,
    config: &RelationshipLabelConfig,
) -> &'static str {
    match strength {
        s if s > config.very_close => "very close",
        s if s > config.friendly => "friendly",
        s if s > config.acquainted => "acquainted",
        s if s > config.cool => "cool",
        s if s > config.strained => "strained",
        _ => "hostile",
    }
}

/// Returns a descriptive label for a relationship strength value using default thresholds.
pub fn relationship_label(strength: f64) -> &'static str {
    relationship_label_with_config(strength, &RelationshipLabelConfig::default())
}

/// Builds an enhanced system prompt for Tier 1 interactions using the given config.
///
/// Extends the base system prompt with relationship summaries and
/// knowledge entries for richer, more contextual NPC dialogue.
pub fn build_enhanced_system_prompt_with_config(
    npc: &Npc,
    improv: bool,
    config: &NpcConfig,
) -> String {
    let mut prompt = build_tier1_system_prompt(npc, improv);

    // Add relationship context
    if !npc.relationships.is_empty() {
        prompt.push_str("\n\nRELATIONSHIPS:\n");
        for (target_id, rel) in &npc.relationships {
            let strength_desc =
                relationship_label_with_config(rel.strength, &config.relationship_labels);
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

/// Builds an enhanced system prompt for Tier 1 interactions.
///
/// Extends the base system prompt with relationship summaries and
/// knowledge entries for richer, more contextual NPC dialogue.
pub fn build_enhanced_system_prompt(npc: &Npc, improv: bool) -> String {
    build_enhanced_system_prompt_with_config(npc, improv, &NpcConfig::default())
}

/// Builds an enhanced context prompt for Tier 1 interactions using the given config.
///
/// Extends the base context with the NPC's recent memories and
/// information about other NPCs present at the same location.
pub fn build_enhanced_context_with_config(
    npc: &Npc,
    world: &WorldState,
    player_input: &str,
    other_npcs: &[&Npc],
    config: &NpcConfig,
) -> String {
    let mut context = build_tier1_context(world, player_input);

    // Add other NPCs present
    if !other_npcs.is_empty() {
        context.push_str("\n\nAlso present here:");
        for other in other_npcs {
            context.push_str(&format!("\n- {} ({})", other.name, other.occupation));
        }
    }

    // Add recent memories
    let memory_ctx = npc.memory.context_string(config.memory_context_count);
    if !memory_ctx.is_empty() {
        context.push_str("\n\nRecent memories:\n");
        context.push_str(&memory_ctx);
    }

    // Add recent player reactions (emoji feedback)
    let reaction_ctx = npc
        .reaction_log
        .context_string(config.reaction_context_count);
    if !reaction_ctx.is_empty() {
        context.push_str("\n\n");
        context.push_str(&reaction_ctx);
    }

    context
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
    build_enhanced_context_with_config(npc, world, player_input, other_npcs, &NpcConfig::default())
}

/// Processes a Tier 1 NPC response using the given config, updating mood and recording a memory.
///
/// Call this after receiving and parsing the LLM response for a Tier 1
/// interaction. Updates the NPC's mood from metadata and adds a memory
/// entry recording the interaction.
///
/// Returns a list of debug event strings (e.g. mood changes, memory commits).
pub fn apply_tier1_response_with_config(
    npc: &mut Npc,
    response: &NpcStreamResponse,
    player_input: &str,
    game_time: chrono::DateTime<Utc>,
    config: &NpcConfig,
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
        truncate_for_memory(&response.dialogue, config.memory_truncation_dialogue)
    );
    events.push(format!(
        "{} remembers: {}",
        npc.name,
        truncate_for_memory(&content, config.memory_truncation_event_log)
    ));
    npc.memory.add(MemoryEntry {
        timestamp: game_time,
        content,
        participants: vec![npc.id],
        location: npc.location,
    });

    events
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
    apply_tier1_response_with_config(
        npc,
        response,
        player_input,
        game_time,
        &NpcConfig::default(),
    )
}

/// Builds the system prompt for a Tier 2 interaction between NPCs at a location.
pub fn build_tier2_prompt(group: &Tier2Group, time_desc: &str, weather: &str) -> String {
    let npc_descriptions: Vec<String> = group
        .npcs
        .iter()
        .map(|snap| {
            format!(
                "- {} ({}), mood: {}, {}",
                snap.name, snap.occupation, snap.mood, snap.intelligence_tag
            )
        })
        .collect();

    let weather_commentary = match weather {
        "Light Rain" | "Heavy Rain" | "Storm" => " People are commenting on the weather.",
        _ => "",
    };

    format!(
        "You are simulating background interactions between characters in a small \
        Irish parish in 1820.\n\n\
        Location: {location}\n\
        Time: {time}\n\
        Weather: {weather}.{weather_commentary}\n\n\
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
        .generate_json::<Tier2Response>(model, &prompt, None, None)
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

/// Applies a Tier 2 event's effects to the relevant NPCs using the given config.
///
/// Updates moods, adjusts relationship strengths, and records memories
/// for all participating NPCs.
///
/// Returns debug event strings describing what happened.
pub fn apply_tier2_event_with_config(
    event: &Tier2Event,
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    game_time: chrono::DateTime<Utc>,
    config: &NpcConfig,
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
    let memory_content = truncate_for_memory(&event.summary, config.event_summary_truncation);
    // Log the memory commit for all participants
    for &pid in &event.participants {
        if let Some(npc) = npcs.get(&pid) {
            debug_events.push(format!(
                "{} remembers: {}",
                npc.name,
                truncate_for_memory(&event.summary, config.event_summary_debug_truncation)
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
    apply_tier2_event_with_config(event, npcs, game_time, &NpcConfig::default())
}

/// Truncates a string to a maximum length, adding "..." if truncated.
fn truncate_for_memory(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = crate::npc::floor_char_boundary(s, max_len.saturating_sub(3));
        let safe_boundary = boundary.min(s.len());
        format!("{}...", &s[..safe_boundary])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::NpcMetadata;
    use crate::npc::memory::ShortTermMemory;
    use crate::npc::types::{
        MoodChange, NpcState, Relationship, RelationshipChange, RelationshipKind,
    };
    use chrono::TimeZone;
    use std::collections::HashMap;

    fn make_test_npc(id: u32, name: &str, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: name.to_string(),
            brief_description: format!("a test NPC named {}", name),
            age: 40,
            occupation: "Test".to_string(),
            personality: "Friendly".to_string(),
            intelligence: crate::npc::types::Intelligence::default(),
            location: LocationId(location),
            mood: "calm".to_string(),
            home: Some(LocationId(location)),
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::default(),
            deflated_summary: None,
            reaction_log: crate::npc::reactions::ReactionLog::default(),
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
                language_hints: Vec::new(),
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
                    intelligence_tag: "INT[V3 A3 E4 P4 W5 C4]".to_string(),
                    mood: "content".to_string(),
                    relationship_context: String::new(),
                },
                NpcSnapshot {
                    id: NpcId(5),
                    name: "Tommy".to_string(),
                    occupation: "Retired Farmer".to_string(),
                    personality: "Storyteller".to_string(),
                    intelligence_tag: "INT[V4 A2 E3 P4 W5 C5]".to_string(),
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
    fn test_relationship_label_with_default_config() {
        let config = RelationshipLabelConfig::default();
        assert_eq!(relationship_label_with_config(0.9, &config), "very close");
        assert_eq!(relationship_label_with_config(0.5, &config), "friendly");
        assert_eq!(relationship_label_with_config(0.1, &config), "acquainted");
        assert_eq!(relationship_label_with_config(-0.1, &config), "cool");
        assert_eq!(relationship_label_with_config(-0.5, &config), "strained");
        assert_eq!(relationship_label_with_config(-0.9, &config), "hostile");
    }

    #[test]
    fn test_relationship_label_with_custom_config() {
        let config = RelationshipLabelConfig {
            very_close: 0.9,
            friendly: 0.5,
            acquainted: 0.0,
            cool: -0.5,
            strained: -0.9,
        };
        // 0.8 is below 0.9, so "friendly" instead of "very close"
        assert_eq!(relationship_label_with_config(0.8, &config), "friendly");
        // 0.3 is below 0.5, so "acquainted" instead of "friendly"
        assert_eq!(relationship_label_with_config(0.3, &config), "acquainted");
    }

    #[test]
    fn test_relationship_label_default_wrapper() {
        assert_eq!(relationship_label(0.9), "very close");
        assert_eq!(relationship_label(-0.9), "hostile");
    }

    #[test]
    fn test_build_enhanced_system_prompt_with_config() {
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.8));

        let config = NpcConfig {
            relationship_labels: RelationshipLabelConfig {
                very_close: 0.9,
                ..RelationshipLabelConfig::default()
            },
            ..NpcConfig::default()
        };
        let prompt = build_enhanced_system_prompt_with_config(&npc, false, &config);
        // 0.8 is below 0.9 threshold, so should be "friendly" not "very close"
        assert!(prompt.contains("friendly"));
        assert!(!prompt.contains("very close"));
    }

    #[test]
    fn test_build_enhanced_context_with_config_memory_count() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        for i in 0..10 {
            npc.memory.add(MemoryEntry {
                timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 8 + i, 0, 0).unwrap(),
                content: format!("Memory {}", i),
                participants: vec![NpcId(1)],
                location: LocationId(1),
            });
        }
        let world = WorldState::new();

        // With memory_context_count = 2, should only include last 2 memories
        let config = NpcConfig {
            memory_context_count: 2,
            ..NpcConfig::default()
        };
        let context = build_enhanced_context_with_config(&npc, &world, "hello", &[], &config);
        assert!(context.contains("Memory 9"));
        assert!(context.contains("Memory 8"));
        assert!(!context.contains("Memory 7"));
    }

    #[test]
    fn test_apply_tier1_response_with_config_truncation() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        let long_dialogue = "a".repeat(200);
        let response = NpcStreamResponse {
            dialogue: long_dialogue,
            metadata: None,
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();

        let config = NpcConfig {
            memory_truncation_dialogue: 40,
            memory_truncation_event_log: 30,
            ..NpcConfig::default()
        };
        let events =
            apply_tier1_response_with_config(&mut npc, &response, "waves", game_time, &config);

        // The debug event log entry should be truncated to ~30 chars
        assert!(events.iter().any(|e| e.contains("remembers:")));
        assert_eq!(npc.memory.len(), 1);
    }

    #[test]
    fn test_apply_tier2_event_with_config_truncation() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));

        let long_summary = "a".repeat(200);
        let event = Tier2Event {
            location: LocationId(2),
            summary: long_summary,
            participants: vec![NpcId(1)],
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let config = NpcConfig {
            event_summary_truncation: 40,
            event_summary_debug_truncation: 20,
            ..NpcConfig::default()
        };

        let events = apply_tier2_event_with_config(&event, &mut npcs, game_time, &config);
        assert!(!events.is_empty());

        // The stored memory content should be truncated to ~40 chars
        let mem = &npcs.get(&NpcId(1)).unwrap().memory;
        let recent = mem.recent(1);
        assert!(recent[0].content.len() <= 40);
    }

    // --- truncate_for_memory edge cases ---

    #[test]
    fn test_truncate_for_memory_empty_string() {
        assert_eq!(truncate_for_memory("", 10), "");
    }

    #[test]
    fn test_truncate_for_memory_exact_boundary() {
        assert_eq!(truncate_for_memory("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_for_memory_one_over() {
        let result = truncate_for_memory("123456", 5);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 5);
    }

    #[test]
    fn test_truncate_for_memory_max_len_zero() {
        let result = truncate_for_memory("hello", 0);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_for_memory_max_len_three() {
        // max_len=3 means only room for "..."
        let result = truncate_for_memory("hello world", 3);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_for_memory_multibyte_utf8() {
        // Ensure truncation doesn't split multi-byte characters
        let irish = "Dia dhuit, a chara. Cén chaoi a bhfuil tú?";
        let result = truncate_for_memory(irish, 15);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 18); // slightly over due to char boundary
        // Should be valid UTF-8 (no panic)
        let _ = result.chars().count();
    }

    #[test]
    fn test_truncate_for_memory_very_long_string() {
        let long = "x".repeat(10000);
        let result = truncate_for_memory(&long, 50);
        assert!(result.len() <= 50);
        assert!(result.ends_with("..."));
    }

    // --- apply_tier2_event edge cases ---

    #[test]
    fn test_apply_tier2_event_missing_npc_in_map() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));
        // NpcId(99) is NOT in the map

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Something happened".to_string(),
            participants: vec![NpcId(1), NpcId(99)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(99),
                new_mood: "happy".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: NpcId(99),
                to: NpcId(1),
                delta: 0.1,
            }],
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        // Should not panic — missing NPCs are silently skipped
        let events = apply_tier2_event(&event, &mut npcs, game_time);
        // Padraig still gets a memory
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 1);
        // Some events generated for the NPC that exists
        assert!(!events.is_empty());
    }

    #[test]
    fn test_apply_tier2_event_empty_participants() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Nothing happened".to_string(),
            participants: Vec::new(),
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier2_event(&event, &mut npcs, game_time);
        assert!(events.is_empty());
        // No memories added
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 0);
    }

    #[test]
    fn test_apply_tier2_event_same_mood_no_debug_event() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc = make_test_npc(1, "Padraig", 2);
        npc.mood = "calm".to_string();
        npcs.insert(NpcId(1), npc);

        let event = Tier2Event {
            location: LocationId(2),
            summary: "Padraig sits quietly".to_string(),
            participants: vec![NpcId(1)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(1),
                new_mood: "calm".to_string(), // same as current
            }],
            relationship_changes: Vec::new(),
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let events = apply_tier2_event(&event, &mut npcs, game_time);
        // No mood change event since mood didn't actually change
        assert!(!events.iter().any(|e| e.contains("mood:")));
        // But memory event should still be there
        assert!(events.iter().any(|e| e.contains("remembers:")));
    }

    #[test]
    fn test_apply_tier2_event_relationship_not_found() {
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        // Padraig has no relationship with Tommy
        npcs.insert(NpcId(1), make_test_npc(1, "Padraig", 2));
        npcs.insert(NpcId(5), make_test_npc(5, "Tommy", 2));

        let event = Tier2Event {
            location: LocationId(2),
            summary: "They chat".to_string(),
            participants: vec![NpcId(1), NpcId(5)],
            mood_changes: Vec::new(),
            relationship_changes: vec![RelationshipChange {
                from: NpcId(1),
                to: NpcId(5),
                delta: 0.1,
            }],
        };

        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        // Should not panic — missing relationship is silently skipped
        let _events = apply_tier2_event(&event, &mut npcs, game_time);
        // Both still get memories
        assert_eq!(npcs.get(&NpcId(1)).unwrap().memory.len(), 1);
        assert_eq!(npcs.get(&NpcId(5)).unwrap().memory.len(), 1);
    }

    // --- run_tier2_for_group solo NPC ---

    #[tokio::test]
    async fn test_run_tier2_solo_npc_template() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            npcs: vec![NpcSnapshot {
                id: NpcId(1),
                name: "Padraig".to_string(),
                occupation: "Publican".to_string(),
                personality: "Warm".to_string(),
                intelligence_tag: "INT[V3 A3 E4 P4 W5 C4]".to_string(),
                mood: "content".to_string(),
                relationship_context: String::new(),
            }],
        };

        // Solo NPC should get a template event without needing inference
        let client = OpenAiClient::new("http://localhost:11434", None);
        let event = run_tier2_for_group(&client, "test", &group, "Morning", "Clear").await;
        assert!(event.is_some());
        let event = event.unwrap();
        assert!(event.summary.contains("Padraig"));
        assert!(event.summary.contains("Darcy's Pub"));
        assert_eq!(event.participants, vec![NpcId(1)]);
        assert!(event.mood_changes.is_empty());
        assert!(event.relationship_changes.is_empty());
    }

    #[tokio::test]
    async fn test_run_tier2_empty_group_returns_none() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            npcs: Vec::new(),
        };

        let client = OpenAiClient::new("http://localhost:11434", None);
        let event = run_tier2_for_group(&client, "test", &group, "Morning", "Clear").await;
        assert!(event.is_none());
    }

    // --- build_tier2_prompt weather commentary ---

    #[test]
    fn test_build_tier2_prompt_rain_commentary() {
        let group = Tier2Group {
            location: LocationId(2),
            location_name: "The Crossroads".to_string(),
            npcs: vec![
                NpcSnapshot {
                    id: NpcId(1),
                    name: "Padraig".to_string(),
                    occupation: "Publican".to_string(),
                    personality: "Warm".to_string(),
                    intelligence_tag: "INT[V3]".to_string(),
                    mood: "calm".to_string(),
                    relationship_context: String::new(),
                },
                NpcSnapshot {
                    id: NpcId(2),
                    name: "Tommy".to_string(),
                    occupation: "Farmer".to_string(),
                    personality: "Gruff".to_string(),
                    intelligence_tag: "INT[V2]".to_string(),
                    mood: "tired".to_string(),
                    relationship_context: String::new(),
                },
            ],
        };

        let prompt = build_tier2_prompt(&group, "Afternoon", "Heavy Rain");
        assert!(prompt.contains("commenting on the weather"));

        let prompt = build_tier2_prompt(&group, "Afternoon", "Clear");
        assert!(!prompt.contains("commenting on the weather"));
    }

    // --- apply_tier1_response same mood no change event ---

    #[test]
    fn test_apply_tier1_response_same_mood_no_change_event() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "calm".to_string();
        let response = NpcStreamResponse {
            dialogue: "Hello.".to_string(),
            metadata: Some(NpcMetadata {
                action: "speaks".to_string(),
                mood: "calm".to_string(), // same mood
                internal_thought: None,
                language_hints: Vec::new(),
            }),
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events = apply_tier1_response(&mut npc, &response, "hello", game_time);
        // No mood change event
        assert!(!events.iter().any(|e| e.contains("mood:")));
        // Memory still recorded
        assert!(events.iter().any(|e| e.contains("remembers:")));
    }

    #[test]
    fn test_apply_tier1_response_empty_mood_no_change() {
        let mut npc = make_test_npc(1, "Padraig", 1);
        npc.mood = "calm".to_string();
        let response = NpcStreamResponse {
            dialogue: "Hello.".to_string(),
            metadata: Some(NpcMetadata {
                action: "speaks".to_string(),
                mood: String::new(), // empty mood
                internal_thought: None,
                language_hints: Vec::new(),
            }),
        };
        let game_time = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let events = apply_tier1_response(&mut npc, &response, "hello", game_time);
        assert_eq!(npc.mood, "calm"); // unchanged
        assert!(!events.iter().any(|e| e.contains("mood:")));
    }
}
