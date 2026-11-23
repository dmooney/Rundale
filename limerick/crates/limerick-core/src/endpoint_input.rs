//! Structured Endpoint inputs built by the engine (ADR-025 §5).
//!
//! When a model call has a published Endpoint (see
//! [`crate::game_mod::endpoints`]), the engine attaches the Endpoint reference
//! and the role's structured input to the [`InferenceCall`]. An Endpoint host
//! sends that input, and the Endpoint's published instructions do the
//! prompting; an in-process host ignores it and sends the call's rendered
//! prompt as before.
//!
//! A host builds the request's `input` with [`EndpointCall::request_input`],
//! which keeps the key order the Endpoint expects. The result matches the
//! role's `inputSchema` in the mod's definition file.
//!
//! The dialogue input is the frozen NPC dialogue prompt's request
//! (`promptfoo/prompt-lab/prompts/frozen.yaml`, issue 2299): the Endpoint's
//! model reads the whole input as "Endpoint input values:" followed by its
//! JSON, so the input is exactly the frozen five sections, in the frozen key
//! order, and carries no invocation envelope. Its correlation identities
//! travel in the request headers. The intent input adds the invocation
//! envelope ([`EndpointCall::invocation`]).
//!
//! [`InferenceCall`]: crate::turn_inference::InferenceCall

use serde::Serialize;
use serde_json::{Value, json};

use crate::game_mod::{DialogueContext, EndpointCatalog, EndpointRef, EndpointRole};
use crate::npc::manager::NpcManager;
use crate::npc::{Npc, NpcId};
use crate::turn::{ExecutionAttemptId, LogicalRequestId, StateRevision};
use crate::world::WorldState;
use chrono::Datelike;
use limerick_types::{ConversationLog, DayType, LocationId, Season};

/// Most entries in any one list of the dialogue input: places, people,
/// relationships, knowledge, the day's schedule, festivals, common
/// knowledge and backstory.
pub const MAX_LIST_ENTRIES: usize = 32;
/// Most recent witnessed exchanges sent with one dialogue call. Each is up
/// to two lines of `justSaid`.
pub const MAX_RECENT_CONVERSATION: usize = 8;
/// Longest player input, in characters, the definitions accept.
pub const MAX_PLAYER_INPUT_CHARS: usize = 4096;
pub use crate::intent_decision::{INTENT_ROLE, IntentEndpointInput, IntentOptions};
/// Invocation contract version the v1 definitions accept.
pub const INVOCATION_CONTRACT_VERSION: (u32, u32) = (1, 0);

/// How the player appears in `justSaid` and is described in `theStranger`.
pub const STRANGER: &str = "the stranger";
/// `theStranger` for a character who knows nothing of the player.
pub const NEVER_MET: &str = "A stranger you have never seen before. You know nothing of them but what they have said to you here.";
/// The start of `theStranger` for a character who knows something of the
/// player from before this line; what they know follows.
pub const MET_BEFORE: &str = "Someone you have met before. What you know of them: ";
/// `notice` when the player's line holds something foreign to the
/// character's time (design §4.4, decision 16).
pub const ANACHRONISM_NOTICE: &str = "Something in what the stranger just said is not a thing you have ever heard of. Work out what puzzles you and react as you would: puzzlement, a guess, a mishearing, or a question.";
/// `alsoHere` when nobody else is in the room.
pub const NOBODY_ELSE: &str = "nobody else";
/// `feastToday` on a day with no festival.
pub const NO_FEAST: &str = "none";

/// The identities a host adds to every intent invocation. They come from
/// the pending request, not from game state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationEnvelope {
    /// Opaque host session identity.
    pub session_id: String,
    /// The logical request; retries keep it.
    pub request_id: LogicalRequestId,
    /// The execution attempt.
    pub attempt_id: ExecutionAttemptId,
    /// The revision the attempt started from.
    pub base_revision: StateRevision,
}

/// The Endpoint a call executes and the input it sends.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EndpointCall {
    /// Published Endpoint identity (role, slug, version).
    pub reference: EndpointRef,
    /// Role-specific structured input.
    pub input: EndpointInput,
}

impl EndpointCall {
    /// The Endpoint request's `input` object as a value. The dialogue input
    /// is sent as built; the intent input gains the envelope's identities.
    ///
    /// `serde_json::Value` sorts object keys, so use [`Self::request_input`]
    /// for the bytes a host sends.
    pub fn invocation(&self, envelope: &InvocationEnvelope) -> Value {
        match &self.input {
            EndpointInput::Dialogue(input) => serde_json::to_value(input).unwrap_or(Value::Null),
            EndpointInput::Intent(input) => {
                let (major, minor) = INVOCATION_CONTRACT_VERSION;
                let mut body = json!({
                    "contractVersion": {"major": major, "minor": minor},
                    "sessionID": envelope.session_id,
                    "logicalRequestID": envelope.request_id.as_str(),
                    "attemptID": envelope.attempt_id.as_str(),
                    "baseRevision": {"rawValue": envelope.base_revision.0},
                    "idempotencyKey": format!(
                        "{}:{}",
                        envelope.request_id.as_str(),
                        envelope.attempt_id.as_str()
                    ),
                });
                if let (Value::Object(body), Ok(Value::Object(input))) =
                    (&mut body, serde_json::to_value(input))
                {
                    body.extend(input);
                }
                body
            }
        }
    }

    /// The Endpoint request's `input` object as JSON text, in the key order
    /// the Endpoint expects. The dialogue Endpoint's model reads this text
    /// as written, so its keys keep the frozen prompt's order (design §6.4,
    /// owner decision 2026-11-01); a host must send these bytes unchanged.
    pub fn request_input(&self, envelope: &InvocationEnvelope) -> String {
        match &self.input {
            EndpointInput::Dialogue(input) => serde_json::to_string(input).unwrap_or_default(),
            EndpointInput::Intent(_) => self.invocation(envelope).to_string(),
        }
    }
}

/// A role's structured input. Serializes as the input object itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum EndpointInput {
    /// Input for the dialogue role (boxed: it carries the character, the
    /// village and the common knowledge).
    Dialogue(Box<DialogueEndpointInput>),
    /// Input for the intent role.
    Intent(Box<IntentEndpointInput>),
}

/// One of the speaker's relationships (`aboutYou.relationships`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Relationship {
    /// The other person's name.
    pub who: String,
    /// What the relationship is, in the speaker's words.
    pub kind: String,
    /// How the speaker feels about them, with the relationship's strength.
    pub feeling: String,
}

/// The speaker (`aboutYou`): their own character, and nobody else's.
/// Fields serialize in the frozen prompt's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AboutYou {
    /// Full name.
    pub name: String,
    /// Age in years.
    pub age: u8,
    /// Pronouns, e.g. `she/her`.
    pub pronouns: String,
    /// How they look (`brief_description`).
    pub appearance: String,
    /// Occupation.
    pub occupation: String,
    /// Personality.
    pub personality: String,
    /// The name of their home, when they have one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    /// The name of their workplace, when they have one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workplace: Option<String>,
    /// Their day this season: `HH:00-HH:59 activity (place)`.
    pub day: Vec<String>,
    /// Their relationships, by the other person's id.
    pub relationships: Vec<Relationship>,
    /// What they know that others don't (`knowledge`).
    pub knows: Vec<String>,
    /// Their mood.
    pub mood: String,
    /// A few lines of their past, when the mod authors them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub backstory: Vec<String>,
}

/// One topic of common knowledge (`commonKnowledge`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnowledgeTopic {
    /// Heading.
    pub topic: String,
    /// What everyone of the time and place knows of it.
    pub text: String,
}

/// A place in the speaker's village.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VillagePlace {
    /// Its name.
    pub name: String,
    /// How it looks, when the mod authors it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub looks: Option<String>,
    /// Its old story (`mythological_significance`), or null.
    pub story: Option<String>,
    /// The ways out of it (`path_description` of each connection).
    pub ways: Vec<String>,
}

/// A person of the village, as the village sees them: nothing private.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VillagePerson {
    /// Full name.
    pub name: String,
    /// How they look (`brief_description`).
    pub looks: String,
    /// Their work, as the village would put it.
    pub work: String,
    /// The name of their home, when they have one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    /// The name of their workplace, when they have one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workplace: Option<String>,
}

/// A festival of the year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VillageFestival {
    /// Its name.
    pub name: String,
    /// Its date, e.g. `1 May`.
    pub date: String,
    /// What it is.
    pub about: String,
}

/// The speaker's village (`theVillage`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TheVillage {
    /// The village's name, when the mod authors one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its places.
    pub places: Vec<VillagePlace>,
    /// Its people.
    pub people: Vec<VillagePerson>,
    /// The feasts of the year.
    pub festivals: Vec<VillageFestival>,
}

/// One line of the conversation the speaker witnessed (`justSaid`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaidLine {
    /// [`STRANGER`] for the player, otherwise the NPC's full name.
    pub speaker: String,
    /// What they said.
    pub said: String,
}

/// How a speaker came to answer the player's line (dynamic NPC context §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Addressing {
    /// The line was spoken to this speaker.
    SpokenTo,
    /// The line was spoken to everyone present.
    PartOfAll,
    /// The line mentioned this speaker, who joins after the recipients.
    JoiningMentioned,
}

impl Addressing {
    /// `howYouCameToAnswer`, as the frozen prompt words it.
    pub fn sentence(self) -> &'static str {
        match self {
            Self::SpokenTo => "The stranger is speaking to you.",
            Self::PartOfAll => {
                "The stranger spoke to everyone here, and you are the one answering."
            }
            Self::JoiningMentioned => {
                "You were not spoken to, but you were mentioned, and you are joining in."
            }
        }
    }
}

/// The present moment (`thisMoment`, design §4.4, dynamic NPC context §6).
/// Fields serialize in the frozen prompt's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThisMoment {
    /// The place of the conversation.
    pub r#where: String,
    /// What the speaker is doing by their schedule, when they are where it
    /// puts them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub what_you_are_doing: Option<String>,
    /// Day, date, clock time and part of day, e.g.
    /// `Monday 20 March 1820, 10:00 (morning)`.
    pub when: String,
    /// The current weather.
    pub weather: String,
    /// Today's festival, or [`NO_FEAST`].
    pub feast_today: String,
    /// The other people in the room, `Name, activity`, or [`NOBODY_ELSE`].
    pub also_here: Vec<String>,
    /// What the speaker knows of the player: [`NEVER_MET`], or
    /// [`MET_BEFORE`] followed by what they know.
    pub the_stranger: String,
    /// [`Addressing::sentence`].
    pub how_you_came_to_answer: String,
    /// The recent conversation the speaker witnessed, oldest first.
    pub just_said: Vec<SaidLine>,
    /// [`ANACHRONISM_NOTICE`] when the player's line holds something foreign
    /// to the speaker's time; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// Input for the dialogue role: the frozen prompt's request for one speaker,
/// holding only what that speaker could know. Fields serialize in the
/// frozen order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueEndpointInput {
    /// The speaker's engine id, for the host's own use (labelling the
    /// committed line); never sent.
    #[serde(skip)]
    pub speaker: NpcId,
    /// The speaker's own character.
    pub about_you: AboutYou,
    /// What everyone in the speaker's time and place knows.
    pub common_knowledge: Vec<KnowledgeTopic>,
    /// The speaker's village.
    pub the_village: TheVillage,
    /// The present moment.
    pub this_moment: ThisMoment,
    /// The player's words (or the autonomous prompt), bounded.
    pub what_the_stranger_says: String,
}

impl DialogueEndpointInput {
    /// Marks the player's line as holding something foreign to the
    /// speaker's time (design §4.4). The engine does not set this yet: the
    /// `rundale-intent` anachronism question (design §6.6) will.
    pub fn with_anachronism_notice(mut self) -> Self {
        self.this_moment.notice = Some(ANACHRONISM_NOTICE.to_string());
        self
    }
}

fn bounded_input(text: &str) -> String {
    text.chars().take(MAX_PLAYER_INPUT_CHARS).collect()
}

fn location_name(world: &WorldState, id: LocationId) -> Option<String> {
    world.graph.get(id).map(|location| location.name.clone())
}

/// A relationship strength as the lab wrote it: two decimals at most, no
/// trailing zeros (`0.9`, `0.55`).
fn strength(value: f64) -> String {
    format!("{}", (value * 100.0).round() / 100.0)
}

/// `aboutYou` for `speaker`: their own character, from `npcs.json` and the
/// mod's dialogue content.
fn about_you(
    world: &WorldState,
    npc_manager: &NpcManager,
    context: &DialogueContext,
    speaker: &Npc,
) -> AboutYou {
    let authored = context.character(speaker.id.0);
    let date = world.clock.now().date_naive();
    let day = speaker
        .schedule()
        .and_then(|schedule| schedule.resolve(Season::from_date(date), DayType::from_date(date)))
        .unwrap_or_default()
        .iter()
        .take(MAX_LIST_ENTRIES)
        .map(|entry| {
            let place = location_name(world, entry.location)
                .map(|name| format!(" ({name})"))
                .unwrap_or_default();
            format!(
                "{:02}:00-{:02}:59 {}{place}",
                entry.start_hour,
                entry.end_hour,
                entry.activity.trim()
            )
        })
        .collect();
    let relationships = speaker
        .relationships
        .iter()
        .filter_map(|(target, relationship)| {
            let other = npc_manager.get(*target)?;
            let words = authored.and_then(|character| {
                character
                    .relationships
                    .iter()
                    .find(|words| words.npc == target.0)
            });
            let (kind, feeling) = match words {
                Some(words) => (
                    words.kind.clone(),
                    format!("{} ({})", words.feeling, strength(relationship.strength)),
                ),
                None => (
                    relationship.kind.to_string(),
                    strength(relationship.strength),
                ),
            };
            Some(Relationship {
                who: other.name.clone(),
                kind,
                feeling,
            })
        })
        .take(MAX_LIST_ENTRIES)
        .collect();
    AboutYou {
        name: speaker.name.clone(),
        age: speaker.age,
        pronouns: speaker.pronouns.clone(),
        appearance: speaker.brief_description.clone(),
        occupation: speaker.occupation.clone(),
        personality: speaker.personality.clone(),
        home: speaker.home.and_then(|id| location_name(world, id)),
        workplace: speaker.workplace.and_then(|id| location_name(world, id)),
        day,
        relationships,
        knows: speaker
            .knowledge
            .iter()
            .take(MAX_LIST_ENTRIES)
            .cloned()
            .collect(),
        mood: speaker.mood.clone(),
        backstory: authored
            .map(|character| {
                character
                    .backstory
                    .iter()
                    .take(MAX_LIST_ENTRIES)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// A festival's `description` as a clause: "Start of spring — the feast of
/// St. Brigid" becomes "start of spring, the feast of St. Brigid".
fn festival_about(description: &str) -> String {
    let text = description.trim().replace(" — ", ", ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn month_name(month: u32) -> &'static str {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    MONTHS
        .get(month.wrapping_sub(1) as usize)
        .copied()
        .unwrap_or("")
}

/// `theVillage` for `speaker`: the village the mod's dialogue content puts
/// them in, or, when it names none, the whole world as one village.
fn the_village(
    world: &WorldState,
    npc_manager: &NpcManager,
    context: &DialogueContext,
    speaker: &Npc,
) -> TheVillage {
    let village = context
        .character(speaker.id.0)
        .and_then(|character| context.village(&character.village));
    let (name, place_ids, people_ids): (Option<String>, Vec<LocationId>, Vec<NpcId>) = match village
    {
        Some(village) => (
            Some(village.name.clone()),
            village
                .places
                .iter()
                .map(|place| LocationId(place.location))
                .collect(),
            village
                .people
                .iter()
                .map(|person| NpcId(person.npc))
                .collect(),
        ),
        None => {
            let mut places = world.graph.location_ids();
            places.sort();
            let mut people: Vec<NpcId> = npc_manager.all_npcs().map(|npc| npc.id).collect();
            people.sort();
            (None, places, people)
        }
    };
    let places = place_ids
        .into_iter()
        .filter_map(|id| {
            let location = world.graph.get(id)?;
            Some(VillagePlace {
                name: location.name.clone(),
                looks: village.and_then(|village| {
                    village
                        .places
                        .iter()
                        .find(|place| place.location == id.0)
                        .map(|place| place.looks.clone())
                }),
                story: location.mythological_significance.clone(),
                ways: location
                    .connections
                    .iter()
                    .take(MAX_LIST_ENTRIES)
                    .map(|connection| connection.path_description.clone())
                    .collect(),
            })
        })
        .take(MAX_LIST_ENTRIES)
        .collect();
    let people = people_ids
        .into_iter()
        .filter_map(|id| {
            let npc = npc_manager.get(id)?;
            let work = village
                .and_then(|village| village.people.iter().find(|person| person.npc == id.0))
                .map(|person| person.work.clone())
                .unwrap_or_else(|| npc.occupation.clone());
            Some(VillagePerson {
                name: npc.name.clone(),
                looks: npc.brief_description.clone(),
                work,
                home: npc.home.and_then(|place| location_name(world, place)),
                workplace: npc.workplace.and_then(|place| location_name(world, place)),
            })
        })
        .take(MAX_LIST_ENTRIES)
        .collect();
    let festivals = context
        .festivals
        .iter()
        .take(MAX_LIST_ENTRIES)
        .map(|festival| VillageFestival {
            name: festival.name.clone(),
            date: format!("{} {}", festival.day, month_name(festival.month)),
            about: festival_about(&festival.description),
        })
        .collect();
    TheVillage {
        name,
        places,
        people,
        festivals,
    }
}

/// `justSaid`: the last exchanges the speaker witnessed, oldest first, as
/// lines. A player's line said to several people is written once, before
/// the first reply to it; an unprompted NPC line has no player line.
fn just_said(world: &WorldState, speaker_id: NpcId) -> Vec<SaidLine> {
    let exchanges = world
        .conversation_log
        .recent_witnessed_by(speaker_id, ConversationLog::capacity());
    let start = exchanges.len().saturating_sub(MAX_RECENT_CONVERSATION);
    let mut lines = Vec::new();
    let mut previous: Option<(chrono::DateTime<chrono::Utc>, &str)> = None;
    for exchange in &exchanges[start..] {
        let player = exchange.player_input.trim();
        if !player.is_empty() && previous != Some((exchange.timestamp, player)) {
            lines.push(SaidLine {
                speaker: STRANGER.to_string(),
                said: player.to_string(),
            });
        }
        previous = Some((exchange.timestamp, player));
        let reply = exchange.npc_dialogue.trim();
        if !reply.is_empty() {
            lines.push(SaidLine {
                speaker: exchange.speaker_name.clone(),
                said: reply.to_string(),
            });
        }
    }
    lines
}

/// What `speaker_id` knows of the player from before this line: the name
/// the player gave them, if any. The engine keeps no other per-NPC knowledge
/// of the player yet (issue 2196).
fn stranger_known(world: &WorldState, npc_manager: &NpcManager, speaker_id: NpcId) -> Vec<String> {
    world
        .player_name
        .as_ref()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty() && npc_manager.knows_player_name(speaker_id))
        .map(|name| vec![format!("They told you their name is {name}.")])
        .unwrap_or_default()
}

/// `thisMoment.theStranger` for what the speaker knows of the player.
pub fn the_stranger(known: &[String]) -> String {
    if known.is_empty() {
        NEVER_MET.to_string()
    } else {
        format!("{MET_BEFORE}{}", known.join(" "))
    }
}

fn this_moment(
    world: &WorldState,
    npc_manager: &NpcManager,
    context: &DialogueContext,
    speaker: &Npc,
    addressing: Addressing,
) -> ThisMoment {
    let here = world.player_location;
    let now = world.clock.now();
    let mut others: Vec<&Npc> = npc_manager
        .npcs_at(here)
        .into_iter()
        .filter(|npc| npc.id != speaker.id)
        .collect();
    others.sort_by_key(|npc| npc.id);
    let mut also_here: Vec<String> = others
        .into_iter()
        .take(MAX_LIST_ENTRIES)
        .map(|npc| match npc.authored_activity_at(now) {
            Some(activity) => format!("{}, {activity}", npc.name),
            None => npc.name.clone(),
        })
        .collect();
    if also_here.is_empty() {
        also_here.push(NOBODY_ELSE.to_string());
    }
    let feast_today = context
        .festivals
        .iter()
        .find(|festival| festival.month == now.month() && festival.day == now.day())
        .map(|festival| festival.name.clone())
        .or_else(|| {
            world
                .clock
                .check_festival()
                .map(|festival| festival.to_string())
        })
        .unwrap_or_else(|| NO_FEAST.to_string());
    ThisMoment {
        r#where: location_name(world, here)
            .unwrap_or_else(|| world.current_location().name.clone()),
        what_you_are_doing: speaker.authored_activity_at(now).map(str::to_string),
        when: format!(
            "{}, {} ({})",
            now.format("%A %-d %B %Y"),
            now.format("%H:%M"),
            world.clock.time_of_day().to_string().to_lowercase()
        ),
        weather: world.weather.to_string(),
        feast_today,
        also_here,
        the_stranger: the_stranger(&stranger_known(world, npc_manager, speaker.id)),
        how_you_came_to_answer: addressing.sentence().to_string(),
        just_said: just_said(world, speaker.id),
        notice: None,
    }
}

/// Builds the dialogue role's input for `speaker_id` answering `player_input`
/// at the player's location, from the state the attempt reads, before the
/// line is applied. Each speaker of a line gets their own input when their
/// call is built, so a later speaker sees the replies already committed.
/// `context` is the mod's authored dialogue content
/// ([`EndpointCatalog::dialogue_context`]). `None` when the speaker is
/// unknown.
pub fn dialogue_input(
    world: &WorldState,
    npc_manager: &NpcManager,
    context: &DialogueContext,
    speaker_id: NpcId,
    player_input: &str,
    addressing: Addressing,
) -> Option<DialogueEndpointInput> {
    let speaker = npc_manager.get(speaker_id)?;
    Some(DialogueEndpointInput {
        speaker: speaker_id,
        about_you: about_you(world, npc_manager, context, speaker),
        common_knowledge: context
            .common_knowledge
            .iter()
            .take(MAX_LIST_ENTRIES)
            .map(|topic| KnowledgeTopic {
                topic: topic.topic.clone(),
                text: topic.text.clone(),
            })
            .collect(),
        the_village: the_village(world, npc_manager, context, speaker),
        this_moment: this_moment(world, npc_manager, context, speaker, addressing),
        what_the_stranger_says: bounded_input(player_input),
    })
}

/// The intent call's Endpoint, when the catalog declares the intent role,
/// and what each option it offers means.
pub fn intent_endpoint(
    catalog: &EndpointCatalog,
    world: &WorldState,
    npc_manager: &NpcManager,
    raw_input: &str,
) -> Option<(EndpointCall, IntentOptions)> {
    let reference = catalog.reference(EndpointRole::Intent)?.clone();
    let (input, options) = crate::intent_decision::intent_input(world, npc_manager, raw_input);
    Some((
        EndpointCall {
            reference,
            input: EndpointInput::Intent(Box::new(input)),
        },
        options,
    ))
}

/// The dialogue call's Endpoint, when the catalog declares the dialogue role
/// and the speaker exists.
pub fn dialogue_endpoint(
    catalog: &EndpointCatalog,
    world: &WorldState,
    npc_manager: &NpcManager,
    speaker_id: NpcId,
    player_input: &str,
    addressing: Addressing,
) -> Option<EndpointCall> {
    let reference = catalog.reference(EndpointRole::Dialogue)?.clone();
    let input = dialogue_input(
        world,
        npc_manager,
        &catalog.dialogue_context,
        speaker_id,
        player_input,
        addressing,
    )?;
    Some(EndpointCall {
        reference,
        input: EndpointInput::Dialogue(Box::new(input)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strengths_and_festivals_read_as_the_lab_wrote_them() {
        assert_eq!(strength(0.55), "0.55");
        assert_eq!(strength(0.9), "0.9");
        assert_eq!(strength(0.72), "0.72");
        assert_eq!(strength(0.899_999_9), "0.9");
        assert_eq!(
            festival_about("Start of spring — the feast of St. Brigid"),
            "start of spring, the feast of St. Brigid"
        );
        assert_eq!(month_name(5), "May");
        assert_eq!(month_name(13), "");
    }

    #[test]
    fn the_stranger_is_never_met_until_the_speaker_knows_something() {
        assert_eq!(the_stranger(&[]), NEVER_MET);
        assert_eq!(
            the_stranger(&["They told you their name is Seán.".to_string()]),
            "Someone you have met before. What you know of them: They told you their name is Seán."
        );
    }
}
