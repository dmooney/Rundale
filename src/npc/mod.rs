//! NPC system — identity, behavior, cognition, and relationships.
//!
//! Each NPC has personality traits, a daily schedule, relationships
//! with other NPCs, and short/long-term memory. Cognition fidelity
//! scales with distance from the player (4 LOD tiers).

use crate::world::{LocationId, WorldState};
use serde::Deserialize;

/// Unique identifier for an NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NpcId(pub u32);

/// A non-player character in the game world.
///
/// Phase 1 includes basic identity fields. Future phases add schedule,
/// relationships, memory, and full cognitive LOD tiers.
#[derive(Debug, Clone)]
pub struct Npc {
    /// Unique identifier.
    pub id: NpcId,
    /// Full name.
    pub name: String,
    /// Age in years.
    pub age: u8,
    /// Occupation or role in the parish.
    pub occupation: String,
    /// Personality description used in system prompts.
    pub personality: String,
    /// Current location.
    pub location: LocationId,
    /// Current emotional state.
    pub mood: String,
}

impl Npc {
    /// Creates a test NPC for Phase 1 development.
    ///
    /// Padraig O'Brien is a 58-year-old publican at The Crossroads,
    /// known for his storytelling and dry wit.
    pub fn new_test_npc() -> Self {
        Self {
            id: NpcId(1),
            name: "Padraig O'Brien".to_string(),
            age: 58,
            occupation: "Publican".to_string(),
            personality: "A gruff but warm-hearted publican who has run the crossroads \
                pub for thirty years. Known for his dry wit, encyclopedic knowledge of \
                local history, and tendency to offer unsolicited advice. He speaks with \
                a thick Roscommon accent and peppers his speech with Irish phrases."
                .to_string(),
            location: LocationId(1),
            mood: "content".to_string(),
        }
    }
}

/// Structured action output from an NPC's LLM response.
///
/// Deserialized from JSON returned by the Ollama inference call.
/// All optional fields use `#[serde(default)]` for robustness against
/// partial or malformed LLM output.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcAction {
    /// What the NPC does (e.g. "speaks", "moves", "gestures").
    #[serde(default)]
    pub action: String,
    /// The target of the action (e.g. "the player", "the door").
    #[serde(default)]
    pub target: Option<String>,
    /// Dialogue spoken by the NPC.
    #[serde(default)]
    pub dialogue: Option<String>,
    /// The NPC's current mood after this action.
    #[serde(default)]
    pub mood: String,
    /// Internal thought (not shown to player, used for simulation).
    #[serde(default)]
    pub internal_thought: Option<String>,
}

/// Builds the Tier 1 system prompt for an NPC.
///
/// Combines the NPC's identity, personality, occupation, and current
/// mood into a system prompt that establishes character for the LLM.
pub fn build_tier1_system_prompt(npc: &Npc) -> String {
    format!(
        "You are {name}, a {age}-year-old {occupation} in a small parish in County Roscommon, Ireland.\n\
        \n\
        Personality: {personality}\n\
        \n\
        Current mood: {mood}\n\
        \n\
        Respond in character as {name}. Your response must be valid JSON with these fields:\n\
        - \"action\": what you physically do (e.g. \"speaks\", \"nods\", \"sighs\")\n\
        - \"target\": who or what your action is directed at (optional)\n\
        - \"dialogue\": what you say out loud (optional)\n\
        - \"mood\": your mood after this interaction\n\
        - \"internal_thought\": what you're thinking but not saying (optional)",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        personality = npc.personality,
        mood = npc.mood,
    )
}

/// Builds the Tier 1 context prompt for an NPC interaction.
///
/// Includes the current location, time of day, weather, season,
/// and the player's action, giving the LLM full situational context.
pub fn build_tier1_context(npc: &Npc, world: &WorldState, player_input: &str) -> String {
    let location = world.current_location();
    let time_of_day = world.clock.time_of_day();
    let season = world.clock.season();

    format!(
        "Location: {loc_name} — {loc_desc}\n\
        Time: {time}\n\
        Season: {season}\n\
        Weather: {weather}\n\
        \n\
        {npc_name} is here.\n\
        \n\
        The player {action}",
        loc_name = location.name,
        loc_desc = location.description,
        time = time_of_day,
        season = season,
        weather = world.weather,
        npc_name = npc.name,
        action = player_input,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npc_test_npc() {
        let npc = Npc::new_test_npc();
        assert_eq!(npc.name, "Padraig O'Brien");
        assert_eq!(npc.age, 58);
        assert_eq!(npc.occupation, "Publican");
        assert_eq!(npc.location, LocationId(1));
    }

    #[test]
    fn test_build_system_prompt() {
        let npc = Npc::new_test_npc();
        let prompt = build_tier1_system_prompt(&npc);
        assert!(prompt.contains("Padraig O'Brien"));
        assert!(prompt.contains("58-year-old"));
        assert!(prompt.contains("Publican"));
        assert!(prompt.contains("content"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_build_context() {
        let npc = Npc::new_test_npc();
        let world = WorldState::new();
        let context = build_tier1_context(&npc, &world, "says hello");
        assert!(context.contains("The Crossroads"));
        assert!(context.contains("Morning"));
        assert!(context.contains("Spring"));
        assert!(context.contains("Clear"));
        assert!(context.contains("Padraig O'Brien"));
        assert!(context.contains("says hello"));
    }

    #[test]
    fn test_npc_action_deserialize_full() {
        let json = r#"{
            "action": "speaks",
            "target": "the player",
            "dialogue": "Ah, good morning to ye!",
            "mood": "friendly",
            "internal_thought": "Haven't seen this one before."
        }"#;
        let action: NpcAction = serde_json::from_str(json).unwrap();
        assert_eq!(action.action, "speaks");
        assert_eq!(action.target, Some("the player".to_string()));
        assert_eq!(action.dialogue, Some("Ah, good morning to ye!".to_string()));
        assert_eq!(action.mood, "friendly");
        assert_eq!(
            action.internal_thought,
            Some("Haven't seen this one before.".to_string())
        );
    }

    #[test]
    fn test_npc_action_deserialize_minimal() {
        let json = r#"{"action": "nods"}"#;
        let action: NpcAction = serde_json::from_str(json).unwrap();
        assert_eq!(action.action, "nods");
        assert!(action.target.is_none());
        assert!(action.dialogue.is_none());
        assert_eq!(action.mood, "");
        assert!(action.internal_thought.is_none());
    }

    #[test]
    fn test_npc_action_deserialize_empty() {
        let json = r#"{}"#;
        let action: NpcAction = serde_json::from_str(json).unwrap();
        assert_eq!(action.action, "");
        assert!(action.target.is_none());
        assert!(action.dialogue.is_none());
    }

    #[test]
    fn test_npc_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(NpcId(1));
        set.insert(NpcId(2));
        assert_eq!(set.len(), 2);
    }
}
