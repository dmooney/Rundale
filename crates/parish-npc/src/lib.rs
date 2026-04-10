//! NPC system for the Parish game engine.
//!
//! Each NPC has personality traits, a daily schedule, relationships
//! with other NPCs, and short-term memory. Cognition fidelity is determined
//! by the NpcManager based on distance from the player.

pub mod anachronism;
pub mod autonomous;
pub mod data;
pub mod manager;
pub mod memory;
pub mod mood;
pub mod overhear;
pub mod reactions;
pub mod ticks;
pub mod tier4;
pub mod transitions;
pub mod types;

/// Re-export conversation types from parish-types for cross-crate path compatibility.
pub mod conversation {
    pub use parish_types::conversation::*;
}

use std::collections::HashMap;

use serde::Deserialize;

use memory::{LongTermMemory, ShortTermMemory};
use parish_types::{DayType, LocationId, Season};
use parish_world::WorldState;
use reactions::ReactionLog;
use transitions::NpcSummary;
use types::{Intelligence, NpcState, Relationship, SeasonalSchedule};

// Re-export shared types from parish-types
pub use parish_types::{
    IrishWordHint, LanguageHint, NpcId, SEPARATOR_HOLDBACK, find_response_separator,
    floor_char_boundary,
};

/// A non-player character in the game world.
///
/// Contains identity, personality, location, schedule, relationships,
/// and short-term memory. Cognition fidelity is determined by the
/// NpcManager based on distance from the player.
#[derive(Debug, Clone)]
pub struct Npc {
    /// Unique identifier.
    pub id: NpcId,
    /// Full name.
    pub name: String,
    /// Brief anonymous description shown before the player is introduced.
    ///
    /// E.g., "a priest", "a middle-aged woman", "an older man".
    pub brief_description: String,
    /// Age in years.
    pub age: u8,
    /// Occupation or role in the parish.
    pub occupation: String,
    /// Personality description used in system prompts.
    pub personality: String,
    /// Multidimensional intelligence profile shaping dialogue generation.
    pub intelligence: Intelligence,
    /// Current location.
    pub location: LocationId,
    /// Current emotional state.
    pub mood: String,
    /// Home location (where the NPC sleeps).
    pub home: Option<LocationId>,
    /// Workplace location (where the NPC works).
    pub workplace: Option<LocationId>,
    /// Season- and day-aware schedule defining where the NPC goes.
    pub schedule: Option<SeasonalSchedule>,
    /// Relationships to other NPCs, keyed by their id.
    pub relationships: HashMap<NpcId, Relationship>,
    /// Ring buffer of recent memories.
    pub memory: ShortTermMemory,
    /// Persistent long-term memory with keyword-based retrieval.
    pub long_term_memory: LongTermMemory,
    /// Things this NPC knows (local gossip, history, etc.).
    pub knowledge: Vec<String>,
    /// Whether the NPC is present at their location or in transit.
    pub state: NpcState,
    /// Compact summary from the last tier deflation, if any.
    ///
    /// Set when the NPC drops to a lower cognitive tier; cleared when
    /// they are inflated back to a higher tier.
    pub deflated_summary: Option<NpcSummary>,
    /// Log of recent player reactions (emoji) toward this NPC.
    pub reaction_log: ReactionLog,
    /// Last activity summary from Tier 3 batch simulation.
    ///
    /// Used in deflated context and Tier 3 prompt construction.
    /// Updated each time a Tier 3 tick processes this NPC.
    pub last_activity: Option<String>,
    /// Whether the NPC is currently ill. Set by Tier 4 rules engine.
    pub is_ill: bool,
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
            brief_description: "an older man behind the bar".to_string(),
            age: 58,
            occupation: "Publican".to_string(),
            personality: "A gruff but warm-hearted publican who has run the crossroads \
                pub for thirty years. Known for his dry wit, encyclopedic knowledge of \
                local history, and tendency to offer unsolicited advice. He speaks with \
                a thick Roscommon accent and peppers his speech with Irish phrases."
                .to_string(),
            intelligence: Intelligence::new(3, 3, 4, 4, 5, 4),
            location: LocationId(1),
            mood: "content".to_string(),
            home: None,
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::default(),
            deflated_summary: None,
            reaction_log: ReactionLog::default(),
            last_activity: None,
            is_ill: false,
        }
    }

    /// Returns the name to display to the player.
    ///
    /// Before the NPC is introduced, returns the brief anonymous description
    /// (e.g., "a priest"). After introduction, returns the full name.
    pub fn display_name(&self, introduced: bool) -> &str {
        if introduced {
            &self.name
        } else {
            &self.brief_description
        }
    }

    /// Returns the NPC's desired location based on their schedule and the current context.
    ///
    /// Returns `None` if the NPC has no schedule or no entry covers the hour.
    pub fn desired_location(
        &self,
        hour: u8,
        season: Season,
        day_type: DayType,
    ) -> Option<LocationId> {
        self.schedule.as_ref()?.location_at(hour, season, day_type)
    }

    /// Returns the active schedule entry for the current context.
    ///
    /// Returns `None` if the NPC has no schedule or no entry covers the hour.
    pub fn schedule_entry(
        &self,
        hour: u8,
        season: Season,
        day_type: DayType,
    ) -> Option<&types::ScheduleEntry> {
        self.schedule.as_ref()?.entry_at(hour, season, day_type)
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
    /// Pronunciation hints for any secondary-language words used in dialogue.
    #[serde(default, alias = "irish_words")]
    pub language_hints: Vec<LanguageHint>,
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

/// Parses a complete NPC response into dialogue and metadata.
///
/// Splits on a `---` separator line (with optional surrounding whitespace).
/// Everything before is player-visible dialogue/actions. Everything after
/// is parsed as JSON metadata.
/// If no separator is found, the entire text is treated as dialogue.
pub fn parse_npc_stream_response(full_text: &str) -> NpcStreamResponse {
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
            language_hints: Vec::new(),
        });
        return NpcStreamResponse { dialogue, metadata };
    }

    NpcStreamResponse {
        dialogue: full_text.trim().to_string(),
        metadata: None,
    }
}

/// The improv craft guidelines injected into the system prompt when improv mode is enabled.
///
/// Distilled from professional long-form improv principles: Yes-And, specificity,
/// emotional truth, physical grounding, active listening, heightening, and
/// making the scene partner shine.
const IMPROV_CRAFT_SECTION: &str = "\n\
    \n\
    IMPROV CRAFT: You are a scene partner. Follow these principles:\n\
    - YES, AND: Accept what the player establishes and build on it. Disagree in character, but never negate their reality.\n\
    - SPECIFICITY: Use real names, exact amounts, particular objects — never generic placeholders.\n\
    - EMOTIONAL TRUTH: Let comedy and drama emerge from honest reactions, not clever lines.\n\
    - PHYSICAL GROUNDING: Reference the environment — turf smoke, creaking chairs, rain on glass.\n\
    - LISTEN AND REACT: Respond to what was actually said. Let surprises surprise your character.\n\
    - HEIGHTEN: Find the first unusual thing and explore its implications and consequences.\n\
    - RAISE EMOTIONAL STAKES: Go deeper emotionally rather than introducing new plot elements.\n\
    - MAKE THE PLAYER SHINE: Endow the player with qualities and create openings for them to react.\n";

/// Builds the Tier 1 system prompt for an NPC.
///
/// Combines the NPC's identity, personality, occupation, and current
/// mood into a system prompt that establishes character for the LLM.
/// When `improv` is true, includes the improv craft guidelines section
/// to improve improvisational quality of NPC responses.
///
/// The prompt instructs the model to output dialogue first (which is
/// streamed to the player), then a `---` separator, then a JSON metadata
/// block (which is parsed silently for simulation state).
pub fn build_tier1_system_prompt(npc: &Npc, improv: bool) -> String {
    let improv_section = if improv { IMPROV_CRAFT_SECTION } else { "" };
    let intel_guidance = npc.intelligence.prompt_guidance();

    format!(
        "You are {name}, a {age}-year-old {occupation} in a small parish in County Roscommon, \
        Ireland, in the year 1820.\n\
        \n\
        HISTORICAL CONTEXT: Ireland is under British rule following the Acts of Union of 1800. \
        Catholic Emancipation has not yet been achieved. The landlord class is predominantly \
        Protestant and English-speaking, while ordinary people speak both Irish and English. \
        Life is rural and agricultural — there is no electricity, no railways, no photography. \
        Travel is by foot, horse, or cart. News arrives by mail coach or word of mouth. \
        Do not reference anything that does not exist in 1820 Ireland.\n\
        \n\
        CULTURAL GUIDELINES: Portray Irish characters with dignity, warmth, and complexity. \
        Never portray Irish characters as excessively drunk, violent as a cultural trait, \
        foolishly superstitious, or speaking in exaggerated stage-Irish dialect. \
        Avoid phrases like \"Top o' the mornin'\" or \"begorrah.\" \
        Show the wit, intelligence, resilience, and warmth of rural Irish people.\
        {improv_section}\n\
        \n\
        Personality: {personality}\n\
        {intel_guidance}\
        Current mood: {mood}\n\
        \n\
        Respond in character as {name}. Write only what you say aloud — \
        pure dialogue, no narration or action descriptions. \
        Pepper your speech naturally with the occasional Irish word or phrase.\n\
        \n\
        LENGTH: 2-4 sentences. Be conversational, not a monologue.\n\
        \n\
        FORMAT: Write your dialogue, then on a new line write exactly: ---\n\
        Then a JSON metadata block:\n\
        {{\"action\": \"what you physically do\", \"mood\": \"your mood after this\", \
        \"internal_thought\": \"what you think but don't say\", \
        \"irish_words\": [{{\"word\": \"...\", \"pronunciation\": \"...\", \"meaning\": \"...\"}}]}}\n\
        \n\
        Example:\n\
        Ah, good morning to ye! Dia dhuit — fine day for it, so it is. \
        Will ye have a drop of something to warm the bones?\n\
        ---\n\
        {{\"action\": \"looks up from polishing glass, speaks warmly\", \"mood\": \"friendly\", \
        \"internal_thought\": \"New face around here\", \
        \"irish_words\": [{{\"word\": \"Dia dhuit\", \"pronunciation\": \"DEE-ah gwit\", \
        \"meaning\": \"Hello (lit. God to you)\"}}]}}",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        personality = npc.personality,
        intel_guidance = if intel_guidance.is_empty() {
            String::new()
        } else {
            format!("Mind and manner: {intel_guidance}\n")
        },
        mood = npc.mood,
        improv_section = improv_section,
    )
}

/// Builds the action line for an NPC prompt from raw player input.
///
/// Detects `*emote*` syntax (input fully wrapped in asterisks) and
/// formats it as a physical action. All other input is treated as speech.
pub fn build_action_line(player_input: &str) -> String {
    if let Some(inner) = player_input
        .strip_prefix('*')
        .and_then(|s| s.strip_suffix('*'))
        .filter(|inner| !inner.is_empty() && !inner.contains('*'))
    {
        return format!(
            "The traveller performs an action: {inner}\n\
            (The traveller is emoting rather than speaking. \
            Respond to their physical action naturally.)"
        );
    }
    format!("The traveller says: \"{player_input}\"")
}

/// Builds the Tier 1 context prompt for an NPC interaction.
///
/// Includes the current location, time of day, weather, and season.
/// The player's action is intentionally omitted here so callers can
/// append it at the end of the full context (after memory, history, etc.).
pub fn build_tier1_context(world: &WorldState) -> String {
    let location = world.current_location();
    let time_of_day = world.clock.time_of_day();
    let season = world.clock.season();

    format!(
        "Your Location: {loc_name} — {loc_desc}\n\
        Time: {time}\n\
        Season: {season}\n\
        Weather: {weather}",
        loc_name = location.name,
        loc_desc = location.description,
        time = time_of_day,
        season = season,
        weather = world.weather,
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
    fn test_display_name_before_introduction() {
        let npc = Npc::new_test_npc();
        assert_eq!(npc.display_name(false), "an older man behind the bar");
    }

    #[test]
    fn test_display_name_after_introduction() {
        let npc = Npc::new_test_npc();
        assert_eq!(npc.display_name(true), "Padraig O'Brien");
    }

    #[test]
    fn test_build_system_prompt() {
        let npc = Npc::new_test_npc();
        let prompt = build_tier1_system_prompt(&npc, false);
        assert!(prompt.contains("Padraig O'Brien"));
        assert!(prompt.contains("58-year-old"));
        assert!(prompt.contains("Publican"));
        assert!(prompt.contains("content"));
        assert!(prompt.contains("---"));
        assert!(prompt.contains("JSON metadata block"));
        assert!(
            prompt.contains("1820"),
            "prompt should specify the year 1820"
        );
        assert!(
            prompt.contains("Acts of Union"),
            "prompt should mention Acts of Union"
        );
        assert!(
            prompt.contains("CULTURAL GUIDELINES"),
            "prompt should include cultural guidelines"
        );
        assert!(
            prompt.contains("irish_words"),
            "prompt should instruct about irish_words metadata"
        );
    }

    #[test]
    fn test_build_context() {
        let world = WorldState::new();
        let context = build_tier1_context(&world);
        assert!(context.contains("The Crossroads"));
        assert!(context.contains("Morning"));
        assert!(context.contains("Spring"));
        assert!(context.contains("Clear"));
        assert!(context.contains("Your Location:"));
        assert!(!context.contains("is here"));
    }

    #[test]
    fn test_build_action_line_emote() {
        let line = build_action_line("*tips hat*");
        assert!(
            line.contains("performs an action: tips hat"),
            "emote should strip asterisks and use action phrasing"
        );
        assert!(
            line.contains("emoting rather than speaking"),
            "emote should include action-mode instruction"
        );
    }

    #[test]
    fn test_build_action_line_normal_input() {
        let line = build_action_line("hello there");
        assert!(line.contains("The traveller says: \"hello there\""));
        assert!(!line.contains("performs an action"));
    }

    #[test]
    fn test_build_action_line_partial_asterisks() {
        let line = build_action_line("*incomplete");
        assert!(line.contains("The traveller says: \"*incomplete\""));
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
}
