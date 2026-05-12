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
    // These are a subset of movement verbs used for bare-destination matching.
    // `move_phrases` above handles multi-word phrases (e.g. "make my way to"),
    // while `move_verbs` handles simple verb + destination (e.g. "go pub").
    // They intentionally do not share the same set of verbs.
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_parse_go_to() {
        let intent = parse_intent_local("go to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));
    }
    #[test]
    fn test_local_parse_walk_to() {
        let intent = parse_intent_local("walk to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));
    }
    #[test]
    fn test_local_parse_go_shorthand() {
        let intent = parse_intent_local("go pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }
    #[test]
    fn test_local_parse_move_bare() {
        let intent = parse_intent_local("move pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));

        let intent = parse_intent_local("move to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));
    }
    #[test]
    fn test_local_parse_head_to() {
        let intent = parse_intent_local("head to Murphy's Farm").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("Murphy's Farm".to_string()));
    }
    #[test]
    fn test_local_parse_visit() {
        let intent = parse_intent_local("visit the fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the fairy fort".to_string()));
    }
    #[test]
    fn test_local_parse_look() {
        let intent = parse_intent_local("look").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);

        let intent = parse_intent_local("look around").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);

        let intent = parse_intent_local("l").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);
    }
    #[test]
    fn test_local_parse_case_insensitive() {
        let intent = parse_intent_local("GO TO THE PUB").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("THE PUB".to_string()));

        let intent = parse_intent_local("LOOK").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);
    }
    #[test]
    fn test_local_parse_no_match() {
        assert!(parse_intent_local("tell Mary hello").is_none());
        assert!(parse_intent_local("pick up the stone").is_none());
        assert!(parse_intent_local("hello there").is_none());
    }
    #[test]
    fn test_local_parse_first_person_narrative_is_talk() {
        // First-person statements that mention place names must not be
        // interpreted as move commands (regression: "I came from the coast"
        // was triggering navigation to Lough Ree Shore).
        let intent = parse_intent_local("I came from the coast").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);
        assert_eq!(intent.target, None);
        assert_eq!(intent.dialogue, Some("I came from the coast".to_string()));

        let intent = parse_intent_local("I was at the shore yesterday").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);

        let intent = parse_intent_local("I'm not from around here").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);

        let intent = parse_intent_local("I've been to the pub before").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);

        // Bare "I" with no continuation is also talk
        let intent = parse_intent_local("I").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);
    }
    #[test]
    fn test_local_parse_empty_target() {
        // "go to " with nothing after should match "go " prefix with target "to",
        // which is fine — the world graph won't find "to" and will say not found.
        // But bare "go" or "walk" with no target should not match.
        assert!(parse_intent_local("go").is_none());
        assert!(parse_intent_local("walk").is_none());
    }
    #[test]
    fn test_local_parse_saunter() {
        let intent = parse_intent_local("saunter to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("saunter pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }
    #[test]
    fn test_local_parse_mosey() {
        let intent = parse_intent_local("mosey to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("mosey church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("church".to_string()));
    }
    #[test]
    fn test_local_parse_wander() {
        let intent = parse_intent_local("wander to the crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the crossroads".to_string()));

        let intent = parse_intent_local("wander crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("crossroads".to_string()));
    }
    #[test]
    fn test_local_parse_stroll() {
        let intent = parse_intent_local("stroll to the fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the fairy fort".to_string()));

        let intent = parse_intent_local("stroll fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("fairy fort".to_string()));
    }
    #[test]
    fn test_local_parse_amble() {
        let intent = parse_intent_local("amble to the village green").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the village green".to_string()));

        let intent = parse_intent_local("amble village green").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("village green".to_string()));
    }
    #[test]
    fn test_local_parse_trek_and_hike() {
        let intent = parse_intent_local("trek to the bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the bog".to_string()));

        let intent = parse_intent_local("hike to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));

        let intent = parse_intent_local("trek bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("bog".to_string()));
    }
    #[test]
    fn test_local_parse_run_jog_dash() {
        let intent = parse_intent_local("run to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("jog to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("dash to the crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the crossroads".to_string()));

        // Without "to"
        let intent = parse_intent_local("run pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }
    #[test]
    fn test_local_parse_hurry_rush() {
        let intent = parse_intent_local("hurry to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("rush to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("hurry pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }
    #[test]
    fn test_local_parse_proceed() {
        let intent = parse_intent_local("proceed to the town square").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the town square".to_string()));

        let intent = parse_intent_local("proceed town square").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("town square".to_string()));
    }
    #[test]
    fn test_local_parse_multi_word_phrases() {
        let intent = parse_intent_local("make my way to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("make my way pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));

        let intent = parse_intent_local("head over to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("head over church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("church".to_string()));

        let intent = parse_intent_local("pop over to the shop").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the shop".to_string()));

        let intent = parse_intent_local("pop over shop").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("shop".to_string()));

        let intent = parse_intent_local("nip to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("swing by the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));
    }
    #[test]
    fn test_local_parse_sprint_march_traipse() {
        let intent = parse_intent_local("sprint to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("march to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("traipse to the bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the bog".to_string()));
    }
    #[test]
    fn test_local_parse_meander_trot_stride() {
        let intent = parse_intent_local("meander to the river").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the river".to_string()));

        let intent = parse_intent_local("trot to the farm").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the farm".to_string()));

        let intent = parse_intent_local("stride to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));
    }
    #[test]
    fn test_local_parse_creep_sneak_bolt_scramble() {
        let intent = parse_intent_local("creep to the graveyard").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the graveyard".to_string()));

        let intent = parse_intent_local("sneak to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("bolt to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("scramble to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));
    }
    #[test]
    fn test_local_parse_unusual_verbs_case_insensitive() {
        let intent = parse_intent_local("SAUNTER TO THE PUB").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("THE PUB".to_string()));

        let intent = parse_intent_local("Mosey To The Church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("The Church".to_string()));

        let intent = parse_intent_local("WANDER crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("crossroads".to_string()));
    }
    #[test]
    fn test_local_parse_bare_unusual_verbs_no_target() {
        // Bare verbs without a target should not match
        assert!(parse_intent_local("saunter").is_none());
        assert!(parse_intent_local("mosey").is_none());
        assert!(parse_intent_local("wander").is_none());
        assert!(parse_intent_local("stroll").is_none());
        assert!(parse_intent_local("amble").is_none());
        assert!(parse_intent_local("run").is_none());
        assert!(parse_intent_local("dash").is_none());
    }
}
