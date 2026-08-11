//! Canonical validation for untrusted Tier-1 dialogue candidates.
//!
//! Prompt instructions improve model behaviour; they are not an authorization
//! boundary. This module is the one side-effect-free apply gate used before model
//! text or metadata may affect memory, events, state, or player-visible UI.

use parish_types::TimeOfDay;

use crate::{
    DialogueSpeakerContext, NpcResponseParseDisposition, NpcStreamResponse, RelationshipToneHint,
    dedupe_cross_npc_openers, guard_acquaintance_question_intent_drift,
    guard_direct_evidence_evasion, guard_fabricated_person_confirmation_with_locations,
    guard_fabricated_person_routing, guard_false_denial_of_known_place,
    guard_false_denial_of_roster_person_with_speaker, guard_invented_place_confirmation,
    guard_mood_register, guard_presumed_prior_acquaintance, guard_priest_tenure_drift,
    guard_repeated_speaker_name, guard_rival_target_neutral_tone,
    guard_stock_nonrecognition_decline_with_speaker, guard_time_of_day_phrase,
    guard_unfounded_first_contact_familiarity, guard_verbosity_runons,
    guard_verbosity_runons_with_mood, guard_work_recommendation, guard_wrong_location_reference,
    guard_wrong_speaker_identity,
};

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
    pub work_roster: Vec<(String, String, Option<String>)>,
    pub relationship_tone_hints: Vec<RelationshipToneHint>,
    pub prior_player_inputs: Vec<String>,
    pub forbidden_output_terms: Vec<String>,
    pub prior_openers: Vec<String>,
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
    let contract_valid = disposition == NpcResponseParseDisposition::FullJson
        && !candidate.dialogue.trim().is_empty();
    if !contract_valid {
        return DialogueValidationOutcome {
            response: NpcStreamResponse {
                dialogue: INVALID_DIALOGUE_FALLBACK.to_string(),
                metadata: None,
            },
            contract_valid: false,
            accepted: false,
            guard_reasons: vec!["response_contract_guard".to_string()],
        };
    }

    if candidate_contains_forbidden_term(candidate, &snapshot.forbidden_output_terms) {
        return DialogueValidationOutcome {
            response: NpcStreamResponse {
                dialogue: INVALID_DIALOGUE_FALLBACK.to_string(),
                metadata: None,
            },
            contract_valid: true,
            accepted: false,
            guard_reasons: vec!["anachronism_output_guard".to_string()],
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
