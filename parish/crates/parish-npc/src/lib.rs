//! NPC system for the Parish game engine.
//!
//! Each NPC has personality traits, a daily schedule, relationships
//! with other NPCs, and short-term memory. Cognition fidelity is determined
//! by the NpcManager based on distance from the player.

pub mod anachronism;
pub mod autonomous;
pub mod banshee;
pub mod data;
pub mod manager;
pub mod memory;
pub mod mood;
pub mod overhear;
pub mod reactions;
pub mod schedule;
pub mod ticks;
pub mod tier4;
pub mod tier_assign;
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
    IrishWordHint, LanguageHint, NpcId, extract_dialogue_from_partial_json, floor_char_boundary,
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
    /// Game-time at which this NPC is fated to die, if set.
    ///
    /// Populated by the Tier 4 rules engine when it rolls a `Death` event —
    /// rather than removing the NPC immediately, the doom is scheduled a few
    /// game-hours ahead so that [`crate::banshee`] can herald it with a
    /// keening cry on the night beforehand. Cleared on removal.
    pub doom: Option<chrono::DateTime<chrono::Utc>>,
    /// `true` once the banshee's cry has been emitted for the current [`Self::doom`].
    ///
    /// Prevents the same wail from being produced on every tick while the
    /// doom window is open. Reset to `false` whenever [`Self::doom`] changes.
    pub banshee_heralded: bool,
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
            doom: None,
            banshee_heralded: false,
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

/// Parsed result from an NPC LLM response.
///
/// Contains the player-visible dialogue/action text and the optional
/// metadata parsed from the JSON response.
#[derive(Debug, Clone)]
pub struct NpcStreamResponse {
    /// The dialogue and action text shown to the player.
    pub dialogue: String,
    /// Parsed metadata from the JSON response, if present.
    pub metadata: Option<NpcMetadata>,
}

/// Full JSON response from an NPC interaction (Tier 1).
///
/// The LLM returns this as a complete JSON object via `response_format: json_object`.
/// Contains both the player-visible dialogue and simulation metadata in a single
/// structured response, eliminating the need for separator-based parsing.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcJsonResponse {
    /// The NPC's spoken words and actions, as shown to the player.
    #[serde(default)]
    pub dialogue: String,
    /// What the NPC physically does (e.g. "speaks warmly", "nods", "sighs").
    #[serde(default)]
    pub action: String,
    /// The NPC's mood after this interaction.
    #[serde(default)]
    pub mood: String,
    /// Internal thought (not shown to player, used for simulation).
    #[serde(default)]
    pub internal_thought: Option<String>,
    /// Pronunciation hints for any secondary-language words used in dialogue.
    #[serde(default, alias = "irish_words")]
    pub language_hints: Vec<LanguageHint>,
    /// People the NPC mentioned by name in their dialogue (self-declared by the LLM).
    #[serde(default)]
    pub mentioned_people: Vec<String>,
}

/// Metadata block from an NPC response.
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

/// Parses a complete NPC response (JSON format) into dialogue and metadata.
///
/// Expects a JSON object with a `dialogue` field and metadata fields.
/// Strips Markdown code fences (`` ```json ... ``` ``) that some providers
/// (notably Anthropic) occasionally wrap around JSON output.
/// Falls back to treating the entire text as plain dialogue if JSON parsing fails.
pub fn parse_npc_stream_response(full_text: &str) -> NpcStreamResponse {
    let trimmed = full_text.trim();
    let stripped = strip_json_fence(trimmed);

    if let Ok(json_resp) = serde_json::from_str::<NpcJsonResponse>(stripped) {
        let dialogue = json_resp.dialogue.clone();
        let metadata = Some(NpcMetadata {
            action: json_resp.action,
            mood: json_resp.mood,
            internal_thought: json_resp.internal_thought,
            language_hints: json_resp.language_hints,
            mentioned_people: json_resp.mentioned_people,
        });
        return NpcStreamResponse { dialogue, metadata };
    }

    NpcStreamResponse {
        dialogue: trimmed.to_string(),
        metadata: None,
    }
}

/// Strips Markdown code-fence wrappers that some models emit around JSON.
fn strip_json_fence(raw: &str) -> &str {
    let t = raw.trim();
    if let Some(inner) = t.strip_prefix("```json") {
        return inner
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    if let Some(inner) = t.strip_prefix("```") {
        return inner
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    t
}

/// Language settings derived from the active mod manifest.
///
/// Carries the BCP 47 locale codes that are injected into every dialogue
/// prompt builder via [`language_directive`].
#[derive(Debug, Clone)]
pub struct LanguageSettings {
    /// BCP 47 tag for the primary dialogue language (e.g. `"en-IE"`).
    pub player: String,
    /// BCP 47 tag for the secondary code-switch language, if any (e.g. `"ga-IE"`).
    pub native: Option<String>,
}

impl LanguageSettings {
    /// Constructs a new `LanguageSettings` from a player language and an
    /// optional native language.
    pub fn new(player: impl Into<String>, native: Option<String>) -> Self {
        Self {
            player: player.into(),
            native,
        }
    }

    /// Convenience constructor for tests or monolingual fallbacks.
    pub fn english_only() -> Self {
        Self {
            player: "en".to_string(),
            native: None,
        }
    }
}

/// Renders the locale directive injected into every dialogue system prompt.
///
/// Always emits a leading `LANGUAGE: Speak in {player}.` clause, plus
/// spelling-discipline guidance. When the player language is an English
/// variant other than `en-US`, the directive forbids en-US spellings.
/// When a `native` language is set, the directive instructs the model to
/// code-switch naturally and record secondary-language words in the
/// `language_hints` metadata array.
pub fn language_directive(lang: &LanguageSettings) -> String {
    let player = &lang.player;
    let mut directive = format!(
        "LANGUAGE: Speak in {player}. \
        Use spelling, idioms, and conventions appropriate to that BCP 47 locale."
    );

    let player_lower = player.to_lowercase();
    if player_lower.starts_with("en") && player_lower != "en-us" {
        directive.push_str(&format!(
            " Never use en-US spellings such as \"color\", \"realize\", \
            \"favor\", \"neighbor\", or \"-ize\" verb endings \
            — use the spelling appropriate to {player}."
        ));
    }

    if let Some(native) = &lang.native {
        directive.push_str(&format!(
            " Where a native speaker would naturally code-switch, sprinkle words \
            and short phrases from {native} into your dialogue and record them \
            in the `language_hints` metadata array."
        ));
    } else {
        directive.push_str(&format!(
            " Stay in {player} — do not invent or import other languages."
        ));
    }

    directive
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
/// The prompt instructs the model to return a JSON object containing
/// both the dialogue (streamed to the player) and metadata fields
/// (parsed for simulation state). The `language` parameter controls
/// the locale directive appended at the end of the prompt.
pub fn build_tier1_system_prompt(npc: &Npc, improv: bool, language: &LanguageSettings) -> String {
    let improv_section = if improv { IMPROV_CRAFT_SECTION } else { "" };
    let intel_guidance = npc.intelligence.prompt_guidance();

    let mut prompt = format!(
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
        Respond in character as {name}. You MUST respond with a JSON object. \
        Put the \"dialogue\" field FIRST. The dialogue should contain only what you say aloud — \
        pure dialogue, no narration or action descriptions.\n\
        \n\
        LENGTH: 2-4 sentences. Be conversational, not a monologue.\n\
        \n\
        JSON fields:\n\
        - \"dialogue\": your spoken words (this is shown to the player)\n\
        - \"action\": what you physically do (e.g. \"speaks warmly\", \"nods\", \"sighs\")\n\
        - \"mood\": your mood after this interaction\n\
        - \"internal_thought\": what you're thinking but not saying (optional)\n\
        - \"language_hints\": array of any secondary-language words you used, each with:\n\
          - \"word\": the word as written\n\
          - \"pronunciation\": phonetic guide in English\n\
          - \"meaning\": English translation\n\
        \n\
        Example response:\n\
        {{\"dialogue\": \"Ah, good morning to ye! Fine day for it, so it is. \
        Will ye have a drop of something to warm the bones?\", \
        \"action\": \"looks up from polishing glass, speaks warmly\", \"mood\": \"friendly\", \
        \"internal_thought\": \"New face around here\", \
        \"language_hints\": []}}",
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
    );

    prompt.push_str("\n\n");
    prompt.push_str(&language_directive(language));
    prompt
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
    client: &parish_inference::AnyClient,
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
        month = now.format("%B"),
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
        let lang = LanguageSettings::english_only();
        let prompt = build_tier1_system_prompt(&npc, false, &lang);
        assert!(prompt.contains("Padraig O'Brien"));
        assert!(prompt.contains("58-year-old"));
        assert!(prompt.contains("Publican"));
        assert!(prompt.contains("content"));
        assert!(
            prompt.contains("JSON object"),
            "prompt should instruct JSON object response format"
        );
        assert!(
            prompt.contains("\"dialogue\""),
            "prompt should mention the dialogue field"
        );
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
            prompt.contains("language_hints"),
            "prompt should instruct about language_hints metadata"
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
    fn test_npc_json_response_deserialize_full() {
        let json = r#"{
            "dialogue": "Ah, good morning to ye!",
            "action": "speaks",
            "mood": "friendly",
            "internal_thought": "Haven't seen this one before.",
            "irish_words": [{"word": "Dia dhuit", "pronunciation": "DEE-ah gwit", "meaning": "Hello"}]
        }"#;
        let resp: NpcJsonResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.dialogue, "Ah, good morning to ye!");
        assert_eq!(resp.action, "speaks");
        assert_eq!(resp.mood, "friendly");
        assert_eq!(
            resp.internal_thought,
            Some("Haven't seen this one before.".to_string())
        );
        assert_eq!(resp.language_hints.len(), 1);
    }

    #[test]
    fn test_npc_json_response_deserialize_minimal() {
        let json = r#"{"dialogue": "Hello!"}"#;
        let resp: NpcJsonResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.dialogue, "Hello!");
        assert_eq!(resp.action, "");
        assert_eq!(resp.mood, "");
        assert!(resp.internal_thought.is_none());
        assert!(resp.language_hints.is_empty());
    }

    #[test]
    fn test_parse_npc_stream_response_json() {
        let text = r#"{"dialogue": "(Looks up) Ah, good morning to ye!", "action": "speaks", "mood": "friendly"}"#;
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "(Looks up) Ah, good morning to ye!");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "speaks");
        assert_eq!(meta.mood, "friendly");
    }

    #[test]
    fn test_parse_npc_stream_response_plain_text_fallback() {
        let text = "Well hello there, stranger!";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Well hello there, stranger!");
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_parse_npc_stream_response_empty() {
        let parsed = parse_npc_stream_response("");
        assert_eq!(parsed.dialogue, "");
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_parse_npc_stream_response_invalid_json() {
        let text = "{not valid json at all";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "{not valid json at all");
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_parse_npc_stream_response_empty_json() {
        let text = "{}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "");
        assert_eq!(meta.mood, "");
        assert!(meta.internal_thought.is_none());
        assert!(meta.language_hints.is_empty());
    }

    #[test]
    fn test_parse_npc_stream_response_fenced_json() {
        let text = "```json\n{\"dialogue\": \"Hello there!\", \"mood\": \"friendly\"}\n```";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Hello there!");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.mood, "friendly");
    }

    #[test]
    fn test_parse_npc_stream_response_fenced_json_untagged() {
        let text = "```\n{\"dialogue\": \"Good day!\", \"action\": \"waves\"}\n```";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Good day!");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.action, "waves");
    }

    #[test]
    fn test_strip_json_fence_plain() {
        assert_eq!(strip_json_fence(r#"{"a":1}"#), r#"{"a":1}"#);
    }

    #[test]
    fn test_strip_json_fence_markdown() {
        assert_eq!(strip_json_fence("```json\n{\"a\":1}\n```"), r#"{"a":1}"#);
    }

    // ── Issue #731 — prompt template placeholder interpolation ───────────────

    /// Tier 1 system prompt: every `{placeholder}` must be substituted.
    ///
    /// Uses the canonical test NPC fixture so any new placeholder added to the
    /// template without a matching format-argument will cause a compile error
    /// or leave a literal `{key}` in the output that this test catches.
    ///
    /// Note: the prompt embeds a JSON example block whose keys use single braces
    /// (e.g. `{"action": ...}`). The regex `\{[a-z_]+\}` matches only
    /// lower-case-word placeholders and skips those JSON key-value pairs, so
    /// false positives from the example block are not possible.
    #[test]
    fn test_tier1_system_no_unsubstituted_placeholders() {
        let re = regex::Regex::new(r"\{[a-z_]+\}").unwrap();
        let npc = Npc::new_test_npc();
        let lang = LanguageSettings::english_only();
        let prompt = build_tier1_system_prompt(&npc, false, &lang);

        // No word-placeholder should survive substitution.
        assert!(
            !re.is_match(&prompt),
            "Unsubstituted placeholder found in tier1 system prompt: {:?}",
            re.find(&prompt).map(|m| m.as_str()),
        );

        // Known values must appear.
        assert!(prompt.contains("Padraig O'Brien"), "NPC name missing");
        assert!(prompt.contains("58"), "NPC age missing");
        assert!(prompt.contains("Publican"), "NPC occupation missing");
        assert!(prompt.contains("content"), "NPC mood missing");

        // Anachronism and cultural guidelines are part of the contract; a
        // future edit that removes them will trip this test intentionally.
        assert!(
            prompt.contains("Acts of Union"),
            "historical context missing"
        );
        assert!(
            prompt.contains("CULTURAL GUIDELINES"),
            "cultural guidelines missing"
        );
    }

    /// Tier 1 context prompt: every `{placeholder}` must be substituted.
    ///
    /// Uses a world backed by a real `WorldGraph` containing a
    /// `description_template` with `{time}`, `{weather}`, and `{npcs_present}`
    /// so that the `render_description` path is exercised — the one place where
    /// silent leakage can actually occur at runtime (`.replace()` is not
    /// compile-checked).
    #[test]
    fn test_tier1_context_no_unsubstituted_placeholders() {
        use parish_world::{WorldState, graph::WorldGraph};

        let re = regex::Regex::new(r"\{[a-z_]+\}").unwrap();

        // Build a world whose description_template contains all three dynamic
        // placeholders, so render_description must replace each of them.
        let graph_json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "The Crossroads",
                    "description_template": "A crossroads at {time}. The sky is {weather}. {npcs_present} stand nearby.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.618,
                    "lon": -8.095,
                    "connections": [{"target": 2, "path_description": "a lane"}],
                    "associated_npcs": []
                },
                {
                    "id": 2,
                    "name": "The Church",
                    "description_template": "The church at {time}.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.620,
                    "lon": -8.097,
                    "connections": [{"target": 1, "path_description": "back"}],
                    "associated_npcs": []
                }
            ]
        }"#;

        let mut world = WorldState::new();
        world.graph = WorldGraph::load_from_str(graph_json).unwrap();

        let context = build_tier1_context(&world);

        // No word-placeholder should survive substitution.
        assert!(
            !re.is_match(&context),
            "Unsubstituted placeholder found in tier1 context: {:?} — full output: {context}",
            re.find(&context).map(|m| m.as_str()),
        );

        // Known values must appear in the rendered output.
        assert!(context.contains("The Crossroads"), "location name missing");
        // WorldState::new() starts at 08:00 → TimeOfDay::Morning → "morning"
        assert!(
            context.contains("morning"),
            "time-of-day substitution missing"
        );
        // WorldState::new() sets Weather::Clear → weather_display produces "clear"
        assert!(context.contains("clear"), "weather substitution missing");
        // Date / season from WorldState::new(): 20 March 1820, Spring
        assert!(context.contains("1820"), "year missing from context");
        assert!(context.contains("Spring"), "season missing from context");
    }

    /// Tier 2 system prompt: every `{placeholder}` must be substituted.
    ///
    /// `build_tier2_prompt` is a pure `format!()` call, so a new placeholder
    /// added without a matching argument will cause a compile error.  This test
    /// guards the runtime values: location name, time, weather, and at least one
    /// NPC name must all appear in the final output.
    #[test]
    fn test_tier2_system_no_unsubstituted_placeholders() {
        use crate::ticks::{NpcSnapshot, Tier2Group, build_tier2_prompt};
        use parish_types::{LocationId, NpcId};

        let re = regex::Regex::new(r"\{[a-z_]+\}").unwrap();

        let group = Tier2Group {
            location: LocationId(2),
            location_name: "Darcy's Pub".to_string(),
            npcs: vec![
                NpcSnapshot {
                    id: NpcId(1),
                    name: "Brigid Murphy".to_string(),
                    occupation: "Weaver".to_string(),
                    personality: "Steady and observant".to_string(),
                    intelligence_tag: "INT[V3 A4 E4 P5 W4 C3]".to_string(),
                    mood: "thoughtful".to_string(),
                    relationship_context: String::new(),
                },
                NpcSnapshot {
                    id: NpcId(7),
                    name: "Seamus Fahey".to_string(),
                    occupation: "Blacksmith".to_string(),
                    personality: "Blunt and loyal".to_string(),
                    intelligence_tag: "INT[V2 A3 E2 P5 W3 C2]".to_string(),
                    mood: "tired".to_string(),
                    relationship_context: String::new(),
                },
            ],
        };

        let lang = LanguageSettings::english_only();
        let prompt = build_tier2_prompt(&group, "Evening", "Overcast", &lang);

        // No word-placeholder should survive substitution.
        assert!(
            !re.is_match(&prompt),
            "Unsubstituted placeholder found in tier2 system prompt: {:?}",
            re.find(&prompt).map(|m| m.as_str()),
        );

        // Known values from the fixture must appear.
        assert!(prompt.contains("Darcy's Pub"), "location name missing");
        assert!(prompt.contains("Evening"), "time missing");
        assert!(prompt.contains("Overcast"), "weather missing");
        assert!(prompt.contains("Brigid Murphy"), "NPC name 1 missing");
        assert!(prompt.contains("Seamus Fahey"), "NPC name 2 missing");
        assert!(prompt.contains("Weaver"), "occupation missing");
        assert!(prompt.contains("thoughtful"), "mood missing");
    }

    // ── language_directive tests ───────────────────────────────────────────────

    #[test]
    fn language_directive_en_ie_with_native_ga_ie() {
        let lang = LanguageSettings::new("en-IE".to_string(), Some("ga-IE".to_string()));
        let directive = language_directive(&lang);
        assert!(
            directive.contains("en-IE"),
            "directive should name the player locale"
        );
        assert!(
            directive.contains("en-US"),
            "directive should warn against en-US spellings for non-en-US English"
        );
        assert!(
            directive.contains("ga-IE"),
            "directive should name the native language"
        );
        assert!(
            directive.contains("language_hints"),
            "directive should mention the language_hints metadata field"
        );
        // Should NOT tell the NPC to stay in one language when a native is given
        assert!(
            !directive.contains("do not invent or import other languages"),
            "mono-language restriction must not appear when native language is set"
        );
    }

    #[test]
    fn language_directive_en_us_no_native() {
        let lang = LanguageSettings::new("en-US".to_string(), None);
        let directive = language_directive(&lang);
        assert!(
            directive.contains("en-US"),
            "directive should name the locale"
        );
        // en-US should NOT get the anti-en-US-spelling warning
        assert!(
            !directive.contains("Never use en-US spellings"),
            "en-US locale must not warn against itself"
        );
        assert!(
            directive.contains("do not invent or import other languages"),
            "mono-language restriction should appear when no native language is set"
        );
    }

    #[test]
    fn language_directive_fr_fr_no_native() {
        let lang = LanguageSettings::new("fr-FR".to_string(), None);
        let directive = language_directive(&lang);
        assert!(
            directive.contains("fr-FR"),
            "directive should name the locale"
        );
        // Non-English locale should NOT get the en-US spelling warning
        assert!(
            !directive.contains("en-US spellings"),
            "en-US spelling warning must not appear for non-English locale"
        );
        assert!(
            directive.contains("do not invent or import other languages"),
            "mono-language restriction should appear when no native language is set"
        );
    }

    #[test]
    fn tier1_prompt_contains_language_directive() {
        let npc = Npc::new_test_npc();
        let lang = LanguageSettings::new("en-IE".to_string(), Some("ga-IE".to_string()));
        let prompt = build_tier1_system_prompt(&npc, false, &lang);
        assert!(
            prompt.contains("LANGUAGE:"),
            "tier1 system prompt should embed the language directive"
        );
        assert!(
            prompt.contains("en-IE"),
            "tier1 system prompt should name the player locale"
        );
        assert!(
            prompt.contains("ga-IE"),
            "tier1 system prompt should name the native language"
        );
    }
}

/// Shared NPC test fixtures used by unit tests across multiple modules.
///
/// Centralised here so that helper constructors like `make_test_npc` are
/// defined once and re-used everywhere, avoiding duplication across
/// `schedule`, `tier_assign`, `banshee`, and `manager` test suites.
#[cfg(test)]
pub(crate) mod test_helpers {
    use std::collections::HashMap;
    use std::path::Path;

    use chrono::{TimeZone, Utc};

    use crate::Npc;
    use crate::memory::{LongTermMemory, ShortTermMemory};
    use crate::reactions::ReactionLog;
    use crate::types::{Intelligence, NpcState, ScheduleEntry, ScheduleVariant, SeasonalSchedule};
    use parish_types::{LocationId, NpcId};
    use parish_world::WorldState;
    use parish_world::graph::WorldGraph;
    use parish_world::time::GameClock;

    /// Minimal NPC with all required fields populated.
    pub fn make_test_npc(id: u32, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {id}"),
            brief_description: "a person".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test personality".to_string(),
            intelligence: Intelligence::default(),
            location: LocationId(location),
            mood: "calm".to_string(),
            home: Some(LocationId(location)),
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::Present,
            deflated_summary: None,
            reaction_log: ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
        }
    }

    /// NPC with a three-slot daily schedule: sleep at home, work, evening at home.
    pub fn make_scheduled_npc(id: u32, home: u32, work: u32) -> Npc {
        let mut npc = make_test_npc(id, home);
        npc.schedule = Some(SeasonalSchedule {
            variants: vec![ScheduleVariant {
                season: None,
                day_type: None,
                entries: vec![
                    ScheduleEntry {
                        start_hour: 0,
                        end_hour: 7,
                        location: LocationId(home),
                        activity: "sleeping".to_string(),
                        cuaird: false,
                    },
                    ScheduleEntry {
                        start_hour: 8,
                        end_hour: 17,
                        location: LocationId(work),
                        activity: "working".to_string(),
                        cuaird: false,
                    },
                    ScheduleEntry {
                        start_hour: 18,
                        end_hour: 23,
                        location: LocationId(home),
                        activity: "evening rest".to_string(),
                        cuaird: false,
                    },
                ],
            }],
        });
        npc
    }

    /// Loads the real parish graph; skips the calling test if the file is absent.
    pub fn load_test_graph() -> Option<WorldGraph> {
        let path = Path::new("data/parish.json");
        if path.exists() {
            WorldGraph::load_from_file(path).ok()
        } else {
            None
        }
    }

    /// Builds a linear chain graph 0 — 1 — 2 — … — n.
    pub fn make_chain_graph(n: u32) -> WorldGraph {
        let locations: Vec<serde_json::Value> = (0..=n)
            .map(|i| {
                let mut conns = Vec::new();
                if i > 0 {
                    conns.push(serde_json::json!({"target": i - 1, "path_description": "a path"}));
                }
                if i < n {
                    conns.push(serde_json::json!({"target": i + 1, "path_description": "a path"}));
                }
                serde_json::json!({
                    "id": i,
                    "name": format!("Loc {i}"),
                    "description_template": "Test",
                    "indoor": false,
                    "public": true,
                    "connections": conns
                })
            })
            .collect();
        let json = serde_json::json!({"locations": locations}).to_string();
        WorldGraph::load_from_str(&json).unwrap()
    }

    /// WorldState with the given graph and player location.
    pub fn make_test_world(graph: WorldGraph, player_location: u32) -> WorldState {
        let mut world = WorldState::new();
        world.graph = graph;
        world.player_location = LocationId(player_location);
        world
    }

    /// WorldState seeded at 22:00 (night, inside the banshee herald window).
    pub fn make_mourning_world() -> WorldState {
        let mut world = WorldState::new();
        world.graph = make_chain_graph(4);
        world.player_location = LocationId(0);
        world.clock = GameClock::new(Utc.with_ymd_and_hms(1820, 6, 15, 22, 0, 0).unwrap());
        world
    }
}
