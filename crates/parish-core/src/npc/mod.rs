//! NPC system — identity, behavior, cognition, and relationships.
//!
//! Each NPC has personality traits, a daily schedule, relationships
//! with other NPCs, and short/long-term memory. Cognition fidelity
//! scales with distance from the player (4 LOD tiers).

pub mod anachronism;
pub mod data;
pub mod manager;
pub mod memory;
pub mod mood;
pub mod overhear;
pub mod ticks;
pub mod transitions;
pub mod types;

use std::collections::HashMap;

use crate::world::{LocationId, WorldState};
use serde::{Deserialize, Serialize};

use memory::ShortTermMemory;
use transitions::NpcSummary;
use types::{DailySchedule, Intelligence, NpcState, Relationship};

/// A pronunciation hint for a word in the setting's secondary language.
///
/// Extracted from NPC response metadata and displayed in the
/// pronunciation sidebar to help players with unfamiliar words.
/// The mod's prompt template instructs the LLM to produce these.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LanguageHint {
    /// The word as it appears in text.
    pub word: String,
    /// Approximate English phonetic pronunciation.
    pub pronunciation: String,
    /// English meaning or gloss.
    #[serde(default)]
    pub meaning: Option<String>,
}

/// Backward-compatible alias for [`LanguageHint`].
pub type IrishWordHint = LanguageHint;

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

/// Unique identifier for an NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NpcId(pub u32);

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
    /// Daily schedule defining where the NPC goes at what time.
    pub schedule: Option<DailySchedule>,
    /// Relationships to other NPCs, keyed by their id.
    pub relationships: HashMap<NpcId, Relationship>,
    /// Ring buffer of recent memories.
    pub memory: ShortTermMemory,
    /// Things this NPC knows (local gossip, history, etc.).
    pub knowledge: Vec<String>,
    /// Whether the NPC is present at their location or in transit.
    pub state: NpcState,
    /// Compact summary from the last tier deflation, if any.
    ///
    /// Set when the NPC drops to a lower cognitive tier; cleared when
    /// they are inflated back to a higher tier.
    pub deflated_summary: Option<NpcSummary>,
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
            knowledge: Vec::new(),
            state: NpcState::default(),
            deflated_summary: None,
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

    /// Returns the NPC's desired location based on their schedule and the current hour.
    ///
    /// Returns `None` if the NPC has no schedule or no entry covers the hour.
    pub fn desired_location(&self, hour: u8) -> Option<LocationId> {
        self.schedule.as_ref()?.location_at(hour)
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
            language_hints: Vec::new(),
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

/// The improv craft guidelines injected into the system prompt when improv mode is enabled.
///
/// Distilled from professional long-form improv principles: Yes-And, specificity,
/// emotional truth, physical grounding, active listening, heightening, and
/// making the scene partner shine.
const IMPROV_CRAFT_SECTION: &str = "\n\
    \n\
    IMPROV CRAFT: You are a scene partner, not a chatbot. Follow these principles:\n\
    \n\
    - YES, AND: Accept everything the player establishes as true and build on it. \
    Add new information that enriches the scene rather than redirecting it. \
    Your character can disagree with the player's character, but never negate \
    the reality they have established. If the player says they saw a ghost on the \
    hill, there was something on the hill — even if your character is skeptical.\n\
    \n\
    - SPECIFICITY: Choose the specific over the general. Real place names, exact \
    amounts, particular objects. \"A cracked jug of buttermilk left over from Tuesday\" \
    not \"a drink.\" \"The third stone from the left in Brennan's wall\" not \"a rock.\" \
    Specific emotions, not vague moods.\n\
    \n\
    - EMOTIONAL TRUTH: Scenes are about relationships and honest reactions, not \
    clever lines. The comedy and drama emerge from truthful characters responding to \
    circumstances. If the moment calls for vulnerability, go there. Do not reach \
    for jokes — let humor arise from specificity and human nature.\n\
    \n\
    - PHYSICAL GROUNDING: Use object work. Touch things, reference the environment. \
    Ground every exchange in the physical space — the creak of a chair, the smell of \
    turf smoke, rain on the windowpane. Your character inhabits a body in a place.\n\
    \n\
    - LISTEN AND REACT: Respond to what was actually said, not what you expected. \
    If the player says something surprising, let it surprise your character too. \
    If they make what seems like a mistake, treat it as intentional and justify it \
    within the scene.\n\
    \n\
    - HEIGHTEN: Notice the first unusual thing and explore its implications. \
    \"If this is true, what else is true?\" Same pattern, higher stakes. If the \
    player mentions they owe money to the landlord, build on that — who else knows? \
    What are the consequences? What does your character know about it?\n\
    \n\
    - RAISE EMOTIONAL STAKES: When a conversation feels stuck, go deeper emotionally, \
    not wider logistically. Do not introduce new plot elements — have your character \
    admit something vulnerable, recall a memory, or reveal a deeper feeling about \
    what is already happening.\n\
    \n\
    - MAKE THE PLAYER SHINE: Set the player up for interesting moments. Endow them \
    with characteristics (\"You always were the sharp one, so\"). Create openings \
    for them to react rather than steamrolling with your own ideas. Mirror their \
    energy and commitment level.\n";

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
    let intel_tag = npc.intelligence.prompt_tag();
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
        \n\
        {intel_legend}\n\
        {intel_tag}\n\
        {intel_guidance}\n\
        \n\
        Current mood: {mood}\n\
        \n\
        Respond in character as {name}.\n\
        \n\
        LENGTH: Keep your dialogue to 2-4 sentences. Be natural and conversational — \
        this is a back-and-forth exchange, not a monologue. Say what you would naturally \
        say, then let the player respond. Do not narrate at length or give speeches.\n\
        \n\
        Use this EXACT format:\n\
        \n\
        1. First, write what you say or do, in plain text. Stay in character. \
        Pepper your speech naturally with the occasional Irish word or phrase. \
        Describe actions in parentheses, e.g. (leans on the bar).\n\
        2. Then on a new line write exactly: ---\n\
        3. Then on the next line write a JSON metadata block with these fields:\n\
        - \"action\": what you physically do (e.g. \"speaks\", \"nods\", \"sighs\")\n\
        - \"mood\": your mood after this interaction\n\
        - \"internal_thought\": what you're thinking but not saying (optional)\n\
        - \"irish_words\": array of any Irish words you used, each with:\n\
          - \"word\": the Irish word as written\n\
          - \"pronunciation\": phonetic guide in English (e.g. \"SLAWN-cha\" for \"sláinte\")\n\
          - \"meaning\": English translation\n\
        \n\
        Example response:\n\
        (Looks up from polishing a glass) Ah, good morning to ye! Dia dhuit — fine day for it, \
        so it is. Will ye have a drop of something to warm the bones?\n\
        ---\n\
        {{\"action\": \"speaks warmly\", \"mood\": \"friendly\", \
        \"internal_thought\": \"New face around here\", \
        \"irish_words\": [{{\"word\": \"Dia dhuit\", \"pronunciation\": \"DEE-ah gwit\", \
        \"meaning\": \"Hello (lit. God to you)\"}}]}}",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        personality = npc.personality,
        intel_legend = Intelligence::prompt_legend(),
        intel_tag = intel_tag,
        intel_guidance = intel_guidance,
        mood = npc.mood,
        improv_section = improv_section,
    )
}

/// Builds the Tier 1 context prompt for an NPC interaction.
///
/// Includes the current location, time of day, weather, season,
/// and the player's action, giving the LLM full situational context.
pub fn build_tier1_context(world: &WorldState, player_input: &str) -> String {
    let location = world.current_location();
    let time_of_day = world.clock.time_of_day();
    let season = world.clock.season();

    format!(
        "Your Location: {loc_name} — {loc_desc}\n\
        Time: {time}\n\
        Season: {season}\n\
        Weather: {weather}\n\
        \n\
        The player {action}",
        loc_name = location.name,
        loc_desc = location.description,
        time = time_of_day,
        season = season,
        weather = world.weather,
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
        assert!(prompt.contains("JSON metadata"));
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
        let npc = Npc::new_test_npc();
        let world = WorldState::new();
        let context = build_tier1_context(&world, "says hello");
        assert!(context.contains("The Crossroads"));
        assert!(context.contains("Morning"));
        assert!(context.contains("Spring"));
        assert!(context.contains("Clear"));
        assert!(context.contains("Your Location:"));
        assert!(!context.contains("is here"));
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
    fn test_irish_word_hint_deserialize() {
        let json =
            r#"{"word": "sláinte", "pronunciation": "SLAWN-cha", "meaning": "Health/cheers"}"#;
        let hint: IrishWordHint = serde_json::from_str(json).unwrap();
        assert_eq!(hint.word, "sláinte");
        assert_eq!(hint.pronunciation, "SLAWN-cha");
        assert_eq!(hint.meaning, Some("Health/cheers".to_string()));
    }

    #[test]
    fn test_irish_word_hint_deserialize_no_meaning() {
        let json = r#"{"word": "craic", "pronunciation": "crack"}"#;
        let hint: IrishWordHint = serde_json::from_str(json).unwrap();
        assert_eq!(hint.word, "craic");
        assert_eq!(hint.pronunciation, "crack");
        assert!(hint.meaning.is_none());
    }

    #[test]
    fn test_irish_word_hint_serialize() {
        let hint = IrishWordHint {
            word: "dia dhuit".to_string(),
            pronunciation: "DEE-ah gwit".to_string(),
            meaning: Some("Hello".to_string()),
        };
        let json = serde_json::to_string(&hint).unwrap();
        assert!(json.contains("dia dhuit"));
        assert!(json.contains("DEE-ah gwit"));
    }

    #[test]
    fn test_npc_metadata_with_language_hints() {
        let json = r#"{
            "action": "speaks",
            "mood": "friendly",
            "language_hints": [
                {"word": "Dia dhuit", "pronunciation": "DEE-ah gwit", "meaning": "Hello"}
            ]
        }"#;
        let meta: NpcMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.action, "speaks");
        assert_eq!(meta.language_hints.len(), 1);
        assert_eq!(meta.language_hints[0].word, "Dia dhuit");
    }

    #[test]
    fn test_npc_metadata_with_irish_words_alias() {
        // LLMs trained on the old prompt may still output "irish_words"
        let json = r#"{
            "action": "speaks",
            "mood": "friendly",
            "irish_words": [
                {"word": "Dia dhuit", "pronunciation": "DEE-ah gwit", "meaning": "Hello"}
            ]
        }"#;
        let meta: NpcMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.language_hints.len(), 1);
        assert_eq!(meta.language_hints[0].word, "Dia dhuit");
    }

    #[test]
    fn test_npc_metadata_empty_language_hints() {
        let json = r#"{"action": "nods", "mood": "content"}"#;
        let meta: NpcMetadata = serde_json::from_str(json).unwrap();
        assert!(meta.language_hints.is_empty());
    }

    #[test]
    fn test_parse_response_with_language_hints() {
        let text = "Dia dhuit! How are ye this fine morning?\n---\n\
            {\"action\": \"speaks\", \"mood\": \"warm\", \
            \"irish_words\": [{\"word\": \"Dia dhuit\", \"pronunciation\": \"DEE-ah gwit\", \
            \"meaning\": \"Hello (lit. God to you)\"}]}";
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Dia dhuit! How are ye this fine morning?");
        let meta = parsed.metadata.unwrap();
        assert_eq!(meta.language_hints.len(), 1);
        assert_eq!(meta.language_hints[0].word, "Dia dhuit");
        assert_eq!(meta.language_hints[0].pronunciation, "DEE-ah gwit");
    }

    #[test]
    fn test_system_prompt_avoids_stereotypes() {
        let npc = Npc::new_test_npc();
        let prompt = build_tier1_system_prompt(&npc, false);
        assert!(prompt.contains("dignity"), "prompt should mention dignity");
        assert!(
            prompt.contains("Never portray"),
            "prompt should have anti-stereotype guidance"
        );
        assert!(
            prompt.contains("begorrah"),
            "prompt should specifically warn against stage-Irish"
        );
    }

    #[test]
    fn test_system_prompt_historical_constraints() {
        let npc = Npc::new_test_npc();
        let prompt = build_tier1_system_prompt(&npc, false);
        assert!(
            prompt.contains("no electricity"),
            "prompt should exclude modern technology"
        );
        assert!(
            prompt.contains("Catholic Emancipation"),
            "prompt should reference political context"
        );
        assert!(
            prompt.contains("Do not reference anything that does not exist in 1820"),
            "prompt should have explicit anachronism guard"
        );
    }

    #[test]
    fn test_system_prompt_improv_enabled() {
        let npc = Npc::new_test_npc();
        let prompt = build_tier1_system_prompt(&npc, true);
        assert!(
            prompt.contains("IMPROV CRAFT"),
            "improv prompt should contain IMPROV CRAFT section"
        );
        assert!(
            prompt.contains("YES, AND"),
            "improv prompt should contain Yes-And principle"
        );
        assert!(
            prompt.contains("SPECIFICITY"),
            "improv prompt should contain specificity principle"
        );
        assert!(
            prompt.contains("EMOTIONAL TRUTH"),
            "improv prompt should contain emotional truth principle"
        );
        assert!(
            prompt.contains("PHYSICAL GROUNDING"),
            "improv prompt should contain physical grounding principle"
        );
        assert!(
            prompt.contains("LISTEN AND REACT"),
            "improv prompt should contain listen-and-react principle"
        );
        assert!(
            prompt.contains("HEIGHTEN"),
            "improv prompt should contain heighten principle"
        );
        assert!(
            prompt.contains("RAISE EMOTIONAL STAKES"),
            "improv prompt should contain raise-emotional-stakes principle"
        );
        assert!(
            prompt.contains("MAKE THE PLAYER SHINE"),
            "improv prompt should contain make-player-shine principle"
        );
    }

    #[test]
    fn test_system_prompt_improv_disabled() {
        let npc = Npc::new_test_npc();
        let prompt = build_tier1_system_prompt(&npc, false);
        assert!(
            !prompt.contains("IMPROV CRAFT"),
            "non-improv prompt should NOT contain IMPROV CRAFT section"
        );
        assert!(
            !prompt.contains("YES, AND"),
            "non-improv prompt should NOT contain improv principles"
        );
    }

    #[test]
    fn test_system_prompt_improv_preserves_identity() {
        let npc = Npc::new_test_npc();
        let prompt = build_tier1_system_prompt(&npc, true);
        // Improv mode should still contain all the standard sections
        assert!(prompt.contains("Padraig O'Brien"));
        assert!(prompt.contains("58-year-old"));
        assert!(prompt.contains("Publican"));
        assert!(prompt.contains("1820"));
        assert!(prompt.contains("CULTURAL GUIDELINES"));
        assert!(prompt.contains("irish_words"));
        assert!(prompt.contains("---"));
    }
}
