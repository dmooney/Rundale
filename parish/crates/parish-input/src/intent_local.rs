//! Local (non-LLM) intent parsing using keyword matching.
//!
//! Catches common, unambiguous movement and look phrases without
//! requiring a network round-trip to the LLM provider.

use crate::intent_types::{IntentKind, PlayerIntent};

/// Attempts to parse intent locally using keyword matching.
///
/// Catches common movement and look phrases without requiring an LLM call.
/// Returns `None` if the input doesn't match any known pattern.
pub fn parse_intent_local(raw_input: &str) -> Option<PlayerIntent> {
    let trimmed = raw_input.trim();
    let lower = trimmed.to_lowercase();

    // Movement patterns — multi-word phrases checked first (longest match wins),
    // then single-verb prefixes. Covers common, colloquial, and unusual verbs.
    let move_phrases = [
        "make my way to ",
        "make my way ",
        "head over to ",
        "head over ",
        "pop over to ",
        "pop over ",
        "nip to ",
        "swing by ",
        "go to ",
        "walk to ",
        "head to ",
        "move to ",
        "travel to ",
        "run to ",
        "jog to ",
        "dash to ",
        "hurry to ",
        "rush to ",
        "stroll to ",
        "saunter to ",
        "mosey to ",
        "wander to ",
        "amble to ",
        "trek to ",
        "hike to ",
        "proceed to ",
        "sprint to ",
        "march to ",
        "traipse to ",
        "meander to ",
        "trot to ",
        "stride to ",
        "creep to ",
        "sneak to ",
        "bolt to ",
        "scramble to ",
    ];

    // Single-verb prefixes (without "to") — "saunter pub", "go pub", etc.
    // Must be kept in sync with `move_phrases` above: every verb in `move_phrases`
    // should also appear here without the "to " suffix, so bare "move pub" works
    // the same as "move to the pub".
    let move_verbs = [
        "go ",
        "walk ",
        "head ",
        "visit ",
        "move ",
        "run ",
        "jog ",
        "dash ",
        "hurry ",
        "rush ",
        "stroll ",
        "saunter ",
        "mosey ",
        "wander ",
        "amble ",
        "trek ",
        "hike ",
        "proceed ",
        "sprint ",
        "march ",
        "traipse ",
        "meander ",
        "trot ",
        "stride ",
        "creep ",
        "sneak ",
        "bolt ",
        "scramble ",
    ];

    // Try multi-word phrases first for longest-match semantics
    if let Some(intent) = try_move_prefix(trimmed, &lower, raw_input, &move_phrases) {
        return Some(intent);
    }

    // Then try bare verb + destination
    if let Some(intent) = try_move_prefix(trimmed, &lower, raw_input, &move_verbs) {
        return Some(intent);
    }

    // Look patterns
    let look_phrases = ["look", "look around", "l", "examine room", "where am i"];
    if look_phrases.contains(&lower.as_str()) {
        return Some(PlayerIntent {
            intent: IntentKind::Look,
            target: None,
            dialogue: None,
            raw: raw_input.to_string(),
        });
    }

    // First-person narrative guard: sentences that begin with a first-person
    // pronoun are clearly conversational, never navigation commands.  Catching
    // them here prevents the LLM from extracting a place name mentioned in the
    // middle of a statement (e.g. "I came from the coast") as a move target.
    let first_person_prefixes = ["i ", "i'm ", "i've ", "i'd ", "i'll ", "i was ", "i am "];
    if first_person_prefixes.iter().any(|p| lower.starts_with(p)) || lower == "i" {
        return Some(PlayerIntent {
            intent: IntentKind::Talk,
            target: None,
            dialogue: Some(raw_input.trim().to_string()),
            raw: raw_input.to_string(),
        });
    }

    None
}

/// Shared helper: checks if `lower` starts with any prefix in `prefixes`,
/// extracts the target from the original (cased) `trimmed` input using
/// char-count-based byte-offset computation, and returns a `Move` intent.
fn try_move_prefix(
    trimmed: &str,
    lower: &str,
    raw_input: &str,
    prefixes: &[&str],
) -> Option<PlayerIntent> {
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            let byte_offset: usize = trimmed
                .char_indices()
                .nth(prefix.chars().count())
                .map(|(i, _)| i)
                .unwrap_or(trimmed.len());
            let target = trimmed[byte_offset..].trim();
            if !target.is_empty() {
                return Some(PlayerIntent {
                    intent: IntentKind::Move,
                    target: Some(target.to_string()),
                    dialogue: None,
                    raw: raw_input.to_string(),
                });
            }
        }
    }
    None
}
