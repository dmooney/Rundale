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

use chrono::{Datelike, Timelike};
use memory::{LongTermMemory, ShortTermMemory};
use parish_types::{DayType, LocationId, Season};
use parish_world::WorldState;
use parish_world::description::render_description;
use reactions::ReactionLog;
use transitions::NpcSummary;
use types::{Intelligence, NpcState, Relationship, SeasonalSchedule};

// Re-export shared types from parish-types
pub use parish_types::{
    IrishWordHint, LanguageHint, NpcId, SEPARATOR_HOLDBACK, find_response_separator,
    floor_char_boundary,
};

// Re-export the NPC data-file schema so downstream crates (e.g. the Parish
// Designer editor) can round-trip `npcs.json` without duplicating the schema.
pub use data::{
    IntelligenceFileEntry, NpcFile, NpcFileEntry, RelationshipFileEntry, ScheduleFileEntry,
    ScheduleVariantFileEntry,
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
    /// People the NPC mentioned by name in their dialogue (self-declared by the LLM).
    #[serde(default)]
    pub mentioned_people: Vec<String>,
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
            mentioned_people: Vec::new(),
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
    - SPECIFICITY: Ground your dialogue in particular objects, sounds, smells, and amounts. Only refer to people by name if they appear in your PEOPLE YOU KNOW list or are present at your location. If you don't know someone's name, describe them naturally ('a lad from the next townland', 'the newcomer').\n\
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
            "The newcomer performs an action: {inner}\n\
            (The newcomer is emoting rather than speaking. \
            Respond to their physical action naturally.)"
        );
    }
    format!("The newcomer says: \"{player_input}\"")
}

/// Builds the action line for an NPC prompt, using the player's name if the NPC knows it.
///
/// This is the name-aware variant of [`build_action_line`]. If `player_name` is provided,
/// the NPC addresses the player by name. Otherwise falls back to "The newcomer".
pub fn build_named_action_line(player_input: &str, player_name: Option<&str>) -> String {
    let label = player_name.unwrap_or("The newcomer");

    if let Some(inner) = player_input
        .strip_prefix('*')
        .and_then(|s| s.strip_suffix('*'))
        .filter(|inner| !inner.is_empty() && !inner.contains('*'))
    {
        return format!(
            "{label} performs an action: {inner}\n\
            ({label} is emoting rather than speaking. \
            Respond to their physical action naturally.)"
        );
    }
    format!("{label} says: \"{player_input}\"")
}

/// Detects if the player is introducing themselves by name.
///
/// Matches patterns like "My name is Ciaran", "I'm Ciaran", "Call me Ciaran".
/// Returns the extracted name if found.
pub fn detect_player_name(input: &str) -> Option<String> {
    use regex::Regex;
    use std::sync::LazyLock;

    static NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)(?:my name(?:'s| is)|I'm|I am|they call me|call me|the name's|name is)\s+(?-i:([A-Z][a-zA-Z']+(?:\s+[A-Z][a-zA-Z']+)?))",
        )
        .unwrap()
    });

    NAME_RE.captures(input).and_then(|caps| -> Option<String> {
        let name = caps.get(1)?.as_str().to_string();
        // Reject very short names (likely false positives)
        if name.len() < 2 {
            return None;
        }
        Some(name)
    })
}

/// Validates the people mentioned in an NPC's dialogue against a known roster.
///
/// Returns a list of hallucinated names — names that appear in `mentioned`
/// but don't match any entry in the roster, the player's name, or known
/// location names.
pub fn validate_mentioned_people(
    mentioned: &[String],
    known_roster: &[(NpcId, String, String)],
    player_name: Option<&str>,
) -> Vec<String> {
    if mentioned.is_empty() {
        return Vec::new();
    }

    let mut hallucinated = Vec::new();
    for name in mentioned {
        let lower = name.to_lowercase();
        // Skip empty names
        if lower.is_empty() {
            continue;
        }

        // Check against player name
        if player_name.is_some_and(|pn| pn.to_lowercase() == lower) {
            continue;
        }

        // Check against roster (full name or first name match)
        let in_roster = known_roster.iter().any(|(_, roster_name, _)| {
            let roster_lower = roster_name.to_lowercase();
            roster_lower == lower
                || roster_lower
                    .split_whitespace()
                    .next()
                    .is_some_and(|first| first == lower)
        });

        if !in_roster {
            hallucinated.push(name.clone());
        }
    }
    hallucinated
}

/// Response type for the reference extraction pre-pass.
#[derive(Debug, Clone, Deserialize)]
struct ReferencePrePassResponse {
    #[serde(default)]
    names: Vec<String>,
}

/// Asks a small/fast model which people from the roster an NPC would
/// naturally reference when responding to the player's input.
///
/// Returns validated names (filtered against the roster). Used as the
/// first pass of two-pass dialogue generation to prevent hallucinated names.
pub async fn extract_intended_references(
    client: &parish_inference::openai_client::OpenAiClient,
    model: &str,
    npc_name: &str,
    player_input: &str,
    known_roster: &[(NpcId, String, String)],
) -> Vec<String> {
    if known_roster.is_empty() {
        return Vec::new();
    }

    let roster_list: Vec<String> = known_roster
        .iter()
        .map(|(_, name, occ)| format!("{} ({})", name, occ))
        .collect();
    let roster_str = roster_list.join(", ");

    let prompt = format!(
        "You are {npc_name}. A newcomer says: \"{player_input}\"\n\
        People you know: {roster_str}\n\n\
        Which of these people would you naturally mention in your reply? \
        Return a JSON object: {{\"names\": [\"Name1\", \"Name2\"]}} \
        or {{\"names\": []}} if none."
    );

    match client
        .generate_json::<ReferencePrePassResponse>(model, &prompt, None, Some(100), None)
        .await
    {
        Ok(resp) => {
            // Filter against roster to be safe
            resp.names
                .into_iter()
                .filter(|name: &String| {
                    let lower = name.to_lowercase();
                    known_roster.iter().any(|(_, rn, _)| {
                        let rl = rn.to_lowercase();
                        rl == lower
                            || rl
                                .split_whitespace()
                                .next()
                                .is_some_and(|first| first == lower)
                    })
                })
                .collect()
        }
        Err(e) => {
            tracing::warn!("Reference pre-pass failed: {e}");
            Vec::new()
        }
    }
}

/// Formats validated references as a context injection for the main dialogue prompt.
pub fn format_reference_hint(validated_names: &[String]) -> String {
    if validated_names.is_empty() {
        "You don't need to mention anyone specific in your response.".to_string()
    } else {
        format!(
            "People you may reference in your response: {}",
            validated_names.join(", ")
        )
    }
}

/// Returns the full name of a calendar month (1–12).
fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}

/// Builds the Tier 1 context prompt for an NPC interaction.
///
/// Renders the location description template (substituting `{time}`,
/// `{weather}`, and `{npcs_present}` placeholders) and includes the
/// full game date and time so NPCs have precise temporal context.
/// The player's action is intentionally omitted here so callers can
/// append it at the end of the full context (after memory, history, etc.).
pub fn build_tier1_context(world: &WorldState) -> String {
    let time_of_day = world.clock.time_of_day();
    let season = world.clock.season();
    let now = world.clock.now();

    // Render the location description with current time/weather substituted.
    let rendered_desc = if let Some(loc_data) = world.current_location_data() {
        render_description(loc_data, time_of_day, &world.weather.to_string(), &[])
    } else {
        world.current_location().description.clone()
    };

    let date_time_str = format!(
        "{day_of_week} {day} {month} {year} | {hour:02}:{minute:02} | {season}",
        day_of_week = now.format("%A"),
        day = now.day(),
        month = month_name(now.month()),
        year = now.year(),
        hour = now.hour(),
        minute = now.minute(),
        season = season,
    );

    format!(
        "Your Location: {loc_name} — {loc_desc}\n\
        Date and time: {date_time}",
        loc_name = world.current_location().name,
        loc_desc = rendered_desc,
        date_time = date_time_str,
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
        // Time of day is conveyed by the clock time (e.g. 08:00), not a separate label
        assert!(context.contains("Spring"));
        assert!(context.contains("1820"));
        assert!(context.contains("Your Location:"));
        assert!(context.contains("Date and time:"));
        assert!(!context.contains("is here"));
        // Weather is now embedded in the rendered description, not a standalone line
        assert!(!context.contains("\nWeather:"));
        assert!(!context.contains("\nTime:"));
        assert!(!context.contains("\nSeason:"));
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
        assert!(line.contains("The newcomer says: \"hello there\""));
        assert!(!line.contains("performs an action"));
    }

    #[test]
    fn test_build_action_line_partial_asterisks() {
        let line = build_action_line("*incomplete");
        assert!(line.contains("The newcomer says: \"*incomplete\""));
    }

    #[test]
    fn test_build_named_action_line_with_name() {
        let line = build_named_action_line("hello", Some("Ciaran"));
        assert_eq!(line, "Ciaran says: \"hello\"");
    }

    #[test]
    fn test_build_named_action_line_without_name() {
        let line = build_named_action_line("hello", None);
        assert_eq!(line, "The newcomer says: \"hello\"");
    }

    #[test]
    fn test_build_named_action_line_emote_with_name() {
        let line = build_named_action_line("*tips hat*", Some("Ciaran"));
        assert!(line.contains("Ciaran performs an action: tips hat"));
    }

    #[test]
    fn test_detect_player_name_my_name_is() {
        assert_eq!(
            detect_player_name("My name is Ciaran"),
            Some("Ciaran".to_string())
        );
    }

    #[test]
    fn test_detect_player_name_im() {
        assert_eq!(
            detect_player_name("I'm Padraig O'Brien"),
            Some("Padraig O'Brien".to_string())
        );
    }

    #[test]
    fn test_detect_player_name_call_me() {
        assert_eq!(detect_player_name("Call me Sean"), Some("Sean".to_string()));
    }

    #[test]
    fn test_detect_player_name_no_match() {
        assert_eq!(detect_player_name("hello there"), None);
        assert_eq!(detect_player_name("what is your name?"), None);
    }

    #[test]
    fn test_detect_player_name_in_sentence() {
        assert_eq!(
            detect_player_name("Well, my name is Maeve if you must know"),
            Some("Maeve".to_string())
        );
    }

    #[test]
    fn test_validate_mentioned_people_known() {
        let roster = vec![
            (
                NpcId(1),
                "Padraig Darcy".to_string(),
                "publican".to_string(),
            ),
            (
                NpcId(2),
                "Mary O'Sullivan".to_string(),
                "weaver".to_string(),
            ),
        ];
        let mentioned = vec!["Padraig".to_string()];
        let hallucinated = validate_mentioned_people(&mentioned, &roster, None);
        assert!(hallucinated.is_empty());
    }

    #[test]
    fn test_validate_mentioned_people_hallucinated() {
        let roster = vec![(
            NpcId(1),
            "Padraig Darcy".to_string(),
            "publican".to_string(),
        )];
        let mentioned = vec!["Padraig".to_string(), "Seamus".to_string()];
        let hallucinated = validate_mentioned_people(&mentioned, &roster, None);
        assert_eq!(hallucinated, vec!["Seamus".to_string()]);
    }

    #[test]
    fn test_validate_mentioned_people_player_name() {
        let roster = vec![];
        let mentioned = vec!["Ciaran".to_string()];
        let hallucinated = validate_mentioned_people(&mentioned, &roster, Some("Ciaran"));
        assert!(hallucinated.is_empty());
    }

    #[test]
    fn test_validate_mentioned_people_empty() {
        let roster = vec![];
        let hallucinated = validate_mentioned_people(&[], &roster, None);
        assert!(hallucinated.is_empty());
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
    fn test_parse_npc_stream_response_empty_metadata() {
        let text = "Hello there!\n---\n{}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Hello there!");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "");
        assert_eq!(meta.mood, "");
        assert!(meta.internal_thought.is_none());
        assert!(meta.language_hints.is_empty());
    }

    #[test]
    fn test_parse_npc_stream_response_separator_only_newlines() {
        let text = "\n---\n";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "");
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_parse_npc_stream_response_multiline_dialogue() {
        let text = "Ah, hello there!\nWelcome to Kilteevan.\nCome in, come in.\n---\n{\"action\": \"beckons\", \"mood\": \"welcoming\"}";
        let parsed = parse_npc_stream_response(text);
        assert!(parsed.dialogue.contains("Welcome to Kilteevan."));
        assert!(parsed.dialogue.contains("Come in, come in."));
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "beckons");
    }

    #[test]
    fn test_parse_npc_stream_response_triple_dash_in_dialogue() {
        let text = "The road is long --- perhaps too long.\n---\n{\"mood\": \"weary\"}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "The road is long --- perhaps too long.");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.mood, "weary");
    }

    #[test]
    fn test_parse_npc_stream_response_whitespace_only_dialogue() {
        let text = "   \n---\n{\"action\": \"silent\", \"mood\": \"pensive\"}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "silent");
    }
}
