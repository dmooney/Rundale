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
pub mod quality;
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
use parish_types::{DayType, LocationId, Season, TimeOfDay};
use parish_world::WorldState;
use parish_world::description::render_description;
use parish_world::movement::{WeatherEffect, weather_effect};
use parish_world::transport::TransportMode;
use reactions::ReactionLog;
use transitions::NpcSummary;
use types::{Intelligence, NpcState, Relationship, SeasonalSchedule};

// Re-export shared types from parish-types
pub use parish_types::{
    LanguageHint, NpcId, extract_dialogue_from_partial_json, floor_char_boundary,
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
    /// Pronouns used when narrating this NPC (e.g. `he/him`, `she/her`,
    /// `they/them`). Defaults to `they/them` for NPCs that don't state them
    /// (#1026).
    pub pronouns: String,
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
            pronouns: "he/him".to_string(),
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
///
/// Three-tier fallback when full JSON parse fails:
///
/// 1. **Full JSON parse** — preferred path, captures dialogue + metadata.
/// 2. **Heuristic `dialogue` extraction** — when the stream is truncated
///    mid-emit (max_tokens cutoff, network blip), the JSON won't close
///    but the `"dialogue": "..."` prefix is intact. Regex-extract the
///    inner string instead of letting the raw `{"dialogue": "..."}`
///    wrapper render as user-visible text.
/// 3. **Raw text** — for non-JSON providers or empty responses.
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

    // Heuristic recovery for truncated / malformed JSON: extract the
    // inner string from a `"dialogue": "..."` pair. Tolerates an
    // unclosed JSON object (Brendan + Cormac at The Mill, 2026-05-17
    // demo).
    if let Some(dlg) = extract_dialogue_field_heuristic(stripped) {
        return NpcStreamResponse {
            dialogue: dlg,
            metadata: None,
        };
    }

    NpcStreamResponse {
        dialogue: trimmed.to_string(),
        metadata: None,
    }
}

/// Extracts the value of a `"dialogue": "..."` JSON field from a possibly
/// truncated / malformed object. Returns `None` if the field is not present
/// or the value is empty after JSON-escape decoding.
fn extract_dialogue_field_heuristic(text: &str) -> Option<String> {
    let t = text.trim_start_matches(|c: char| c.is_whitespace() || c == '{');
    let after_key = t.strip_prefix("\"dialogue\"").or_else(|| {
        t.strip_prefix("'dialogue'")
            .or_else(|| t.strip_prefix("dialogue"))
    })?;
    let after_colon = after_key.trim_start();
    let after_colon = after_colon.strip_prefix(':')?.trim_start();
    let (inner, opener) = if let Some(rest) = after_colon.strip_prefix('"') {
        (rest, '"')
    } else if let Some(rest) = after_colon.strip_prefix('\'') {
        (rest, '\'')
    } else {
        return None;
    };

    // Walk the string body, honoring JSON-style backslash escapes. Stop
    // at the first unescaped quote that MATCHES the opener; if the stream
    // ran out (truncated), take everything we have. Tracking the matching
    // closer is important — a single-quoted body containing `"` should
    // not terminate early, and vice versa.
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => break,
            },
            c if c == opener => break,
            other => out.push(other),
        }
    }

    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
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

/// Curated ga-IE phrase list appended to the directive when native is
/// Irish. The May 2026 Opus-blind eval found that Qwen2.5 knows *where*
/// to drop a sprinkle but often picks ungrammatical or anachronistic
/// Irish — e.g. "Tá mo chuid lánaí eile" ("my share of other children"
/// literally). A small list of pre-vetted phrases lifts authenticity
/// without changing model.
///
/// Categories cover the situations most likely to come up in NPC
/// dialogue: greetings, blessings, exclamations, terms of endearment,
/// thanks, and a few period-appropriate plant/herb names. Each entry
/// includes a brief English gloss so the model can match phrase to
/// register.
/// Pig Latin phrase guide appended to the directive when native is `x-pig-lat`.
///
/// Gives the model the transformation rules and a small set of pre-verified
/// forms so NPCs produce recognisable Pig Latin rather than garbled output.
const PIG_LAT_PHRASE_GUIDE: &str = "\n\
    Pig Latin rules: move the initial consonant cluster to the end and add \
    \"ay\" (e.g. \"pig\" → \"igpay\", \"street\" → \"eetstray\", \
    \"there\" → \"erethay\"). \
    For words starting with a vowel, append \"way\" \
    (e.g. \"inside\" → \"insideway\", \"all\" → \"allway\"). \
    Pre-verified forms you may use: ellohay (hello), oodgay (good), \
    ayday (day), ankthay ouyay (thank you), easyplay (please), \
    eresway (where's), atwhay (what), oday ouyay (do you), \
    iendsfray (friends), omingcay (coming).";

const GA_IE_PHRASE_GUIDE: &str = "\n\
    Preferred ga-IE phrases (use these where natural; do not confabulate \
    other Irish): \
    Greetings: \"Dia dhuit\" (hello), \"Dia is Muire dhuit\" (reply), \
    \"Conas atá tú?\" (how are you), \"Slán\" (goodbye), \
    \"Slán abhaile\" (safe home). \
    Blessings / thanks: \"Go raibh maith agat\" (thank you), \
    \"Le cúnamh Dé\" (with God's help), \"Buíochas le Dia\" (thank God), \
    \"Beannacht Dé ort\" (God bless you), \"Go n-éirí leat\" (good luck to you). \
    Exclamations: \"Mo ghrá\" (my love), \"A chroí\" (dear, sweetheart), \
    \"A stór\" (treasure / dear), \"A leanbh\" (child), \"Mhuise\" (well, indeed), \
    \"Faith\", \"Bedad\", \"Bedambut\". \
    Concepts: \"sídhe\" (fairy folk), \"sí\" (fairy mound), \
    \"seanchaí\" (storyteller), \"céilí\" (gathering), \
    \"poitín\" (illicit spirits), \"piseog\" (superstition).";

/// Renders the locale directive injected into every dialogue system prompt.
///
/// Always emits a leading `LANGUAGE: Speak in {player}.` clause, plus
/// spelling-discipline guidance. When the player language is an English
/// variant other than `en-US`, the directive forbids en-US spellings.
/// When a `native` language is set, the directive instructs the model to
/// code-switch naturally and record secondary-language words in the
/// `language_hints` metadata array. When `native` is `ga-IE` specifically,
/// the directive appends a curated phrase list ([`GA_IE_PHRASE_GUIDE`])
/// so the model picks from grammatical, period-appropriate Irish rather
/// than confabulating its own.
///
/// Closes with a character-set guard that forbids non-Latin scripts
/// (Cyrillic, Han, Hiragana, Katakana, Hangul, Arabic, Hebrew, Greek,
/// Devanagari). Multilingual model weights — notably Qwen2.5 — drift
/// into Chinese or Russian mid-sentence on rural-Irish prompts at higher
/// temperatures; the explicit allow-list disciplines the output side.
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
            in the `language_hints` metadata array. \
            CRITICAL: {native} is a SPRINKLE only — at most one short phrase \
            (1-5 words) per reply, woven into otherwise-{player} prose. \
            {player} must carry the meaning of every sentence. \
            NEVER reply entirely in {native}, even if the player's question \
            seems to invite it. The player may not speak {native}; the meaning \
            of your reply must be clear to a {player} speaker who knows zero \
            {native}. \
            Use ONLY {player} and {native} — no other language under any \
            circumstances."
        ));
        if native.eq_ignore_ascii_case("ga-IE") || native.eq_ignore_ascii_case("ga") {
            directive.push_str(GA_IE_PHRASE_GUIDE);
        } else if native.eq_ignore_ascii_case("x-pig-lat") {
            directive.push_str(PIG_LAT_PHRASE_GUIDE);
        }
    } else {
        directive.push_str(&format!(
            " Stay in {player} — do not invent or import other languages."
        ));
    }

    directive.push_str(
        " Every character you emit must be Latin script (a-z, A-Z, accented \
        Latin such as á é í ó ú ü ñ ç ß) or standard punctuation. \
        Do NOT emit Cyrillic (Russian), Han (Chinese), Hiragana / Katakana \
        (Japanese), Hangul (Korean), Arabic, Hebrew, Greek, or Devanagari \
        characters — replace any tempted non-Latin word with its English or \
        native-language equivalent, or omit it.",
    );

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

/// Example JSON response block injected into the Tier 1 system prompt
/// to demonstrate the expected output format to the LLM.
const EXAMPLE_RESPONSE_BLOCK: &str = "\
Example response:\n\
{\"dialogue\": \"Ah, good morning to ye! Fine day for it, so it is. \
Will ye have a drop of something to warm the bones?\", \
\"action\": \"looks up from polishing glass, speaks warmly\", \"mood\": \"friendly\", \
\"internal_thought\": \"New face around here\", \
\"language_hints\": []}";

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
        STAY IN YOUR LANE: a midwife knows herbs, births, sickness, and women's matters — \
        she does NOT track livestock predators, hunt, or speak as a farmer would. \
        A farmer talks of land, beasts, and weather — not deliveries. \
        A priest speaks of souls and gossip, not arithmetic. \
        A teacher speaks of pupils and books, not midwifery. \
        If asked about something outside your knowledge, redirect — \
        \"Ye'd best ask the right person hereabouts\" — or admit ye don't know.\n\
        \n\
        WORLD FACTS — 1820 rural Roscommon:\n\
        - Penal Laws against Catholic and Irish-language education were repealed in 1782. \
        Hedge schools operate openly; teaching in Irish is tolerated. \
        Do NOT claim it is illegal or in secret.\n\
        - Catholic Emancipation: pending in 1829. Has NOT happened yet.\n\
        - Great Famine: 1845. Has NOT happened yet. The potato is a staple but the blight has not struck.\n\
        - The British Crown rules Ireland. Daniel O'Connell is active but not yet famous.\n\
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
        Show the wit, intelligence, resilience, and warmth of rural Irish people.\n\
        \n\
        ALLOWED IRISH PHRASES — you MAY use these verbatim, and ONLY these:\n\
        - \"Slán abhaile\" (safe home)\n\
        - \"Slán leat\" (goodbye)\n\
        - \"Dia dhuit\" (hello, lit. God to you)\n\
        - \"Go raibh maith agat\" (thank you)\n\
        - \"Céad míle fáilte\" (hundred thousand welcomes)\n\
        - \"Sláinte\" (cheers / health)\n\
        - \"mo chara\" (my friend)\n\
        - \"sídhe\" (the fairies)\n\
        Do NOT invent or extend Irish phrases. Do NOT improvise Irish grammar. \
        If unsure, stay in Hiberno-English. Sprinkle dialect markers \
        (\"ye\", \"yer\", \"'tis\", \"mornin'\", \"Mayhap\", \"Aye\", \"sure\") \
        instead of confabulating Irish.\n\
        \n\
        REGISTER: avoid 21st-century words. Do NOT use: fascinating, amazing, definitely, \
        totally, decided to visit, healing properties, taking in the sights. \
        Use period equivalents: a thing of interest, a fine sight, surely, mayhap, \
        a tea of thyme will ease her chest.\n\
        \n\
        FRESH PHRASING: Do not close with stock politeness templates such as \
        \"if I might ask it so bold,\" \"if ye don't mind my asking,\" or similar \
        repeated softeners. Every reply must use distinct wording — never recycle \
        the closer of any earlier turn in the conversation, and never echo another \
        NPC's phrasing. End on a concrete observation, question, or action rooted \
        in your character, not a formula.\n\
        \n\
        NEVER FAREWELL MID-CONVERSATION: Do not end your reply with \"Slán\", \
        \"Slán abhaile\", \"Slán leat\", \"Goodbye\", \"Farewell\", \"safe home\", \
        or any other parting phrase unless the player has explicitly said they \
        are leaving (e.g. \"I'll be off\", \"I must go\", \"goodbye\"). A \
        farewell closes the dialogue — only use one when the conversation is \
        actually ending. While the conversation continues, end on a question, \
        an observation, or an offer — never on a goodbye.\
        {improv_section}\n\
        \n\
        Personality: {personality}\n\
        {intel_guidance}\
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
          - \"meaning\": English translation\n",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        personality = npc.personality,
        intel_guidance = if intel_guidance.is_empty() {
            String::new()
        } else {
            format!("Mind and manner: {intel_guidance}\n")
        },
        improv_section = improv_section,
    );

    prompt.push_str(EXAMPLE_RESPONSE_BLOCK);
    prompt.push_str("\n\n");
    prompt.push_str(&language_directive(language));
    prompt
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

/// Returns a directive forbidding greetings that are wrong for the current time of day.
///
/// Small models sometimes ignore the affirmative "greet accordingly" cue and
/// fall back to the training-data majority ("good morning"). A negative
/// constraint paired with the positive cue is more reliable (#1225).
///
/// Only emits a directive when the time of day is clearly NOT morning —
/// Morning and Midday have no forbidden-greeting because "good morning" and
/// "good day" are appropriate for those buckets.
pub fn forbidden_greeting_directive(time_of_day: TimeOfDay) -> Option<&'static str> {
    match time_of_day {
        TimeOfDay::Dawn => Some(
            "Do NOT say 'good morning' — it is only just dawning. \
             A simple 'Dia dhuit' or 'good day to ye' is right.",
        ),
        TimeOfDay::Afternoon => Some(
            "Do NOT say 'good morning' — it is the afternoon. \
             'Good afternoon' or 'good day' is fitting.",
        ),
        TimeOfDay::Dusk => Some(
            "Do NOT say 'good morning' or 'good day' — it is dusk, \
             the sun is low. 'Good evening' or 'good e'en to ye' is right.",
        ),
        TimeOfDay::Night => Some(
            "Do NOT say 'good morning' or 'good day' — it is night. \
             'Good evening' or 'good night' is fitting.",
        ),
        TimeOfDay::Midnight => Some(
            "Do NOT say 'good morning' — it is well past midnight. \
             A hushed greeting or simple nod is in order.",
        ),
        // Morning and Midday: "good morning" / "good day" are appropriate.
        TimeOfDay::Morning | TimeOfDay::Midday => None,
    }
}

// ── #1228 — anti-repetition guard ──────────────────────────────────────────────
//
// Small / quantized models (e.g. the local MLX Qwen2.5-14B-4bit in #1228)
// occasionally enter a degenerate sampling loop and emit the same clause dozens
// of times inside one reply, or echo their own previous line near-verbatim on
// the next turn. The #1224 length cap clips the *tail* but the surviving prefix
// is still a wall of duplicate clauses, and it does nothing across turns. These
// helpers are the deterministic, model-agnostic backstop: they need no live
// inference and run identically for every provider.

/// Normalizes a dialogue line for repetition comparison.
///
/// Lower-cases, collapses internal whitespace to single spaces, and trims
/// surrounding whitespace and trailing sentence punctuation. Two lines that
/// differ only in case, spacing, or trailing `.`/`!`/`?`/`…` normalize equal.
fn normalize_for_repetition(s: &str) -> String {
    let lowered = s.to_lowercase();
    let collapsed = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '.' | '!' | '?' | '…' | ',' | ';'))
        .to_string()
}

/// Splits a dialogue body into sentence-ish units on terminal punctuation,
/// keeping the delimiter with each piece. Used by [`collapse_repeated_sentences`].
fn split_sentences(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in s.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '…') {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// Collapses consecutive duplicate sentences/clauses within a single dialogue
/// string (#1228, intra-line repetition).
///
/// When a sentence normalizes equal to the one immediately before it, the
/// duplicate is dropped. A run of N identical clauses collapses to a single
/// instance. Non-repeating dialogue is returned unchanged (modulo whitespace
/// re-joining). This is the primary defense against the degenerate
/// "Speak yer mind, and we'll see what be in it, m'friend." loop in #1228.
pub fn collapse_repeated_sentences(dialogue: &str) -> String {
    let sentences = split_sentences(dialogue);
    if sentences.len() < 2 {
        return dialogue.to_string();
    }
    let mut kept: Vec<&str> = Vec::with_capacity(sentences.len());
    let mut last_norm: Option<String> = None;
    for sentence in &sentences {
        let norm = normalize_for_repetition(sentence);
        // Skip empty fragments and consecutive duplicates.
        if norm.is_empty() {
            continue;
        }
        if last_norm.as_deref() == Some(norm.as_str()) {
            continue;
        }
        kept.push(sentence.trim());
        last_norm = Some(norm);
    }
    if kept.is_empty() {
        return dialogue.trim().to_string();
    }
    kept.join(" ")
}

/// Word-level Jaccard similarity of two normalized dialogue lines, in `[0.0, 1.0]`.
///
/// `1.0` means the two lines share the exact same set of words (ignoring order,
/// case, spacing, and trailing punctuation); `0.0` means no shared words. Used as
/// the deterministic near-identity signal for the cross-turn guard.
fn dialogue_similarity(a: &str, b: &str) -> f32 {
    let norm_a = normalize_for_repetition(a);
    let norm_b = normalize_for_repetition(b);
    if norm_a.is_empty() && norm_b.is_empty() {
        return 1.0;
    }
    let set_a: std::collections::HashSet<&str> =
        norm_a.split(' ').filter(|w| !w.is_empty()).collect();
    let set_b: std::collections::HashSet<&str> =
        norm_b.split(' ').filter(|w| !w.is_empty()).collect();
    if set_a.is_empty() || set_b.is_empty() {
        return if norm_a == norm_b { 1.0 } else { 0.0 };
    }
    let intersection = set_a.intersection(&set_b).count() as f32;
    let union = set_a.union(&set_b).count() as f32;
    intersection / union
}

/// Reports whether two dialogue lines are near-identical (#1228, cross-turn).
///
/// `threshold` is the minimum word-level Jaccard similarity (see
/// [`dialogue_similarity`]) at or above which two lines count as near-identical.
/// Exact normalized equality always counts. A threshold of `1.0` requires the
/// same word set; lower thresholds catch lines that merely reshuffle/echo the
/// same content.
pub fn is_near_identical(a: &str, b: &str, threshold: f32) -> bool {
    if normalize_for_repetition(a) == normalize_for_repetition(b) {
        return true;
    }
    dialogue_similarity(a, b) >= threshold
}

/// Deterministic, varied fallback line used when an NPC would otherwise repeat
/// its own previous line verbatim (#1228).
///
/// Picks from a small pool keyed by `seed` so the substituted line is stable for
/// a given turn but varies across NPCs/turns. Period-neutral and content-free, so
/// it is safe for any mod and any time of day. These are intentionally short — a
/// brief acknowledgement reads better than re-emitting a degenerate wall of text.
pub fn varied_repetition_fallback(seed: u64) -> &'static str {
    const POOL: [&str; 6] = [
        "Aye, as I said.",
        "Sure, ye have the right of it.",
        "Mm. There's little more to add to that.",
        "Well now, I'll not say the same twice.",
        "Aye, that's the way of it.",
        "I've said my piece on that, so.",
    ];
    POOL[(seed as usize) % POOL.len()]
}

/// Applies the full anti-repetition guard to a freshly generated NPC line
/// (#1228). This is the single entry point the shared dialogue path calls.
///
/// Steps, in order:
/// 1. Collapse consecutive duplicate clauses *within* the new line
///    ([`collapse_repeated_sentences`]).
/// 2. If the collapsed line is near-identical to the NPC's own `previous_line`
///    ([`is_near_identical`] at `threshold`), substitute a deterministic varied
///    fallback ([`varied_repetition_fallback`] keyed by `seed`).
///
/// `previous_line` is `None` when the NPC has no prior line at this location.
/// A `threshold` of `0.0` disables the cross-turn check (intra-line collapse
/// still runs, since runaway loops are always undesirable).
pub fn guard_against_repetition(
    new_line: &str,
    previous_line: Option<&str>,
    threshold: f32,
    seed: u64,
) -> String {
    let collapsed = collapse_repeated_sentences(new_line);
    if threshold <= 0.0 {
        return collapsed;
    }
    if let Some(prev) = previous_line
        && !collapsed.trim().is_empty()
        && !prev.trim().is_empty()
        && is_near_identical(&collapsed, prev, threshold)
    {
        return varied_repetition_fallback(seed).to_string();
    }
    collapsed
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

    // Regression (fixed: #13, reinforced: #1225): explicit time-of-day cue so
    // the model picks the right greeting register. NPCs were saying "good
    // morning" at Dusk because the only time signal in the context was the bare
    // 17:30 HH:MM — without an English label the model defaulted to "morning".
    // Spell out the bucket and direct the model to greet accordingly. A
    // forbidden-greeting directive is also injected for non-morning buckets
    // because small models need negative examples to override the training-data
    // majority bias toward "good morning".
    let time_of_day_label = time_of_day.to_string();
    let loc_details = location_details_block(world);
    let forbidden = forbidden_greeting_directive(time_of_day)
        .map(|d| format!(" {d}"))
        .unwrap_or_default();
    format!(
        "Your Location: {loc_name} — {loc_desc}{loc_details}\n\
        Date and time: {date_time}\n\
        Time of day: {tod} ({hour:02}:{minute:02}) — greet and refer to the time of day accordingly.{forbidden}",
        loc_name = world.current_location().name,
        loc_desc = rendered_desc,
        loc_details = loc_details,
        date_time = date_time_str,
        tod = time_of_day_label,
        hour = now.hour(),
        minute = now.minute(),
        forbidden = forbidden,
    )
}

/// Builds the situational location block appended to the Tier 1 context:
/// indoor/outdoor + public/private framing, any mythological note, and the
/// paths leading away — each neighbour by name, its prose path, an on-foot
/// travel estimate, and a live weather-hazard note when the current weather
/// blocks or slows that edge.
///
/// Returns an empty string when the current location is absent from the world
/// graph, so callers fall back to the legacy name + description text unchanged.
fn location_details_block(world: &WorldState) -> String {
    let Some(loc) = world.current_location_data() else {
        return String::new();
    };

    let setting = if loc.indoor {
        "an enclosed indoor space"
    } else {
        "an open outdoor place"
    };
    let access = if loc.public {
        "open to all"
    } else {
        "a private place"
    };
    let mut out = format!("\nThis is {setting}, {access}.");

    if let Some(sig) = &loc.mythological_significance {
        out.push_str(&format!("\nOf local note: {sig}"));
    }

    let walking_speed = TransportMode::walking().speed_m_per_s;
    let neighbors = world.graph.neighbors(world.player_location);
    if neighbors.is_empty() {
        out.push_str("\nThere are no paths leading away from here.");
        return out;
    }

    out.push_str("\nPaths from here:");
    for (target, conn) in neighbors {
        let Some(dest) = world.graph.get(target) else {
            continue;
        };
        let mut minutes =
            world
                .graph
                .edge_travel_minutes(world.player_location, target, walking_speed);
        let hazard = match weather_effect(conn, world.weather) {
            WeatherEffect::Impassable { reason } => {
                format!(" — impassable in this weather: {reason}")
            }
            WeatherEffect::Slowed { factor, note } => {
                // Mirror movement::weather_adjusted_path so the quoted time
                // matches the slowdown the player will actually experience.
                minutes = ((minutes as f64 / factor).ceil() as u16).max(minutes);
                format!(" — slow going today: {note}")
            }
            WeatherEffect::Clear => String::new(),
        };
        out.push_str(&format!(
            "\n- {name} — {path} (about {minutes} min on foot){hazard}",
            name = dest.name,
            path = conn.path_description,
        ));
    }

    out
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
        // Mood is NOT in the system prompt — it lives in the dynamic context
        // so that mood changes never bust the stable system-prompt prefix that
        // the model-runtime prefix cache (vllm-mlx --enable-prefix-cache) depends on.
        assert!(
            !prompt.contains("content"),
            "mood must not appear in the static system prompt"
        );
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
        assert!(context.contains("Spring"));
        assert!(context.contains("1820"));
        assert!(context.contains("Your Location:"));
        assert!(context.contains("Date and time:"));
        // Regression (fixed: #13) — explicit time-of-day cue with both the bucket
        // label and HH:MM so the model picks the right greeting
        // register (NPCs were saying "good morning" at Dusk because
        // the only time signal was 17:30 with no English label).
        assert!(
            context.contains("Time of day:"),
            "missing time-of-day greeting cue:\n{context}"
        );
        // WorldState::new() starts at 08:00 → TimeOfDay::Morning
        assert!(
            context.contains("Time of day: Morning (08:00)"),
            "time-of-day cue must include both label and HH:MM:\n{context}"
        );
        assert!(
            context.contains("greet and refer to the time of day accordingly"),
            "missing greeting-register directive:\n{context}"
        );
        assert!(!context.contains("is here"));
        assert!(!context.contains("\nWeather:"));
        // Old "\nTime:" / "\nSeason:" standalone lines must stay absent.
        // (The new "\nTime of day:" line is allowed — note the space.)
        assert!(!context.contains("\nTime:"));
        assert!(!context.contains("\nSeason:"));
    }

    #[test]
    fn test_build_named_action_line_emote() {
        let line = build_named_action_line("*tips hat*", None);
        assert!(
            line.contains("The newcomer performs an action: tips hat"),
            "emote should strip asterisks and use action phrasing"
        );
        assert!(
            line.contains("emoting rather than speaking"),
            "emote should include action-mode instruction"
        );
    }

    #[test]
    fn test_build_named_action_line_normal_input() {
        let line = build_named_action_line("hello there", None);
        assert!(line.contains("The newcomer says: \"hello there\""));
        assert!(!line.contains("performs an action"));
    }

    #[test]
    fn test_build_named_action_line_partial_asterisks() {
        let line = build_named_action_line("*incomplete", None);
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
    fn test_parse_npc_stream_response_truncated_json() {
        // Live demo (2026-05-17) — Brendan Duffy at The Mill. JSON stream
        // ran out of tokens before the closing brace; previous behaviour
        // surfaced the raw `{"dialogue": "..."}` wrapper as user-visible
        // dialogue text.
        let text = r#"{"dialogue": "Aye, the process of milling, is it? 'Tis a simple enough thing, so it is. Ye bring yer grain, we grind it"#;
        let parsed = parse_npc_stream_response(text);
        assert_eq!(
            parsed.dialogue,
            "Aye, the process of milling, is it? 'Tis a simple enough thing, so it is. Ye bring yer grain, we grind it"
        );
        assert!(parsed.metadata.is_none());
    }

    #[test]
    fn test_parse_npc_stream_response_truncated_at_escape() {
        // Defensive: stream ends mid backslash-escape.
        let text = r#"{"dialogue": "Aye \"good"#;
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "Aye \"good");
    }

    #[test]
    fn test_parse_npc_stream_response_truncated_empty_dialogue() {
        // `dialogue: ""` should fall through to raw text instead of
        // surfacing an empty bubble.
        let text = r#"{"dialogue": ""#;
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, text);
    }

    #[test]
    fn test_parse_npc_stream_response_single_quoted_with_inner_double_quote() {
        // Bot review (PR #990, codex P2): the heuristic accepted `'` as
        // an opening quote but only stopped at `"`. For pseudo-JSON like
        // `{'dialogue':'Aye, "good", said he'}` the inner double quote
        // must NOT terminate the body; we should keep going until the
        // matching single-quote closer.
        let text = r#"{'dialogue':'Aye, "good", said he'}"#;
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, r#"Aye, "good", said he"#);
    }

    #[test]
    fn test_parse_npc_stream_response_double_quoted_with_inner_single_quote() {
        // Mirror case: standard `"dialogue": "ye'll see"` body contains
        // an inner apostrophe; the opener `"` must drive the terminator
        // so we don't break early on the apostrophe.
        let text = r#"{"dialogue": "ye'll see, lad"}"#;
        let parsed = parse_npc_stream_response(text);
        assert_eq!(parsed.dialogue, "ye'll see, lad");
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
        // Mood is NOT in the system prompt; it is injected in the dynamic context
        // so mood changes do not bust the stable prefix-cache prefix.
        assert!(
            !prompt.contains("content"),
            "mood must not appear in the static system prompt"
        );

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

        // Lane-keeping clause (issue: midwife-replied-as-tracker, grok 2/5).
        assert!(
            prompt.contains("STAY IN YOUR LANE"),
            "lane-keeping clause missing"
        );

        // 1820 fact preamble (issue: Aoife claimed Irish-language teaching
        // was outlawed in 1820, but the Penal Laws were repealed in 1782).
        assert!(
            prompt.contains("Penal Laws"),
            "1820 fact preamble missing — Penal Laws clause"
        );
        assert!(
            prompt.contains("1782"),
            "1820 fact preamble missing — repeal date"
        );

        // Allowed-Irish-phrases whitelist (issue: "go connachtú"
        // hallucinated past the known "Slán abhaile").
        assert!(
            prompt.contains("ALLOWED IRISH PHRASES"),
            "Irish-phrase whitelist clause missing"
        );
        assert!(
            prompt.contains("Slán abhaile"),
            "anchor phrase missing from whitelist"
        );
        assert!(
            prompt.contains("Do NOT invent or extend Irish phrases"),
            "Irish-grammar improvisation guard missing"
        );

        // Modern-register blacklist (issue: "healing properties" in midwife
        // reply scored 2/5).
        assert!(
            prompt.contains("REGISTER:"),
            "modern-register guard missing"
        );
        assert!(
            prompt.contains("healing properties"),
            "modern-register negative example missing"
        );

        // Anti-farewell directive (fixed: #4 — Cormac signed off with
        // "Slán abhaile" mid-conversation in cycle 1 of the demo audit).
        assert!(
            prompt.contains("NEVER FAREWELL MID-CONVERSATION"),
            "anti-farewell directive header missing"
        );
        for tok in ["Slán abhaile", "Slán leat", "Goodbye", "Farewell"] {
            assert!(
                prompt.contains(tok),
                "anti-farewell directive missing gated token {tok:?}"
            );
        }
    }

    /// Regression guard: the Tier 1 system prompt must be byte-identical across
    /// two calls that change only the NPC's `mood` field between them.
    ///
    /// This protects the model-runtime prefix cache contract (vllm-mlx
    /// `--enable-prefix-cache`): the system prompt is the stable prefix shared
    /// across turns, and any field that mutates between turns must NOT appear in
    /// it. If mood is accidentally re-added, this test fails immediately.
    #[test]
    fn tier1_system_prompt_stable_across_mood_change() {
        let mut npc = Npc::new_test_npc();
        let lang = LanguageSettings::english_only();

        npc.mood = "cheerful".to_string();
        let prompt_before = build_tier1_system_prompt(&npc, false, &lang);

        npc.mood = "melancholy".to_string();
        let prompt_after = build_tier1_system_prompt(&npc, false, &lang);

        assert_eq!(
            prompt_before, prompt_after,
            "system prompt must be byte-identical across mood changes — \
             mood must live in the dynamic context, not the static system prompt"
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

    /// Helper: a world whose current location (id 1) carries the given
    /// indoor/public flags, optional mythological note, and a single hazard
    /// connection to "The Church" (id 2). `player_location` defaults to 1.
    #[cfg(test)]
    fn world_with_location_details(
        indoor: bool,
        public: bool,
        myth: Option<&str>,
        hazard: Option<&str>,
        weather: parish_world::Weather,
    ) -> parish_world::WorldState {
        use parish_world::graph::WorldGraph;

        let myth_field = match myth {
            Some(s) => format!(r#""mythological_significance": "{s}","#),
            None => String::new(),
        };
        let hazard_field = match hazard {
            Some(h) => format!(r#", "hazard": "{h}""#),
            None => String::new(),
        };
        let graph_json = format!(
            r#"{{
                "locations": [
                    {{
                        "id": 1,
                        "name": "The Crossroads",
                        "description_template": "A crossroads.",
                        "indoor": {indoor},
                        "public": {public},
                        {myth_field}
                        "lat": 53.618,
                        "lon": -8.095,
                        "connections": [{{"target": 2, "path_description": "a narrow boreen lined with hawthorn"{hazard_field}}}],
                        "associated_npcs": []
                    }},
                    {{
                        "id": 2,
                        "name": "St. Brigid's Church",
                        "description_template": "The church.",
                        "indoor": false,
                        "public": true,
                        "lat": 53.640,
                        "lon": -8.120,
                        "connections": [{{"target": 1, "path_description": "back the way you came"}}],
                        "associated_npcs": []
                    }}
                ]
            }}"#
        );

        let mut world = parish_world::WorldState::new();
        world.graph = WorldGraph::load_from_str(&graph_json).unwrap();
        world.player_location = LocationId(1);
        world.weather = weather;
        world
    }

    #[test]
    fn tier1_context_lists_connections_with_path_and_walk_time() {
        let world =
            world_with_location_details(false, true, None, None, parish_world::Weather::Clear);
        let context = build_tier1_context(&world);

        assert!(
            context.contains("Paths from here:"),
            "paths block missing: {context}"
        );
        assert!(
            context.contains("St. Brigid's Church"),
            "connected neighbour name missing: {context}"
        );
        assert!(
            context.contains("a narrow boreen lined with hawthorn"),
            "path description missing: {context}"
        );
        assert!(
            context.contains("min on foot"),
            "on-foot travel estimate missing: {context}"
        );
    }

    #[test]
    fn tier1_context_states_indoor_and_public_framing() {
        let indoor_private =
            world_with_location_details(true, false, None, None, parish_world::Weather::Clear);
        let ctx = build_tier1_context(&indoor_private);
        assert!(
            ctx.contains("an enclosed indoor space"),
            "indoor framing missing: {ctx}"
        );
        assert!(
            ctx.contains("a private place"),
            "private framing missing: {ctx}"
        );

        let outdoor_public =
            world_with_location_details(false, true, None, None, parish_world::Weather::Clear);
        let ctx = build_tier1_context(&outdoor_public);
        assert!(
            ctx.contains("an open outdoor place"),
            "outdoor framing missing: {ctx}"
        );
        assert!(ctx.contains("open to all"), "public framing missing: {ctx}");
    }

    #[test]
    fn tier1_context_includes_mythological_note_only_when_present() {
        let with_myth = world_with_location_details(
            false,
            true,
            Some("a fairy fort the old people will not disturb"),
            None,
            parish_world::Weather::Clear,
        );
        let ctx = build_tier1_context(&with_myth);
        assert!(
            ctx.contains("Of local note: a fairy fort the old people will not disturb"),
            "mythological note missing when present: {ctx}"
        );

        let without_myth =
            world_with_location_details(false, true, None, None, parish_world::Weather::Clear);
        let ctx = build_tier1_context(&without_myth);
        assert!(
            !ctx.contains("Of local note:"),
            "mythological note present when it should be absent: {ctx}"
        );
    }

    #[test]
    fn tier1_context_flags_weather_hazard_on_connections() {
        // A flood-hazard edge is impassable in a Storm.
        let stormy = world_with_location_details(
            false,
            true,
            None,
            Some("flood"),
            parish_world::Weather::Storm,
        );
        let ctx = build_tier1_context(&stormy);
        assert!(
            ctx.contains("impassable in this weather"),
            "hazard note missing under storm: {ctx}"
        );

        // Same edge under clear weather carries no hazard note.
        let clear = world_with_location_details(
            false,
            true,
            None,
            Some("flood"),
            parish_world::Weather::Clear,
        );
        let ctx = build_tier1_context(&clear);
        assert!(
            !ctx.contains("impassable in this weather"),
            "hazard note present under clear weather: {ctx}"
        );
        assert!(
            !ctx.contains("slow going today"),
            "slowed note present under clear weather: {ctx}"
        );
    }

    #[test]
    fn tier1_context_slowed_edge_quotes_weather_adjusted_travel_time() {
        // A flood edge under heavy rain is Slowed (factor 0.6), so the quoted
        // on-foot time must rise above the clear-weather estimate — the NPC
        // can't say "slow going today" while quoting the dry-road time.
        fn quoted_minutes(ctx: &str) -> u16 {
            let marker = "(about ";
            let start = ctx.find(marker).expect("travel estimate present") + marker.len();
            let rest = &ctx[start..];
            let end = rest.find(" min on foot").expect("minutes label present");
            rest[..end].trim().parse().expect("minutes parse")
        }

        let clear = world_with_location_details(
            false,
            true,
            None,
            Some("flood"),
            parish_world::Weather::Clear,
        );
        let clear_minutes = quoted_minutes(&build_tier1_context(&clear));

        let rainy = world_with_location_details(
            false,
            true,
            None,
            Some("flood"),
            parish_world::Weather::HeavyRain,
        );
        let rainy_ctx = build_tier1_context(&rainy);
        assert!(
            rainy_ctx.contains("slow going today"),
            "slowed note missing under heavy rain: {rainy_ctx}"
        );
        assert!(
            quoted_minutes(&rainy_ctx) > clear_minutes,
            "slowed travel time should exceed clear-weather time: clear={clear_minutes}, ctx={rainy_ctx}"
        );
    }

    #[test]
    fn tier1_context_falls_back_cleanly_when_location_absent_from_graph() {
        use parish_world::graph::WorldGraph;

        // Graph contains only ids 998/999, so the player's location (1) is
        // absent from the graph: current_location_data() is None.
        let graph_json = r#"{
            "locations": [
                {
                    "id": 998,
                    "name": "Nowhere",
                    "description_template": "Empty.",
                    "indoor": false,
                    "public": true,
                    "lat": 0.0,
                    "lon": 0.0,
                    "connections": [{"target": 999, "path_description": "a track"}],
                    "associated_npcs": []
                },
                {
                    "id": 999,
                    "name": "Elsewhere",
                    "description_template": "Empty.",
                    "indoor": false,
                    "public": true,
                    "lat": 0.1,
                    "lon": 0.1,
                    "connections": [{"target": 998, "path_description": "a track"}],
                    "associated_npcs": []
                }
            ]
        }"#;

        let mut world = parish_world::WorldState::new();
        world.graph = WorldGraph::load_from_str(graph_json).unwrap();
        world.player_location = LocationId(1);

        // Must not panic, and must emit no paths block / setting line.
        let ctx = build_tier1_context(&world);
        assert!(
            !ctx.contains("Paths from here:"),
            "paths block leaked: {ctx}"
        );
        assert!(
            !ctx.contains("There are no paths leading away"),
            "no-paths sentinel leaked: {ctx}"
        );
        assert!(
            !ctx.contains("This is an"),
            "setting line leaked without graph data: {ctx}"
        );
        assert!(
            ctx.contains("Your Location:"),
            "legacy location line missing: {ctx}"
        );
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
                    pronouns: "she/her".to_string(),
                    intelligence_prose: "Sharp-minded and perceptive.".to_string(),
                    mood: "thoughtful".to_string(),
                    relationship_summary: String::new(),
                },
                NpcSnapshot {
                    id: NpcId(7),
                    name: "Seamus Fahey".to_string(),
                    occupation: "Blacksmith".to_string(),
                    personality: "Blunt and loyal".to_string(),
                    pronouns: "he/him".to_string(),
                    intelligence_prose: "Plain-spoken and blunt.".to_string(),
                    mood: "tired".to_string(),
                    relationship_summary: String::new(),
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
        assert!(
            directive.contains("Use ONLY en-IE and ga-IE"),
            "directive should name an explicit two-language allow-list"
        );
        // Should NOT tell the NPC to stay in one language when a native is given
        assert!(
            !directive.contains("do not invent or import other languages"),
            "mono-language restriction must not appear when native language is set"
        );
    }

    #[test]
    fn language_directive_includes_ga_ie_phrase_guide_for_irish_native() {
        let lang = LanguageSettings::new("en-IE", Some("ga-IE".into()));
        let directive = language_directive(&lang);
        assert!(
            directive.contains("Preferred ga-IE phrases"),
            "ga-IE native should append the curated phrase guide"
        );
        assert!(
            directive.contains("\"Dia dhuit\""),
            "phrase guide should include canonical greetings"
        );
        assert!(
            directive.contains("\"seanchaí\""),
            "phrase guide should include period-appropriate concept words"
        );
    }

    #[test]
    fn language_directive_omits_ga_ie_phrase_guide_for_non_irish_native() {
        let lang = LanguageSettings::new("en-US", Some("fr-FR".into()));
        let directive = language_directive(&lang);
        assert!(
            !directive.contains("Preferred ga-IE phrases"),
            "ga-IE phrase guide must only fire for Irish native"
        );
    }

    #[test]
    fn language_directive_includes_pig_lat_guide() {
        let lang = LanguageSettings::new("en", Some("x-pig-lat".into()));
        let directive = language_directive(&lang);
        assert!(
            directive.contains("x-pig-lat"),
            "directive should name the pig Latin code"
        );
        assert!(
            directive.contains("Pig Latin rules"),
            "x-pig-lat native should append the pig Latin phrase guide"
        );
        assert!(
            directive.contains("igpay"),
            "phrase guide should include a canonical example"
        );
        assert!(
            !directive.contains("Preferred ga-IE phrases"),
            "ga-IE phrase guide must not appear for pig Latin native"
        );
    }

    #[test]
    fn language_directive_forbids_non_latin_scripts() {
        // Character-set guard must appear regardless of locale config so the
        // multilingual-model drift (Qwen → Han / Cyrillic mid-sentence) is
        // disciplined at the prompt boundary.
        for lang in [
            LanguageSettings::new("en-IE", Some("ga-IE".into())),
            LanguageSettings::english_only(),
            LanguageSettings::new("fr-FR", None),
        ] {
            let directive = language_directive(&lang);
            assert!(
                directive.contains("Latin script"),
                "directive should allow-list Latin script"
            );
            for forbidden in [
                "Cyrillic",
                "Han",
                "Hiragana",
                "Hangul",
                "Arabic",
                "Hebrew",
                "Greek",
                "Devanagari",
            ] {
                assert!(
                    directive.contains(forbidden),
                    "directive should explicitly forbid {forbidden}: {directive:?}"
                );
            }
        }
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

    // ── #1225 — time-of-day greeting guard ───────────────────────────────────

    /// AC-1/AC-2 (fix-1224-1225): `build_tier1_context` must carry an explicit
    /// time-of-day label and HH:MM for every non-morning bucket, so small
    /// models that ignore the plain HH:MM timestamp still see an unambiguous
    /// English cue for greeting register.
    #[test]
    fn build_tier1_context_carries_dusk_label_and_time() {
        use chrono::TimeZone;
        let mut world = WorldState::new();
        // Advance the clock to 17:30 — Dusk bucket.
        world.clock = parish_types::GameClock::new(
            chrono::Utc
                .with_ymd_and_hms(1820, 3, 20, 17, 30, 0)
                .unwrap(),
        );
        let context = build_tier1_context(&world);

        assert!(
            context.contains("Time of day:"),
            "AC-1: time-of-day cue missing at Dusk:\n{context}"
        );
        assert!(
            context.contains("Dusk"),
            "AC-2: time-of-day label must be 'Dusk' at 17:30:\n{context}"
        );
        assert!(
            context.contains("17:"),
            "AC-2: HH:MM must accompany the Dusk label:\n{context}"
        );
        assert!(
            context.contains("greet and refer to the time of day accordingly"),
            "AC-2: greeting-register directive missing:\n{context}"
        );
    }

    /// AC-3 (fix-1224-1225): when the world clock is Dusk, the context must
    /// include a negative directive forbidding "good morning" greetings so
    /// small models cannot override the positive cue with training-data bias.
    #[test]
    fn build_tier1_context_includes_forbidden_morning_greeting_at_dusk() {
        use chrono::TimeZone;
        let mut world = WorldState::new();
        world.clock = parish_types::GameClock::new(
            chrono::Utc
                .with_ymd_and_hms(1820, 3, 20, 17, 30, 0)
                .unwrap(),
        );
        let context = build_tier1_context(&world);

        assert!(
            context.contains("Do NOT say 'good morning'"),
            "AC-3: forbidden-morning-greeting directive missing at Dusk:\n{context}"
        );
    }

    /// Corollary to AC-3: at Morning the forbidden-greeting directive must NOT
    /// appear (a morning greeting is appropriate and the directive would confuse
    /// the model into avoiding a correct response).
    #[test]
    fn build_tier1_context_no_forbidden_greeting_at_morning() {
        // WorldState::new() starts at 08:00 — Morning bucket.
        let world = WorldState::new();
        let context = build_tier1_context(&world);

        assert!(
            !context.contains("Do NOT say 'good morning'"),
            "forbidden-greeting directive must not appear at Morning:\n{context}"
        );
    }

    /// Spot-check `forbidden_greeting_directive` for each bucket.
    #[test]
    fn forbidden_greeting_directive_covers_non_morning_buckets() {
        // Non-morning buckets must produce a directive.
        for tod in [
            TimeOfDay::Dawn,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
            TimeOfDay::Night,
            TimeOfDay::Midnight,
        ] {
            assert!(
                forbidden_greeting_directive(tod).is_some(),
                "expected a directive for {tod:?}"
            );
        }

        // Morning/Midday must produce None — "good morning"/"good day" are correct.
        for tod in [TimeOfDay::Morning, TimeOfDay::Midday] {
            assert!(
                forbidden_greeting_directive(tod).is_none(),
                "unexpected directive for {tod:?}"
            );
        }
    }

    // ── #1224 — length instruction in system prompt (AC-4) ────────────────────

    /// AC-4 (fix-1224-1225): the Tier 1 system prompt must contain a length
    /// instruction so the model has an explicit sentence-count target.
    #[test]
    fn build_tier1_system_prompt_contains_length_guidance() {
        let npc = Npc::new_test_npc();
        let lang = LanguageSettings::english_only();
        let prompt = build_tier1_system_prompt(&npc, false, &lang);
        assert!(
            prompt.contains("2-4 sentences"),
            "AC-4: length guidance missing from system prompt:\n{prompt}"
        );
    }

    // ── #1228 — anti-repetition guard ─────────────────────────────────────────

    /// AC-1: a degenerate loop of one clause repeated many times collapses to a
    /// single instance. This is the dominant #1228 failure mode.
    #[test]
    fn collapse_repeated_sentences_collapses_runaway_loop() {
        let clause = "Speak yer mind, and we'll see what be in it, m'friend.";
        let runaway = clause.repeat(20);
        let collapsed = collapse_repeated_sentences(&runaway);
        // The clause must appear exactly once after collapse.
        let occurrences = collapsed.matches("Speak yer mind").count();
        assert_eq!(
            occurrences, 1,
            "AC-1: runaway clause must collapse to one instance, got {occurrences}:\n{collapsed}"
        );
    }

    /// AC-1: non-repeating multi-sentence dialogue is preserved (no false
    /// collapse). Distinct adjacent sentences must all survive.
    #[test]
    fn collapse_repeated_sentences_preserves_distinct_sentences() {
        let line = "Good evening to ye. The harvest looks fair this year. Will ye have a pint?";
        let collapsed = collapse_repeated_sentences(line);
        assert!(collapsed.contains("Good evening"), "got: {collapsed}");
        assert!(collapsed.contains("harvest looks fair"), "got: {collapsed}");
        assert!(
            collapsed.contains("Will ye have a pint"),
            "got: {collapsed}"
        );
    }

    /// AC-2: identical text, case/whitespace/punctuation-only differences, and
    /// high-overlap text are all flagged near-identical; distinct lines are not.
    #[test]
    fn is_near_identical_normalizes_and_thresholds() {
        let a = "Aye, the rents be cruel this season.";
        // Exact (always near-identical regardless of threshold).
        assert!(is_near_identical(a, a, 1.0));
        // Case / whitespace / trailing punctuation only.
        assert!(is_near_identical(
            a,
            "  AYE,   the rents be cruel this season  ",
            1.0
        ));
        // Clearly distinct content must NOT be flagged at a high threshold.
        assert!(!is_near_identical(
            a,
            "The fiddler will play a reel at the céilí tonight.",
            0.92
        ));
    }

    /// AC-3: when the new line is near-identical to the NPC's previous line, the
    /// guard substitutes a varied, non-empty fallback that differs from both the
    /// previous line and a verbatim repeat.
    #[test]
    fn guard_against_repetition_varies_cross_turn_repeat() {
        let prev = "Sure, the talk 'round here be always of the weather and the rents, m'friend.";
        // Model echoes its own previous line near-verbatim (only trailing
        // punctuation changed).
        let new_line =
            "Sure, the talk 'round here be always of the weather and the rents, m'friend!";
        let out = guard_against_repetition(new_line, Some(prev), 0.92, 42);
        assert!(!out.trim().is_empty(), "AC-3: fallback must be non-empty");
        assert!(
            !is_near_identical(&out, prev, 0.92),
            "AC-3: guard must vary the repeated line, got:\n{out}"
        );
    }

    /// AC-4: a clearly different new line passes through unchanged (no false
    /// positive). Only intra-line collapse may touch it, and here there is none.
    #[test]
    fn guard_against_repetition_passes_distinct_line() {
        let prev = "Good evening to ye, traveller.";
        let new_line = "The miller's wife claims she saw a boat on the stream by night.";
        let out = guard_against_repetition(new_line, Some(prev), 0.92, 7);
        assert_eq!(
            out, new_line,
            "AC-4: distinct line must pass through unchanged"
        );
    }

    /// AC-3 + AC-1 combined: the worst-case #1228 input (a previous line plus a
    /// new line that is BOTH an internal loop AND a repeat of the previous line)
    /// yields a short, varied, non-degenerate line.
    #[test]
    fn guard_against_repetition_handles_loop_that_echoes_previous() {
        let prev = "Speak yer mind, and we'll see what be in it, m'friend.";
        let new_line = "Speak yer mind, and we'll see what be in it, m'friend. ".repeat(15);
        let out = guard_against_repetition(&new_line, Some(prev), 0.92, 3);
        assert!(out.matches("Speak yer mind").count() <= 1, "got:\n{out}");
        assert!(
            !is_near_identical(&out, prev, 0.92),
            "must not echo the previous line, got:\n{out}"
        );
    }

    /// A threshold of 0.0 disables the cross-turn check, but intra-line collapse
    /// still runs (runaway loops are always undesirable).
    #[test]
    fn guard_against_repetition_threshold_zero_disables_cross_turn_only() {
        let prev = "Aye, as I said before.";
        let new_line = "Aye, as I said before.";
        let out = guard_against_repetition(new_line, Some(prev), 0.0, 1);
        // Cross-turn check disabled: the line is NOT replaced by a fallback.
        assert_eq!(out, new_line);
        // But a runaway loop is still collapsed even with the check disabled.
        let loop_line = "Same clause here. ".repeat(10);
        let collapsed = guard_against_repetition(&loop_line, None, 0.0, 1);
        assert_eq!(collapsed.matches("Same clause here").count(), 1);
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
            pronouns: "they/them".to_string(),
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

    /// NPC with a name and location set — shared by prompt / tier2 / tier3 test modules.
    ///
    /// Mirrors the repeated `named_npc` / local `make_test_npc` wrappers that
    /// previously existed in each of those modules (TD-002).
    pub fn make_named_npc(id: u32, name: &str, location: u32) -> Npc {
        let mut npc = make_test_npc(id, location);
        npc.name = name.to_string();
        npc.brief_description = format!("a test NPC named {}", name);
        npc.age = 40;
        npc.personality = "Friendly".to_string();
        npc
    }

    /// NPC with name, occupation, and optional workplace — shared by
    /// `reactions/emoji_reactions` and `reactions/arrival_reactions/tests` (TD-002).
    ///
    /// Intelligence is set to an all-3 vector so tests that assert on reaction
    /// thresholds get consistent results.
    pub fn make_named_occupation_npc(
        id: u32,
        name: &str,
        occupation: &str,
        workplace: Option<LocationId>,
    ) -> Npc {
        let mut npc = make_test_npc(id, workplace.map(|l| l.0).unwrap_or(1));
        npc.name = name.to_string();
        npc.brief_description = format!("a {}", occupation.to_lowercase());
        npc.age = 40;
        npc.occupation = occupation.to_string();
        npc.personality = "A friendly person.".to_string();
        npc.intelligence = Intelligence {
            verbal: 3,
            analytical: 3,
            emotional: 3,
            practical: 3,
            wisdom: 3,
            creative: 3,
        };
        npc.workplace = workplace;
        npc
    }

    /// NPC with a specific age and occupation — shared by `tier4` tests (TD-002).
    ///
    /// Location is fixed to 1 and workplace to 2, matching the previous hand-rolled
    /// `make_npc` in `tier4.rs::tests`.
    pub fn make_aged_occupation_npc(id: u32, age: u8, occupation: &str) -> Npc {
        let mut npc = make_test_npc(id, 1);
        npc.age = age;
        npc.occupation = occupation.to_string();
        npc.personality = "friendly".to_string();
        npc.mood = "content".to_string();
        npc.workplace = Some(LocationId(2));
        npc
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
