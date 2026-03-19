//! NPC system — identity, behavior, cognition, and relationships.
//!
//! Each NPC has personality traits, a daily schedule, relationships
//! with other NPCs, and short/long-term memory. Cognition fidelity
//! scales with distance from the player (4 LOD tiers).

pub mod manager;
pub mod memory;
pub mod overhear;
pub mod relationship;
pub mod schedule;
pub mod tier;

use std::collections::HashMap;

use crate::world::{LocationId, WorldState};
use serde::{Deserialize, Serialize};

/// Maximum number of bytes to hold back during streaming to detect
/// the separator pattern. Must be large enough to catch ` --- ` inline
/// or `  ---\n` on its own line, even when preceded by text on the same line.
pub const SEPARATOR_HOLDBACK: usize = 24;

/// Rounds a byte offset down to the nearest UTF-8 char boundary in `s`.
///
/// If `pos` is already a char boundary, returns it unchanged. Otherwise
/// scans backwards to the nearest valid boundary. Returns 0 if no valid
/// boundary is found before `pos`.
pub fn floor_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Finds the separator between dialogue and metadata in an NPC response.
///
/// Looks for `---` as a separator. Handles two cases:
/// 1. `---` on its own line (with optional whitespace)
/// 2. `---` appearing inline, e.g. `(smiles) --- {"action": ...}`
///
/// Returns `Some((dialogue_end, metadata_start))` — the byte offset where
/// dialogue ends and where metadata begins (after the separator).
/// Returns `None` if no separator is found.
pub fn find_response_separator(text: &str) -> Option<(usize, usize)> {
    // First try: --- on its own line
    let mut byte_offset = 0;
    for line in text.split('\n') {
        if line.trim() == "---" {
            let dialogue_end = byte_offset;
            let metadata_start = (byte_offset + line.len() + 1).min(text.len());
            return Some((dialogue_end, metadata_start));
        }
        byte_offset += line.len() + 1; // +1 for the \n
    }

    // Second try: --- appearing inline (e.g. "text --- {json}")
    // Look for " --- " or " ---\n" pattern
    if let Some(pos) = text.find(" --- ") {
        let dialogue_end = pos;
        let metadata_start = pos + 5; // skip " --- "
        return Some((dialogue_end, metadata_start));
    }
    if let Some(pos) = text.find(" ---\n") {
        let dialogue_end = pos;
        let metadata_start = (pos + 5).min(text.len());
        return Some((dialogue_end, metadata_start));
    }

    None
}

use memory::ShortTermMemory;
use relationship::Relationship;
use schedule::{DailySchedule, NpcState};

/// Unique identifier for an NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NpcId(pub u32);

/// A non-player character in the game world.
///
/// Each NPC has identity, personality, a daily schedule, relationships
/// with other NPCs, short-term memory, and location/movement state.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Current emotional state.
    pub mood: String,
    /// Home location.
    pub home: LocationId,
    /// Workplace location (if any).
    #[serde(default)]
    pub workplace: Option<LocationId>,
    /// Current movement state (present at a location or in transit).
    #[serde(default = "default_npc_state")]
    pub state: NpcState,
    /// Daily schedule determining where the NPC should be.
    pub schedule: DailySchedule,
    /// Relationships with other NPCs, keyed by target NPC id.
    #[serde(default)]
    pub relationships: HashMap<NpcId, Relationship>,
    /// Short-term memory of recent events.
    #[serde(default)]
    pub memory: ShortTermMemory,
    /// Knowledge — facts the NPC knows (used in prompt context).
    #[serde(default)]
    pub knowledge: Vec<String>,
}

/// Default NPC state for deserialization (present at location 1).
fn default_npc_state() -> NpcState {
    NpcState::Present(LocationId(1))
}

impl Npc {
    /// Returns the NPC's current location, if present (not in transit).
    pub fn location(&self) -> Option<LocationId> {
        self.state.location()
    }

    /// Creates a test NPC for Phase 1 backward compatibility.
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
            mood: "content".to_string(),
            home: LocationId(2),
            workplace: Some(LocationId(2)),
            state: NpcState::Present(LocationId(1)),
            schedule: DailySchedule {
                weekday: vec![],
                weekend: vec![],
                overrides: HashMap::new(),
            },
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            knowledge: Vec::new(),
        }
    }
}

/// Parsed result from a streaming NPC response.
///
/// Contains the player-visible dialogue/action text and the optional
/// metadata parsed from the JSON block after the `---` separator.
#[derive(Debug, Clone)]
pub struct NpcStreamResponse {
    /// The dialogue and action text shown to the player.
    pub dialogue: String,
    /// Parsed metadata from the JSON block, if present.
    pub metadata: Option<NpcMetadata>,
}

/// Metadata block from an NPC response (parsed from JSON after separator).
#[derive(Debug, Clone, Deserialize)]
pub struct NpcMetadata {
    /// What the NPC physically does.
    #[serde(default)]
    pub action: String,
    /// The NPC's mood after this interaction.
    #[serde(default)]
    pub mood: String,
    /// Internal thought (not shown to player).
    #[serde(default)]
    pub internal_thought: Option<String>,
}

/// Parses a complete NPC response into dialogue and metadata.
///
/// Splits on a `---` separator line (with optional surrounding whitespace).
/// Everything before is player-visible dialogue/actions. Everything after
/// is parsed as JSON metadata.
/// If no separator is found, the entire text is treated as dialogue.
pub fn parse_npc_stream_response(full_text: &str) -> NpcStreamResponse {
    // Try the separator format first
    if let Some((dialogue_end, metadata_start)) = find_response_separator(full_text) {
        let dialogue = full_text[..dialogue_end].trim().to_string();
        let meta_text = full_text[metadata_start..].trim();
        let metadata = serde_json::from_str::<NpcMetadata>(meta_text).ok();
        return NpcStreamResponse { dialogue, metadata };
    }

    // Fallback: try parsing entire response as legacy JSON NpcAction
    if let Ok(action) = serde_json::from_str::<NpcAction>(full_text) {
        let dialogue = action.dialogue.clone().unwrap_or_default();
        let metadata = Some(NpcMetadata {
            action: action.action,
            mood: action.mood,
            internal_thought: action.internal_thought,
        });
        return NpcStreamResponse { dialogue, metadata };
    }

    // Plain text fallback
    NpcStreamResponse {
        dialogue: full_text.trim().to_string(),
        metadata: None,
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
///
/// The prompt instructs the model to output dialogue first (which is
/// streamed to the player), then a `---` separator, then a JSON metadata
/// block (which is parsed silently for simulation state).
pub fn build_tier1_system_prompt(npc: &Npc) -> String {
    format!(
        "You are {name}, a {age}-year-old {occupation} in a small parish in County Roscommon, Ireland.\n\
        \n\
        Personality: {personality}\n\
        \n\
        Current mood: {mood}\n\
        \n\
        Respond in character as {name}. Use this EXACT format:\n\
        \n\
        1. First, write what you say or do, in plain text. Stay in character. \
        Describe actions in parentheses, e.g. (leans on the bar).\n\
        2. Then on a new line write exactly: ---\n\
        3. Then on the next line write a JSON metadata block with these fields:\n\
        - \"action\": what you physically do (e.g. \"speaks\", \"nods\", \"sighs\")\n\
        - \"mood\": your mood after this interaction\n\
        - \"internal_thought\": what you're thinking but not saying (optional)\n\
        \n\
        Example response:\n\
        (Looks up from polishing a glass) Ah, good morning to ye! Fine day for it, so it is.\n\
        ---\n\
        {{\"action\": \"speaks warmly\", \"mood\": \"friendly\", \"internal_thought\": \"New face around here\"}}",
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
        assert!(npc.state.is_at(LocationId(1)));
        assert_eq!(npc.home, LocationId(2));
    }

    #[test]
    fn test_npc_location() {
        let npc = Npc::new_test_npc();
        assert_eq!(npc.location(), Some(LocationId(1)));
    }

    #[test]
    fn test_build_system_prompt() {
        let npc = Npc::new_test_npc();
        let prompt = build_tier1_system_prompt(&npc);
        assert!(prompt.contains("Padraig O'Brien"));
        assert!(prompt.contains("58-year-old"));
        assert!(prompt.contains("Publican"));
        assert!(prompt.contains("content"));
        assert!(prompt.contains("---"));
        assert!(prompt.contains("JSON metadata"));
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
    fn test_parse_npc_stream_response_with_separator() {
        let text = "(Looks up) Ah, good morning to ye!\n---\n{\"action\": \"speaks\", \"mood\": \"friendly\"}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "(Looks up) Ah, good morning to ye!");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "speaks");
        assert_eq!(meta.mood, "friendly");
    }

    #[test]
    fn test_parse_npc_stream_response_separator_with_spaces() {
        let text = "Good morning to ye!\n  ---\n {\"action\": \"speaks\", \"mood\": \"friendly\"}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Good morning to ye!");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "speaks");
        assert_eq!(meta.mood, "friendly");
    }

    #[test]
    fn test_find_response_separator_exact() {
        let text = "hello\n---\n{\"a\":1}";
        let (d_end, m_start) = find_response_separator(text).unwrap();
        assert_eq!(&text[..d_end], "hello\n");
        assert!(text[m_start..].trim().starts_with('{'));
    }

    #[test]
    fn test_find_response_separator_with_spaces() {
        let text = "hello\n  ---  \n{\"a\":1}";
        let result = find_response_separator(text);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_response_separator_none() {
        assert!(find_response_separator("no separator here").is_none());
    }

    #[test]
    fn test_find_response_separator_inline() {
        // LLM sometimes puts --- inline with text and JSON
        let text = "(smiles) --- {\"action\": \"speaks\", \"mood\": \"content\"}";
        let (d_end, m_start) = find_response_separator(text).unwrap();
        assert_eq!(&text[..d_end], "(smiles)");
        assert!(text[m_start..].trim().starts_with('{'));
    }

    #[test]
    fn test_find_response_separator_inline_with_newline() {
        let text = "(smiles) ---\n{\"action\": \"speaks\"}";
        let (d_end, m_start) = find_response_separator(text).unwrap();
        assert_eq!(&text[..d_end], "(smiles)");
        assert!(text[m_start..].trim().starts_with('{'));
    }

    #[test]
    fn test_parse_inline_separator_real_example() {
        let text = "(Leans on the bar) Morning to ye, lad. (smiles) --- {\"action\": \"speaks\", \"mood\": \"content\", \"internal_thought\": \"Hoping they'll stay\"}";
        let parsed = parse_npc_stream_response(text);
        assert!(
            !parsed.dialogue.contains("action"),
            "metadata should not leak into dialogue: {}",
            parsed.dialogue
        );
        assert!(parsed.dialogue.contains("Morning to ye"));
        assert!(parsed.metadata.is_some());
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.mood, "content");
    }

    #[test]
    fn test_parse_npc_stream_response_no_separator() {
        let text = "Well hello there, stranger!";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Well hello there, stranger!");
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_parse_npc_stream_response_legacy_json() {
        let text = r#"{"action": "speaks", "dialogue": "Hello!", "mood": "friendly"}"#;
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Hello!");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "speaks");
        assert_eq!(meta.mood, "friendly");
    }

    #[test]
    fn test_parse_npc_stream_response_separator_no_trailing_newline() {
        let text = "Good day to ye!\n---\n{\"action\": \"nods\", \"mood\": \"content\"}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Good day to ye!");
        assert!(parsed.metadata.is_some());
    }

    #[test]
    fn test_parse_npc_stream_response_bad_json_after_separator() {
        let text = "Hello there!\n---\nnot json at all";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Hello there!");
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_parse_npc_stream_response_with_internal_thought() {
        let text = "Top of the mornin!\n---\n{\"action\": \"waves\", \"mood\": \"cheerful\", \"internal_thought\": \"Who's this now?\"}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Top of the mornin!");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.internal_thought, Some("Who's this now?".to_string()));
    }

    #[test]
    fn test_parse_npc_stream_response_empty() {
        let parsed = parse_npc_stream_response("");
        assert_eq!(parsed.dialogue, "");
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_floor_char_boundary_ascii() {
        let s = "hello";
        assert_eq!(floor_char_boundary(s, 3), 3);
    }

    #[test]
    fn test_floor_char_boundary_multibyte() {
        // em-dash — is 3 bytes (E2 80 94)
        let s = "ab\u{2014}cd";
        // bytes: a(0) b(1) E2(2) 80(3) 94(4) c(5) d(6)
        assert_eq!(floor_char_boundary(s, 3), 2); // snaps back to before —
        assert_eq!(floor_char_boundary(s, 4), 2); // same
        assert_eq!(floor_char_boundary(s, 5), 5); // c is at boundary
    }

    #[test]
    fn test_floor_char_boundary_at_len() {
        let s = "hello";
        assert_eq!(floor_char_boundary(s, 10), 5);
    }

    #[test]
    fn test_separator_holdback_sufficient() {
        // Holdback must be large enough to catch " --- " inline pattern
        assert!(SEPARATOR_HOLDBACK >= 20);
    }

    #[test]
    fn test_npc_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(NpcId(1));
        set.insert(NpcId(2));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_npc_serialize_deserialize() {
        let npc = Npc::new_test_npc();
        let json = serde_json::to_string(&npc).unwrap();
        let deser: Npc = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "Padraig O'Brien");
        assert_eq!(deser.age, 58);
        assert!(deser.state.is_at(LocationId(1)));
    }

    #[test]
    fn test_default_npc_state() {
        let state = default_npc_state();
        assert!(state.is_at(LocationId(1)));
    }
}
