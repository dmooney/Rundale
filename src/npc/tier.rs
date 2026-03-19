//! Cognitive tier tick functions.
//!
//! Tier 1: Full LLM inference for NPCs at the player's location.
//! Tier 2: Lighter group inference for NPCs 1-2 edges away.

use serde::{Deserialize, Serialize};

use super::memory::MemoryEntry;
use super::{Npc, NpcAction, NpcId, build_tier1_system_prompt};
use crate::inference::InferenceQueue;
use crate::world::{LocationId, WorldState};

/// A Tier 2 event summarizing NPC-NPC interaction at a location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier2Event {
    /// Where the interaction occurred.
    pub location: LocationId,
    /// NPCs involved.
    pub participants: Vec<NpcId>,
    /// A prose summary of what happened.
    pub summary: String,
    /// Relationship changes: (npc_a, npc_b, delta).
    #[serde(default)]
    pub relationship_changes: Vec<(NpcId, NpcId, f32)>,
}

/// Runs a Tier 1 tick for an NPC — full LLM inference.
///
/// Builds a context prompt including the NPC's personality, memory,
/// relationships, and the player's action, then sends it to the
/// inference queue. Returns the resulting `NpcAction`.
pub async fn tick_tier1(
    npc: &mut Npc,
    world: &WorldState,
    player_input: &str,
    model: &str,
    queue: &InferenceQueue,
    npc_names: &std::collections::HashMap<NpcId, String>,
) -> Result<NpcAction, crate::error::ParishError> {
    let system_prompt = build_tier1_system_prompt(npc);
    let context = build_tier1_full_context(npc, world, player_input, npc_names);

    let rx = queue
        .send(
            npc.id.0 as u64,
            model.to_string(),
            context,
            Some(system_prompt),
            None,
        )
        .await
        .map_err(|e| crate::error::ParishError::Inference(e.to_string()))?;

    let response = rx
        .await
        .map_err(|e| crate::error::ParishError::Inference(e.to_string()))?;

    if let Some(err) = &response.error {
        return Err(crate::error::ParishError::Inference(err.clone()));
    }

    let action: NpcAction = serde_json::from_str(&response.text).unwrap_or(NpcAction {
        action: "listens".to_string(),
        target: None,
        dialogue: Some(response.text.clone()),
        mood: npc.mood.clone(),
        internal_thought: None,
    });

    // Update NPC state from action
    if !action.mood.is_empty() {
        npc.mood = action.mood.clone();
    }

    // Add to memory
    let memory_content = if let Some(dialogue) = &action.dialogue {
        format!("Said to player: \"{}\"", dialogue)
    } else {
        format!("Action: {}", action.action)
    };

    npc.memory.add(MemoryEntry {
        timestamp: world.clock.now(),
        content: memory_content,
        participants: Vec::new(),
        location: world.player_location,
    });

    Ok(action)
}

/// Builds a full Tier 1 context prompt including memory and relationships.
///
/// Public wrapper for use by the main game loop.
pub fn build_tier1_full_context_pub(
    npc: &Npc,
    world: &WorldState,
    player_input: &str,
    npc_names: &std::collections::HashMap<NpcId, String>,
) -> String {
    build_tier1_full_context(npc, world, player_input, npc_names)
}

/// Builds a full Tier 1 context prompt including memory and relationships.
fn build_tier1_full_context(
    npc: &Npc,
    world: &WorldState,
    player_input: &str,
    npc_names: &std::collections::HashMap<NpcId, String>,
) -> String {
    let location = world.current_location();
    let time_of_day = world.clock.time_of_day();
    let season = world.clock.season();

    let mut context = format!(
        "Location: {} — {}\n\
         Time: {}\n\
         Season: {}\n\
         Weather: {}\n",
        location.name, location.description, time_of_day, season, world.weather,
    );

    // Add relationship context
    if !npc.relationships.is_empty() {
        context.push_str("\nRelationships:\n");
        for (target_id, rel) in &npc.relationships {
            let target_name = npc_names
                .get(target_id)
                .map(|s| s.as_str())
                .unwrap_or("someone");
            context.push_str(&format!("- {}\n", rel.context_string(target_name)));
        }
    }

    // Add memory context
    let memory_ctx = npc.memory.context_string();
    if !memory_ctx.is_empty() {
        context.push('\n');
        context.push_str(&memory_ctx);
        context.push('\n');
    }

    // Add knowledge
    if !npc.knowledge.is_empty() {
        context.push_str("\nThings you know:\n");
        for item in &npc.knowledge {
            context.push_str(&format!("- {}\n", item));
        }
    }

    context.push_str(&format!(
        "\n{} is here.\n\nThe player {}",
        npc.name, player_input
    ));

    context
}

/// Runs a Tier 2 tick for a group of NPCs at the same location.
///
/// Sends a lighter prompt to the LLM asking for a brief summary
/// of what happens between the NPCs. Returns a `Tier2Event`.
pub async fn tick_tier2(
    npcs: &[&Npc],
    location: LocationId,
    world: &WorldState,
    model: &str,
    queue: &InferenceQueue,
) -> Result<Tier2Event, crate::error::ParishError> {
    if npcs.is_empty() {
        return Ok(Tier2Event {
            location,
            participants: Vec::new(),
            summary: String::new(),
            relationship_changes: Vec::new(),
        });
    }

    let loc_name = world
        .locations
        .get(&location)
        .map(|l| l.name.as_str())
        .unwrap_or("an unknown place");

    let time_of_day = world.clock.time_of_day();

    let npc_descriptions: Vec<String> = npcs
        .iter()
        .map(|npc| {
            format!(
                "- {} ({}), mood: {}, personality: {}",
                npc.name, npc.occupation, npc.mood, npc.personality
            )
        })
        .collect();

    let prompt = format!(
        "It is {} at {}. The following people are here:\n\
         {}\n\n\
         In 1-2 sentences, briefly describe a natural interaction between them. \
         Focus on dialogue and small actions. Stay in character.\n\
         Respond with just the narrative text, no JSON.",
        time_of_day,
        loc_name,
        npc_descriptions.join("\n"),
    );

    let system = "You are a narrator for an Irish village simulation. \
                  Write brief, atmospheric prose about village life. \
                  Keep responses to 1-2 sentences."
        .to_string();

    let rx = queue
        .send(0, model.to_string(), prompt, Some(system), None)
        .await
        .map_err(|e| crate::error::ParishError::Inference(e.to_string()))?;

    let response = rx
        .await
        .map_err(|e| crate::error::ParishError::Inference(e.to_string()))?;

    let summary = if response.error.is_some() || response.text.trim().is_empty() {
        format!(
            "The people at {} go about their business quietly.",
            loc_name
        )
    } else {
        response.text.trim().to_string()
    };

    let participants: Vec<NpcId> = npcs.iter().map(|n| n.id).collect();

    Ok(Tier2Event {
        location,
        participants,
        summary,
        relationship_changes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier2_event_serialize() {
        let event = Tier2Event {
            location: LocationId(1),
            participants: vec![NpcId(1), NpcId(2)],
            summary: "They chat about the weather.".to_string(),
            relationship_changes: vec![(NpcId(1), NpcId(2), 0.1)],
        };
        let json = serde_json::to_string(&event).unwrap();
        let deser: Tier2Event = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.location, LocationId(1));
        assert_eq!(deser.participants.len(), 2);
        assert_eq!(deser.summary, "They chat about the weather.");
        assert_eq!(deser.relationship_changes.len(), 1);
    }

    #[test]
    fn test_tier2_event_empty_defaults() {
        let json = r#"{"location": 1, "participants": [], "summary": "quiet"}"#;
        let event: Tier2Event = serde_json::from_str(json).unwrap();
        assert!(event.relationship_changes.is_empty());
    }

    #[test]
    fn test_build_tier1_full_context() {
        use crate::npc::memory::ShortTermMemory;
        use crate::npc::relationship::{Relationship, RelationshipKind};
        use crate::npc::schedule::{DailySchedule, NpcState};

        let mut npc = Npc {
            id: NpcId(1),
            name: "Padraig".to_string(),
            age: 58,
            occupation: "Publican".to_string(),
            personality: "Warm and gregarious".to_string(),
            mood: "content".to_string(),
            home: LocationId(2),
            workplace: Some(LocationId(2)),
            state: NpcState::Present(LocationId(1)),
            schedule: DailySchedule {
                weekday: vec![],
                weekend: vec![],
                overrides: std::collections::HashMap::new(),
            },
            relationships: std::collections::HashMap::new(),
            memory: ShortTermMemory::new(),
            knowledge: vec!["The GAA match is next Sunday".to_string()],
        };

        npc.relationships.insert(
            NpcId(2),
            Relationship::new(NpcId(2), RelationshipKind::Friend, 0.7),
        );

        let world = WorldState::new();
        let mut names = std::collections::HashMap::new();
        names.insert(NpcId(2), "Siobhan".to_string());

        let ctx = build_tier1_full_context(&npc, &world, "says hello", &names);
        assert!(ctx.contains("The Crossroads"));
        assert!(ctx.contains("Relationships:"));
        assert!(ctx.contains("Siobhan"));
        assert!(ctx.contains("very close"));
        assert!(ctx.contains("GAA match"));
        assert!(ctx.contains("says hello"));
    }

    #[test]
    fn test_build_tier1_full_context_no_extras() {
        use crate::npc::memory::ShortTermMemory;
        use crate::npc::schedule::{DailySchedule, NpcState};

        let npc = Npc {
            id: NpcId(1),
            name: "Padraig".to_string(),
            age: 58,
            occupation: "Publican".to_string(),
            personality: "Warm".to_string(),
            mood: "content".to_string(),
            home: LocationId(1),
            workplace: None,
            state: NpcState::Present(LocationId(1)),
            schedule: DailySchedule {
                weekday: vec![],
                weekend: vec![],
                overrides: std::collections::HashMap::new(),
            },
            relationships: std::collections::HashMap::new(),
            memory: ShortTermMemory::new(),
            knowledge: Vec::new(),
        };

        let world = WorldState::new();
        let names = std::collections::HashMap::new();

        let ctx = build_tier1_full_context(&npc, &world, "looks around", &names);
        assert!(ctx.contains("The Crossroads"));
        assert!(!ctx.contains("Relationships:"));
        assert!(!ctx.contains("Recent memories:"));
        assert!(!ctx.contains("Things you know:"));
        assert!(ctx.contains("looks around"));
    }
}
