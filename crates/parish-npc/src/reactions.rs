//! Player–NPC emoji reaction system and NPC arrival reaction system.
//!
//! # Emoji Reactions (player ↔ NPC)
//!
//! Supports three flows:
//! 1. Player reacts to NPC messages (stored in [`ReactionLog`], injected into prompts)
//! 2. NPCs react to player messages (rule-based keyword matching)
//! 3. NPC-to-NPC reactions (future, via Tier 2 ticks)
//!
//! # Arrival Reactions (NPC → player on location entry)
//!
//! When the player arrives at a location, NPCs present may react —
//! greeting, nodding, welcoming, introducing themselves, or ignoring
//! the player entirely. Reactions are determined by dice rolls modified
//! by NPC personality, occupation, workplace context, mood, time of day,
//! and whether they've already been introduced.
//!
//! Each reaction includes canned fallback text. When `use_llm` is set,
//! the caller can optionally fire a short-timeout LLM call for a richer
//! greeting, falling back to the canned text on timeout or error.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

use crate::{Npc, NpcId};
use parish_config::ReactionConfig;
use parish_inference::AnyClient;
use parish_types::dice::DiceRoll;
use parish_world::graph::LocationData;
use parish_world::time::TimeOfDay;

// ── Emoji reaction system ────────────────────────────────────────────────────

/// The canonical reaction palette mapping emoji to natural-language descriptions.
///
/// Period-appropriate gestures for an 1820s Irish parish. The UI shows emoji;
/// NPC context receives the description string.
pub const REACTION_PALETTE: &[(&str, &str)] = &[
    ("😊", "smiled warmly"),
    ("😠", "looked angry"),
    ("😢", "looked sorrowful"),
    ("😳", "looked startled"),
    ("🤔", "looked thoughtful"),
    ("😏", "smirked knowingly"),
    ("👀", "raised an eyebrow"),
    ("🤫", "made a hushing gesture"),
    ("😂", "laughed heartily"),
    ("🙄", "rolled their eyes"),
    ("🍺", "raised a glass"),
    ("✝️", "crossed themselves"),
];

/// Look up the natural-language description for a reaction emoji.
///
/// Returns `None` if the emoji is not in the palette.
pub fn reaction_description(emoji: &str) -> Option<&'static str> {
    REACTION_PALETTE
        .iter()
        .find(|(e, _)| *e == emoji)
        .map(|(_, desc)| *desc)
}

/// A single reaction entry recording a player's nonverbal response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionEntry {
    /// The emoji used.
    pub emoji: String,
    /// Natural-language description (e.g. "looked angry").
    pub description: String,
    /// Truncated context — what the NPC said that was reacted to.
    pub context: String,
    /// When the reaction occurred.
    pub timestamp: DateTime<Utc>,
}

/// Ring buffer of recent player reactions toward an NPC.
///
/// Stores the last [`MAX_ENTRIES`] reactions and formats them as prompt
/// context so the NPC is aware of the player's nonverbal feedback.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReactionLog {
    entries: Vec<ReactionEntry>,
}

/// Maximum number of reaction entries to retain.
const MAX_ENTRIES: usize = 10;

impl ReactionLog {
    /// Adds a player→NPC reaction (player reacts to something the NPC said).
    ///
    /// Evicts the oldest entry if at capacity. Only adds when the emoji is in
    /// the canonical palette. `context` is what the NPC said.
    pub fn add(&mut self, emoji: &str, context: &str, timestamp: DateTime<Utc>) {
        if let Some(desc) = reaction_description(emoji) {
            self.entries.push(ReactionEntry {
                emoji: emoji.to_string(),
                description: desc.to_string(),
                context: context.chars().take(80).collect(),
                timestamp,
            });
            if self.entries.len() > MAX_ENTRIES {
                self.entries.remove(0);
            }
        }
    }

    /// Adds an NPC→player-message reaction (NPC reacts to a player's spoken line).
    ///
    /// Evicts the oldest entry if at capacity. Only adds when the emoji is in
    /// the canonical palette. `player_message` is the player's message that
    /// triggered the reaction.
    pub fn add_player_message_reaction(
        &mut self,
        emoji: &str,
        player_message: &str,
        timestamp: DateTime<Utc>,
    ) {
        if let Some(desc) = reaction_description(emoji) {
            self.entries.push(ReactionEntry {
                emoji: emoji.to_string(),
                description: desc.to_string(),
                context: player_message.chars().take(80).collect(),
                timestamp,
            });
            if self.entries.len() > MAX_ENTRIES {
                self.entries.remove(0);
            }
        }
    }

    /// Formats the `n` most recent player→NPC reactions as prompt context.
    ///
    /// Each line reads: "- The player [description] when you said [context]".
    /// Returns an empty string if there are no reactions.
    pub fn context_string(&self, n: usize) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let lines: Vec<String> = self
            .entries
            .iter()
            .rev()
            .take(n)
            .map(|e| {
                format!(
                    "- The player {} when you said \"{}\"",
                    e.description, e.context
                )
            })
            .collect();
        format!(
            "Recent nonverbal reactions from the player:\n{}",
            lines.join("\n")
        )
    }

    /// Formats the `n` most recent NPC→player-message reactions as prompt context.
    ///
    /// Each line reads: "- You [description] in response to the player saying [message]".
    /// Returns an empty string if there are no entries.
    pub fn npc_context_string(&self, n: usize) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let lines: Vec<String> = self
            .entries
            .iter()
            .rev()
            .take(n)
            .map(|e| {
                format!(
                    "- You {} in response to the player saying \"{}\"",
                    e.description, e.context
                )
            })
            .collect();
        format!(
            "Recent nonverbal reactions you showed to the player:\n{}",
            lines.join("\n")
        )
    }

    /// Returns the number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the stored entries in chronological order (oldest first).
    pub fn entries(&self) -> &[ReactionEntry] {
        &self.entries
    }
}

/// Keyword groups that trigger NPC reactions, with the corresponding emoji.
const KEYWORD_REACTIONS: &[(&[&str], &str)] = &[
    (&["death", "died", "killed", "murder"], "😢"),
    (&["fairy", "fairies", "púca", "banshee", "sidhe"], "✝️"),
    (&["drink", "whiskey", "poitín", "ale", "stout"], "🍺"),
    (&["joke", "funny", "laugh", "haha"], "😂"),
    (&["secret", "don't tell", "between us", "confidence"], "🤫"),
    (&["rent", "evict", "landlord", "agent", "tithe"], "😠"),
    (&["gold", "treasure", "fortune", "money", "reward"], "👀"),
    (&["strange", "ghost", "haunted", "spirit"], "😳"),
];

/// Generates a rule-based NPC reaction to player input.
///
/// Returns `Some(emoji)` if a keyword match triggers a reaction (60% chance),
/// or `None` if no reaction is generated.
pub fn generate_rule_reaction(player_input: &str) -> Option<String> {
    let input_lower = player_input.to_lowercase();

    for (keywords, emoji) in KEYWORD_REACTIONS {
        if keywords.iter().any(|kw| input_lower.contains(kw)) {
            // 60% chance to react — not every NPC reacts every time
            if rand::random::<f64>() < 0.6 {
                return Some((*emoji).to_string());
            }
        }
    }

    None
}

/// Deterministic variant for testing — always returns a reaction if keywords match.
#[cfg(test)]
fn generate_rule_reaction_deterministic(player_input: &str) -> Option<String> {
    let input_lower = player_input.to_lowercase();

    for (keywords, emoji) in KEYWORD_REACTIONS {
        if keywords.iter().any(|kw| input_lower.contains(kw)) {
            return Some((*emoji).to_string());
        }
    }

    None
}

// ── LLM-informed player-message reactions ────────────────────────────────────

/// Structured output returned by the player-message reaction inference call.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LlmReactionDecision {
    /// Emoji from [`REACTION_PALETTE`], or `None` for no visible reaction.
    pub emoji: Option<String>,
}

/// Builds the system and user prompts used to infer an NPC emoji reaction to
/// a player message.
///
/// The system prompt enumerates the full [`REACTION_PALETTE`] and the legacy
/// keyword cues as weak few-shot examples. The user prompt contains the NPC's
/// name, occupation, mood, and personality snippet followed by the player
/// message.
pub fn build_player_message_reaction_prompt(npc: &Npc, player_input: &str) -> (String, String) {
    let palette_lines: Vec<String> = REACTION_PALETTE
        .iter()
        .map(|(emoji, desc)| format!("- {emoji}: {desc}"))
        .chain(std::iter::once("- null: no visible reaction".to_string()))
        .collect();

    let keyword_examples = KEYWORD_REACTIONS
        .iter()
        .map(|(group, emoji)| format!("- {:?} -> {}", group, emoji))
        .collect::<Vec<_>>()
        .join("\n");

    let system = format!(
        "You decide whether a single NPC would visibly react to a player's spoken line.\n\
         Return STRICT JSON only with schema: {{\"emoji\": <emoji-or-null>}}.\n\
         If unsure or neutral, return {{\"emoji\": null}}.\n\
         Never invent new emoji.\n\
         Available palette:\n{}\n\
         Legacy keyword cues (weak examples only; use full meaning and tone):\n{}",
        palette_lines.join("\n"),
        keyword_examples,
    );

    let personality_snippet: String = npc.personality.chars().take(300).collect();
    let user = format!(
        "NPC:\n\
         - Name: {}\n\
         - Occupation: {}\n\
         - Mood: {}\n\
         - Personality: {}\n\n\
         Player message:\n\
         \"{}\"\n\n\
         Choose one emoji or null.",
        npc.name, npc.occupation, npc.mood, personality_snippet, player_input,
    );

    (system, user)
}

/// Uses the LLM to infer an NPC emoji reaction for a player message.
///
/// Returns `Some(emoji)` only when:
/// - inference succeeds within `timeout`,
/// - the output emoji is in [`REACTION_PALETTE`], and
/// - the 60 % probabilistic gate fires (same rate as rule-based reactions).
///
/// Returns `None` for all errors, unknown emoji, explicit null output, or when
/// the probabilistic gate does not fire. This function never panics.
pub async fn infer_player_message_reaction(
    client: &AnyClient,
    model: &str,
    npc: &Npc,
    player_input: &str,
    timeout: Duration,
) -> Option<String> {
    let (system, prompt) = build_player_message_reaction_prompt(npc, player_input);
    let call =
        client.generate_json::<LlmReactionDecision>(model, &prompt, Some(&system), Some(40), None);

    let response = tokio::time::timeout(timeout, call).await.ok()?.ok()?;
    let emoji = response.emoji?;
    reaction_description(&emoji)?;
    if rand::random::<f64>() >= 0.6 {
        return None;
    }
    Some(emoji)
}

// ── Arrival reaction system ──────────────────────────────────────────────────

/// The kind of reaction an NPC has to the player's arrival.
#[derive(Debug, Clone, PartialEq)]
pub enum ReactionKind {
    /// A silent gesture: nod, wave, glance up.
    Gesture,
    /// A short verbal greeting.
    Greeting,
    /// A role-specific welcome at the NPC's workplace.
    Welcome,
    /// The NPC introduces themselves by name.
    Introduction,
}

/// A resolved reaction from one NPC.
#[derive(Debug, Clone)]
pub struct NpcReaction {
    /// Which NPC reacted.
    pub npc_id: NpcId,
    /// Display name (full name or brief description).
    pub npc_display_name: String,
    /// What kind of reaction.
    pub kind: ReactionKind,
    /// The canned fallback text for this reaction.
    pub canned_text: String,
    /// Whether this reaction should mark the NPC as introduced.
    pub introduces: bool,
    /// Whether the caller should attempt an LLM-generated greeting.
    pub use_llm: bool,
}

// ── Template bank ───────────────────────────────────────────────────────────

/// Mod-overridable reaction text templates.
#[derive(Debug, Clone, Deserialize)]
pub struct ReactionTemplates {
    /// Silent gesture descriptions.
    #[serde(default = "default_gestures")]
    pub gestures: Vec<String>,
    /// Greetings by time of day.
    #[serde(default = "default_greetings")]
    pub greetings: GreetingsByTime,
    /// Workplace welcome lines keyed by occupation.
    #[serde(default = "default_welcomes")]
    pub welcomes: WelcomesByOccupation,
    /// Introduction lines.
    #[serde(default = "default_introductions")]
    pub introductions: IntroductionTemplates,
    /// Occupation-specific greetings (non-workplace).
    #[serde(default = "default_occupation_greetings")]
    pub occupation_greetings: OccupationGreetings,
}

impl Default for ReactionTemplates {
    fn default() -> Self {
        Self {
            gestures: default_gestures(),
            greetings: default_greetings(),
            welcomes: default_welcomes(),
            introductions: default_introductions(),
            occupation_greetings: default_occupation_greetings(),
        }
    }
}

/// Greetings keyed by time of day.
#[derive(Debug, Clone, Deserialize)]
pub struct GreetingsByTime {
    /// Morning greetings.
    #[serde(default)]
    pub morning: Vec<String>,
    /// Afternoon greetings.
    #[serde(default)]
    pub afternoon: Vec<String>,
    /// Evening / night greetings.
    #[serde(default)]
    pub evening: Vec<String>,
    /// Greetings suitable for any time.
    #[serde(default)]
    pub any: Vec<String>,
}

/// Workplace welcome lines keyed by lowercase occupation.
#[derive(Debug, Clone, Deserialize)]
pub struct WelcomesByOccupation {
    /// Publican-specific welcomes.
    #[serde(default)]
    pub publican: Vec<String>,
    /// Shopkeeper-specific welcomes.
    #[serde(default)]
    pub shopkeeper: Vec<String>,
    /// Priest-specific welcomes (at church).
    #[serde(default)]
    pub priest: Vec<String>,
    /// Teacher-specific welcomes.
    #[serde(default)]
    pub teacher: Vec<String>,
    /// Generic welcome for other occupations at workplace.
    #[serde(default)]
    pub generic: Vec<String>,
}

/// Introduction text templates.
#[derive(Debug, Clone, Deserialize)]
pub struct IntroductionTemplates {
    /// Introductions at their workplace.
    #[serde(default)]
    pub workplace: Vec<String>,
    /// Casual introductions elsewhere.
    #[serde(default)]
    pub casual: Vec<String>,
}

/// Occupation-specific greetings used outside the workplace.
#[derive(Debug, Clone, Deserialize)]
pub struct OccupationGreetings {
    /// Priest greetings (blessings etc.).
    #[serde(default)]
    pub priest: Vec<String>,
}

// ── Core algorithm ──────────────────────────────────────────────────────────

/// Returns `true` if the mood string suggests a negative emotional state.
fn is_negative_mood(mood: &str) -> bool {
    let m = mood.to_lowercase();
    m.contains("angry")
        || m.contains("furious")
        || m.contains("sad")
        || m.contains("grief")
        || m.contains("irritat")
        || m.contains("frustrat")
        || m.contains("anxious")
        || m.contains("afraid")
        || m.contains("hostile")
        || m.contains("bitter")
        || m.contains("sullen")
        || m.contains("withdrawn")
}

/// Returns `true` if the NPC is currently at their workplace.
fn is_at_workplace(npc: &Npc, location: &LocationData) -> bool {
    npc.workplace.is_some_and(|wp| wp == location.id)
}

/// Computes the reaction threshold for a given NPC — the probability
/// (0.0–1.0) that this NPC will react to the player's arrival.
pub fn reaction_threshold(
    npc: &Npc,
    location: &LocationData,
    time_of_day: TimeOfDay,
    config: &ReactionConfig,
) -> f64 {
    let mut threshold = config.base_chance;

    if is_at_workplace(npc, location) {
        threshold += config.workplace_bonus;
    }
    if location.indoor {
        threshold += config.indoor_bonus;
    }
    if npc.intelligence.emotional >= 4 {
        threshold += config.empathy_bonus;
    }
    if is_negative_mood(&npc.mood) {
        threshold -= config.negative_mood_penalty;
    }
    if matches!(time_of_day, TimeOfDay::Night | TimeOfDay::Midnight) {
        threshold -= config.night_penalty;
    }

    threshold.clamp(0.0, 1.0)
}

/// Generates arrival reactions for NPCs at the player's current location.
///
/// Each NPC needs **two** dice rolls in `dice` (one for reaction chance,
/// one for type/template selection). So `dice.len()` must be `≥ npcs.len() * 2`.
///
/// Returns only NPCs that actually react — silent NPCs are omitted.
#[allow(clippy::too_many_arguments)]
pub fn generate_arrival_reactions(
    npcs: &[&Npc],
    introduced: &HashSet<NpcId>,
    location: &LocationData,
    time_of_day: TimeOfDay,
    weather: &str,
    templates: &ReactionTemplates,
    config: &ReactionConfig,
    dice: &[DiceRoll],
) -> Vec<NpcReaction> {
    let mut reactions = Vec::new();

    for (i, npc) in npcs.iter().enumerate() {
        let roll_idx = i * 2;
        if roll_idx + 1 >= dice.len() {
            break;
        }
        let chance_roll = &dice[roll_idx];
        let type_roll = &dice[roll_idx + 1];

        let threshold = reaction_threshold(npc, location, time_of_day, config);
        if !chance_roll.check(threshold) {
            continue; // NPC stays silent
        }

        let is_introduced = introduced.contains(&npc.id);
        let at_workplace = is_at_workplace(npc, location);
        let occupation = npc.occupation.to_lowercase();

        let (kind, introduces, use_llm) = if at_workplace && !is_introduced {
            (ReactionKind::Introduction, true, true)
        } else if at_workplace && is_introduced {
            (ReactionKind::Welcome, false, true)
        } else if !is_introduced && type_roll.check(0.25) {
            (ReactionKind::Introduction, true, true)
        } else if !is_introduced {
            (ReactionKind::Gesture, false, false)
        } else if is_priest_occupation(&occupation) {
            (ReactionKind::Greeting, false, type_roll.check(0.5))
        } else if type_roll.check(0.5) {
            (ReactionKind::Greeting, false, false)
        } else {
            (ReactionKind::Gesture, false, false)
        };

        let display_name = if is_introduced || introduces {
            npc.name.clone()
        } else {
            npc.brief_description.clone()
        };

        let canned_text = pick_canned_text(
            &kind,
            npc,
            &display_name,
            at_workplace,
            &occupation,
            time_of_day,
            weather,
            templates,
            type_roll,
        );

        reactions.push(NpcReaction {
            npc_id: npc.id,
            npc_display_name: display_name,
            kind,
            canned_text,
            introduces,
            use_llm,
        });
    }

    // Cap the number of simultaneous reactions, prioritising the most
    // socially significant kinds first so the publican always greets you
    // before a random patron does.
    if config.max_reactions > 0 && reactions.len() > config.max_reactions {
        reactions.sort_by_key(|r| match r.kind {
            ReactionKind::Introduction => 0u8,
            ReactionKind::Welcome => 1,
            ReactionKind::Greeting => 2,
            ReactionKind::Gesture => 3,
        });
        reactions.truncate(config.max_reactions);
    }

    reactions
}

fn is_priest_occupation(occupation: &str) -> bool {
    occupation.contains("priest") || occupation.contains("clergy") || occupation.contains("curate")
}

/// Picks a canned text template and substitutes placeholders.
#[allow(clippy::too_many_arguments)]
fn pick_canned_text(
    kind: &ReactionKind,
    npc: &Npc,
    display_name: &str,
    at_workplace: bool,
    occupation: &str,
    time_of_day: TimeOfDay,
    weather: &str,
    templates: &ReactionTemplates,
    roll: &DiceRoll,
) -> String {
    let raw = match kind {
        ReactionKind::Gesture => roll.pick(&templates.gestures).clone(),
        ReactionKind::Greeting => {
            if is_priest_occupation(occupation) {
                // Try occupation-specific greetings first
                if !templates.occupation_greetings.priest.is_empty() {
                    roll.pick(&templates.occupation_greetings.priest).clone()
                } else {
                    pick_greeting_by_time(time_of_day, templates, roll)
                }
            } else {
                pick_greeting_by_time(time_of_day, templates, roll)
            }
        }
        ReactionKind::Welcome => {
            let pool = match () {
                _ if occupation.contains("publican") => &templates.welcomes.publican,
                _ if occupation.contains("shopkeeper") => &templates.welcomes.shopkeeper,
                _ if is_priest_occupation(occupation) => &templates.welcomes.priest,
                _ if occupation.contains("teacher") => &templates.welcomes.teacher,
                _ => &templates.welcomes.generic,
            };
            if pool.is_empty() {
                pick_greeting_by_time(time_of_day, templates, roll)
            } else {
                roll.pick(pool).clone()
            }
        }
        ReactionKind::Introduction => {
            let pool = if at_workplace {
                &templates.introductions.workplace
            } else {
                &templates.introductions.casual
            };
            if pool.is_empty() {
                "\"I'm {},\" they say.".to_string()
            } else {
                roll.pick(pool).clone()
            }
        }
    };

    substitute_placeholders(&raw, npc, display_name, time_of_day, weather)
}

fn pick_greeting_by_time(
    time_of_day: TimeOfDay,
    templates: &ReactionTemplates,
    roll: &DiceRoll,
) -> String {
    let time_pool = match time_of_day {
        TimeOfDay::Dawn | TimeOfDay::Morning => &templates.greetings.morning,
        TimeOfDay::Midday | TimeOfDay::Afternoon => &templates.greetings.afternoon,
        TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight => &templates.greetings.evening,
    };

    // 30% chance to use an "any time" greeting instead
    if !templates.greetings.any.is_empty() && roll.value() < 0.3 {
        return roll.pick(&templates.greetings.any).clone();
    }

    if time_pool.is_empty() && !templates.greetings.any.is_empty() {
        roll.pick(&templates.greetings.any).clone()
    } else if time_pool.is_empty() {
        "\"Hello,\" they say.".to_string()
    } else {
        roll.pick(time_pool).clone()
    }
}

/// Substitutes `{name}`, `{first_name}`, `{last_name}`, `{occupation}`,
/// `{time}`, `{weather}` placeholders in a template string.
fn substitute_placeholders(
    template: &str,
    npc: &Npc,
    display_name: &str,
    time_of_day: TimeOfDay,
    weather: &str,
) -> String {
    let first_name = npc.name.split_whitespace().next().unwrap_or(&npc.name);
    let last_name = npc.name.split_whitespace().last().unwrap_or(&npc.name);
    let time_str = match time_of_day {
        TimeOfDay::Dawn => "dawn",
        TimeOfDay::Morning => "morning",
        TimeOfDay::Midday => "midday",
        TimeOfDay::Afternoon => "afternoon",
        TimeOfDay::Dusk => "evening",
        TimeOfDay::Night => "evening",
        TimeOfDay::Midnight => "night",
    };

    template
        .replace("{name}", display_name)
        .replace("{first_name}", first_name)
        .replace("{last_name}", last_name)
        .replace("{occupation}", &npc.occupation)
        .replace("{time}", time_str)
        .replace("{weather}", weather)
}

// ── Default template banks ──────────────────────────────────────────────────

fn default_gestures() -> Vec<String> {
    vec![
        "{name} nods in your direction.".into(),
        "{name} glances up as you arrive.".into(),
        "{name} gives a brief wave.".into(),
        "{name} tips their hat without a word.".into(),
        "{name} looks up from what they're doing.".into(),
        "{name} shifts to make room.".into(),
        "{name} raises a hand in greeting.".into(),
        "{name} glances over, then goes back to what they were doing.".into(),
        "{name} pauses mid-step and looks your way.".into(),
        "{name} touches the brim of their hat.".into(),
        "{name} gives a curt nod.".into(),
        "{name} half-turns and acknowledges you with a look.".into(),
        "{name} catches your eye for a moment.".into(),
        "{name} straightens up as you approach.".into(),
        "{name} steps aside to let you pass.".into(),
        "{name} sets down what they're holding and looks up.".into(),
        "{name} watches you arrive with quiet interest.".into(),
        "{name} barely looks up.".into(),
        "{name} grunts softly in acknowledgement.".into(),
        "{name} gives the slightest nod.".into(),
        "{name} leans against the wall and watches you enter.".into(),
        "{name} lifts a hand from their pocket in greeting.".into(),
    ]
}

fn default_greetings() -> GreetingsByTime {
    GreetingsByTime {
        morning: vec![
            "\"Good morning to you,\" {name} says.".into(),
            "\"Ah, morning,\" says {name}.".into(),
            "\"God bless this fine morning,\" {name} says warmly.".into(),
            "\"You're up early,\" {name} remarks.".into(),
            "\"Maidin mhaith,\" says {name}.".into(),
            "\"A fresh morning, thanks be to God,\" says {name}.".into(),
            "\"Grand morning for it,\" {name} observes.".into(),
            "\"Morning,\" {name} says simply.".into(),
            "\"You're welcome this morning,\" says {name}.".into(),
            "\"Dia dhuit ar maidin,\" {name} says.".into(),
            "\"An early start,\" {name} remarks approvingly.".into(),
            "\"The morning air has life in it today,\" says {name}.".into(),
        ],
        afternoon: vec![
            "\"Grand day,\" says {name}.".into(),
            "\"Good day to you,\" {name} says.".into(),
            "\"Afternoon,\" says {name} with a nod.".into(),
            "\"Dia dhuit,\" says {name}.".into(),
            "\"It's yourself,\" {name} says.".into(),
            "\"You're welcome,\" says {name}.".into(),
            "\"God bless,\" says {name}.".into(),
            "\"Fine afternoon,\" {name} remarks.".into(),
            "\"You picked a good day for it,\" says {name}.".into(),
            "\"Tráthnóna maith,\" says {name}.".into(),
            "\"Not a bad day at all,\" says {name}.".into(),
            "\"The day's wearing on,\" {name} observes.".into(),
        ],
        evening: vec![
            "\"Good evening,\" {name} says quietly.".into(),
            "\"Late enough to be out,\" {name} observes.".into(),
            "\"Evening,\" {name} says.".into(),
            "\"God bless the evening,\" says {name}.".into(),
            "\"Oíche mhaith,\" says {name}.".into(),
            "\"A quiet night,\" {name} says.".into(),
            "\"You're out late,\" {name} remarks.".into(),
            "\"Evening to you,\" says {name}.".into(),
            "\"Not many out at this hour,\" {name} says.".into(),
            "\"The night is drawing in,\" says {name}.".into(),
            "\"A cold one tonight,\" {name} says with a shiver.".into(),
            "\"The stars are out,\" {name} observes.".into(),
        ],
        any: vec![
            "\"God bless,\" says {name}.".into(),
            "\"Dia dhuit,\" {name} says.".into(),
            "\"You're welcome,\" says {name}.".into(),
            "\"Ah, it's yourself,\" says {name}.".into(),
            "\"How are you keeping?\" asks {name}.".into(),
            "\"Céad míle fáilte,\" says {name} warmly.".into(),
            "\"Safe travels to you,\" says {name}.".into(),
            "\"Well now,\" says {name}.".into(),
            "\"Fair play to you for coming,\" says {name}.".into(),
            "\"And here you are,\" says {name}.".into(),
            "\"You're a welcome sight,\" says {name}.".into(),
            "\"Good to see a face,\" says {name}.".into(),
        ],
    }
}

fn default_welcomes() -> WelcomesByOccupation {
    WelcomesByOccupation {
        publican: vec![
            "\"Come in, come in! Take a seat by the fire,\" says {name}.".into(),
            "\"Welcome! What'll it be?\" says {name}, reaching for a glass.".into(),
            "\"Ah, you're back. The usual?\" says {name}.".into(),
            "\"In you come out of the {weather},\" says {name}. \"What can I get you?\"".into(),
            "\"You're welcome here,\" says {name}, wiping down the bar.".into(),
            "\"Fáilte! Come in and rest yourself,\" says {name}.".into(),
            "\"The fire's going well. Sit yourself down,\" says {name}.".into(),
            "\"Sit down there and I'll bring you something,\" says {name}.".into(),
            "\"You look like you could use a drink,\" says {name} with a grin.".into(),
            "\"Come in out of the cold. The fire's lit,\" says {name}.".into(),
            "\"Ah, a customer! Come in, come in,\" says {name}.".into(),
            "\"There's a stool here with your name on it,\" says {name}.".into(),
        ],
        shopkeeper: vec![
            "\"Come in, come in,\" says {name}. \"What can I get you?\"".into(),
            "\"Ah, good {time}! Looking for anything in particular?\" says {name}.".into(),
            "\"You're welcome,\" says {name}, looking up from the counter.".into(),
            "\"In you come,\" says {name}. \"I've fresh stock in today.\"".into(),
            "\"What'll it be today?\" asks {name}.".into(),
            "\"Fáilte,\" says {name}. \"Have a look around.\"".into(),
            "\"Come in out of the {weather},\" says {name}.".into(),
            "\"Ah, you're here. Good timing,\" says {name}.".into(),
            "\"Step in, step in. The door's open,\" says {name}.".into(),
            "\"What are you after today?\" asks {name} pleasantly.".into(),
            "\"Another fine customer,\" {name} says with a smile.".into(),
            "\"I was just arranging the shelves. Come in,\" says {name}.".into(),
        ],
        priest: vec![
            "\"Welcome to God's house,\" says {name} warmly.".into(),
            "\"Blessings on you this {time},\" says {name}.".into(),
            "\"Peace be with you,\" says {name}, making a small sign of the cross.".into(),
            "\"God bless you, child,\" says {name}.".into(),
            "\"You are always welcome here,\" says {name} gently.".into(),
            "\"Dia dhuit. Come in, come in,\" says {name}.".into(),
            "\"The Lord's house is open to all,\" says {name}.".into(),
            "\"A good {time} to visit,\" says {name}. \"The church is quiet.\"".into(),
            "\"Come in and be at peace,\" says {name}.".into(),
            "\"Fáilte romhat. God bless,\" says {name}.".into(),
        ],
        teacher: vec![
            "\"Ah, a visitor,\" says {name}, setting down a book.".into(),
            "\"Come in quietly if you will,\" says {name}. \"The lesson's nearly done.\"".into(),
            "\"You're welcome,\" says {name}. \"Mind the slate on the bench.\"".into(),
            "\"Dia dhuit,\" says {name}. \"Are you here to learn?\"".into(),
            "\"Good {time},\" says {name}, brushing chalk from their hands.".into(),
            "\"You're welcome here,\" says {name}. \"Knowledge is for all.\"".into(),
            "\"Step in,\" says {name}. \"We were just finishing.\"".into(),
            "\"Ah, a new face,\" says {name} with curiosity.".into(),
        ],
        generic: vec![
            "\"Come in, you're welcome,\" says {name}.".into(),
            "\"Good {time},\" says {name}. \"Can I help you?\"".into(),
            "\"Ah, hello there,\" says {name}.".into(),
            "\"You're welcome,\" {name} says pleasantly.".into(),
            "\"Come in, come in,\" says {name}.".into(),
            "\"Fáilte,\" says {name} warmly.".into(),
        ],
    }
}

fn default_introductions() -> IntroductionTemplates {
    IntroductionTemplates {
        workplace: vec![
            "\"I'm {name}, the {occupation} here. You're welcome,\" they say.".into(),
            "\"The name's {first_name}. I'm the {occupation},\" they say, extending a hand.".into(),
            "\"I don't think we've met. {first_name} {last_name}, {occupation},\" they say.".into(),
            "\"Welcome. I'm {name} — I run this place,\" they say.".into(),
            "\"{first_name},\" they say with a nod. \"I'm the {occupation} here.\"".into(),
            "\"And who might you be? I'm {name}, the {occupation},\" they say.".into(),
            "\"You must be new to the parish. I'm {name},\" they say.".into(),
            "\"I don't believe I've seen you before. I'm {name}, {occupation},\" they say.".into(),
            "\"Welcome to my place. {first_name} {last_name},\" they say. \"{occupation}.\"".into(),
            "\"I'm {first_name}. This is my place of work,\" they say.".into(),
        ],
        casual: vec![
            "\"I don't think we've met. I'm {name},\" they say.".into(),
            "\"{first_name},\" they say simply, with a nod.".into(),
            "\"The name is {name},\" they say. \"{occupation}.\"".into(),
            "\"I'm {first_name}. Are you new to the parish?\" they ask.".into(),
            "\"Have we met? I'm {name},\" they say.".into(),
            "{name} extends a hand. \"{first_name}.\"".into(),
            "\"You're a stranger to me. I'm {name},\" they say.".into(),
            "\"New around here, are you? {first_name} {last_name},\" they say.".into(),
            "\"I'm {name}. I don't think I've seen you about before,\" they say.".into(),
            "\"And you are? I'm {first_name},\" they say with a friendly nod.".into(),
        ],
    }
}

fn default_occupation_greetings() -> OccupationGreetings {
    OccupationGreetings {
        priest: vec![
            "\"God be with you,\" says {name}.".into(),
            "\"Blessings on you this {time},\" says {name}.".into(),
            "\"Peace of Christ be with you,\" says {name}.".into(),
            "\"The Lord keep you,\" says {name} with a gentle nod.".into(),
            "\"God bless, child,\" says {name}.".into(),
            "\"Dia dhuit agus Muire dhuit,\" says {name}.".into(),
            "\"May the road rise with you,\" says {name}.".into(),
            "\"Go mbeannaí Dia dhuit,\" says {name} softly.".into(),
            "\"A blessing on your journey,\" says {name}.".into(),
            "\"The peace of God be upon you,\" says {name}.".into(),
        ],
    }
}

// ── LLM greeting ────────────────────────────────────────────────────────────

/// Builds a short system prompt for an LLM-generated arrival greeting.
pub fn build_reaction_prompt(
    npc: &Npc,
    location_name: &str,
    time_of_day: TimeOfDay,
    weather: &str,
    is_introduced: bool,
    at_workplace: bool,
) -> (String, String) {
    let time_str = match time_of_day {
        TimeOfDay::Dawn => "dawn",
        TimeOfDay::Morning => "morning",
        TimeOfDay::Midday => "midday",
        TimeOfDay::Afternoon => "afternoon",
        TimeOfDay::Dusk => "dusk",
        TimeOfDay::Night => "night",
        TimeOfDay::Midnight => "late at night",
    };

    let personality_snippet: String = npc.personality.chars().take(200).collect();

    let intro_context = if !is_introduced && at_workplace {
        format!(
            "You have not met this person before. You are working here as the {}. \
             Introduce yourself briefly.",
            npc.occupation
        )
    } else if !is_introduced {
        "You have not met this person before. You may introduce yourself or simply acknowledge them."
            .to_string()
    } else if at_workplace {
        format!(
            "You know this person. You are working here as the {}.",
            npc.occupation
        )
    } else {
        "You have met this person before.".to_string()
    };

    let system = format!(
        "You are {name}, a {age}-year-old {occupation} in rural Ireland, 1820.\n\
         {personality}\n\
         Current mood: {mood}\n\n\
         Write a single brief greeting or reaction (1-2 sentences max). \
         Dialogue only, no narration or action descriptions. \
         Do not use any modern language.",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        personality = personality_snippet,
        mood = npc.mood,
    );

    let context = format!(
        "A newcomer has just arrived at {location}. It is {time}, {weather}.\n{intro}",
        location = location_name,
        time = time_str,
        weather = weather,
        intro = intro_context,
    );

    (system, context)
}

/// Attempts an LLM-generated greeting with a short timeout.
///
/// Returns the LLM text if it responds in time, or the canned fallback
/// text from the reaction if the call times out or errors.
#[allow(clippy::too_many_arguments)]
pub async fn resolve_llm_greeting(
    reaction: &NpcReaction,
    npc: &Npc,
    location_name: &str,
    time_of_day: TimeOfDay,
    weather: &str,
    is_introduced: bool,
    at_workplace: bool,
    client: &AnyClient,
    model: &str,
    timeout_secs: u64,
) -> String {
    let (system, context) = build_reaction_prompt(
        npc,
        location_name,
        time_of_day,
        weather,
        is_introduced,
        at_workplace,
    );

    let timeout = Duration::from_secs(timeout_secs);
    let result = tokio::time::timeout(
        timeout,
        client.generate(model, &context, Some(&system), Some(100), None),
    )
    .await;

    match result {
        Ok(Ok(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                reaction.canned_text.clone()
            } else {
                // Clean up: remove any metadata block the LLM might append
                let cleaned = trimmed.split("---").next().unwrap_or(trimmed).trim();
                if cleaned.is_empty() {
                    reaction.canned_text.clone()
                } else {
                    cleaned.to_string()
                }
            }
        }
        _ => reaction.canned_text.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{LongTermMemory, ShortTermMemory};
    use crate::types::{Intelligence, NpcState};
    use chrono::TimeZone;
    use parish_types::LocationId;
    use parish_types::dice::{DiceRoll, fixed_n};
    use parish_world::graph::GeoKind;
    use std::collections::HashMap;

    fn test_npc(id: u32, name: &str, occupation: &str, workplace: Option<LocationId>) -> Npc {
        Npc {
            id: NpcId(id),
            name: name.to_string(),
            brief_description: format!("a {}", occupation.to_lowercase()),
            age: 40,
            occupation: occupation.to_string(),
            personality: "A friendly person.".to_string(),
            intelligence: Intelligence {
                verbal: 3,
                analytical: 3,
                emotional: 3,
                practical: 3,
                wisdom: 3,
                creative: 3,
            },
            location: LocationId(1),
            mood: "content".to_string(),
            home: Some(LocationId(1)),
            workplace,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: vec![],
            state: NpcState::Present,
            deflated_summary: None,
            reaction_log: ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
        }
    }

    fn test_location(id: u32, indoor: bool) -> LocationData {
        LocationData {
            id: LocationId(id),
            name: "Test Location".to_string(),
            description_template: String::new(),
            indoor,
            public: true,
            connections: vec![],
            lat: 0.0,
            lon: 0.0,
            associated_npcs: vec![],
            mythological_significance: None,
            aliases: vec![],
            geo_kind: GeoKind::Fictional,
            relative_to: None,
            geo_source: None,
        }
    }

    // ── Emoji reaction tests ─────────────────────────────────────────────────

    #[test]
    fn reaction_description_known_emoji() {
        assert_eq!(reaction_description("😊"), Some("smiled warmly"));
        assert_eq!(reaction_description("😠"), Some("looked angry"));
        assert_eq!(reaction_description("✝️"), Some("crossed themselves"));
    }

    #[test]
    fn reaction_description_unknown_emoji() {
        assert_eq!(reaction_description("💀"), None);
        assert_eq!(reaction_description("hello"), None);
    }

    #[test]
    fn reaction_log_add_and_len() {
        let mut log = ReactionLog::default();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);

        log.add(
            "😊",
            "Hello there",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        assert_eq!(log.len(), 1);
        assert!(!log.is_empty());
    }

    #[test]
    fn reaction_log_ignores_unknown_emoji() {
        let mut log = ReactionLog::default();
        log.add(
            "💀",
            "test",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        assert!(log.is_empty());
    }

    #[test]
    fn reaction_log_caps_at_max_entries() {
        let mut log = ReactionLog::default();
        for i in 0..15 {
            log.add(
                "😊",
                &format!("message {}", i),
                Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            );
        }
        assert_eq!(log.len(), MAX_ENTRIES);
        // Oldest entries should be evicted
        assert!(log.entries[0].context.contains("message 5"));
    }

    #[test]
    fn reaction_log_truncates_context() {
        let mut log = ReactionLog::default();
        let long_context = "a".repeat(200);
        log.add(
            "😊",
            &long_context,
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        assert_eq!(log.entries[0].context.len(), 80);
    }

    #[test]
    fn reaction_log_context_string_empty() {
        let log = ReactionLog::default();
        assert_eq!(log.context_string(5), "");
    }

    #[test]
    fn reaction_log_context_string_formats_correctly() {
        let mut log = ReactionLog::default();
        log.add(
            "😠",
            "The rent was raised",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        log.add(
            "😊",
            "Welcome to the pub",
            Utc.with_ymd_and_hms(1820, 3, 20, 11, 0, 0).unwrap(),
        );

        let ctx = log.context_string(5);
        assert!(ctx.contains("Recent nonverbal reactions from the player:"));
        assert!(ctx.contains("smiled warmly"));
        assert!(ctx.contains("looked angry"));
        assert!(ctx.contains("The rent was raised"));
    }

    #[test]
    fn reaction_log_context_string_respects_limit() {
        let mut log = ReactionLog::default();
        for i in 0..5 {
            log.add(
                "😊",
                &format!("msg {}", i),
                Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            );
        }
        let ctx = log.context_string(2);
        // Should only contain the 2 most recent
        assert!(ctx.contains("msg 4"));
        assert!(ctx.contains("msg 3"));
        assert!(!ctx.contains("msg 2"));
    }

    #[test]
    fn reaction_log_serde_round_trip() {
        let mut log = ReactionLog::default();
        log.add(
            "😊",
            "test message",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );

        let json = serde_json::to_string(&log).unwrap();
        let deser: ReactionLog = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.len(), 1);
        assert_eq!(deser.entries[0].emoji, "😊");
    }

    #[test]
    fn generate_rule_reaction_keyword_match() {
        // Deterministic variant always returns on match
        assert_eq!(
            generate_rule_reaction_deterministic("The fairy fort is cursed"),
            Some("✝️".to_string())
        );
        assert_eq!(
            generate_rule_reaction_deterministic("Let's have a drink of poitín"),
            Some("🍺".to_string())
        );
        assert_eq!(
            generate_rule_reaction_deterministic("The rent is too high"),
            Some("😠".to_string())
        );
    }

    #[test]
    fn generate_rule_reaction_no_match() {
        assert_eq!(
            generate_rule_reaction_deterministic("Good morning to you"),
            None
        );
    }

    #[test]
    fn llm_reaction_decision_allows_null() {
        let parsed: LlmReactionDecision = serde_json::from_str(r#"{"emoji":null}"#).unwrap();
        assert!(parsed.emoji.is_none());
    }

    #[test]
    fn llm_reaction_decision_accepts_missing_field() {
        let parsed: LlmReactionDecision = serde_json::from_str(r#"{}"#).unwrap();
        assert!(parsed.emoji.is_none());
    }

    #[test]
    fn llm_reaction_decision_non_null_emoji() {
        let parsed: LlmReactionDecision = serde_json::from_str(r#"{"emoji":"test"}"#).unwrap();
        assert_eq!(parsed.emoji.as_deref(), Some("test"));
    }

    #[test]
    fn build_player_message_reaction_prompt_contains_palette_and_npc_name() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let (system, user) = build_player_message_reaction_prompt(&npc, "The landlord is coming.");

        assert!(
            system.contains("Available palette"),
            "system missing palette"
        );
        assert!(
            system.contains("null: no visible reaction"),
            "system missing null option"
        );
        assert!(
            system.contains("Legacy keyword cues"),
            "system missing keyword cues"
        );
        assert!(user.contains("Padraig Darcy"), "user missing NPC name");
        assert!(
            user.contains("Player message"),
            "user missing player message label"
        );
        assert!(user.contains("landlord"), "user missing player text");
    }

    #[test]
    fn palette_has_expected_size() {
        assert_eq!(REACTION_PALETTE.len(), 12);
    }

    // ── Arrival reaction tests ───────────────────────────────────────────────

    #[test]
    fn test_publican_at_pub_reacts_with_low_roll() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let loc = test_location(2, true); // indoor pub, same as workplace
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        // Low rolls: will pass threshold and pick introduction
        let dice = fixed_n(&[0.0, 0.1]);

        let reactions = generate_arrival_reactions(
            &[&npc],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Introduction);
        assert!(reactions[0].introduces);
        assert!(reactions[0].use_llm);
        assert!(reactions[0].canned_text.contains("Padraig"));
    }

    #[test]
    fn test_introduced_publican_at_pub_gives_welcome() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let loc = test_location(2, true);
        let mut introduced = HashSet::new();
        introduced.insert(NpcId(1));
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        let dice = fixed_n(&[0.0, 0.5]);

        let reactions = generate_arrival_reactions(
            &[&npc],
            &introduced,
            &loc,
            TimeOfDay::Afternoon,
            "overcast",
            &templates,
            &config,
            &dice,
        );

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Welcome);
        assert!(!reactions[0].introduces);
        assert!(reactions[0].use_llm);
    }

    #[test]
    fn test_high_roll_no_reaction() {
        let npc = test_npc(1, "Siobhan", "Farmer", None);
        let loc = test_location(1, false); // outdoor, not workplace
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        // Roll 0.99 will be above threshold (~0.55 base for outdoor non-workplace)
        let dice = fixed_n(&[0.99, 0.5]);

        let reactions = generate_arrival_reactions(
            &[&npc],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert!(reactions.is_empty());
    }

    #[test]
    fn test_priest_gives_blessing_greeting() {
        let npc = test_npc(
            3,
            "Fr. Declan Tierney",
            "Parish Priest",
            Some(LocationId(3)),
        );
        let loc = test_location(1, false); // not at workplace
        let mut introduced = HashSet::new();
        introduced.insert(NpcId(3));
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        let dice = fixed_n(&[0.0, 0.3]); // low roll passes, type_roll < 0.5 for priest greeting

        let reactions = generate_arrival_reactions(
            &[&npc],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Greeting);
        // Priest greetings should contain blessing/religious language
        let text = &reactions[0].canned_text;
        assert!(
            text.contains("God")
                || text.contains("Dia")
                || text.contains("peace")
                || text.contains("bless")
                || text.contains("Lord"),
            "Priest greeting should have religious content, got: {}",
            text
        );
    }

    #[test]
    fn test_night_reduces_reaction_chance() {
        let npc = test_npc(1, "Siobhan", "Farmer", None);
        let loc = test_location(1, false);
        let _introduced: HashSet<NpcId> = HashSet::new();
        let config = ReactionConfig::default();

        // Threshold for outdoor, non-workplace, night: 0.55 - 0.15 = 0.40
        let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Night, &config);
        assert!((threshold - 0.40).abs() < 0.01);

        // Threshold for outdoor, non-workplace, morning: 0.55
        let threshold_morning = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
        assert!((threshold_morning - 0.55).abs() < 0.01);
    }

    #[test]
    fn test_workplace_bonus() {
        let npc = test_npc(1, "Padraig", "Publican", Some(LocationId(2)));
        let loc = test_location(2, true); // indoor workplace
        let config = ReactionConfig::default();

        // base 0.55 + workplace 0.35 + indoor 0.10 = 1.00 (clamped)
        let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
        assert!((threshold - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_negative_mood_penalty() {
        let mut npc = test_npc(1, "Siobhan", "Farmer", None);
        npc.mood = "angry and frustrated".to_string();
        let loc = test_location(1, false);
        let config = ReactionConfig::default();

        // base 0.55 - negative_mood 0.20 = 0.35
        let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
        assert!((threshold - 0.35).abs() < 0.01);
    }

    #[test]
    fn test_empty_npc_list() {
        let loc = test_location(1, false);
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        let dice: Vec<DiceRoll> = vec![];

        let reactions = generate_arrival_reactions(
            &[],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert!(reactions.is_empty());
    }

    #[test]
    fn test_multiple_npcs() {
        let npc1 = test_npc(1, "Padraig", "Publican", Some(LocationId(2)));
        let npc2 = test_npc(2, "Siobhan", "Farmer", None);
        let loc = test_location(2, true);
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        // NPC1: low rolls → reacts. NPC2: high chance_roll → silent
        let dice = fixed_n(&[0.0, 0.1, 0.99, 0.5]);

        let reactions = generate_arrival_reactions(
            &[&npc1, &npc2],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].npc_id, NpcId(1));
    }

    #[test]
    fn test_unintroduced_npc_gesture() {
        let npc = test_npc(1, "Siobhan Murphy", "Farmer", None);
        let loc = test_location(1, false);
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        // type_roll 0.5 >= 0.25 → gesture for unintroduced non-workplace NPC
        let dice = fixed_n(&[0.0, 0.5]);

        let reactions = generate_arrival_reactions(
            &[&npc],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Gesture);
        assert!(!reactions[0].introduces);
        assert!(!reactions[0].use_llm);
        // Gesture for unintroduced NPC uses brief_description
        assert!(reactions[0].npc_display_name.contains("farmer"));
    }

    #[test]
    fn test_unintroduced_npc_casual_introduction() {
        let npc = test_npc(1, "Siobhan Murphy", "Farmer", None);
        let loc = test_location(1, false);
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        // type_roll 0.1 < 0.25 → casual introduction
        let dice = fixed_n(&[0.0, 0.1]);

        let reactions = generate_arrival_reactions(
            &[&npc],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Introduction);
        assert!(reactions[0].introduces);
        assert!(reactions[0].use_llm);
        // Introduction uses real name
        assert_eq!(reactions[0].npc_display_name, "Siobhan Murphy");
    }

    #[test]
    fn test_max_reactions_cap() {
        // Four introduced non-workplace NPCs all roll to react.
        // With max_reactions = 2 only two should come through.
        let npc1 = test_npc(1, "Aoife", "Farmer", None);
        let npc2 = test_npc(2, "Brigid", "Farmer", None);
        let npc3 = test_npc(3, "Cormac", "Farmer", None);
        let npc4 = test_npc(4, "Donal", "Farmer", None);
        let loc = test_location(1, false);
        let mut introduced = HashSet::new();
        for id in [1u32, 2, 3, 4] {
            introduced.insert(NpcId(id));
        }
        let templates = ReactionTemplates::default();
        let config = ReactionConfig {
            max_reactions: 2,
            ..Default::default()
        };
        // All chance rolls = 0.0 (pass), type rolls = 0.3 → Greeting for each
        let dice = fixed_n(&[0.0, 0.3, 0.0, 0.3, 0.0, 0.3, 0.0, 0.3]);

        let reactions = generate_arrival_reactions(
            &[&npc1, &npc2, &npc3, &npc4],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert_eq!(reactions.len(), 2);
    }

    #[test]
    fn test_max_reactions_priority_keeps_introduction() {
        // NPC1 is introduced → Gesture (type_roll 0.6 ≥ 0.5).
        // NPC2 is not introduced → Introduction (type_roll 0.1 < 0.25).
        // With cap = 1 the Introduction beats the Gesture.
        let npc1 = test_npc(1, "Eoin", "Farmer", None);
        let npc2 = test_npc(2, "Fiona Murphy", "Farmer", None);
        let loc = test_location(1, false);
        let mut introduced = HashSet::new();
        introduced.insert(NpcId(1)); // npc1 introduced; npc2 is not
        let templates = ReactionTemplates::default();
        let config = ReactionConfig {
            max_reactions: 1,
            ..Default::default()
        };
        // npc1: chance 0.0 (pass), type 0.6 → Gesture
        // npc2: chance 0.0 (pass), type 0.1 → Introduction
        let dice = fixed_n(&[0.0, 0.6, 0.0, 0.1]);

        let reactions = generate_arrival_reactions(
            &[&npc1, &npc2],
            &introduced,
            &loc,
            TimeOfDay::Morning,
            "clear",
            &templates,
            &config,
            &dice,
        );

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Introduction);
        assert_eq!(reactions[0].npc_id, NpcId(2));
    }

    #[test]
    fn test_placeholder_substitution() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let result = substitute_placeholders(
            "\"Welcome, says {name}. It's {time}, {weather}.\"",
            &npc,
            "Padraig Darcy",
            TimeOfDay::Morning,
            "overcast",
        );
        assert_eq!(
            result,
            "\"Welcome, says Padraig Darcy. It's morning, overcast.\""
        );
    }

    #[test]
    fn test_reaction_templates_default_has_content() {
        let t = ReactionTemplates::default();
        assert!(t.gestures.len() >= 20);
        assert!(t.greetings.morning.len() >= 10);
        assert!(t.greetings.afternoon.len() >= 10);
        assert!(t.greetings.evening.len() >= 10);
        assert!(t.greetings.any.len() >= 10);
        assert!(t.welcomes.publican.len() >= 10);
        assert!(t.welcomes.shopkeeper.len() >= 10);
        assert!(t.welcomes.priest.len() >= 8);
        assert!(t.introductions.workplace.len() >= 8);
        assert!(t.introductions.casual.len() >= 8);
        assert!(t.occupation_greetings.priest.len() >= 8);
    }

    #[test]
    fn test_is_negative_mood() {
        assert!(is_negative_mood("angry"));
        assert!(is_negative_mood("frustrated and bitter"));
        assert!(is_negative_mood("anxious"));
        assert!(!is_negative_mood("content"));
        assert!(!is_negative_mood("cheerful"));
        assert!(!is_negative_mood("contemplative"));
    }

    #[test]
    fn test_high_emotional_intelligence_bonus() {
        let mut npc = test_npc(1, "Padraig", "Publican", None);
        npc.intelligence.emotional = 5;
        let loc = test_location(1, false);
        let config = ReactionConfig::default();

        // base 0.55 + empathy 0.05 = 0.60
        let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
        assert!((threshold - 0.60).abs() < 0.01);
    }

    #[test]
    fn test_build_reaction_prompt_not_introduced() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let (system, context) = build_reaction_prompt(
            &npc,
            "Darcy's Pub",
            TimeOfDay::Morning,
            "overcast",
            false,
            true,
        );
        assert!(system.contains("Padraig Darcy"));
        assert!(system.contains("Publican"));
        assert!(context.contains("Darcy's Pub"));
        assert!(context.contains("morning"));
        assert!(context.contains("Introduce yourself"));
    }

    #[test]
    fn test_build_reaction_prompt_introduced() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let (_, context) = build_reaction_prompt(
            &npc,
            "Darcy's Pub",
            TimeOfDay::Afternoon,
            "clear",
            true,
            true,
        );
        assert!(context.contains("You know this person"));
    }

    // ── LLM player-message reaction tests ───────────────────────────────────

    #[test]
    fn build_player_message_reaction_prompt_contains_palette_and_npc() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let (system, user) =
            build_player_message_reaction_prompt(&npc, "Your landlord's agent is at the door.");

        // System prompt must enumerate the palette and keyword cues.
        assert!(system.contains("Available palette"));
        assert!(system.contains("null: no visible reaction"));
        assert!(system.contains("Legacy keyword cues"));
        assert!(system.contains("landlord"));
        // The STRICT JSON instruction must be present.
        assert!(system.contains("STRICT JSON"));

        // User prompt must identify the NPC and include the player message.
        assert!(user.contains("Padraig Darcy"));
        assert!(user.contains("Publican"));
        assert!(user.contains("Player message"));
        assert!(user.contains("landlord's agent"));
    }

    #[test]
    fn build_player_message_reaction_prompt_truncates_long_personality() {
        let mut npc = test_npc(2, "Brigid", "Healer", None);
        npc.personality = "A".repeat(500);
        let (_system, user) = build_player_message_reaction_prompt(&npc, "Hello");
        // Personality snippet is capped at 300 chars.
        let after_personality = user
            .split("- Personality: ")
            .nth(1)
            .unwrap_or("")
            .split('\n')
            .next()
            .unwrap_or("");
        assert!(after_personality.chars().count() <= 300);
    }

    /// End-to-end: simulator returns a valid palette emoji → reaction emitted.
    #[tokio::test]
    async fn infer_player_message_reaction_simulator_returns_palette_emoji() {
        use parish_inference::AnyClient;

        let client = AnyClient::simulator();
        let npc = test_npc(3, "Seán Brennan", "Farmer", None);

        // The simulator always succeeds; we just confirm the function either
        // returns None or a string that lives in REACTION_PALETTE.
        let result = infer_player_message_reaction(
            &client,
            "any-model",
            &npc,
            "The landlord is coming to collect rent.",
            std::time::Duration::from_secs(5),
        )
        .await;

        if let Some(ref emoji) = result {
            assert!(
                reaction_description(emoji).is_some(),
                "returned emoji {emoji:?} not in palette"
            );
        }
        // None is also valid (probabilistic gate or null from model).
    }

    /// Fallback: generate_rule_reaction still fires for keyword inputs when
    /// called directly (the deterministic variant in tests always returns).
    #[test]
    fn generate_rule_reaction_deterministic_matches_known_keywords() {
        assert!(generate_rule_reaction_deterministic("rent and landlord").is_some());
        assert!(generate_rule_reaction_deterministic("strange ghost").is_some());
        assert!(generate_rule_reaction_deterministic("perfectly normal day").is_none());
    }

    // ── NPC→player-message reaction log context string (fix: gemini high) ────

    /// `add_player_message_reaction` stores an entry and `npc_context_string`
    /// renders it with the correct "You [described] in response to the player
    /// saying [message]" format — not the player→NPC "The player [described]
    /// when you said [msg]" format.
    #[test]
    fn npc_reaction_log_context_string_correct_format() {
        let mut log = ReactionLog::default();
        log.add_player_message_reaction(
            "😠",
            "The rent is too high",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );

        let ctx = log.npc_context_string(5);
        // Must name the NPC's action, not the player's action.
        assert!(
            ctx.contains("You looked angry"),
            "npc_context_string should say 'You looked angry', got: {ctx}"
        );
        assert!(
            ctx.contains("the player saying"),
            "npc_context_string should contain 'the player saying', got: {ctx}"
        );
        assert!(
            ctx.contains("The rent is too high"),
            "npc_context_string should contain the player message, got: {ctx}"
        );
        // Must NOT use the player→NPC phrasing.
        assert!(
            !ctx.contains("The player"),
            "npc_context_string must not use 'The player' phrasing, got: {ctx}"
        );
    }

    /// `context_string` (player→NPC path) is unaffected — it still uses the
    /// "The player [described] when you said [msg]" format.
    #[test]
    fn player_reaction_log_context_string_unchanged() {
        let mut log = ReactionLog::default();
        log.add(
            "😊",
            "Good morning to you",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );

        let ctx = log.context_string(5);
        assert!(
            ctx.contains("The player smiled warmly"),
            "context_string should say 'The player smiled warmly', got: {ctx}"
        );
        assert!(
            ctx.contains("when you said"),
            "context_string should contain 'when you said', got: {ctx}"
        );
    }

    /// `add_player_message_reaction` ignores unknown emoji (same guard as `add`).
    #[test]
    fn npc_reaction_log_ignores_unknown_emoji() {
        let mut log = ReactionLog::default();
        log.add_player_message_reaction(
            "💀",
            "some message",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        assert!(log.is_empty());
    }

    /// `npc_context_string` returns empty string when there are no entries.
    #[test]
    fn npc_reaction_log_context_string_empty() {
        let log = ReactionLog::default();
        assert_eq!(log.npc_context_string(5), "");
    }
}
