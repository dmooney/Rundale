//! Canonical validation for untrusted Tier-1 dialogue candidates.
//!
//! Prompt instructions improve model behaviour; they are not an authorization
//! boundary. This module is the one side-effect-free apply gate used before model
//! text or metadata may affect memory, events, state, or player-visible UI.

use parish_types::TimeOfDay;

use crate::{
    DialogueObligation, DialogueSpeakerContext, GroundedWorkFact, NpcResponseParseDisposition,
    NpcStreamResponse, RelationshipToneHint, dedupe_cross_npc_openers,
    dialogue_fulfills_obligations, dialogue_obligation_fallback,
    guard_acquaintance_question_intent_drift, guard_direct_evidence_evasion,
    guard_fabricated_person_confirmation_with_locations, guard_fabricated_person_routing,
    guard_false_denial_of_known_place, guard_false_denial_of_roster_person_with_speaker,
    guard_invented_place_confirmation, guard_mood_register, guard_presumed_prior_acquaintance,
    guard_priest_tenure_drift, guard_repeated_speaker_name, guard_rival_target_neutral_tone,
    guard_stock_nonrecognition_decline_with_speaker, guard_time_of_day_phrase,
    guard_unfounded_first_contact_familiarity, guard_verbosity_runons,
    guard_verbosity_runons_with_mood, guard_work_recommendation, guard_wrong_location_reference,
    guard_wrong_speaker_identity,
};

/// Authored person facts available to the canonical dialogue claim validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundedPersonFact {
    pub name: String,
    pub occupation: String,
    pub workplace: Option<String>,
    pub current_location: Option<String>,
}

/// Authored geography available to the canonical dialogue claim validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundedLocationFact {
    pub name: String,
    /// Locations that may truthfully be described as containing, adjoining,
    /// or anchoring this location (authored graph connections/relative anchor).
    pub nearby_locations: Vec<String>,
}

/// The semantic type of a player-introduced referent absent from authored data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogueReferentKind {
    UnknownPerson,
    UnknownPlace,
}

/// One unresolved player-introduced referent retained across a local exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueReferent {
    pub kind: DialogueReferentKind,
    pub label: String,
}

/// Small per-conversation context for pronoun/appositive follow-ups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogueReferentContext {
    referents: Vec<DialogueReferent>,
}

impl DialogueReferentContext {
    const MAX_REFERENTS: usize = 4;

    /// Observe explicit referents in a player line. Known authored names clear
    /// stale unknowns of the same type. New unknowns remain together in the
    /// bounded context so pronouns are disabled whenever their referent would
    /// be ambiguous.
    pub fn observe_player_input(
        &mut self,
        input: &str,
        known_people: &[String],
        known_places: &[String],
        player_name: Option<&str>,
    ) {
        // Real-loop commands retain their routing prefix (for example,
        // `talk to Padraig about ...`). The addressed NPC is not the subject
        // of the player's factual question and must not clear an unresolved
        // referent from the prior turn.
        let input = routed_utterance(input);
        let people = extract_unknown_people(input, known_people, known_places, player_name);
        let places = extract_unknown_common_noun_places(input, known_places);

        if !people.is_empty() {
            self.extend(DialogueReferentKind::UnknownPerson, people);
        } else if contains_any_authored_name(input, known_people) {
            self.referents
                .retain(|referent| referent.kind != DialogueReferentKind::UnknownPerson);
        }

        if !places.is_empty() {
            self.extend(DialogueReferentKind::UnknownPlace, places);
        } else if contains_any_authored_name(input, known_places) {
            self.referents
                .retain(|referent| referent.kind != DialogueReferentKind::UnknownPlace);
        }
    }

    fn extend(&mut self, kind: DialogueReferentKind, labels: Vec<String>) {
        for label in labels {
            if !self.referents.iter().any(|referent| {
                referent.kind == kind && normalize(&referent.label) == normalize(&label)
            }) {
                self.referents.push(DialogueReferent { kind, label });
            }
        }
        if self.referents.len() > Self::MAX_REFERENTS {
            self.referents
                .drain(0..self.referents.len() - Self::MAX_REFERENTS);
        }
    }

    fn unambiguous(&self, kind: DialogueReferentKind) -> Option<&DialogueReferent> {
        let mut matching = self
            .referents
            .iter()
            .filter(|referent| referent.kind == kind);
        let referent = matching.next()?;
        matching.next().is_none().then_some(referent)
    }
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

/// Immutable authored facts captured before inference starts.
#[derive(Debug, Clone)]
pub struct DialogueGroundingSnapshot {
    pub speaker_name: String,
    pub speaker_context: Option<DialogueSpeakerContext>,
    pub canonical_mood: String,
    pub had_prior_exchange: bool,
    pub time_of_day: TimeOfDay,
    pub known_person_names: Vec<String>,
    pub roster_names_occupations: Vec<(String, String)>,
    pub current_location_name: String,
    pub known_location_names: Vec<String>,
    pub player_name: Option<String>,
    pub work_roster: Vec<GroundedWorkFact>,
    pub relationship_tone_hints: Vec<RelationshipToneHint>,
    pub prior_player_inputs: Vec<String>,
    pub forbidden_output_terms: Vec<String>,
    pub prior_openers: Vec<String>,
    pub current_festival: Option<String>,
    pub person_facts: Vec<GroundedPersonFact>,
    pub location_facts: Vec<GroundedLocationFact>,
    pub referent_context: DialogueReferentContext,
    /// Explicit facets derived from the exact player utterance before inference.
    pub dialogue_obligations: Vec<DialogueObligation>,
}

impl Default for DialogueGroundingSnapshot {
    fn default() -> Self {
        Self {
            speaker_name: String::new(),
            speaker_context: None,
            canonical_mood: String::new(),
            had_prior_exchange: false,
            time_of_day: TimeOfDay::Morning,
            known_person_names: Vec::new(),
            roster_names_occupations: Vec::new(),
            current_location_name: String::new(),
            known_location_names: Vec::new(),
            player_name: None,
            work_roster: Vec::new(),
            relationship_tone_hints: Vec::new(),
            prior_player_inputs: Vec::new(),
            forbidden_output_terms: Vec::new(),
            prior_openers: Vec::new(),
            current_festival: None,
            person_facts: Vec::new(),
            location_facts: Vec::new(),
            referent_context: DialogueReferentContext::default(),
            dialogue_obligations: Vec::new(),
        }
    }
}

/// Default-on semantic guard policy. Callers may preserve existing kill
/// switches, but cannot bypass response-contract or anachronism rejection.
#[derive(Debug, Clone, Copy)]
pub struct DialogueValidationPolicy {
    pub person_confirmation: bool,
    pub person_routing: bool,
    pub wrong_location: bool,
    pub false_denial: bool,
    pub invented_place: bool,
    pub polish: bool,
    pub verbosity: bool,
    pub mood_sentence_cap: bool,
    pub wrong_speaker: bool,
    pub acquaintance_intent: bool,
    pub anti_repetition: bool,
}

impl Default for DialogueValidationPolicy {
    fn default() -> Self {
        Self {
            person_confirmation: true,
            person_routing: true,
            wrong_location: true,
            false_denial: true,
            invented_place: true,
            polish: true,
            verbosity: true,
            mood_sentence_cap: true,
            wrong_speaker: true,
            acquaintance_intent: true,
            anti_repetition: true,
        }
    }
}

/// Result of the single canonical validation pass.
#[derive(Debug, Clone)]
pub struct DialogueValidationOutcome {
    pub response: NpcStreamResponse,
    pub contract_valid: bool,
    pub accepted: bool,
    pub guard_reasons: Vec<String>,
}

/// Canonical deterministic line used for every rejected Tier-1 candidate.
pub const INVALID_DIALOGUE_FALLBACK: &str = "I beg your pardon; I lost the thread of that.";

fn contains_forbidden_term(text: &str, terms: &[String]) -> bool {
    fn words(value: &str) -> Vec<String> {
        value
            .split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect()
    }

    let candidate_words = words(text);
    terms.iter().any(|term| {
        let term_words = words(term);
        if term_words.is_empty() {
            return false;
        }
        candidate_words.windows(term_words.len()).any(|window| {
            window.iter().enumerate().all(|(index, actual)| {
                let expected = &term_words[index];
                actual == expected
                    || (index + 1 == term_words.len()
                        && (actual == &format!("{expected}s")
                            || actual == &format!("{expected}es")))
            })
        })
    })
}

fn candidate_contains_forbidden_term(candidate: &NpcStreamResponse, terms: &[String]) -> bool {
    contains_forbidden_term(&candidate.dialogue, terms)
        || candidate.metadata.as_ref().is_some_and(|metadata| {
            contains_forbidden_term(&metadata.action, terms)
                || metadata
                    .internal_thought
                    .as_deref()
                    .is_some_and(|value| contains_forbidden_term(value, terms))
                || metadata
                    .assigned_task
                    .as_deref()
                    .is_some_and(|value| contains_forbidden_term(value, terms))
                || metadata
                    .mentioned_people
                    .iter()
                    .any(|value| contains_forbidden_term(value, terms))
                || metadata.language_hints.iter().any(|hint| {
                    contains_forbidden_term(&hint.word, terms)
                        || contains_forbidden_term(&hint.pronunciation, terms)
                        || hint
                            .meaning
                            .as_deref()
                            .is_some_and(|value| contains_forbidden_term(value, terms))
                })
        })
}

fn normalize(value: &str) -> String {
    value
        .split(|character: char| !character.is_alphanumeric() && character != '\'')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    let text = format!(" {} ", normalize(text));
    let phrase = normalize(phrase);
    !phrase.is_empty() && text.contains(&format!(" {phrase} "))
}

fn contains_any_authored_name(text: &str, names: &[String]) -> bool {
    names.iter().any(|name| contains_phrase(text, name))
}

fn extract_title_case_names(input: &str) -> Vec<String> {
    let words: Vec<&str> = input.split_whitespace().collect();
    let mut candidates = Vec::new();
    for index in 0..words.len() {
        let first = words[index].trim_matches(|character: char| !character.is_alphabetic());
        if first.len() < 2 || !first.chars().next().is_some_and(char::is_uppercase) {
            continue;
        }
        for width in (2..=3).rev() {
            if index + width > words.len() {
                continue;
            }
            let parts: Vec<&str> = words[index..index + width]
                .iter()
                .map(|word| word.trim_matches(|character: char| !character.is_alphabetic()))
                .collect();
            if parts.iter().all(|part| {
                part.len() >= 2
                    && part.chars().next().is_some_and(char::is_uppercase)
                    && part
                        .chars()
                        .all(|character| character.is_alphabetic() || character == '\'')
            }) {
                candidates.push(parts.join(" "));
                break;
            }
        }
    }
    candidates
}

fn extract_unknown_people(
    input: &str,
    known_people: &[String],
    known_places: &[String],
    player_name: Option<&str>,
) -> Vec<String> {
    let player_name = player_name.map(normalize);
    let mut candidates: Vec<String> = extract_title_case_names(input)
        .into_iter()
        .filter(|candidate| {
            let normalized = normalize(candidate);
            let first = normalized.split_whitespace().next().unwrap_or_default();
            player_name.as_deref() != Some(normalized.as_str())
                && !matches!(
                    first,
                    "and" | "but" | "do" | "does" | "good" | "have" | "is" | "tell" | "where"
                )
                && !known_people
                    .iter()
                    .any(|known| normalize(known) == normalized)
                && !known_places
                    .iter()
                    .any(|known| normalize(known).contains(&normalized))
                && !matches!(
                    normalized.as_str(),
                    "saint brigid" | "st brigid" | "good morning" | "good evening"
                )
        })
        .collect();
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.split_whitespace().count()));
    let mut selected: Vec<String> = Vec::new();
    for candidate in candidates {
        let normalized = normalize(&candidate);
        if !selected
            .iter()
            .any(|existing| normalize(existing).contains(&normalized))
        {
            selected.push(candidate);
        }
    }
    selected
}

fn extract_unknown_common_noun_places(input: &str, known_places: &[String]) -> Vec<String> {
    const PLACE_HEADS: &[&str] = &[
        "abbey",
        "castle",
        "chapel",
        "church",
        "farm",
        "forge",
        "house",
        "inn",
        "mill",
        "monastery",
        "pub",
        "ruins",
        "tavern",
        "tower",
        "village",
        "well",
    ];
    const PLACE_MODIFIERS: &[&str] = &[
        "abandoned",
        "ancient",
        "burned",
        "burnt",
        "haunted",
        "old",
        "ruined",
        "roofless",
    ];

    let words: Vec<String> = input
        .split_whitespace()
        .map(normalize)
        .filter(|word| !word.is_empty())
        .collect();
    let mut candidates = Vec::new();
    for index in 0..words.len() {
        let word = words[index].as_str();
        let candidate = if PLACE_HEADS.contains(&word)
            && index > 0
            && PLACE_MODIFIERS.contains(&words[index - 1].as_str())
        {
            Some(format!("{} {}", words[index - 1], word))
        } else if PLACE_MODIFIERS.contains(&word)
            && index + 1 < words.len()
            && PLACE_HEADS.contains(&words[index + 1].as_str())
        {
            Some(format!("{} {}", word, words[index + 1]))
        } else {
            None
        };
        if let Some(candidate) = candidate
            && !known_places.iter().any(|known| {
                let known = normalize(known);
                known.contains(&candidate) || candidate.contains(&known)
            })
            && !candidates.contains(&candidate)
        {
            candidates.push(candidate);
        }
    }
    candidates
}

fn has_denial(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "don't know",
        "do not know",
        "never heard",
        "no such",
        "cannot say",
        "can't say",
        "not heard",
        "know nothing",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn affirms_unknown_person(dialogue: &str, referent: &str) -> bool {
    if has_denial(dialogue) {
        return false;
    }
    let lower = dialogue.to_lowercase();
    let names_referent = contains_phrase(dialogue, referent)
        || referent
            .split_whitespace()
            .next()
            .is_some_and(|first| contains_phrase(dialogue, first));
    let affirmation = [
        "i've seen",
        "i have seen",
        "was here",
        "he was",
        "she was",
        "he is",
        "she is",
        "he's",
        "she's",
        "made for",
        "went to",
        "headed",
        "you'll find",
        "you will find",
        "my cousin",
        "your cousin",
        "yer cousin",
        "the lad",
        "the woman",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    affirmation
        && (names_referent
            || [" he ", " she ", " him ", " her "]
                .iter()
                .any(|pronoun| format!(" {lower} ").contains(pronoun)))
}

fn affirms_unknown_place(dialogue: &str, referent: &str) -> bool {
    if has_denial(dialogue) {
        return false;
    }
    let lower = dialogue.to_lowercase();
    let affirmation = [
        "the ruins",
        "the abbey",
        "walk to",
        "road to",
        "path to",
        "lies to",
        "stands to",
        "you'll find",
        "you will find",
        "past the",
        "stones",
        "swallowed by",
        "it is near",
        "it's near",
        "there is",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    affirmation
        && (contains_phrase(dialogue, referent)
            || lower.contains("the ruins")
            || lower.contains("the abbey")
            || lower.contains(" there"))
}

fn current_festival_claim(dialogue: &str) -> Option<&'static str> {
    let lower = dialogue.to_lowercase();
    let current_marker = [
        "on this day",
        "this very day",
        "today",
        "today's",
        "today is",
        "we celebrate",
        "we're celebrating",
        "we are celebrating",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if !current_marker {
        return None;
    }
    if lower.contains("saint brigid") || lower.contains("st brigid") || lower.contains("imbolc") {
        Some("imbolc")
    } else if lower.contains("bealtaine") {
        Some("bealtaine")
    } else if lower.contains("lughnasa") {
        Some("lughnasa")
    } else if lower.contains("samhain") {
        Some("samhain")
    } else {
        None
    }
}

fn queried_occupation<'a>(input: &str, facts: &'a [GroundedPersonFact]) -> Option<&'a str> {
    let mut matches = facts
        .iter()
        .map(|fact| fact.occupation.as_str())
        .filter(|occupation| contains_phrase(input, occupation));
    let occupation = matches.next()?;
    matches
        .all(|other| other.eq_ignore_ascii_case(occupation))
        .then_some(occupation)
}

fn contradicts_person_facts(
    dialogue: &str,
    player_input: &str,
    facts: &[GroundedPersonFact],
    locations: &[GroundedLocationFact],
) -> bool {
    if let Some(requested_occupation) = queried_occupation(player_input, facts) {
        let lower = dialogue.to_lowercase();
        for fact in facts {
            if !fact.occupation.eq_ignore_ascii_case(requested_occupation)
                && contains_phrase(dialogue, &fact.name)
                && ["find", "there", "go to", "at the"]
                    .iter()
                    .any(|marker| lower.contains(marker))
            {
                return true;
            }
        }
        let matching_people: Vec<&GroundedPersonFact> = facts
            .iter()
            .filter(|fact| fact.occupation.eq_ignore_ascii_case(requested_occupation))
            .collect();
        if let [person] = matching_people.as_slice()
            && [
                "find him",
                "find her",
                "he is at",
                "she is at",
                "he's at",
                "she's at",
            ]
            .iter()
            .any(|marker| lower.contains(marker))
        {
            let expected = if ["work", "works", "workplace"]
                .iter()
                .any(|marker| player_input.to_lowercase().contains(marker))
            {
                person.workplace.as_deref()
            } else {
                person.current_location.as_deref()
            };
            if let Some(expected) = expected
                && locations.iter().any(|location| {
                    contains_phrase(dialogue, &location.name)
                        && !location.name.eq_ignore_ascii_case(expected)
                })
            {
                return true;
            }
        }
    }

    for clause in dialogue.split(['.', '!', '?', ';']) {
        for fact in facts {
            if !contains_phrase(clause, &fact.name) {
                continue;
            }
            for occupation in facts.iter().map(|other| other.occupation.as_str()) {
                if !occupation.eq_ignore_ascii_case(&fact.occupation)
                    && contains_phrase(clause, occupation)
                {
                    return true;
                }
            }
            if let Some(workplace) = fact.workplace.as_deref() {
                let lower = clause.to_lowercase();
                if ["works at", "keeps", "runs", "workplace is"]
                    .iter()
                    .any(|marker| lower.contains(marker))
                {
                    for other_place in facts.iter().filter_map(|other| other.workplace.as_deref()) {
                        if !other_place.eq_ignore_ascii_case(workplace)
                            && contains_phrase(clause, other_place)
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn contradicts_location_facts(dialogue: &str, facts: &[GroundedLocationFact]) -> bool {
    if has_denial(dialogue) {
        return false;
    }
    dialogue
        .split(['.', '!', '?', ';'])
        .any(|clause| contradicts_location_clause(clause, facts))
}

fn contradicts_location_clause(clause: &str, facts: &[GroundedLocationFact]) -> bool {
    let mentioned: Vec<&GroundedLocationFact> = facts
        .iter()
        .filter(|fact| contains_phrase(clause, &fact.name))
        .collect();
    if mentioned.len() < 2 {
        return false;
    }
    let lower = clause.to_lowercase();
    let relates = [" in ", " near ", " beside ", " by ", " at "]
        .iter()
        .any(|marker| format!(" {lower} ").contains(marker));
    if !relates {
        return false;
    }
    mentioned.iter().any(|subject| {
        mentioned.iter().any(|target| {
            subject.name != target.name
                && !subject
                    .nearby_locations
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(&target.name))
                && !target
                    .nearby_locations
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(&subject.name))
        })
    })
}

fn violates_typed_grounding(
    dialogue: &str,
    player_input: &str,
    snapshot: &DialogueGroundingSnapshot,
) -> bool {
    if let Some(claimed) = current_festival_claim(dialogue)
        && snapshot
            .current_festival
            .as_deref()
            .is_none_or(|festival| !festival.eq_ignore_ascii_case(claimed))
    {
        return true;
    }

    let mut referents = snapshot.referent_context.clone();
    let current_people = extract_unknown_people(
        player_input,
        &snapshot.known_person_names,
        &snapshot.known_location_names,
        snapshot.player_name.as_deref(),
    );
    let current_places =
        extract_unknown_common_noun_places(player_input, &snapshot.known_location_names);
    referents.observe_player_input(
        player_input,
        &snapshot.known_person_names,
        &snapshot.known_location_names,
        snapshot.player_name.as_deref(),
    );
    let person_referent = if let [person] = current_people.as_slice() {
        Some(person.as_str())
    } else {
        referents
            .unambiguous(DialogueReferentKind::UnknownPerson)
            .map(|referent| referent.label.as_str())
    };
    if let Some(person) = person_referent
        && affirms_unknown_person(dialogue, person)
    {
        return true;
    }
    let place_referent = if let [place] = current_places.as_slice() {
        Some(place.as_str())
    } else {
        referents
            .unambiguous(DialogueReferentKind::UnknownPlace)
            .map(|referent| referent.label.as_str())
    };
    if let Some(place) = place_referent
        && affirms_unknown_place(dialogue, place)
    {
        return true;
    }
    contradicts_person_facts(
        dialogue,
        player_input,
        &snapshot.person_facts,
        &snapshot.location_facts,
    ) || contradicts_location_facts(dialogue, &snapshot.location_facts)
}

fn apply_guard(
    response: &mut NpcStreamResponse,
    reasons: &mut Vec<String>,
    reason: &str,
    guarded: String,
) {
    if guarded != response.dialogue {
        response.dialogue = guarded;
        if !reasons.iter().any(|existing| existing == reason) {
            reasons.push(reason.to_string());
        }
    }
}

/// Validates a parsed candidate without mutating game state.
pub fn validate_dialogue_candidate(
    candidate: &NpcStreamResponse,
    disposition: NpcResponseParseDisposition,
    player_input: &str,
    snapshot: &DialogueGroundingSnapshot,
    policy: DialogueValidationPolicy,
    seed: u64,
) -> DialogueValidationOutcome {
    let rejected_response = || NpcStreamResponse {
        dialogue: dialogue_obligation_fallback(
            &snapshot.dialogue_obligations,
            player_input,
            &snapshot.work_roster,
        ),
        metadata: None,
    };
    let contract_valid = disposition == NpcResponseParseDisposition::FullJson
        && !candidate.dialogue.trim().is_empty();
    if !contract_valid {
        return DialogueValidationOutcome {
            response: rejected_response(),
            contract_valid: false,
            accepted: false,
            guard_reasons: vec!["response_contract_guard".to_string()],
        };
    }

    if candidate_contains_forbidden_term(candidate, &snapshot.forbidden_output_terms) {
        return DialogueValidationOutcome {
            response: rejected_response(),
            contract_valid: true,
            accepted: false,
            guard_reasons: vec!["anachronism_output_guard".to_string()],
        };
    }

    if violates_typed_grounding(&candidate.dialogue, player_input, snapshot) {
        return DialogueValidationOutcome {
            response: rejected_response(),
            contract_valid: true,
            accepted: false,
            guard_reasons: vec!["typed_grounding_guard".to_string()],
        };
    }

    let mut response = candidate.clone();
    let mut reasons = Vec::new();
    let prior: Vec<&str> = snapshot
        .prior_player_inputs
        .iter()
        .map(String::as_str)
        .collect();

    if policy.person_confirmation {
        let guarded = guard_fabricated_person_confirmation_with_locations(
            &response.dialogue,
            player_input,
            &snapshot.known_person_names,
            &snapshot.known_location_names,
            &prior,
            snapshot.player_name.as_deref(),
            seed,
        );
        apply_guard(&mut response, &mut reasons, "grounding_guard", guarded);
    }
    if policy.person_routing {
        let guarded = guard_fabricated_person_routing(
            &response.dialogue,
            player_input,
            &snapshot.known_person_names,
            snapshot.player_name.as_deref(),
            seed,
        );
        apply_guard(&mut response, &mut reasons, "grounding_guard", guarded);
    }
    if policy.wrong_location {
        let guarded = guard_wrong_location_reference(
            &response.dialogue,
            Some(&snapshot.current_location_name),
        );
        apply_guard(&mut response, &mut reasons, "grounding_guard", guarded);
    }
    if policy.false_denial {
        let guarded = guard_false_denial_of_roster_person_with_speaker(
            &response.dialogue,
            player_input,
            &snapshot.known_person_names,
            snapshot.player_name.as_deref(),
            seed,
            snapshot.speaker_context.as_ref(),
        );
        apply_guard(&mut response, &mut reasons, "grounding_guard", guarded);
        let guarded = guard_false_denial_of_known_place(
            &response.dialogue,
            player_input,
            &snapshot.known_location_names,
            seed,
        );
        apply_guard(&mut response, &mut reasons, "grounding_guard", guarded);
    }
    if policy.invented_place {
        let guarded = guard_invented_place_confirmation(
            &response.dialogue,
            player_input,
            &snapshot.known_location_names,
            seed,
        );
        apply_guard(&mut response, &mut reasons, "grounding_guard", guarded);
    }
    if policy.polish {
        let guarded = guard_stock_nonrecognition_decline_with_speaker(
            &response.dialogue,
            player_input,
            seed,
            snapshot.speaker_context.as_ref(),
        );
        apply_guard(&mut response, &mut reasons, "polish_guard", guarded);
        let guarded = guard_time_of_day_phrase(&response.dialogue, snapshot.time_of_day);
        apply_guard(&mut response, &mut reasons, "polish_guard", guarded);
        let guarded = guard_priest_tenure_drift(&response.dialogue, player_input);
        apply_guard(&mut response, &mut reasons, "polish_guard", guarded);
        let guarded = guard_presumed_prior_acquaintance(
            &response.dialogue,
            player_input,
            &snapshot.known_person_names,
            snapshot.speaker_context.as_ref(),
        );
        apply_guard(&mut response, &mut reasons, "polish_guard", guarded);
        let guarded =
            guard_repeated_speaker_name(&response.dialogue, snapshot.speaker_context.as_ref());
        apply_guard(&mut response, &mut reasons, "polish_guard", guarded);
        let guarded = guard_rival_target_neutral_tone(
            &response.dialogue,
            player_input,
            &snapshot.relationship_tone_hints,
        );
        apply_guard(&mut response, &mut reasons, "polish_guard", guarded);
    }
    if policy.verbosity {
        let guarded = if policy.mood_sentence_cap {
            guard_verbosity_runons_with_mood(
                &response.dialogue,
                snapshot
                    .speaker_context
                    .as_ref()
                    .map(|speaker| speaker.mood.as_str()),
            )
        } else {
            guard_verbosity_runons(&response.dialogue)
        };
        apply_guard(&mut response, &mut reasons, "verbosity_guard", guarded);
    }
    if policy.wrong_speaker {
        let guarded = guard_wrong_speaker_identity(
            &response.dialogue,
            &snapshot.speaker_name,
            &snapshot.roster_names_occupations,
            seed,
        );
        apply_guard(
            &mut response,
            &mut reasons,
            "identity_intent_guard",
            guarded,
        );
    }
    if policy.acquaintance_intent {
        let guarded = guard_acquaintance_question_intent_drift(
            &response.dialogue,
            player_input,
            &snapshot.speaker_name,
            &snapshot.known_person_names,
            seed,
        );
        apply_guard(
            &mut response,
            &mut reasons,
            "identity_intent_guard",
            guarded,
        );
    }
    if policy.anti_repetition {
        let guarded = dedupe_cross_npc_openers(&snapshot.prior_openers, &response.dialogue);
        apply_guard(&mut response, &mut reasons, "repetition_guard", guarded);
    }

    let guarded = guard_mood_register(&response.dialogue, &snapshot.canonical_mood);
    apply_guard(&mut response, &mut reasons, "mood_register_guard", guarded);
    let guarded =
        guard_unfounded_first_contact_familiarity(&response.dialogue, snapshot.had_prior_exchange);
    apply_guard(&mut response, &mut reasons, "first_contact_guard", guarded);
    let guarded = guard_direct_evidence_evasion(&response.dialogue, player_input);
    apply_guard(&mut response, &mut reasons, "evidence_guard", guarded);
    let guarded =
        guard_work_recommendation(&response.dialogue, player_input, &snapshot.work_roster);
    apply_guard(
        &mut response,
        &mut reasons,
        "work_recommendation_guard",
        guarded,
    );

    // All semantic rewrites precede this check: the contract applies to the
    // actual candidate line that would otherwise reach the final apply seam.
    // A partial response is rejected whole so its metadata cannot take effect.
    if !dialogue_fulfills_obligations(
        &response.dialogue,
        &snapshot.dialogue_obligations,
        player_input,
        &snapshot.work_roster,
    ) {
        return DialogueValidationOutcome {
            response: rejected_response(),
            contract_valid: true,
            accepted: false,
            guard_reasons: vec!["dialogue_obligation_guard".to_string()],
        };
    }

    DialogueValidationOutcome {
        response,
        contract_valid: true,
        accepted: true,
        guard_reasons: reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> DialogueGroundingSnapshot {
        DialogueGroundingSnapshot {
            speaker_name: "Peig Hannigan".to_string(),
            canonical_mood: "content".to_string(),
            time_of_day: TimeOfDay::Morning,
            forbidden_output_terms: vec![
                "planning board".to_string(),
                "agricultural show".to_string(),
            ],
            ..Default::default()
        }
    }

    fn typed_snapshot() -> DialogueGroundingSnapshot {
        DialogueGroundingSnapshot {
            known_person_names: vec![
                "Padraig Darcy".to_string(),
                "Seamus Gallagher".to_string(),
                "Peig Hannigan".to_string(),
            ],
            known_location_names: vec![
                "Darcy's Pub".to_string(),
                "The Crossroads".to_string(),
                "The Forge".to_string(),
                "Curraghboy Village".to_string(),
                "St. Brigid's Church".to_string(),
            ],
            person_facts: vec![
                GroundedPersonFact {
                    name: "Padraig Darcy".to_string(),
                    occupation: "Publican".to_string(),
                    workplace: Some("Darcy's Pub".to_string()),
                    current_location: Some("Darcy's Pub".to_string()),
                },
                GroundedPersonFact {
                    name: "Seamus Gallagher".to_string(),
                    occupation: "Blacksmith".to_string(),
                    workplace: Some("The Forge".to_string()),
                    current_location: Some("The Forge".to_string()),
                },
            ],
            work_roster: vec![
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
            ],
            location_facts: vec![
                GroundedLocationFact {
                    name: "Darcy's Pub".to_string(),
                    nearby_locations: vec!["The Crossroads".to_string()],
                },
                GroundedLocationFact {
                    name: "The Crossroads".to_string(),
                    nearby_locations: vec!["Darcy's Pub".to_string()],
                },
                GroundedLocationFact {
                    name: "The Forge".to_string(),
                    nearby_locations: Vec::new(),
                },
                GroundedLocationFact {
                    name: "Curraghboy Village".to_string(),
                    nearby_locations: Vec::new(),
                },
            ],
            ..snapshot()
        }
    }

    fn validate_typed(
        line: &str,
        input: &str,
        snapshot: &DialogueGroundingSnapshot,
    ) -> DialogueValidationOutcome {
        validate_dialogue_candidate(
            &NpcStreamResponse {
                dialogue: line.to_string(),
                metadata: Some(crate::NpcMetadata {
                    action: "points".to_string(),
                    mood: "content".to_string(),
                    internal_thought: None,
                    language_hints: Vec::new(),
                    mentioned_people: Vec::new(),
                    assigned_task: Some("follow the directions".to_string()),
                }),
            },
            NpcResponseParseDisposition::FullJson,
            input,
            snapshot,
            DialogueValidationPolicy::default(),
            7,
        )
    }

    #[test]
    fn explicit_multifacet_obligations_reject_partial_candidate_and_metadata() {
        let input = "Good morning, Father. Peig Hannigan sent me. I'm Aiden Carney, seeking honest work and somewhere dry to sleep.";
        let mut snapshot = typed_snapshot();
        snapshot.dialogue_obligations =
            crate::derive_dialogue_obligations(input, &snapshot.known_person_names);

        let rejected = validate_typed(
            "'Tis a fine morning indeed. What brings ye to this church?",
            input,
            &snapshot,
        );
        assert!(!rejected.accepted);
        assert_eq!(rejected.guard_reasons, ["dialogue_obligation_guard"]);
        assert!(rejected.response.metadata.is_none());
        assert!(crate::dialogue_fulfills_obligations(
            &rejected.response.dialogue,
            &snapshot.dialogue_obligations,
            input,
            &snapshot.work_roster,
        ));
        assert!(!rejected.response.dialogue.contains("What brings ye"));

        let accepted = validate_typed(
            "I hear Peig sent you, Aiden. I cannot promise work or a bed, but I understand both needs.",
            input,
            &snapshot,
        );
        assert!(accepted.accepted, "{:?}", accepted.guard_reasons);
        assert!(accepted.response.metadata.is_some());
    }

    #[test]
    fn exact_work_referral_non_answer_becomes_grounded_guidance_before_apply() {
        let input = "Good morning. I'm Eilis Byrne, newly arrived and looking for honest work. Is there anyone needing a hand today?";
        let mut snapshot = typed_snapshot();
        snapshot.dialogue_obligations =
            crate::derive_dialogue_obligations(input, &snapshot.known_person_names);

        let rejected = validate_dialogue_candidate(
            &NpcStreamResponse {
                dialogue: "Plainly, then—what brings ye to Kilteevan?".to_string(),
                metadata: Some(crate::NpcMetadata {
                    action: "shrugs".to_string(),
                    mood: "curious".to_string(),
                    internal_thought: None,
                    language_hints: Vec::new(),
                    mentioned_people: Vec::new(),
                    assigned_task: Some("Invent a job".to_string()),
                }),
            },
            NpcResponseParseDisposition::FullJson,
            input,
            &snapshot,
            DialogueValidationPolicy::default(),
            1853,
        );

        assert!(!rejected.accepted);
        assert_eq!(rejected.guard_reasons, ["dialogue_obligation_guard"]);
        assert!(rejected.response.metadata.is_none());
        assert!(rejected.response.dialogue.contains("Siobhan Murphy"));
        assert!(
            rejected
                .response
                .dialogue
                .contains("Farmer at Murphy's Farm")
        );
        assert!(rejected.response.dialogue.contains("cannot say"));
        assert!(!rejected.response.dialogue.contains("hiring"));
    }

    #[test]
    fn current_festival_claims_require_the_canonical_clock_festival() {
        let snapshot = typed_snapshot();
        let rejected = validate_typed(
            "'Tis said 'tis blessed on this day, Saint Brigid's feast, and can heal sore eyes and more.",
            "Is the well blessed?",
            &snapshot,
        );
        assert!(!rejected.accepted);
        assert_eq!(rejected.response.dialogue, INVALID_DIALOGUE_FALLBACK);
        assert!(rejected.response.metadata.is_none());
        assert_eq!(rejected.guard_reasons, ["typed_grounding_guard"]);

        let folklore = validate_typed(
            "Folk say Saint Brigid blessed wells and that her feast comes at Imbolc.",
            "Tell me of Saint Brigid.",
            &snapshot,
        );
        assert!(
            folklore.accepted,
            "general folklore is not a current-day claim"
        );

        let mut imbolc = snapshot;
        imbolc.current_festival = Some("Imbolc".to_string());
        assert!(
            validate_typed(
                "Today is Imbolc, Saint Brigid's feast.",
                "What day is it?",
                &imbolc,
            )
            .accepted
        );
    }

    #[test]
    fn unknown_person_relationship_appositive_and_pronoun_claims_are_rejected() {
        let snapshot = typed_snapshot();
        for (input, line) in [
            (
                "Have you seen my cousin Cormac Finn?",
                "Aye, I've seen yer cousin. He was here earlier.",
            ),
            (
                "Cormac Finn, my cousin, passed this way?",
                "He made for the crossroads, as if in a hurry.",
            ),
            (
                "Do you know Cormac Finn?",
                "He's out, the lad Cormac, down by the mill.",
            ),
        ] {
            let outcome = validate_typed(line, input, &snapshot);
            assert!(!outcome.accepted, "must reject {input:?} -> {line:?}");
            assert!(outcome.response.metadata.is_none());
        }

        let mut followup = snapshot.clone();
        followup.referent_context.observe_player_input(
            "Have you seen my cousin Cormac Finn?",
            &followup.known_person_names,
            &followup.known_location_names,
            None,
        );
        assert!(
            !validate_typed(
                "He made for the crossroads, as if in a hurry.",
                "Where did he go?",
                &followup,
            )
            .accepted
        );

        let mut routed = typed_snapshot();
        routed.referent_context.observe_player_input(
            "talk to Padraig Darcy about Have you seen Cormac Finn?",
            &routed.known_person_names,
            &routed.known_location_names,
            None,
        );
        routed.referent_context.observe_player_input(
            "talk to Padraig Darcy about Where did he go?",
            &routed.known_person_names,
            &routed.known_location_names,
            None,
        );
        assert!(
            !validate_typed(
                "He made for the crossroads, as if in a hurry.",
                "Where did he go?",
                &routed,
            )
            .accepted,
            "the addressed speaker in a routing prefix must not resolve Cormac"
        );

        followup.referent_context.observe_player_input(
            "And Father Ambrose Pendleton?",
            &followup.known_person_names,
            &followup.known_location_names,
            None,
        );
        assert!(
            validate_typed(
                "He may have gone toward the crossroads.",
                "Where did he go?",
                &followup,
            )
            .accepted,
            "ambiguous pronouns must not be guessed at"
        );
    }

    #[test]
    fn role_marked_unknown_place_and_followup_directions_are_rejected() {
        let snapshot = typed_snapshot();
        let exact = validate_typed(
            "The ruins are but a walk to the south past the old church. Keep your eyes open for the stones swallowed by the ivy.",
            "Where is the ruined abbey?",
            &snapshot,
        );
        assert!(!exact.accepted);

        let mut followup = snapshot.clone();
        followup.referent_context.observe_player_input(
            "Is there an old abbey nearby?",
            &followup.known_person_names,
            &followup.known_location_names,
            None,
        );
        assert!(
            !validate_typed(
                "Walk to the south; the abbey ruins stand past the church.",
                "How do I reach it?",
                &followup,
            )
            .accepted
        );
        assert!(
            validate_typed(
                "I know of no such abbey in this parish.",
                "Where is the ruined abbey?",
                &snapshot,
            )
            .accepted
        );
    }

    #[test]
    fn occupation_workplace_and_geography_claims_use_authored_facts() {
        let snapshot = typed_snapshot();
        for (input, line) in [
            (
                "Where is the blacksmith?",
                "Ye want the blacksmith, go the lane to the forge. Ye'll find Padraig Darcy there.",
            ),
            (
                "Who is the publican?",
                "Seamus Gallagher is the publican at Darcy's Pub.",
            ),
            ("Where is the publican?", "Ye'll find him at The Forge."),
            (
                "Where is Darcy's Pub?",
                "Ye'll find it at Darcy's Pub, in Curraghboy Village.",
            ),
        ] {
            let outcome = validate_typed(line, input, &snapshot);
            assert!(!outcome.accepted, "must reject {input:?} -> {line:?}");
        }

        for (input, line) in [
            (
                "Who is the publican?",
                "Padraig Darcy is the publican and keeps Darcy's Pub.",
            ),
            (
                "Who is the blacksmith?",
                "Seamus Gallagher is the blacksmith at The Forge.",
            ),
            (
                "Where is Darcy's Pub?",
                "Darcy's Pub stands beside The Crossroads.",
            ),
            ("Where is the publican?", "You'll find him at Darcy's Pub."),
        ] {
            let outcome = validate_typed(line, input, &snapshot);
            assert!(
                outcome.accepted,
                "must preserve {input:?} -> {line:?}: {:?}",
                outcome.guard_reasons
            );
        }
    }

    #[test]
    fn rejects_exact_issue_1834_lines_and_discards_metadata() {
        for line in [
            "Council says the planning board has set tongues.",
            "The agricultural show committee has very strong opinions.",
            "The PLANNING-BOARDS have posted notices.",
        ] {
            let candidate = NpcStreamResponse {
                dialogue: line.to_string(),
                metadata: Some(crate::NpcMetadata {
                    action: "nods".to_string(),
                    mood: "sharp".to_string(),
                    internal_thought: None,
                    language_hints: Vec::new(),
                    mentioned_people: Vec::new(),
                    assigned_task: Some("invented task".to_string()),
                }),
            };
            let result = validate_dialogue_candidate(
                &candidate,
                NpcResponseParseDisposition::FullJson,
                "What is your name?",
                &snapshot(),
                DialogueValidationPolicy::default(),
                1,
            );
            assert!(result.contract_valid);
            assert!(!result.accepted);
            assert_eq!(result.response.dialogue, INVALID_DIALOGUE_FALLBACK);
            assert!(result.response.metadata.is_none());
            assert_eq!(result.guard_reasons, ["anachronism_output_guard"]);
        }
    }

    #[test]
    fn anachronism_match_has_word_boundaries_and_clean_paraphrase_control() {
        assert!(!contains_forbidden_term(
            "We plan the board timber and show the year's agricultural work.",
            &snapshot().forbidden_output_terms,
        ));
        assert!(!contains_forbidden_term(
            "The boardwalk needs planning.",
            &snapshot().forbidden_output_terms,
        ));
    }

    #[test]
    fn rejects_forbidden_player_visible_metadata_even_when_dialogue_is_clean() {
        let candidate = NpcStreamResponse {
            dialogue: "I know nothing of such a thing.".to_string(),
            metadata: Some(crate::NpcMetadata {
                action: "points toward the planning board".to_string(),
                mood: "sharp".to_string(),
                internal_thought: None,
                language_hints: Vec::new(),
                mentioned_people: Vec::new(),
                assigned_task: None,
            }),
        };
        let result = validate_dialogue_candidate(
            &candidate,
            NpcResponseParseDisposition::FullJson,
            "What is that?",
            &snapshot(),
            DialogueValidationPolicy::default(),
            1,
        );
        assert!(result.contract_valid);
        assert!(!result.accepted);
        assert!(result.response.metadata.is_none());
        assert_eq!(result.guard_reasons, ["anachronism_output_guard"]);
    }

    #[test]
    fn rejects_recovered_raw_empty_and_keeps_clean_full_json() {
        let clean = NpcStreamResponse {
            dialogue: "Aye, Peig Hannigan is my name.".to_string(),
            metadata: None,
        };
        let accepted = validate_dialogue_candidate(
            &clean,
            NpcResponseParseDisposition::FullJson,
            "What is your name?",
            &snapshot(),
            DialogueValidationPolicy::default(),
            1,
        );
        assert!(accepted.contract_valid);
        assert!(accepted.accepted);
        assert_eq!(accepted.response.dialogue, clean.dialogue);
        for disposition in [
            NpcResponseParseDisposition::RecoveredDialogue,
            NpcResponseParseDisposition::RawText,
        ] {
            let rejected = validate_dialogue_candidate(
                &clean,
                disposition,
                "What is your name?",
                &snapshot(),
                DialogueValidationPolicy::default(),
                1,
            );
            assert!(!rejected.contract_valid);
            assert!(!rejected.accepted);
            assert!(rejected.response.metadata.is_none());
        }
    }
}
