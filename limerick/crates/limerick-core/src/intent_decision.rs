//! The intent call as a decision (design `docs/design/dynamic-npc-context.md`
//! §4.6): the engine offers the people, places and phrases a line could be
//! about, the `rundale-intent` Endpoint's decision model answers with a
//! probability per option, and the engine turns the answer into an intent and
//! a recipient. The model only selects among the options built here; every
//! answer is checked against them.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::debug_view::fold;
use crate::input::{AtmosphericTopic, IntentKind, PlayerIntent};
use crate::npc::NpcId;
use crate::npc::manager::NpcManager;
use crate::world::WorldState;
use limerick_types::LocationId;

/// The intent input's fixed `role` value.
pub const INTENT_ROLE: &str = "player_intent";
/// Longest line the definition accepts, in characters.
pub const MAX_LINE_CHARS: usize = 1024;
/// Minimum probability for an answer to be acted on (design §4.3).
pub const ANSWER_THRESHOLD: f64 = 0.70;
/// Minimum probability that a line talks about a person present (design
/// §4.3, question 2).
pub const MENTION_THRESHOLD: f64 = 0.70;
/// The recipient option for everyone present.
pub const ALL_OPTION: &str = "all";
/// The recipient option for someone not present.
pub const SOMEONE_ELSE_OPTION: &str = "someone_else";
/// The recipient option for no one in particular.
pub const NO_ONE_OPTION: &str = "no_one";

const MAX_PRESENT: usize = 32;
const MAX_REFERENCES: usize = 16;
const MAX_PLACES: usize = 32;
const MAX_PHRASES: usize = 24;
const MAX_ALIASES: usize = 16;
const MAX_NAME_CHARS: usize = 120;
const MAX_DESCRIPTION_CHARS: usize = 240;
const MAX_ALIAS_CHARS: usize = 60;

/// Input for the intent role, matching `rundale-intent` v1's `inputSchema`
/// once the host adds the invocation envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentEndpointInput {
    /// Always [`INTENT_ROLE`].
    pub role: String,
    /// The player's line.
    pub line: String,
    /// Each person in the room, in id order.
    pub present: Vec<PresentOption>,
    /// Each word of the line that fits several people present.
    pub references: Vec<ReferenceOption>,
    /// [`ALL_OPTION`], offered only when two or more people are present.
    pub everyone: Vec<GroupOption>,
    /// [`SOMEONE_ELSE_OPTION`], offered only when two or more are present.
    pub someone_else: Vec<GroupOption>,
    /// [`NO_ONE_OPTION`], offered unless exactly one person is present, so
    /// the recipient question has a single option (answered in code) when
    /// one person or nobody is present (design §4.3).
    pub no_one: Vec<GroupOption>,
    /// The people present again, when two or more are: the subjects of the
    /// mention question.
    pub mentionable: Vec<MentionOption>,
    /// Each place, in id order.
    pub places: Vec<PlaceOption>,
    /// Candidate phrases cut from the line.
    pub phrases: Vec<PhraseOption>,
}

/// A person present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentOption {
    /// `p<n>`.
    pub option_id: String,
    /// Full name; used for routing, never shown.
    pub name: String,
    /// How the person looks to someone who has not met them.
    pub description: String,
    /// Occupation.
    pub occupation: String,
    /// Whether the player has been introduced to them.
    pub introduced: bool,
}

/// One of the recipient question's group options (everyone present,
/// someone not present, no one in particular).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupOption {
    /// The option's fixed id.
    pub option_id: String,
}

/// A person present, as the subject of the mention question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionOption {
    /// The person's `p<n>` id.
    pub option_id: String,
    /// Full name.
    pub name: String,
    /// How the person looks to someone who has not met them.
    pub description: String,
}

/// A word of the line that fits several people present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceOption {
    /// `r<n>`.
    pub option_id: String,
    /// The word as the player typed it.
    pub phrase: String,
    /// How the player knows each person it fits.
    pub people: Vec<String>,
}

/// A place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOption {
    /// `l<n>`.
    pub option_id: String,
    /// Name.
    pub name: String,
    /// Other names for it.
    pub aliases: Vec<String>,
    /// First sentence of its description.
    pub description: String,
    /// Who lives there, as the player knows them.
    pub residents: Vec<String>,
}

/// A candidate phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhraseOption {
    /// `s<n>`.
    pub option_id: String,
    /// The phrase, in the player's spelling.
    pub text: String,
}

/// What each option id offered in one call means, kept to read the answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntentOptions {
    present: HashMap<String, (NpcId, String)>,
    references: HashMap<String, String>,
    places: HashMap<String, String>,
    phrases: HashMap<String, String>,
    groups: Vec<&'static str>,
}

/// Who a line is spoken to, as the decision settled it (design §3.1,
/// step 4). Journaled with the request, so a clarification answer or a
/// retry reuses it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
pub enum DecidedRecipient {
    /// Below the threshold, or the call failed: the game asks.
    #[default]
    Undecided,
    /// A person present, by full name.
    Present(String),
    /// A word that fits several people present ("Which Connolly?").
    Reference(String),
    /// Everyone present.
    All,
    /// Someone not present, or a name that fits no one present: everyone
    /// present hears it.
    SomeoneElse,
    /// Nobody in particular: the unaddressed rules apply.
    NoOne,
}

/// The System 1 answer about who a line is for and whom it talks about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientDecision {
    /// Who the line is spoken to.
    pub recipient: DecidedRecipient,
    /// Full names of people present the line talks about without speaking
    /// to them, in roster order.
    #[serde(default)]
    pub mentioned: Vec<String>,
}

/// One answered intent call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentDecision {
    /// The intent, in the engine's usual form.
    pub intent: PlayerIntent,
    /// Who a `Talk` intent is for, and whom it mentions.
    pub recipient: RecipientDecision,
}

fn bounded(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

/// Whether `c` belongs to a word of the player's line: a letter, or a
/// combining mark (a fada typed as a separate character stays in its word).
fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || ('\u{0300}'..='\u{036F}').contains(&c)
}

/// Whether a folded `word` of the line is a first name, family name or
/// occupation word of `npc`, whether or not the player has met them. This is
/// the one test behind both the reference options offered to System 1 and the
/// people a chosen reference means, so the two cannot disagree.
fn reference_fits(npc: &crate::npc::Npc, word: &str) -> bool {
    let names = words(&npc.name);
    let roles = words(&npc.occupation);
    names.first().is_some_and(|first| first == word)
        || names.last().is_some_and(|last| last == word)
        || roles
            .iter()
            .any(|role| role == word && !MINOR.contains(&role.as_str()))
}

/// The people among `candidates` that `reference` fits, by the same test as
/// the options offered, so a reference System 1 chose means the people it
/// offered however many of them the player has met.
pub fn reference_candidates(
    reference: &str,
    candidates: &[NpcId],
    npc_manager: &NpcManager,
) -> Vec<NpcId> {
    let folded = words(reference);
    let [word] = folded.as_slice() else {
        return Vec::new();
    };
    candidates
        .iter()
        .copied()
        .filter(|id| {
            npc_manager
                .get(*id)
                .is_some_and(|npc| reference_fits(npc, word))
        })
        .collect()
}

/// Words of `text`, folded for matching.
fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

const MINOR: &[&str] = &["and", "the", "of", "a"];
const DETERMINERS: &[&str] = &[
    "the", "a", "an", "my", "his", "her", "their", "your", "our", "this", "that",
];
const PREPOSITIONS: &[&str] = &[
    "on", "in", "at", "to", "from", "with", "by", "for", "of", "behind", "under", "over", "near",
    "into", "about", "before", "after", "across", "around", "beside",
];

/// Candidate phrases cut from the line (design §4.6): each run of words that
/// starts at a determiner, a possessive or a capitalised word (past the
/// clause's first word) and ends before punctuation or a preposition, with
/// every shortening from the end. Phrases keep the player's spelling.
pub fn candidate_phrases(line: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for clause in line.split([',', '.', ';', ':', '!', '?']) {
        let tokens: Vec<&str> = clause.split_whitespace().collect();
        for start in 0..tokens.len() {
            let word = tokens[start];
            let lower = fold(word);
            let opens = DETERMINERS.contains(&lower.as_str())
                || lower.ends_with("'s")
                || lower.ends_with("s'")
                || (start > 0
                    && word.chars().next().is_some_and(char::is_uppercase)
                    && lower != "i");
            if !opens {
                continue;
            }
            let mut end = start + 1;
            while end < tokens.len() && !PREPOSITIONS.contains(&fold(tokens[end]).as_str()) {
                end += 1;
            }
            for stop in (start + 1..=end).rev() {
                if stop == start + 1 && DETERMINERS.contains(&lower.as_str()) {
                    continue;
                }
                let phrase = bounded(&tokens[start..stop].join(" "), MAX_NAME_CHARS);
                if !phrase.is_empty() && !found.contains(&phrase) && found.len() < MAX_PHRASES {
                    found.push(phrase);
                }
            }
        }
    }
    found
}

/// Builds the intent input for `line` from the state the attempt reads, and
/// what each option id means.
pub fn intent_input(
    world: &WorldState,
    npc_manager: &NpcManager,
    line: &str,
) -> (IntentEndpointInput, IntentOptions) {
    let here = world.player_location;
    let mut options = IntentOptions::default();
    let mut people: Vec<&crate::npc::Npc> = npc_manager.all_npcs().collect();
    people.sort_by_key(|npc| npc.id.0);
    // The same people the recipient rules count as present.
    let here_ids = npc_manager.npcs_at_ids(here);
    let present_people: Vec<&crate::npc::Npc> = people
        .iter()
        .copied()
        .filter(|npc| here_ids.contains(&npc.id))
        .take(MAX_PRESENT)
        .collect();

    let present: Vec<PresentOption> = present_people
        .iter()
        .enumerate()
        .map(|(index, npc)| {
            let option_id = format!("p{}", index + 1);
            options
                .present
                .insert(option_id.clone(), (npc.id, npc.name.clone()));
            PresentOption {
                option_id,
                name: bounded(&npc.name, MAX_NAME_CHARS),
                description: bounded(&npc.brief_description, MAX_DESCRIPTION_CHARS),
                occupation: bounded(&npc.occupation, MAX_NAME_CHARS),
                introduced: npc_manager.is_introduced(npc.id),
            }
        })
        .collect();

    // With one person present the line goes to them, so the recipient
    // question keeps that single option and is answered in code; with
    // nobody present it keeps "no one" alone. Group options and mentions
    // only mean something with two or more present (design §3.1, §4.3).
    let group = |id: &str| {
        vec![GroupOption {
            option_id: id.to_string(),
        }]
    };
    let several = present.len() >= 2;

    let everyone = if several {
        group(ALL_OPTION)
    } else {
        Vec::new()
    };
    let someone_else = if several {
        group(SOMEONE_ELSE_OPTION)
    } else {
        Vec::new()
    };
    let no_one = if present.len() == 1 {
        Vec::new()
    } else {
        group(NO_ONE_OPTION)
    };
    for (offered, id) in [
        (&everyone, ALL_OPTION),
        (&someone_else, SOMEONE_ELSE_OPTION),
        (&no_one, NO_ONE_OPTION),
    ] {
        if !offered.is_empty() {
            options.groups.push(id);
        }
    }
    let mentionable = if several {
        present
            .iter()
            .map(|person| MentionOption {
                option_id: person.option_id.clone(),
                name: person.name.clone(),
                description: person.description.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    // One reference per word of the line that is a first name, family name
    // or occupation word of several people present (design §4.3).
    let mut references = Vec::new();
    let original: Vec<&str> = line
        .split(|c: char| !is_word_char(c))
        .filter(|word| !word.is_empty())
        .collect();
    let mut seen: Vec<String> = Vec::new();
    for (index, word) in words(line).into_iter().enumerate() {
        if MINOR.contains(&word.as_str()) || seen.contains(&word) {
            continue;
        }
        let fits: Vec<&crate::npc::Npc> = present_people
            .iter()
            .copied()
            .filter(|npc| reference_fits(npc, &word))
            .collect();
        if fits.len() < 2 || references.len() >= MAX_REFERENCES {
            continue;
        }
        seen.push(word.clone());
        let option_id = format!("r{}", references.len() + 1);
        let phrase = bounded(
            original.get(index).copied().unwrap_or(&word),
            MAX_NAME_CHARS,
        );
        options.references.insert(option_id.clone(), phrase.clone());
        references.push(ReferenceOption {
            option_id,
            phrase,
            people: fits
                .iter()
                .map(|npc| bounded(npc_manager.display_name(npc), MAX_NAME_CHARS))
                .collect(),
        });
    }

    let mut place_ids: Vec<LocationId> = world.graph.location_ids();
    place_ids.sort_by_key(|id| id.0);
    let places = place_ids
        .into_iter()
        .filter_map(|id| world.graph.get(id).map(|location| (id, location)))
        .take(MAX_PLACES)
        .enumerate()
        .map(|(index, (id, location))| {
            let option_id = format!("l{}", index + 1);
            options
                .places
                .insert(option_id.clone(), location.name.clone());
            let first_sentence = location
                .description_template
                .split_inclusive(". ")
                .next()
                .unwrap_or_default();
            let description = strip_placeholders(first_sentence);
            PlaceOption {
                option_id,
                name: bounded(&location.name, MAX_NAME_CHARS),
                aliases: location
                    .aliases
                    .iter()
                    .take(MAX_ALIASES)
                    .map(|alias| bounded(alias, MAX_ALIAS_CHARS))
                    .collect(),
                description: bounded(&description, MAX_DESCRIPTION_CHARS),
                residents: people
                    .iter()
                    .filter(|npc| npc.home == Some(id))
                    .map(|npc| bounded(npc_manager.display_name(npc), MAX_NAME_CHARS))
                    .collect(),
            }
        })
        .collect();

    let phrases = candidate_phrases(line)
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let option_id = format!("s{}", index + 1);
            options.phrases.insert(option_id.clone(), text.clone());
            PhraseOption { option_id, text }
        })
        .collect();

    (
        IntentEndpointInput {
            role: INTENT_ROLE.to_string(),
            line: bounded(line, MAX_LINE_CHARS),
            present,
            references,
            everyone,
            someone_else,
            no_one,
            mentionable,
            places,
            phrases,
        },
        options,
    )
}

/// `text` without `{placeholder}` spans, trimmed.
fn strip_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The chosen option and its probability, when the answer is well formed.
fn top(output: &Value, question: &str) -> Option<(String, f64)> {
    let answer = output.get(question)?;
    let choice = answer.get("choice")?.as_str()?.to_string();
    let p = answer.get("probabilities")?.get(&choice)?.as_f64()?;
    Some((choice, p))
}

/// Reads an Endpoint reply as a `rundale-intent` decision. `None` when the
/// reply is not one, or names an option this call did not offer.
pub fn intent_from_decision(
    text: &str,
    options: &IntentOptions,
    raw: &str,
) -> Option<IntentDecision> {
    let output: Value = serde_json::from_str(text).ok()?;
    let (action, action_p) = top(&output, "action")?;
    let mut intent = PlayerIntent {
        intent: IntentKind::Talk,
        target: None,
        dialogue: Some(raw.to_string()),
        atmosphere: None,
        raw: raw.to_string(),
    };
    let phrase = || match top(&output, "target") {
        Some((choice, p)) if p >= ANSWER_THRESHOLD => options.phrases.get(&choice).cloned(),
        _ => None,
    };
    // Look, examine and interact only narrate, so the top action is taken.
    // Travel changes the world and "none" drops the line, so they must clear
    // the threshold; below it the line is speech, the game's default.
    let narration = matches!(action.as_str(), "look" | "examine" | "interact");
    if action_p >= ANSWER_THRESHOLD || narration {
        match action.as_str() {
            "move" => {
                intent.intent = IntentKind::Move;
                intent.dialogue = None;
                intent.target = match top(&output, "destination") {
                    Some((choice, p)) if p >= ANSWER_THRESHOLD => match choice.as_str() {
                        "none" => None,
                        // An unknown place: the engine says the way is not known.
                        "not_listed" => Some(phrase().unwrap_or_else(|| raw.to_string())),
                        id => Some(options.places.get(id)?.clone()),
                    },
                    _ => None,
                };
                return Some(IntentDecision {
                    intent,
                    recipient: RecipientDecision::default(),
                });
            }
            "look" | "examine" | "interact" => {
                intent.intent = match action.as_str() {
                    "look" => IntentKind::Look,
                    "examine" => IntentKind::Examine,
                    _ => IntentKind::Interact,
                };
                intent.dialogue = None;
                intent.target = phrase();
                return Some(IntentDecision {
                    intent,
                    recipient: RecipientDecision::default(),
                });
            }
            "none" => {
                intent.intent = IntentKind::Unknown;
                intent.dialogue = None;
                return Some(IntentDecision {
                    intent,
                    recipient: RecipientDecision::default(),
                });
            }
            "talk" => {}
            _ => return None,
        }
    }
    intent.atmosphere = match top(&output, "atmosphere") {
        Some((choice, p)) if p >= ANSWER_THRESHOLD => match choice.as_str() {
            "listen" => Some(AtmosphericTopic::Listen),
            "omen" => Some(AtmosphericTopic::Omen),
            "folklore" => Some(AtmosphericTopic::Folklore),
            _ => None,
        },
        _ => None,
    };
    let (choice, p) = top(&output, "recipient")?;
    // Every answer must be an option this call offered, confident or not.
    let recipient = match choice.as_str() {
        id if options.groups.contains(&id) => match id {
            ALL_OPTION => DecidedRecipient::All,
            SOMEONE_ELSE_OPTION => DecidedRecipient::SomeoneElse,
            _ => DecidedRecipient::NoOne,
        },
        id => {
            if let Some((_, name)) = options.present.get(id) {
                DecidedRecipient::Present(name.clone())
            } else {
                DecidedRecipient::Reference(options.references.get(id)?.clone())
            }
        }
    };
    let recipient = if p >= ANSWER_THRESHOLD {
        recipient
    } else {
        DecidedRecipient::Undecided
    };
    // Mentions of people present, keyed by their `p<n>` id; a mention of
    // anyone not offered is not an answer.
    let mut mentioned: Vec<(String, String)> = Vec::new();
    if let Some(answers) = output.get("mentioned").and_then(Value::as_object) {
        for (id, p) in answers {
            let (_, name) = options.present.get(id)?;
            if p.as_f64()? >= MENTION_THRESHOLD {
                mentioned.push((id.clone(), name.clone()));
            }
        }
    }
    // Roster order: option ids are assigned in it (`p1`, `p2`, ...).
    mentioned.sort_by_key(|(id, _)| id[1..].parse::<u32>().unwrap_or(u32::MAX));
    Some(IntentDecision {
        intent,
        recipient: RecipientDecision {
            recipient,
            mentioned: mentioned.into_iter().map(|(_, name)| name).collect(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn options() -> IntentOptions {
        IntentOptions {
            present: HashMap::from([
                ("p1".into(), (NpcId(1), "Mícheál Connolly".into())),
                ("p2".into(), (NpcId(2), "Róisín Connolly".into())),
            ]),
            references: HashMap::from([("r1".into(), "Connolly".into())]),
            places: HashMap::from([("l2".into(), "the Letter Office".into())]),
            phrases: HashMap::from([("s1".into(), "the old well".into())]),
            groups: vec![ALL_OPTION, SOMEONE_ELSE_OPTION, NO_ONE_OPTION],
        }
    }

    /// Two Mícheáls and Róisín at the cottage (location 3).
    fn two_micheals() -> (WorldState, NpcManager) {
        use crate::npc::Npc;
        use crate::npc::types::NpcState;
        let mut world = WorldState::new();
        world.player_location = crate::world::LocationId(3);
        let mut npcs = NpcManager::new();
        for (id, name) in [
            (1, "Mícheál Connolly"),
            (2, "Mícheál Ó Briain"),
            (3, "Róisín Connolly"),
        ] {
            let mut npc = Npc::new_test_npc();
            npc.id = NpcId(id);
            npc.name = name.to_string();
            npc.occupation = "Farmer".to_string();
            npc.set_location_and_state(crate::world::LocationId(3), NpcState::Present);
            npcs.add_npc(npc);
        }
        (world, npcs)
    }

    #[test]
    fn a_reference_keeps_the_players_spelling_when_a_fada_is_a_separate_character() {
        let (world, npcs) = two_micheals();
        for line in [
            "ask Mícheál about the road",
            "ask Mi\u{301}chea\u{301}l about the road",
        ] {
            let (input, _) = intent_input(&world, &npcs, line);
            let phrases: Vec<&str> = input
                .references
                .iter()
                .map(|reference| reference.phrase.as_str())
                .collect();
            let wanted = line.split_whitespace().nth(1).unwrap();
            assert_eq!(phrases, [wanted], "{line:?}");
        }
    }

    #[test]
    fn a_chosen_reference_means_everyone_it_was_offered_for_met_or_not() {
        let (_, mut npcs) = two_micheals();
        let present = npcs.npcs_at_ids(crate::world::LocationId(3));
        let both = reference_candidates("Mícheál", &present, &npcs);
        assert_eq!(both.len(), 2);
        npcs.mark_introduced(NpcId(1));
        assert_eq!(reference_candidates("Mícheál", &present, &npcs), both);
        npcs.mark_introduced(NpcId(2));
        assert_eq!(
            reference_candidates("Mi\u{301}chea\u{301}l", &present, &npcs),
            both
        );
        assert_eq!(reference_candidates("Róisín", &present, &npcs).len(), 1);
    }

    fn choice(choice: &str, p: f64) -> Value {
        json!({"choice": choice, "confidence": p, "probabilities": {choice: p}})
    }

    fn answer(action: (&str, f64), recipient: (&str, f64)) -> Value {
        json!({
            "action": choice(action.0, action.1),
            "destination": choice("none", 0.9),
            "target": choice("none", 0.9),
            "recipient": choice(recipient.0, recipient.1),
            "mentioned": {},
            "atmosphere": choice("none", 0.9),
        })
    }

    fn decide(output: Value) -> Option<IntentDecision> {
        intent_from_decision(&output.to_string(), &options(), "line")
    }

    #[test]
    fn a_confident_move_names_the_offered_place() {
        let mut output = answer(("move", 0.95), ("no_one", 0.9));
        output["destination"] = choice("l2", 0.9);
        let decision = decide(output).unwrap();
        assert_eq!(decision.intent.intent, IntentKind::Move);
        assert_eq!(decision.intent.target.as_deref(), Some("the Letter Office"));
    }

    #[test]
    fn an_unsure_move_is_speech() {
        let decision = decide(answer(("move", 0.5), ("p2", 0.9))).unwrap();
        assert_eq!(decision.intent.intent, IntentKind::Talk);
        assert_eq!(
            decision.recipient.recipient,
            DecidedRecipient::Present("Róisín Connolly".into())
        );
    }

    #[test]
    fn examine_takes_its_phrase_even_when_unsure_of_the_action() {
        let mut output = answer(("examine", 0.4), ("no_one", 0.9));
        output["target"] = choice("s1", 0.8);
        let decision = decide(output).unwrap();
        assert_eq!(decision.intent.intent, IntentKind::Examine);
        assert_eq!(decision.intent.target.as_deref(), Some("the old well"));
    }

    #[test]
    fn recipients_map_to_people_references_and_replies() {
        let cases = [
            ("r1", DecidedRecipient::Reference("Connolly".into())),
            ("all", DecidedRecipient::All),
            ("someone_else", DecidedRecipient::SomeoneElse),
            ("no_one", DecidedRecipient::NoOne),
        ];
        for (id, expected) in cases {
            let decision = decide(answer(("talk", 0.9), (id, 0.9))).unwrap();
            assert_eq!(decision.recipient.recipient, expected, "{id}");
        }
        let unsure = decide(answer(("talk", 0.9), ("p1", 0.6))).unwrap();
        assert_eq!(unsure.recipient.recipient, DecidedRecipient::Undecided);
    }

    #[test]
    fn confident_mentions_name_people_present_in_roster_order() {
        let mut output = answer(("talk", 0.9), ("no_one", 0.9));
        output["mentioned"] = json!({"p2": 0.91, "p1": 0.4});
        let decision = decide(output.clone()).unwrap();
        assert_eq!(decision.recipient.mentioned, ["Róisín Connolly"]);
        output["mentioned"] = json!({"p2": 0.8, "p1": 0.75});
        let decision = decide(output.clone()).unwrap();
        assert_eq!(
            decision.recipient.mentioned,
            ["Mícheál Connolly", "Róisín Connolly"]
        );
        output["mentioned"] = json!({"p7": 0.9});
        assert!(decide(output).is_none(), "a mention of no one offered");
    }

    #[test]
    fn an_option_that_was_not_offered_is_not_a_decision() {
        assert!(decide(answer(("talk", 0.9), ("p9", 0.9))).is_none());
        // With one person present no group option is offered.
        let one = IntentOptions {
            groups: Vec::new(),
            ..options()
        };
        let output = answer(("talk", 0.9), ("all", 0.9));
        assert!(intent_from_decision(&output.to_string(), &one, "x").is_none());
        let mut output = answer(("move", 0.9), ("no_one", 0.9));
        output["destination"] = choice("l9", 0.9);
        assert!(decide(output).is_none());
        assert!(intent_from_decision(r#"{"intent":"move"}"#, &options(), "x").is_none());
    }

    #[test]
    fn phrases_start_at_determiners_names_and_possessives() {
        let phrases = candidate_phrases("examine Peig's satchel, then go to the old well");
        assert!(
            phrases.contains(&"Peig's satchel".to_string()),
            "{phrases:?}"
        );
        assert!(phrases.contains(&"the old well".to_string()), "{phrases:?}");
        assert!(!phrases.contains(&"the".to_string()));
    }
}
