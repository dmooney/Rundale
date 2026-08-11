//! Typed obligations derived from the player's live utterance.
//!
//! These are intentionally conservative: they cover only explicit current-turn
//! requests that can be checked mechanically without guessing at intent.

use crate::detect_player_name;

pub const PLAYER_REQUESTS_HEADING: &str = "PLAYER REQUESTS TO ANSWER NOW";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogueObligation {
    Referral { referrer: String },
    Name { player_name: String },
    Work,
    Lodging,
}

fn normalized_words(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric() && character != '\'')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn contains_phrase(value: &str, phrase: &str) -> bool {
    let value = normalized_words(value);
    let phrase = normalized_words(phrase);
    !phrase.is_empty() && value.windows(phrase.len()).any(|window| window == phrase)
}

fn routed_utterance(input: &str) -> &str {
    let lower = input.to_lowercase();
    if lower.starts_with("talk to ") {
        lower
            .find(" about ")
            .map(|index| &input[index + " about ".len()..])
            .unwrap_or("")
    } else {
        input
    }
}

fn first_position(input: &str, phrases: &[&str]) -> Option<usize> {
    let lower = input.to_lowercase();
    phrases.iter().filter_map(|phrase| lower.find(phrase)).min()
}

fn find_bounded(value: &str, needle: &str) -> Option<usize> {
    value.match_indices(needle).find_map(|(start, _)| {
        let end = start + needle.len();
        let before = value[..start].chars().next_back();
        let after = value[end..].chars().next();
        let is_name_character = |character: char| character.is_alphanumeric() || character == '\'';
        (!before.is_some_and(is_name_character) && !after.is_some_and(is_name_character))
            .then_some(start)
    })
}

fn unique_person_aliases(known_people: &[String]) -> Vec<(String, String)> {
    let mut aliases = Vec::new();
    for person in known_people {
        let words = normalized_words(person);
        let mut candidates = vec![person.to_lowercase()];
        if let Some(first) = words.first()
            && known_people
                .iter()
                .filter(|known| normalized_words(known).first() == Some(first))
                .count()
                == 1
        {
            candidates.push(first.clone());
        }
        if let Some(last) = words.last()
            && known_people
                .iter()
                .filter(|known| normalized_words(known).last() == Some(last))
                .count()
                == 1
        {
            candidates.push(last.clone());
        }
        candidates.sort();
        candidates.dedup();
        aliases.extend(candidates.into_iter().map(|alias| (alias, person.clone())));
    }
    aliases
}

fn explicit_referral(input: &str, known_people: &[String]) -> Option<(usize, String)> {
    const REFERRAL_PATTERNS: &[&str] = &[
        "sent me",
        "referred me",
        "told me to ask",
        "told me to come",
        "told me to see",
        "told me to speak",
        "said i should ask",
        "said i should come",
        "said i should see",
        "said i should speak",
        "said you might need",
        "said ye might need",
        "recommended i",
        "asked me to",
    ];
    let lower = input.to_lowercase();
    unique_person_aliases(known_people)
        .into_iter()
        // Keep the authored deterministic fallback well below the configured
        // display cap even for malformed mod data.
        .filter(|(_, canonical)| canonical.chars().count() <= 64)
        .filter_map(|(alias, canonical)| {
            let name_position = find_bounded(&lower, &alias)?;
            let tail = &lower[name_position + alias.len()..];
            let forward = REFERRAL_PATTERNS
                .iter()
                .filter_map(|pattern| tail.find(pattern))
                .filter(|offset| *offset <= 48)
                .min()
                .map(|offset| name_position + offset);
            let prefix = &lower[..name_position];
            let reverse = ["sent by ", "referred by "]
                .iter()
                .filter_map(|pattern| prefix.rfind(pattern))
                .filter(|offset| name_position.saturating_sub(*offset) <= 24)
                .min();
            forward.or(reverse).map(|position| (position, canonical))
        })
        .min_by_key(|(position, _)| *position)
}

/// Derive ordered, explicit obligations from the current player utterance.
pub fn derive_dialogue_obligations(
    player_input: &str,
    known_people: &[String],
) -> Vec<DialogueObligation> {
    const WORK_PATTERNS: &[&str] = &[
        "seeking honest work",
        "seeking work",
        "looking for honest work",
        "looking for work",
        "need honest work",
        "need work",
        "want work",
        "find work",
        "work for me",
        "work available",
        "hire me",
        "ready for whatever work",
        "extra hand",
        "pair of hands",
        "after honest work",
        "after work",
        "in search of work",
        "hoping for work",
        "earn my keep",
    ];
    const LODGING_PATTERNS: &[&str] = &[
        "somewhere dry to sleep",
        "somewhere to sleep",
        "somewhere to stay",
        "place to sleep",
        "place to stay",
        "roof for the night",
        "roof for tonight",
        "roof over my head",
        "need a bed",
        "looking for a bed",
        "bed for the night",
        "bed for tonight",
        "need lodging",
        "seeking lodging",
        "need shelter",
        "seeking shelter",
        "sleep tonight",
        "dry place",
        "looking for lodging",
        "after lodging",
        "in need of lodging",
        "lodging for the night",
        "night's lodging",
        "place for the night",
    ];

    let input = routed_utterance(player_input);
    let lower = input.to_lowercase();
    let mut positioned: Vec<(usize, u8, DialogueObligation)> = Vec::new();

    let referral = explicit_referral(input, known_people);
    let work_position = first_position(input, WORK_PATTERNS);
    let lodging_position = first_position(input, LODGING_PATTERNS);
    if let Some((position, referrer)) = referral.as_ref() {
        positioned.push((
            *position,
            0,
            DialogueObligation::Referral {
                referrer: referrer.clone(),
            },
        ));
    }
    // A self-introduction alone is context, not a demand that every response
    // repeat the player's name. Make it an answer obligation only when it
    // anchors a referral or a compound work-and-lodging appeal (#1832).
    let name_is_material =
        referral.is_some() || (work_position.is_some() && lodging_position.is_some());
    if name_is_material
        && let Some(player_name) =
            detect_player_name(input).filter(|name| name.chars().count() <= 64)
        && let Some(position) = lower.find(&player_name.to_lowercase())
    {
        positioned.push((position, 1, DialogueObligation::Name { player_name }));
    }
    if let Some(position) = work_position {
        positioned.push((position, 2, DialogueObligation::Work));
    }
    if let Some(position) = lodging_position {
        positioned.push((position, 3, DialogueObligation::Lodging));
    }

    positioned.sort_by_key(|(position, kind, _)| (*position, *kind));
    positioned
        .into_iter()
        .map(|(_, _, obligation)| obligation)
        .collect()
}

pub fn render_dialogue_obligation_contract(obligations: &[DialogueObligation]) -> String {
    if obligations.is_empty() {
        return String::new();
    }
    let mut block = format!("\n\n{PLAYER_REQUESTS_HEADING} (in order):\n");
    for (index, obligation) in obligations.iter().enumerate() {
        let instruction = match obligation {
            DialogueObligation::Referral { referrer } => format!(
                "REFERRAL: acknowledge that {referrer} sent or referred the player"
            ),
            DialogueObligation::Name { player_name } => {
                format!("NAME: acknowledge the player's stated name, {player_name}")
            }
            DialogueObligation::Work => "WORK: address the request for work; do not claim that you or anyone else is hiring without authored evidence".to_string(),
            DialogueObligation::Lodging => "LODGING: address the request for a bed, shelter, or dry place to sleep; do not claim that any person or place offers lodging without authored evidence".to_string(),
        };
        block.push_str(&format!("{}. {instruction}.\n", index + 1));
    }
    block.push_str(
        "The final spoken dialogue must fulfill EVERY numbered item. A polite greeting, a question back, or an answer to only one facet is incomplete. If you lack grounded help, acknowledge the facet and say so plainly rather than inventing a promise or capability.\n",
    );
    block
}

fn mentions_person(dialogue: &str, person: &str) -> bool {
    if contains_phrase(dialogue, person) {
        return true;
    }
    normalized_words(person)
        .first()
        .is_some_and(|first| contains_phrase(dialogue, first))
}

fn invents_work_capability(dialogue: &str) -> bool {
    let lower = dialogue.to_lowercase();
    [
        "i have work for",
        "i've work for",
        "i can hire",
        "i'll hire",
        "will hire",
        "is hiring",
        "has work for you",
        "give you work",
        "there's work for you",
        "needs a hand",
        "could use an extra hand",
        "work awaits",
        "there is work",
        "there's work",
        "can find you work",
        "will find you work",
        "needs workers",
        "needs labour",
        "needs labor",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

fn invents_lodging_capability(dialogue: &str) -> bool {
    let lower = dialogue.to_lowercase();
    [
        "takes travellers",
        "takes travelers",
        "has a bed",
        "has beds",
        "has a room",
        "has rooms",
        "can put you up",
        "will put you up",
        "can lodge you",
        "will lodge you",
        "you can sleep at",
        "you can stay at",
        "a bed at",
        "bed is ready",
        "bed awaits",
        "room is ready",
        "there is a bed",
        "there's a bed",
        "spare bed",
        "can find you lodging",
        "will find you lodging",
        "offers lodging",
        "provides lodging",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

fn is_noncommittal(dialogue: &str) -> bool {
    [
        "cannot",
        "can't",
        "could not say",
        "do not know",
        "don't know",
        "no promise",
        "understand",
        "seeking",
        "looking for",
        "asking for",
        "need work",
        "need a bed",
        "need lodging",
        "no work",
        "no bed",
        "no lodging",
        "offer no",
        "have no",
    ]
    .iter()
    .any(|marker| contains_phrase(dialogue, marker))
}

/// True only when the final player-visible line acknowledges every recognized
/// facet without inventing hiring or lodging capability.
pub fn dialogue_fulfills_obligations(dialogue: &str, obligations: &[DialogueObligation]) -> bool {
    obligations.iter().all(|obligation| match obligation {
        DialogueObligation::Referral { referrer } => {
            mentions_person(dialogue, referrer)
                && ["sent", "referred", "told", "word", "hear", "know"]
                    .iter()
                    .any(|marker| contains_phrase(dialogue, marker))
        }
        DialogueObligation::Name { player_name } => mentions_person(dialogue, player_name),
        DialogueObligation::Work => {
            is_noncommittal(dialogue)
                && !invents_work_capability(dialogue)
                && [
                    "work",
                    "job",
                    "labour",
                    "labor",
                    "hire",
                    "employment",
                    "earning",
                    "farm hand",
                    "extra hand",
                    "pair of hands",
                ]
                .iter()
                .any(|marker| contains_phrase(dialogue, marker))
        }
        DialogueObligation::Lodging => {
            is_noncommittal(dialogue)
                && !invents_lodging_capability(dialogue)
                && [
                    "lodging",
                    "bed",
                    "roof",
                    "shelter",
                    "sleep",
                    "stay",
                    "night",
                    "dry place",
                ]
                .iter()
                .any(|marker| contains_phrase(dialogue, marker))
        }
    })
}

/// Authored, noncommittal replacement that covers every recognized facet.
pub fn dialogue_obligation_fallback(obligations: &[DialogueObligation]) -> String {
    if obligations.is_empty() {
        return crate::INVALID_DIALOGUE_FALLBACK.to_string();
    }
    obligations
        .iter()
        .map(|obligation| match obligation {
            DialogueObligation::Referral { referrer } => {
                format!("I hear that {referrer} sent you.")
            }
            DialogueObligation::Name { player_name } => format!("{player_name}, is it? I have it."),
            DialogueObligation::Work => {
                "I cannot promise work, but I understand you are seeking it.".to_string()
            }
            DialogueObligation::Lodging => {
                "I cannot promise lodging, but I understand you need a dry place to sleep."
                    .to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> Vec<String> {
        vec![
            "Peig Hannigan".to_string(),
            "Fr. Declan Tierney".to_string(),
            "Siobhan Murphy".to_string(),
        ]
    }

    #[test]
    fn exact_issue_line_derives_four_ordered_obligations() {
        let obligations = derive_dialogue_obligations(
            "Good morning, Father. Peig Hannigan sent me. I'm Aiden Carney, seeking honest work and somewhere dry to sleep.",
            &people(),
        );
        assert_eq!(
            obligations,
            vec![
                DialogueObligation::Referral {
                    referrer: "Peig Hannigan".to_string()
                },
                DialogueObligation::Name {
                    player_name: "Aiden Carney".to_string()
                },
                DialogueObligation::Work,
                DialogueObligation::Lodging,
            ]
        );
    }

    #[test]
    fn declarative_paraphrases_are_recognized_but_topic_mentions_are_not() {
        let obligations = derive_dialogue_obligations(
            "Peig told me to speak with you. My name is Maeve Byrne. I need work and a roof for tonight.",
            &people(),
        );
        assert_eq!(obligations.len(), 4);
        assert!(matches!(
            derive_dialogue_obligations(
                "I was sent by Peig. Call me Maeve Byrne; I need work and a bed for tonight.",
                &people(),
            )
            .first(),
            Some(DialogueObligation::Referral { referrer }) if referrer == "Peig Hannigan"
        ));
        assert!(
            derive_dialogue_obligations("Tell me about your work at the forge.", &people())
                .is_empty()
        );
        assert!(
            derive_dialogue_obligations(
                "Peig said the road is wet. I slept under a dry roof.",
                &people()
            )
            .is_empty()
        );
        assert!(derive_dialogue_obligations("What is your name?", &people()).is_empty());
        assert!(
            derive_dialogue_obligations(
                "I am Aiden Carney, a cooper newly arrived in Kilteevan. Might there be work here?",
                &people(),
            )
            .is_empty()
        );
        assert!(
            derive_dialogue_obligations(
                "Hannigan sent me to mend a wall.",
                &["Ann Moore".to_string()],
            )
            .is_empty()
        );
    }

    #[test]
    fn fulfillment_requires_every_facet_and_refuses_invented_capabilities() {
        let obligations = derive_dialogue_obligations(
            "Peig sent me. I'm Aiden Carney, seeking work and somewhere to sleep.",
            &people(),
        );
        assert!(!dialogue_fulfills_obligations(
            "Aye, I know Peig. What brings ye here?",
            &obligations,
        ));
        assert!(dialogue_fulfills_obligations(
            "I hear Peig sent you, Aiden. I cannot promise work, and I cannot promise a bed, but I understand both needs.",
            &obligations,
        ));
        assert!(!dialogue_fulfills_obligations(
            "Peig sent you, Aiden. Siobhan is hiring and Darcy's Pub has rooms.",
            &obligations,
        ));
        assert!(!dialogue_fulfills_obligations(
            "Peig sent you, Aiden. Work awaits you, and a spare bed is ready.",
            &obligations,
        ));
    }

    #[test]
    fn fallback_and_prompt_cover_every_obligation_in_order() {
        let obligations = derive_dialogue_obligations(
            "Peig sent me. I'm Aiden Carney, seeking work and somewhere to sleep.",
            &people(),
        );
        let contract = render_dialogue_obligation_contract(&obligations);
        assert!(contract.contains(PLAYER_REQUESTS_HEADING));
        assert!(contract.find("REFERRAL").unwrap() < contract.find("NAME").unwrap());
        assert!(contract.find("NAME").unwrap() < contract.find("WORK").unwrap());
        assert!(contract.find("WORK").unwrap() < contract.find("LODGING").unwrap());
        let fallback = dialogue_obligation_fallback(&obligations);
        assert!(dialogue_fulfills_obligations(&fallback, &obligations));
        assert!(!fallback.contains("is hiring"));
        assert!(!fallback.contains("has rooms"));
    }
}
