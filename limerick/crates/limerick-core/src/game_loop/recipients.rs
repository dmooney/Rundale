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
use crate::intent_decision::{DecidedRecipient, RecipientDecision};
use crate::npc::NpcId;
use crate::npc::manager::{NpcManager, NpcReference};
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
            match npc_manager.resolve_reference_at(phrase, here) {
                NpcReference::Unique(id) => return one(id),
                NpcReference::Ambiguous(candidates) if clarify => {
                    return Resolution::Ask(which_prompt(npc_manager, phrase, &candidates));
                }
                NpcReference::Ambiguous(_) => return everyone(),
                // The room changed since the call: treat it as unsure.
                NpcReference::NotFound => true,
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
    if answers.everyone {
        return everyone();
    }
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
        choices: candidates
            .iter()
            .filter_map(|id| npc_manager.get(*id))
            .map(|npc| person_choice(npc_manager, npc))
            .collect(),
        reference: Some(reference.to_string()),
    }
}

/// "Who are you speaking to?": each person present by label, then All.
pub fn who_prompt(npc_manager: &NpcManager, present: &[NpcId]) -> ClarificationPrompt {
    let mut choices: Vec<ClarificationChoice> = present
        .iter()
        .filter_map(|id| npc_manager.get(*id))
        .map(|npc| person_choice(npc_manager, npc))
        .collect();
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

fn person_choice(npc_manager: &NpcManager, npc: &crate::npc::Npc) -> ClarificationChoice {
    ClarificationChoice {
        id: format!("choose-npc-{}", npc.id.0),
        label: npc_manager.display_name(npc).to_string(),
        entity_id: Some(npc.id.0.to_string()),
        accessibility_label: None,
    }
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
            "Who are you speaking to: a person called Mícheál Connolly, a person called \
             Róisín Connolly or All?"
        );

        // After the choice, the person mentioned joins unless chosen.
        let chosen = [("a person called Mícheál Connolly".to_string(), NpcId(2))];
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
