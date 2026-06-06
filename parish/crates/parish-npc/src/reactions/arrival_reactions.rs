//! NPC arrival reaction system — how NPCs greet and react when the player enters a location.
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

use std::collections::HashSet;
use std::time::Duration;

use crate::{LanguageSettings, Npc, NpcId};
use parish_config::ReactionConfig;
use parish_inference::{AnyClient, GenerateParams};
use parish_types::dice::DiceRoll;
use parish_world::graph::LocationData;
use parish_world::time::TimeOfDay;

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
#[derive(Debug, Clone, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Deserialize)]
pub struct IntroductionTemplates {
    /// Introductions at their workplace.
    #[serde(default)]
    pub workplace: Vec<String>,
    /// Casual introductions elsewhere.
    #[serde(default)]
    pub casual: Vec<String>,
}

/// Occupation-specific greetings used outside the workplace.
#[derive(Debug, Clone, serde::Deserialize)]
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

/// Scene context passed to [`generate_arrival_reactions`].
///
/// Bundles the location, time, weather, template bank, and reaction config
/// so the function signature stays manageable as game state grows.
pub struct ArrivalContext<'a> {
    /// The location the player has just entered.
    pub location: &'a LocationData,
    /// Current time of day.
    pub time_of_day: TimeOfDay,
    /// Current weather description (e.g. `"clear"`, `"rainy"`).
    pub weather: &'a str,
    /// Mod-provided (or default) text templates.
    pub templates: &'a ReactionTemplates,
    /// Reaction probability config.
    pub config: &'a ReactionConfig,
}

fn is_priest_occupation(occupation: &str) -> bool {
    occupation.contains("priest") || occupation.contains("clergy") || occupation.contains("curate")
}

/// Selects the reaction kind for an NPC based on context and a type roll.
///
/// Returns `(kind, introduces, use_llm)`.
fn select_reaction_kind(
    at_workplace: bool,
    is_introduced: bool,
    is_priest: bool,
    type_roll: &DiceRoll,
) -> (ReactionKind, bool, bool) {
    if at_workplace && !is_introduced {
        (ReactionKind::Introduction, true, true)
    } else if at_workplace && is_introduced {
        (ReactionKind::Welcome, false, true)
    } else if !is_introduced && type_roll.check(0.25) {
        (ReactionKind::Introduction, true, true)
    } else if !is_introduced {
        (ReactionKind::Gesture, false, false)
    } else if is_priest {
        (ReactionKind::Greeting, false, type_roll.check(0.5))
    } else if type_roll.check(0.5) {
        (ReactionKind::Greeting, false, false)
    } else {
        (ReactionKind::Gesture, false, false)
    }
}

/// Caps reaction count, prioritising socially significant kinds.
fn cap_reactions_by_priority(reactions: &mut Vec<NpcReaction>, max_reactions: usize) {
    if max_reactions == 0 || reactions.len() <= max_reactions {
        return;
    }
    reactions.sort_by_key(|r| match r.kind {
        ReactionKind::Introduction => 0u8,
        ReactionKind::Welcome => 1,
        ReactionKind::Greeting => 2,
        ReactionKind::Gesture => 3,
    });
    reactions.truncate(max_reactions);
}

/// Generates arrival reactions for NPCs at the player's current location.
///
/// Each NPC needs **two** dice rolls in `dice` (one for reaction chance,
/// one for type/template selection). So `dice.len()` must be `≥ npcs.len() * 2`.
///
/// Returns only NPCs that actually react — silent NPCs are omitted.
pub fn generate_arrival_reactions(
    npcs: &[&Npc],
    introduced: &HashSet<NpcId>,
    ctx: &ArrivalContext<'_>,
    dice: &[DiceRoll],
) -> Vec<NpcReaction> {
    let location = ctx.location;
    let time_of_day = ctx.time_of_day;
    let weather = ctx.weather;
    let templates = ctx.templates;
    let config = ctx.config;
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
            continue;
        }

        let is_introduced = introduced.contains(&npc.id);
        let at_workplace = is_at_workplace(npc, location);
        let occupation = npc.occupation.to_lowercase();
        let is_priest = is_priest_occupation(&occupation);

        let (kind, introduces, use_llm) =
            select_reaction_kind(at_workplace, is_introduced, is_priest, type_roll);

        let display_name = if is_introduced || introduces {
            npc.name.clone()
        } else {
            npc.brief_description.clone()
        };

        let npc_ctx = NpcArrivalCtx {
            npc,
            display_name: &display_name,
            at_workplace,
            occupation: &occupation,
        };
        let canned_text =
            pick_canned_text(&kind, &npc_ctx, time_of_day, weather, templates, type_roll);

        reactions.push(NpcReaction {
            npc_id: npc.id,
            npc_display_name: display_name,
            kind,
            canned_text,
            introduces,
            use_llm,
        });
    }

    cap_reactions_by_priority(&mut reactions, config.max_reactions);
    reactions
}

/// Per-NPC context used inside [`pick_canned_text`].
struct NpcArrivalCtx<'a> {
    npc: &'a Npc,
    display_name: &'a str,
    at_workplace: bool,
    occupation: &'a str,
}

/// Picks a canned text template and substitutes placeholders.
fn pick_canned_text(
    kind: &ReactionKind,
    npc_ctx: &NpcArrivalCtx<'_>,
    time_of_day: TimeOfDay,
    weather: &str,
    templates: &ReactionTemplates,
    roll: &DiceRoll,
) -> String {
    let occupation = npc_ctx.occupation;
    let raw = match kind {
        ReactionKind::Gesture => roll.pick(&templates.gestures).clone(),
        ReactionKind::Greeting => {
            if is_priest_occupation(occupation) {
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
            let pool = if npc_ctx.at_workplace {
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

    substitute_placeholders(
        &raw,
        npc_ctx.npc,
        npc_ctx.display_name,
        time_of_day,
        weather,
    )
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
    language: &LanguageSettings,
) -> (String, String) {
    use crate::language_directive;

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

    let mut system = format!(
        "You are {name}, a {age}-year-old {occupation} in rural Ireland, 1820.\n\
         {personality}\n\
         Current mood: {mood}\n\n\
         Write a single brief greeting or reaction (1-2 sentences max). \
         Dialogue only, no narration or action descriptions. \
         Do not use any modern language.\n\n\
         FORBIDDEN phrases — never say any of these, they break the 1820 \
         rural-Irish voice and read as an AI assistant: \
         \"How may I assist\", \"How can I help\", \"Is there anything I can \
         do for you\", \"How may I be of service\", \"What brings you here \
         today\". \
         Greet the way an 1820 villager would: \"Aye, {name_first}\", \
         \"Bedad,\", \"Faith,\", \"Begob,\", \"God save ye,\", or by simply \
         calling out their name. Cap the reply at ~20 words.",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        personality = personality_snippet,
        mood = npc.mood,
        name_first = npc.name.split_whitespace().next().unwrap_or(&npc.name),
    );

    system.push_str("\n\n");
    system.push_str(&language_directive(language));

    let context = format!(
        "A newcomer has just arrived at {location}. It is {time}, {weather}.\n{intro}",
        location = location_name,
        time = time_str,
        weather = weather,
        intro = intro_context,
    );

    (system, context)
}

/// Scene context for [`resolve_llm_greeting`].
///
/// Bundles the location, time, weather, and introduction/workplace flags
/// so the async function signature stays below clippy's argument threshold.
pub struct LlmGreetingParams<'a> {
    /// Name of the location the player entered.
    pub location_name: &'a str,
    /// Current time of day.
    pub time_of_day: TimeOfDay,
    /// Current weather description.
    pub weather: &'a str,
    /// Whether the player has been introduced to this NPC.
    pub is_introduced: bool,
    /// Whether the NPC is currently at their workplace.
    pub at_workplace: bool,
    /// LLM inference client.
    pub client: &'a AnyClient,
    /// Model identifier to pass to the client.
    pub model: &'a str,
    /// Timeout in seconds for the LLM call.
    pub timeout_secs: u64,
}

/// Attempts an LLM-generated greeting with a short timeout.
///
/// Returns the LLM text if it responds in time, or the canned fallback
/// text from the reaction if the call times out or errors.
pub async fn resolve_llm_greeting(
    reaction: &NpcReaction,
    npc: &Npc,
    params: &LlmGreetingParams<'_>,
) -> String {
    let lang = LanguageSettings::english_only();
    let (system, context) = build_reaction_prompt(
        npc,
        params.location_name,
        params.time_of_day,
        params.weather,
        params.is_introduced,
        params.at_workplace,
        &lang,
    );
    let client = params.client;
    let model = params.model;
    let timeout_secs = params.timeout_secs;

    // Streaming variant: discards chunks but uses the streaming code path
    // so the underlying provider streams tokens (TTFT visibility) and a
    // future preemption pathway can cancel mid-flight (#9). Reaction is a
    // short greeting (~14-20 tokens), so cancellation is rarely needed —
    // but keeping every NPC turn on the same code path avoids divergence.
    let timeout = Duration::from_secs(timeout_secs);
    let (sink_tx, mut sink_rx) =
        tokio::sync::mpsc::channel::<String>(parish_inference::TOKEN_CHANNEL_CAPACITY);
    tokio::spawn(async move { while sink_rx.recv().await.is_some() {} });
    let result = tokio::time::timeout(
        timeout,
        client.generate_stream(
            model,
            &context,
            Some(&system),
            sink_tx,
            GenerateParams {
                max_tokens: Some(100),
                temperature: None,
                frequency_penalty: None,
            },
        ),
    )
    .await;

    match result {
        Ok(Ok(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                reaction.canned_text.clone()
            } else {
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
    use crate::types::Intelligence;
    use parish_types::LocationId;
    use parish_types::dice::{DiceRoll, fixed_n};
    use parish_world::graph::GeoKind;
    use std::collections::HashSet;

    fn test_npc(id: u32, name: &str, occupation: &str, workplace: Option<LocationId>) -> Npc {
        let mut npc = crate::test_helpers::make_test_npc(id, workplace.unwrap_or(LocationId(1)).0);
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

    #[test]
    fn test_publican_at_pub_reacts_with_low_roll() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let loc = test_location(2, true);
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        let dice = fixed_n(&[0.0, 0.1]);

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

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

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Afternoon,
            weather: "overcast",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Welcome);
        assert!(!reactions[0].introduces);
        assert!(reactions[0].use_llm);
    }

    #[test]
    fn test_high_roll_no_reaction() {
        let npc = test_npc(1, "Siobhan", "Farmer", None);
        let loc = test_location(1, false);
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        let dice = fixed_n(&[0.99, 0.5]);

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

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
        let loc = test_location(1, false);
        let mut introduced = HashSet::new();
        introduced.insert(NpcId(3));
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        let dice = fixed_n(&[0.0, 0.3]);

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Greeting);
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

        let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Night, &config);
        assert!((threshold - 0.40).abs() < 0.01);

        let threshold_morning = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
        assert!((threshold_morning - 0.55).abs() < 0.01);
    }

    #[test]
    fn test_workplace_bonus() {
        let npc = test_npc(1, "Padraig", "Publican", Some(LocationId(2)));
        let loc = test_location(2, true);
        let config = ReactionConfig::default();

        let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
        assert!((threshold - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_negative_mood_penalty() {
        let mut npc = test_npc(1, "Siobhan", "Farmer", None);
        npc.mood = "angry and frustrated".to_string();
        let loc = test_location(1, false);
        let config = ReactionConfig::default();

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

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[], &introduced, &ctx, &dice);

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
        let dice = fixed_n(&[0.0, 0.1, 0.99, 0.5]);

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[&npc1, &npc2], &introduced, &ctx, &dice);

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
        let dice = fixed_n(&[0.0, 0.5]);

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Gesture);
        assert!(!reactions[0].introduces);
        assert!(!reactions[0].use_llm);
        assert!(reactions[0].npc_display_name.contains("farmer"));
    }

    #[test]
    fn test_unintroduced_npc_casual_introduction() {
        let npc = test_npc(1, "Siobhan Murphy", "Farmer", None);
        let loc = test_location(1, false);
        let introduced: HashSet<NpcId> = HashSet::new();
        let templates = ReactionTemplates::default();
        let config = ReactionConfig::default();
        let dice = fixed_n(&[0.0, 0.1]);

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[&npc], &introduced, &ctx, &dice);

        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].kind, ReactionKind::Introduction);
        assert!(reactions[0].introduces);
        assert!(reactions[0].use_llm);
        assert_eq!(reactions[0].npc_display_name, "Siobhan Murphy");
    }

    #[test]
    fn test_max_reactions_cap() {
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
        let dice = fixed_n(&[0.0, 0.3, 0.0, 0.3, 0.0, 0.3, 0.0, 0.3]);

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions =
            generate_arrival_reactions(&[&npc1, &npc2, &npc3, &npc4], &introduced, &ctx, &dice);

        assert_eq!(reactions.len(), 2);
    }

    #[test]
    fn test_max_reactions_priority_keeps_introduction() {
        let npc1 = test_npc(1, "Eoin", "Farmer", None);
        let npc2 = test_npc(2, "Fiona Murphy", "Farmer", None);
        let loc = test_location(1, false);
        let mut introduced = HashSet::new();
        introduced.insert(NpcId(1));
        let templates = ReactionTemplates::default();
        let config = ReactionConfig {
            max_reactions: 1,
            ..Default::default()
        };
        let dice = fixed_n(&[0.0, 0.6, 0.0, 0.1]);

        let ctx = ArrivalContext {
            location: &loc,
            time_of_day: TimeOfDay::Morning,
            weather: "clear",
            templates: &templates,
            config: &config,
        };
        let reactions = generate_arrival_reactions(&[&npc1, &npc2], &introduced, &ctx, &dice);

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

        let threshold = reaction_threshold(&npc, &loc, TimeOfDay::Morning, &config);
        assert!((threshold - 0.60).abs() < 0.01);
    }

    #[test]
    fn test_build_reaction_prompt_not_introduced() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let lang = LanguageSettings::english_only();
        let (system, context) = build_reaction_prompt(
            &npc,
            "Darcy's Pub",
            TimeOfDay::Morning,
            "overcast",
            false,
            true,
            &lang,
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
        let lang = LanguageSettings::english_only();
        let (_, context) = build_reaction_prompt(
            &npc,
            "Darcy's Pub",
            TimeOfDay::Afternoon,
            "clear",
            true,
            true,
            &lang,
        );
        assert!(context.contains("You know this person"));
    }
}
