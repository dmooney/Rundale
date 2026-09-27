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

/// Immutable authored occupation/workplace facts used to answer work-seeking
/// appeals without turning a roster entry into a claim that anyone is hiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundedWorkFact {
    /// Canonical authored display name.
    pub name: String,
    /// Canonical authored occupation, never inferred from model text.
    pub occupation: String,
    /// Canonical authored workplace name when the NPC has one.
    pub workplace: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkReferralKind {
    General,
    Farmer,
    Tradesperson,
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

fn requests_work_referral(input: &str) -> bool {
    let lower = input.to_lowercase();
    let seeks_work = [
        "looking for honest work",
        "looking for work",
        "need honest work",
        "need work",
        "seeking honest work",
        "seeking work",
        "needing a hand",
        "needs a hand",
        "farmer or tradesperson",
        "farmer or tradesman",
        "who should i ask",
        "who might i ask",
        "anyone needing",
        "anyone who needs",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase));
    let seeks_person = [
        "anyone",
        "which farmer",
        "what farmer",
        "farmer or tradesperson",
        "farmer or tradesman",
        "who should i ask",
        "who might i ask",
        "who can i ask",
        "tell me plainly which",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase));
    seeks_work && seeks_person
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
    // This contract protects declarative, multi-facet appeals such as the
    // issue report's referral + name + work + lodging introduction. Ordinary
    // questions continue through their established intent-specific paths. In
    // particular, treating "Is there work for me?" as a noncommittal-answer
    // obligation would erase an otherwise grounded task assignment at the
    // canonical apply seam.
    if input.trim_end().ends_with('?') && !requests_work_referral(input) {
        return Vec::new();
    }
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
            DialogueObligation::Work => "WORK: answer a request for work or a work referral with a canonically suitable authored occupation/workplace lead when one is known; make clear that this is guidance, not a claim that anyone is hiring; otherwise state the uncertainty plainly".to_string(),
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
pub fn dialogue_fulfills_obligations(
    dialogue: &str,
    obligations: &[DialogueObligation],
    player_input: &str,
    work_roster: &[GroundedWorkFact],
) -> bool {
    obligations.iter().all(|obligation| match obligation {
        DialogueObligation::Referral { referrer } => {
            mentions_person(dialogue, referrer)
                && ["sent", "referred", "told", "word", "hear", "know"]
                    .iter()
                    .any(|marker| contains_phrase(dialogue, marker))
        }
        DialogueObligation::Name { player_name } => mentions_person(dialogue, player_name),
        DialogueObligation::Work => {
            let grounded_referral = grounded_work_referral(player_input, work_roster);
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
                && (!requests_work_referral(player_input)
                    || grounded_referral.is_none()
                    || names_suitable_grounded_referral(dialogue, player_input, work_roster))
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

fn work_referral_kind(player_input: &str) -> WorkReferralKind {
    let lower = player_input.to_lowercase();
    if lower.contains("farmer") {
        WorkReferralKind::Farmer
    } else if (lower.contains("tradesperson") || lower.contains("tradesman"))
        && !lower.contains("farmer")
    {
        WorkReferralKind::Tradesperson
    } else {
        WorkReferralKind::General
    }
}

fn is_grounded_work_candidate(fact: &GroundedWorkFact, kind: WorkReferralKind) -> bool {
    let occupation = fact.occupation.to_lowercase();
    if occupation.contains("retired")
        || occupation.contains("child")
        || occupation.contains("widow")
        || occupation.contains("wife")
        || occupation.contains("daughter")
        || occupation.contains("son")
        || fact.name.trim().is_empty()
        || fact.occupation.trim().is_empty()
    {
        return false;
    }
    let is_farmer = occupation.contains("farmer") || occupation.contains("farm boy");
    let is_trade = [
        "blacksmith",
        "miller",
        "weaver",
        "shopkeeper",
        "publican",
        "boatman",
        "labourer",
        "clerk",
    ]
    .iter()
    .any(|trade| occupation.contains(trade));
    match kind {
        WorkReferralKind::Farmer => is_farmer,
        WorkReferralKind::Tradesperson => is_trade,
        WorkReferralKind::General => is_farmer || is_trade,
    }
}

fn grounded_work_referral<'a>(
    player_input: &str,
    work_roster: &'a [GroundedWorkFact],
) -> Option<&'a GroundedWorkFact> {
    let kind = work_referral_kind(player_input);
    let find = |kind| {
        work_roster
            .iter()
            .find(|fact| is_grounded_work_candidate(fact, kind))
    };
    match kind {
        WorkReferralKind::General => {
            find(WorkReferralKind::Farmer).or_else(|| find(WorkReferralKind::Tradesperson))
        }
        specific => find(specific),
    }
}

fn names_suitable_grounded_referral(
    dialogue: &str,
    player_input: &str,
    work_roster: &[GroundedWorkFact],
) -> bool {
    let kind = work_referral_kind(player_input);
    work_roster.iter().any(|fact| {
        let suitable = match kind {
            WorkReferralKind::General => {
                is_grounded_work_candidate(fact, WorkReferralKind::Farmer)
                    || is_grounded_work_candidate(fact, WorkReferralKind::Tradesperson)
            }
            specific => is_grounded_work_candidate(fact, specific),
        };
        suitable && mentions_person(dialogue, &fact.name)
    })
}

fn grounded_work_guidance(player_input: &str, work_roster: &[GroundedWorkFact]) -> String {
    let Some(fact) = grounded_work_referral(player_input, work_roster) else {
        return "I know no suitable worker to name from what is certain, and I cannot promise work."
            .to_string();
    };
    let workplace = fact
        .workplace
        .as_deref()
        .filter(|place| !place.trim().is_empty())
        .map(|place| format!(" at {place}"))
        .unwrap_or_default();
    format!(
        "You could ask {}, the {}{}; I cannot say whether they can offer work.",
        fact.name, fact.occupation, workplace
    )
}

/// Authored, noncommittal replacement that covers every recognized facet.
pub fn dialogue_obligation_fallback(
    obligations: &[DialogueObligation],
    player_input: &str,
    work_roster: &[GroundedWorkFact],
) -> String {
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
                if requests_work_referral(player_input) {
                    grounded_work_guidance(player_input, work_roster)
                } else {
                    "I cannot promise work, but I understand you are seeking it.".to_string()
                }
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
        assert!(derive_dialogue_obligations("Is there work for me?", &people()).is_empty());
        assert!(
            derive_dialogue_obligations("Did Peig Hannigan send me to ask about work?", &people(),)
                .is_empty()
        );
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
        let input = "Peig sent me. I'm Aiden Carney, seeking work and somewhere to sleep.";
        let obligations = derive_dialogue_obligations(input, &people());
        assert!(!dialogue_fulfills_obligations(
            "Aye, I know Peig. What brings ye here?",
            &obligations,
            input,
            &[],
        ));
        assert!(dialogue_fulfills_obligations(
            "I hear Peig sent you, Aiden. I cannot promise work, and I cannot promise a bed, but I understand both needs.",
            &obligations,
            input,
            &[],
        ));
        assert!(!dialogue_fulfills_obligations(
            "Peig sent you, Aiden. Siobhan is hiring and Darcy's Pub has rooms.",
            &obligations,
            input,
            &[],
        ));
        assert!(!dialogue_fulfills_obligations(
            "Peig sent you, Aiden. Work awaits you, and a spare bed is ready.",
            &obligations,
            input,
            &[],
        ));
    }

    #[test]
    fn fallback_and_prompt_cover_every_obligation_in_order() {
        let input = "Peig sent me. I'm Aiden Carney, seeking work and somewhere to sleep.";
        let obligations = derive_dialogue_obligations(input, &people());
        let contract = render_dialogue_obligation_contract(&obligations);
        assert!(contract.contains(PLAYER_REQUESTS_HEADING));
        assert!(contract.find("REFERRAL").unwrap() < contract.find("NAME").unwrap());
        assert!(contract.find("NAME").unwrap() < contract.find("WORK").unwrap());
        assert!(contract.find("WORK").unwrap() < contract.find("LODGING").unwrap());
        let fallback = dialogue_obligation_fallback(&obligations, input, &[]);
        assert!(dialogue_fulfills_obligations(
            &fallback,
            &obligations,
            input,
            &[]
        ));
        assert!(!fallback.contains("is hiring"));
        assert!(!fallback.contains("has rooms"));
    }

    #[test]
    fn exact_work_referral_appeals_require_grounded_useful_guidance() {
        let roster = vec![
            GroundedWorkFact {
                name: "Peig Hannigan".to_string(),
                occupation: "Widow".to_string(),
                workplace: None,
            },
            GroundedWorkFact {
                name: "Siobhan Murphy".to_string(),
                occupation: "Farmer".to_string(),
                workplace: Some("Murphy's Farm".to_string()),
            },
            GroundedWorkFact {
                name: "Seamus Gallagher".to_string(),
                occupation: "Blacksmith".to_string(),
                workplace: Some("The Forge".to_string()),
            },
        ];
        for input in [
            "Good morning. I'm Eilis Byrne, newly arrived and looking for honest work. Is there anyone needing a hand today?",
            "I came to Kilteevan because I need honest work. Tell me plainly which farmer or tradesperson I should ask for a task today.",
        ] {
            let obligations = derive_dialogue_obligations(input, &people());
            assert_eq!(obligations, [DialogueObligation::Work]);
            assert!(!dialogue_fulfills_obligations(
                "I cannot promise work, but I understand you are seeking it.",
                &obligations,
                input,
                &roster,
            ));
            let fallback = dialogue_obligation_fallback(&obligations, input, &roster);
            assert!(fallback.contains("Siobhan Murphy"), "{fallback}");
            assert!(fallback.contains("Farmer at Murphy's Farm"), "{fallback}");
            assert!(fallback.contains("cannot say"), "{fallback}");
            assert!(!fallback.contains("hiring"), "{fallback}");
            assert!(dialogue_fulfills_obligations(
                &fallback,
                &obligations,
                input,
                &roster,
            ));
            if !input.contains("farmer") {
                assert!(dialogue_fulfills_obligations(
                    "You could ask Seamus Gallagher, the Blacksmith at The Forge; I do not know whether he has work.",
                    &obligations,
                    input,
                    &roster,
                ));
            }
        }
        let input = "I need work; who should I ask?";
        let obligations = derive_dialogue_obligations(input, &people());
        let uncertain = dialogue_obligation_fallback(&obligations, input, &[]);
        assert!(uncertain.contains("no suitable worker"), "{uncertain}");
        assert!(dialogue_fulfills_obligations(
            &uncertain,
            &obligations,
            input,
            &[],
        ));
        assert!(derive_dialogue_obligations("Is there work for me?", &people()).is_empty());
    }
}
