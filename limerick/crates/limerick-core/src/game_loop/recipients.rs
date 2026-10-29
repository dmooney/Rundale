//! Who a spoken line is for: the recipient policy of the dynamic NPC context
//! design (`docs/design/dynamic-npc-context.md` §3.1 to §3.3).
//!
//! A line spoken aloud reaches the people in the room. With nobody present
//! the player gets the idle message; with one person present that person
//! answers whatever the line says; with two or more the System 1 answer
//! decides, and the game asks when it cannot. No line is ever answered with
//! a system line about someone who is not here. [`resolve_recipients`] is
//! pure: it reads the attempt's state and says what should happen, and the
//! conversation turn acts on it.

use crate::endpoint_input::Addressing;
use crate::intent_decision::{DecidedRecipient, RecipientDecision, reference_candidates};
use crate::npc::manager::{NpcManager, NpcReference};
use crate::npc::{Npc, NpcId};
use crate::turn::{ClarificationChoice, ClarificationPrompt};
use crate::world::WorldState;

/// The question for unaddressed speech (design §3.3).
pub const WHO_QUESTION: &str = "Who are you speaking to?";
/// The choice id of **All** in [`WHO_QUESTION`].
pub const ALL_CHOICE_ID: &str = "choose-all";
/// The label of the **All** choice.
pub const ALL_LABEL: &str = "All";
/// What VoiceOver reads for the **All** choice.
pub const ALL_ACCESSIBILITY_LABEL: &str = "Everyone here";

/// What the player's line says about who it is for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddressRequest {
    /// People the player picked with chips or @tags, as typed. Non-empty
    /// means an explicit selection: no System 1 answer, and nobody
    /// mentioned joins (design §3.1, step 1).
    pub explicit: Vec<String>,
    /// The System 1 answer, or `None` when no decision Endpoint answered
    /// (the desktop's in-process model): the line is then taken as spoken
    /// to no one in particular.
    pub decision: Option<RecipientDecision>,
}

/// The player's answers to questions this request already asked.
#[derive(Debug, Clone, Copy, Default)]
pub struct Answers<'a> {
    /// Each answered reference and the person chosen for it. An answer to
    /// [`WHO_QUESTION`] is keyed by the chosen person's label.
    pub chosen: &'a [(String, NpcId)],
    /// The player chose **All**.
    pub everyone: bool,
}

/// How the partnership changes when the line is spoken (design §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartnerChange {
    /// The partner answered an unaddressed line; nothing changes.
    Keep,
    /// The line had exactly one recipient, who becomes the partner.
    Set(NpcId),
    /// The player addressed several people, everyone, or someone else.
    End,
}

/// What happens to the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Nobody is present: the idle message.
    Nobody,
    /// These people answer, in order, each knowing how they came to.
    Speak {
        /// Recipients first, then anyone mentioned who joins.
        speakers: Vec<(NpcId, Addressing)>,
        /// The partnership after the line.
        partner: PartnerChange,
    },
    /// Ask the player first; nothing is sent until they answer.
    Ask(ClarificationPrompt),
    /// The person the player chose has left since the question (or since
    /// the attempt that is being retried): "{label} is no longer here."
    Gone(String),
}

/// Decides who answers `request` in the room the attempt reads.
/// `clarify` is the addressee-clarification flag: with it off the game
/// never asks, and a line it would ask about reaches everyone present.
pub fn resolve_recipients(
    world: &WorldState,
    npc_manager: &NpcManager,
    request: &AddressRequest,
    answers: Answers<'_>,
    clarify: bool,
) -> Resolution {
    let here = world.player_location;
    let present = npc_manager.npcs_at_ids(here);
    if present.is_empty() {
        return Resolution::Nobody;
    }
    let label = |id: NpcId| {
        npc_manager
            .get(id)
            .map(|npc| npc_manager.display_name(npc).to_string())
            .unwrap_or_default()
    };
    let everyone = || Resolution::Speak {
        speakers: present
            .iter()
            .map(|id| (*id, Addressing::PartOfAll))
            .collect(),
        partner: PartnerChange::End,
    };
    let one = |id: NpcId| Resolution::Speak {
        speakers: vec![(id, Addressing::SpokenTo)],
        partner: PartnerChange::Set(id),
    };
    // A person chosen in answer to a question, if still here.
    let chosen = |reference: &str| {
        let wanted = crate::npc::fold_name(reference);
        answers
            .chosen
            .iter()
            .find(|(answered, _)| crate::npc::fold_name(answered) == wanted)
            .map(|(_, id)| *id)
    };

    // 1. Explicit selection: chips and @tags.
    if !request.explicit.is_empty() {
        let mut resolved: Vec<NpcId> = Vec::new();
        for name in &request.explicit {
            if let Some(id) = chosen(name) {
                if !present.contains(&id) {
                    return Resolution::Gone(label(id));
                }
                if !resolved.contains(&id) {
                    resolved.push(id);
                }
                continue;
            }
            // A single word (a first or family name, or an occupation) means
            // every person present it fits, met or not: the same test as the
            // reference options offered to System 1 (design §3.5).
            match reference_candidates(name, &present, npc_manager).as_slice() {
                [id] => {
                    if !resolved.contains(id) {
                        resolved.push(*id);
                    }
                    continue;
                }
                several @ [_, _, ..] if clarify => {
                    return Resolution::Ask(which_prompt(npc_manager, name, several));
                }
                [_, _, ..] => continue,
                [] => {}
            }
            match npc_manager.resolve_reference_at(name, here) {
                NpcReference::Unique(id) => {
                    if !resolved.contains(&id) {
                        resolved.push(id);
                    }
                }
                NpcReference::Ambiguous(candidates) if clarify => {
                    return Resolution::Ask(which_prompt(npc_manager, name, &candidates));
                }
                // Nobody here by that name: like a line spoken to someone
                // elsewhere (design §3.1, step 1).
                NpcReference::Ambiguous(_) | NpcReference::NotFound => {}
            }
        }
        // Several recipients answer in roster (id) order, whatever order the
        // player chose them in (design §2).
        resolved.sort_by_key(|id| id.0);
        return match resolved.as_slice() {
            [] if present.len() == 1 => one(present[0]),
            [] => everyone(),
            [only] => one(*only),
            several => Resolution::Speak {
                speakers: several
                    .iter()
                    .map(|id| (*id, Addressing::SpokenTo))
                    .collect(),
                partner: PartnerChange::End,
            },
        };
    }

    // A run that reuses an earlier answer (a clarification answered, or a
    // retry) checks it again: someone chosen who has since left is "no
    // longer here" rather than replaced by whoever remains (design §7).
    if let Some(id) = answers.chosen.first().map(|(_, id)| *id)
        && !present.contains(&id)
    {
        return Resolution::Gone(label(id));
    }
    if let Some(DecidedRecipient::Present(name)) = request
        .decision
        .as_ref()
        .map(|decision| &decision.recipient)
        && let Some(npc) = npc_manager.all_npcs().find(|npc| &npc.name == name)
        && !present.contains(&npc.id)
    {
        return Resolution::Gone(label(npc.id));
    }

    // **All** was the player's answer: everyone still present answers as
    // part of all, even if only one remains, and no partnership forms.
    if answers.everyone {
        return everyone();
    }

    // 2. One person present answers whatever the line says.
    if present.len() == 1 {
        return one(present[0]);
    }

    // 3 and 4. Two or more present: the System 1 answer.
    let decision = request.decision.clone().unwrap_or(RecipientDecision {
        recipient: DecidedRecipient::NoOne,
        mentioned: Vec::new(),
    });
    let mentioned: Vec<NpcId> = present
        .iter()
        .copied()
        .filter(|id| {
            npc_manager
                .get(*id)
                .is_some_and(|npc| decision.mentioned.contains(&npc.name))
        })
        .collect();
    let uncertain = match &decision.recipient {
        DecidedRecipient::Present(name) => {
            return match npc_manager.all_npcs().find(|npc| &npc.name == name) {
                Some(npc) if present.contains(&npc.id) => one(npc.id),
                Some(npc) => Resolution::Gone(label(npc.id)),
                None => everyone(),
            };
        }
        DecidedRecipient::All | DecidedRecipient::SomeoneElse => return everyone(),
        DecidedRecipient::Reference(phrase) => {
            if let Some(id) = chosen(phrase) {
                return if present.contains(&id) {
                    one(id)
                } else {
                    Resolution::Gone(label(id))
                };
            }
            // The people the reference fits are the ones it was offered for,
            // whoever has been introduced.
            match reference_candidates(phrase, &present, npc_manager).as_slice() {
                [id] => return one(*id),
                several @ [_, ..] if clarify => {
                    return Resolution::Ask(which_prompt(npc_manager, phrase, several));
                }
                [_, ..] => return everyone(),
                // The room changed since the call: treat it as unsure.
                [] => true,
            }
        }
        DecidedRecipient::NoOne => false,
        DecidedRecipient::Undecided => true,
    };

    // 5. Unaddressed (or unsure): the partner, else ask.
    let with_joins = |first: NpcId, partner: PartnerChange| {
        let mut speakers = vec![(first, Addressing::SpokenTo)];
        speakers.extend(
            mentioned
                .iter()
                .filter(|id| **id != first)
                .map(|id| (*id, Addressing::JoiningMentioned)),
        );
        Resolution::Speak { speakers, partner }
    };
    if let Some((_, id)) = answers.chosen.first() {
        return if present.contains(id) {
            with_joins(*id, PartnerChange::Set(*id))
        } else {
            Resolution::Gone(label(*id))
        };
    }
    if !uncertain && let Some(partner) = npc_manager.partner().filter(|id| present.contains(id)) {
        return with_joins(partner, PartnerChange::Keep);
    }
    if !clarify {
        return everyone();
    }
    Resolution::Ask(who_prompt(npc_manager, &present))
}

/// "Which Connolly do you mean?", listing only the people `reference`
/// fits, by the name the player knows them by.
pub fn which_prompt(
    npc_manager: &NpcManager,
    reference: &str,
    candidates: &[NpcId],
) -> ClarificationPrompt {
    ClarificationPrompt {
        question: format!("Which {} do you mean?", reference.trim()),
        choices: person_choices(npc_manager, candidates),
        reference: Some(reference.to_string()),
    }
}

/// "Who are you speaking to?": each person present by label, then All.
pub fn who_prompt(npc_manager: &NpcManager, present: &[NpcId]) -> ClarificationPrompt {
    let mut choices = person_choices(npc_manager, present);
    choices.push(ClarificationChoice {
        id: ALL_CHOICE_ID.to_string(),
        label: ALL_LABEL.to_string(),
        entity_id: None,
        accessibility_label: Some(ALL_ACCESSIBILITY_LABEL.to_string()),
    });
    ClarificationPrompt {
        question: WHO_QUESTION.to_string(),
        choices,
        reference: None,
    }
}

fn person_choices(npc_manager: &NpcManager, ids: &[NpcId]) -> Vec<ClarificationChoice> {
    let npcs: Vec<&Npc> = ids.iter().filter_map(|id| npc_manager.get(*id)).collect();
    let labels = choice_labels(npc_manager, &npcs);
    npcs.iter()
        .zip(labels)
        .map(|(npc, label)| ClarificationChoice {
            id: format!("choose-npc-{}", npc.id.0),
            label,
            entity_id: Some(npc.id.0.to_string()),
            accessibility_label: None,
        })
        .collect()
}

/// The label of each person offered in a question (issue 2128). Someone
/// introduced is named. The others get the shortest phrase from their
/// authored data that tells them apart from the other strangers offered:
/// "the man" / "the woman", then "the older woman" / "the younger woman",
/// then the occupation as authored, its first letter lower-cased (the
/// canonical world's "the smallholder and cattle drover"), and only then
/// the full description.
/// Each step applies to the strangers who share a noun, so three people can
/// read "the man", "the older woman" and "the younger woman". Age tells
/// exactly two apart; three or more who share a noun go to occupation.
fn choice_labels(npc_manager: &NpcManager, npcs: &[&Npc]) -> Vec<String> {
    let mut labels: Vec<String> = npcs
        .iter()
        .map(|npc| npc_manager.display_name(npc).to_string())
        .collect();
    let strangers: Vec<usize> = (0..npcs.len())
        .filter(|i| !npc_manager.is_introduced(npcs[*i].id))
        .collect();
    let mut nouns: Vec<&str> = Vec::new();
    for i in &strangers {
        let noun = person_noun(npcs[*i]);
        if !nouns.contains(&noun) {
            nouns.push(noun);
        }
    }
    for noun in nouns {
        let group: Vec<usize> = strangers
            .iter()
            .copied()
            .filter(|i| person_noun(npcs[*i]) == noun)
            .collect();
        let occupations: Vec<String> = group
            .iter()
            .map(|i| lowercase_first(npcs[*i].occupation.trim()))
            .collect();
        match group.as_slice() {
            [only] => labels[*only] = format!("the {noun}"),
            [a, b] if npcs[*a].age != npcs[*b].age => {
                let (older, younger) = if npcs[*a].age > npcs[*b].age {
                    (*a, *b)
                } else {
                    (*b, *a)
                };
                labels[older] = format!("the older {noun}");
                labels[younger] = format!("the younger {noun}");
            }
            _ if all_distinct(&occupations) => {
                for (i, occupation) in group.iter().zip(occupations) {
                    labels[*i] = format!("the {occupation}");
                }
            }
            // Nothing shorter tells them apart: the full descriptions stay.
            _ => {}
        }
    }
    // Groups are told apart by their nouns until occupation, which two
    // groups can share ("the farmer" among the men and the women): anyone
    // whose label is still not unique keeps the full description.
    let clashing: Vec<usize> = strangers
        .iter()
        .copied()
        .filter(|i| labels.iter().filter(|l| **l == labels[*i]).count() > 1)
        .collect();
    for i in clashing {
        labels[i] = npc_manager.display_name(npcs[i]).to_string();
    }
    labels
}

/// "man", "woman", "boy", "girl", or "person" / "child" for anyone whose
/// pronouns are neither he nor she.
fn person_noun(npc: &Npc) -> &'static str {
    let child = npc.age < 16;
    let subject = npc.pronouns.split('/').next().unwrap_or("").trim();
    match (subject.to_lowercase().as_str(), child) {
        ("he", false) => "man",
        ("he", true) => "boy",
        ("she", false) => "woman",
        ("she", true) => "girl",
        (_, false) => "person",
        (_, true) => "child",
    }
}

/// "Keeper of the Kilteevan inn" as "keeper of the Kilteevan inn": only the
/// first letter, so proper nouns keep their capitals.
fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn all_distinct(values: &[String]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(i, value)| !value.is_empty() && !values[..i].contains(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::Npc;
    use crate::npc::types::NpcState;

    const HERE: u32 = 3;

    fn person(id: u32, name: &str, occupation: &str, at: u32) -> Npc {
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(id);
        npc.name = name.to_string();
        npc.occupation = occupation.to_string();
        // As authored in `mods/rundale/npcs.json`.
        (npc.pronouns, npc.age) = match id {
            1 => ("she/her".to_string(), 54),
            2 => ("he/him".to_string(), 38),
            3 => ("she/her".to_string(), 31),
            _ => ("he/him".to_string(), 40),
        };
        npc.brief_description = format!("a person called {name}");
        npc.set_location_and_state(crate::world::LocationId(at), NpcState::Present);
        npc
    }

    /// Peig at the Letter Office (2), both Connollys at the cottage (3).
    fn room(here: u32) -> (WorldState, NpcManager) {
        let mut world = WorldState::new();
        world.player_location = crate::world::LocationId(here);
        let mut npcs = NpcManager::new();
        npcs.add_npc(person(1, "Peig Hannigan", "Postmistress", 2));
        npcs.add_npc(person(2, "Mícheál Connolly", "Drover", HERE));
        npcs.add_npc(person(3, "Róisín Connolly", "Spinner", HERE));
        (world, npcs)
    }

    fn decided(recipient: DecidedRecipient, mentioned: &[&str]) -> AddressRequest {
        AddressRequest {
            explicit: Vec::new(),
            decision: Some(RecipientDecision {
                recipient,
                mentioned: mentioned.iter().map(|name| name.to_string()).collect(),
            }),
        }
    }

    fn resolve(world: &WorldState, npcs: &NpcManager, request: &AddressRequest) -> Resolution {
        resolve_recipients(world, npcs, request, Answers::default(), true)
    }

    fn speakers(resolution: &Resolution) -> Vec<(u32, Addressing)> {
        match resolution {
            Resolution::Speak { speakers, .. } => {
                speakers.iter().map(|(id, how)| (id.0, *how)).collect()
            }
            other => panic!("expected speakers, got {other:?}"),
        }
    }

    #[test]
    fn several_explicit_recipients_answer_in_roster_order() {
        let (world, npcs) = room(HERE);
        let request = AddressRequest {
            explicit: vec!["Róisín Connolly".into(), "Mícheál Connolly".into()],
            decision: None,
        };
        assert_eq!(
            speakers(&resolve(&world, &npcs, &request)),
            [(2, Addressing::SpokenTo), (3, Addressing::SpokenTo)]
        );
    }

    #[test]
    fn all_still_means_everyone_when_one_of_two_has_left() {
        let (world, mut npcs) = room(HERE);
        // Róisín leaves while "Who are you speaking to?" waits.
        npcs.get_mut(NpcId(3))
            .unwrap()
            .set_location_and_state(crate::world::LocationId(2), NpcState::Present);
        let answers = Answers {
            chosen: &[],
            everyone: true,
        };
        let request = decided(DecidedRecipient::NoOne, &[]);
        let resolution = resolve_recipients(&world, &npcs, &request, answers, true);
        assert_eq!(speakers(&resolution), [(2, Addressing::PartOfAll)]);
        assert!(matches!(
            resolution,
            Resolution::Speak {
                partner: PartnerChange::End,
                ..
            }
        ));
    }

    #[test]
    fn nobody_present_gets_the_idle_message_whatever_is_said() {
        let (world, npcs) = room(1);
        for recipient in [
            DecidedRecipient::Present("Mícheál Connolly".into()),
            DecidedRecipient::SomeoneElse,
            DecidedRecipient::Undecided,
        ] {
            assert_eq!(
                resolve(&world, &npcs, &decided(recipient, &[])),
                Resolution::Nobody
            );
        }
    }

    #[test]
    fn one_person_present_answers_any_line() {
        let (world, npcs) = room(2);
        for request in [
            decided(DecidedRecipient::NoOne, &[]),
            decided(DecidedRecipient::SomeoneElse, &[]),
            decided(DecidedRecipient::Undecided, &[]),
            AddressRequest::default(),
            AddressRequest {
                explicit: vec!["Mícheál".into()],
                decision: None,
            },
        ] {
            let resolution = resolve(&world, &npcs, &request);
            assert_eq!(speakers(&resolution), [(1, Addressing::SpokenTo)]);
            assert!(matches!(
                resolution,
                Resolution::Speak {
                    partner: PartnerChange::Set(NpcId(1)),
                    ..
                }
            ));
        }
    }

    #[test]
    fn someone_elsewhere_or_unknown_with_two_present_reaches_everyone() {
        let (world, npcs) = room(HERE);
        for request in [
            decided(DecidedRecipient::SomeoneElse, &[]),
            decided(DecidedRecipient::All, &[]),
            AddressRequest {
                explicit: vec!["Peig Hannigan".into()],
                decision: None,
            },
        ] {
            let resolution = resolve(&world, &npcs, &request);
            assert_eq!(
                speakers(&resolution),
                [(2, Addressing::PartOfAll), (3, Addressing::PartOfAll)]
            );
            assert!(matches!(
                resolution,
                Resolution::Speak {
                    partner: PartnerChange::End,
                    ..
                }
            ));
        }
    }

    #[test]
    fn unaddressed_speech_asks_who_with_all_unless_a_partner_is_here() {
        let (world, mut npcs) = room(HERE);
        let request = decided(DecidedRecipient::NoOne, &["Róisín Connolly"]);
        let Resolution::Ask(prompt) = resolve(&world, &npcs, &request) else {
            panic!("asks");
        };
        assert_eq!(prompt.question, WHO_QUESTION);
        let ids: Vec<&str> = prompt.choices.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["choose-npc-2", "choose-npc-3", ALL_CHOICE_ID]);
        assert_eq!(
            prompt.choices[2].accessibility_label.as_deref(),
            Some(ALL_ACCESSIBILITY_LABEL)
        );
        assert_eq!(
            prompt.as_line(),
            "Who are you speaking to: the man, the woman or All?"
        );

        // After the choice, the person mentioned joins unless chosen.
        let chosen = [("the man".to_string(), NpcId(2))];
        let answers = Answers {
            chosen: &chosen,
            everyone: false,
        };
        let resolution = resolve_recipients(&world, &npcs, &request, answers, true);
        assert_eq!(
            speakers(&resolution),
            [(2, Addressing::SpokenTo), (3, Addressing::JoiningMentioned)]
        );

        // A partner present answers, then the mentioned joins.
        npcs.set_partner(Some(NpcId(2)));
        let resolution = resolve(&world, &npcs, &request);
        assert_eq!(
            speakers(&resolution),
            [(2, Addressing::SpokenTo), (3, Addressing::JoiningMentioned)]
        );
        assert!(matches!(
            resolution,
            Resolution::Speak {
                partner: PartnerChange::Keep,
                ..
            }
        ));
        // Unsure asks even with a partner: never a silent default.
        assert!(matches!(
            resolve(&world, &npcs, &decided(DecidedRecipient::Undecided, &[])),
            Resolution::Ask(_)
        ));
    }

    #[test]
    fn a_reference_fitting_several_asks_which_and_the_answer_picks_one() {
        let (world, npcs) = room(HERE);
        let request = decided(DecidedRecipient::Reference("Connolly".into()), &[]);
        let Resolution::Ask(prompt) = resolve(&world, &npcs, &request) else {
            panic!("asks");
        };
        assert_eq!(prompt.question, "Which Connolly do you mean?");
        assert_eq!(prompt.choices.len(), 2);
        let chosen = [("Connolly".to_string(), NpcId(3))];
        let answers = Answers {
            chosen: &chosen,
            everyone: false,
        };
        assert_eq!(
            speakers(&resolve_recipients(&world, &npcs, &request, answers, true)),
            [(3, Addressing::SpokenTo)]
        );
    }

    fn labels(prompt: &ClarificationPrompt) -> Vec<&str> {
        prompt.choices.iter().map(|c| c.label.as_str()).collect()
    }

    fn which(npcs: &NpcManager, ids: &[u32]) -> Vec<String> {
        let ids: Vec<NpcId> = ids.iter().map(|id| NpcId(*id)).collect();
        labels(&which_prompt(npcs, "Connolly", &ids))
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn strangers_are_told_apart_by_the_man_and_the_woman_then_by_name() {
        let (world, mut npcs) = room(HERE);
        let request = decided(DecidedRecipient::Reference("Connolly".into()), &[]);
        let Resolution::Ask(prompt) = resolve(&world, &npcs, &request) else {
            panic!("asks");
        };
        assert_eq!(labels(&prompt), ["the man", "the woman"]);
        assert_eq!(
            prompt.as_line(),
            "Which Connolly do you mean: the man or the woman?"
        );

        // Someone introduced is named; the stranger keeps the short label.
        npcs.mark_introduced(NpcId(2));
        assert_eq!(which(&npcs, &[2, 3]), ["Mícheál Connolly", "the woman"]);
        npcs.mark_introduced(NpcId(3));
        assert_eq!(
            which(&npcs, &[2, 3]),
            ["Mícheál Connolly", "Róisín Connolly"]
        );
    }

    #[test]
    fn two_women_are_told_apart_by_age() {
        let (_, npcs) = room(HERE);
        assert_eq!(
            which(&npcs, &[1, 3]),
            ["the older woman", "the younger woman"]
        );
        assert_eq!(
            which(&npcs, &[3, 1]),
            ["the younger woman", "the older woman"]
        );
        // Three strangers: one man, and the two women by age.
        assert_eq!(
            which(&npcs, &[1, 2, 3]),
            ["the older woman", "the man", "the younger woman"]
        );
    }

    #[test]
    fn two_men_of_an_age_are_told_apart_by_occupation() {
        let (_, mut npcs) = room(HERE);
        npcs.add_npc(person(4, "Mícheál Ó Briain", "Farmer", HERE));
        npcs.get_mut(NpcId(4)).unwrap().age = 38;
        assert_eq!(which(&npcs, &[2, 4]), ["the drover", "the farmer"]);
    }

    #[test]
    fn an_occupation_keeps_its_proper_nouns() {
        let (_, mut npcs) = room(HERE);
        npcs.add_npc(person(
            4,
            "Tomás Ó Briain",
            "Keeper of the Kilteevan inn",
            HERE,
        ));
        npcs.get_mut(NpcId(4)).unwrap().age = 38;
        assert_eq!(
            which(&npcs, &[2, 4]),
            ["the drover", "the keeper of the Kilteevan inn"]
        );
    }

    #[test]
    fn the_full_description_stays_when_nothing_shorter_tells_them_apart() {
        let (_, mut npcs) = room(HERE);
        npcs.add_npc(person(4, "Seán Connolly", "Drover", HERE));
        npcs.get_mut(NpcId(4)).unwrap().age = 38;
        assert_eq!(
            which(&npcs, &[2, 4]),
            [
                "a person called Mícheál Connolly",
                "a person called Seán Connolly"
            ]
        );
    }

    #[test]
    fn an_occupation_shared_across_men_and_women_keeps_the_full_description() {
        let (_, mut npcs) = room(HERE);
        for (id, pronouns, age, occupation) in [
            (4, "he/him", 40, "Farmer"),
            (5, "he/him", 40, "Drover"),
            (6, "she/her", 30, "Farmer"),
            (7, "she/her", 30, "Spinner"),
        ] {
            npcs.add_npc(person(id, &format!("Person {id}"), occupation, HERE));
            let npc = npcs.get_mut(NpcId(id)).unwrap();
            npc.pronouns = pronouns.to_string();
            npc.age = age;
        }
        assert_eq!(
            which(&npcs, &[4, 5, 6, 7]),
            [
                "a person called Person 4",
                "the drover",
                "a person called Person 6",
                "the spinner"
            ]
        );
    }

    #[test]
    fn children_and_people_who_are_neither_he_nor_she_get_their_own_nouns() {
        let (_, mut npcs) = room(HERE);
        for (id, pronouns, age) in [
            (4, "he/him", 9),
            (5, "she/her", 12),
            (6, "they/them", 30),
            (7, "they/them", 8),
        ] {
            npcs.add_npc(person(id, &format!("Person {id}"), "", HERE));
            let npc = npcs.get_mut(NpcId(id)).unwrap();
            npc.pronouns = pronouns.to_string();
            npc.age = age;
        }
        assert_eq!(
            which(&npcs, &[4, 5, 6, 7]),
            ["the boy", "the girl", "the person", "the child"]
        );
    }

    #[test]
    fn a_shared_first_name_asks_which_whoever_has_been_introduced() {
        let (world, mut npcs) = room(HERE);
        npcs.add_npc(person(4, "Mícheál Ó Briain", "Farmer", HERE));
        let request = decided(DecidedRecipient::Reference("Mícheál".into()), &[]);
        for introduced in [&[][..], &[2][..], &[2, 4][..]] {
            for id in introduced {
                npcs.mark_introduced(NpcId(*id));
            }
            let Resolution::Ask(prompt) = resolve(&world, &npcs, &request) else {
                panic!("asks which Mícheál with {introduced:?} introduced");
            };
            assert_eq!(prompt.choices.len(), 2, "{introduced:?}");
        }
    }

    #[test]
    fn a_chosen_person_who_left_is_no_longer_here() {
        let (world, mut npcs) = room(HERE);
        npcs.mark_introduced(NpcId(3));
        npcs.get_mut(NpcId(3))
            .unwrap()
            .set_location_and_state(crate::world::LocationId(1), NpcState::Present);
        let request = decided(DecidedRecipient::Present("Róisín Connolly".into()), &[]);
        assert_eq!(
            resolve(&world, &npcs, &request),
            Resolution::Gone("Róisín Connolly".into())
        );
    }

    #[test]
    fn with_clarification_off_a_question_becomes_everyone() {
        let (world, npcs) = room(HERE);
        let request = decided(DecidedRecipient::NoOne, &[]);
        assert_eq!(
            speakers(&resolve_recipients(
                &world,
                &npcs,
                &request,
                Answers::default(),
                false
            )),
            [(2, Addressing::PartOfAll), (3, Addressing::PartOfAll)]
        );
    }
}
