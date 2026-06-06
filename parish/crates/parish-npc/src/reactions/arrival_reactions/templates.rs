//! Template structs, defaults, and placeholder substitution for NPC arrival reactions.
//!
//! All mod-overridable text templates live here, along with the `Default`
//! implementations and default-content functions that populate them.

use crate::Npc;
use parish_world::time::TimeOfDay;

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

// ── Default template banks ──────────────────────────────────────────────────

pub(crate) fn default_gestures() -> Vec<String> {
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

pub(crate) fn default_greetings() -> GreetingsByTime {
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

pub(crate) fn default_welcomes() -> WelcomesByOccupation {
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

pub(crate) fn default_introductions() -> IntroductionTemplates {
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

pub(crate) fn default_occupation_greetings() -> OccupationGreetings {
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

// ── Placeholder substitution ────────────────────────────────────────────────

/// Substitutes `{name}`, `{first_name}`, `{last_name}`, `{occupation}`,
/// `{time}`, `{weather}` placeholders in a template string.
pub(crate) fn substitute_placeholders(
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
