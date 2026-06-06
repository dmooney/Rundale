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
    //
    // First-person movement intents (fixed: #41/#46/#53) MUST appear in this
    // list so they match before the generic first-person narrative guard
    // below classifies them as `Talk`. The auto-player in the demo audit
    // produced lines like "I'll make for the Crossroads then" 9 turns in
    // a row that vanished into Talk-no-NPC because none of these were
    // registered as move prefixes.
    let move_phrases = [
        // First-person movement intents — longest first.
        "i'll be making for ",
        "i'll be makin' for ",
        "i'll be heading to ",
        "i'll be walking to ",
        "i'll be on me way to ",
        "i'll be on my way to ",
        "i'll be off to ",
        "i'll make my way to ",
        "i'll head over to ",
        "i'll head to ",
        "i'll make for ",
        "i'll venture to ",
        "i'll wander to ",
        "i'll stroll to ",
        "i'll walk to ",
        "i'll go to ",
        "i'm off to ",
        "i'm headed to ",
        "i'm heading to ",
        "off i go to ",
        // Modal first-person movement (fixed: #53) — "might i" / "i shall"
        // forms only catch when followed by a recognised move verb so
        // conversational "might i ask" / "i shall ponder" stay as Talk.
        "might i make my way to ",
        "i shall make my way to ",
        "might i venture to ",
        "might i make for ",
        "might i walk to ",
        "might i head to ",
        "might i go to ",
        "i shall venture to ",
        "i shall make for ",
        "i shall walk to ",
        "i shall head to ",
        "i shall go to ",
        // Generic multi-word movement phrases.
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
    /// Regression test (fixed: #41/#46/#53): first-person + movement-verb phrasings the
    /// demo auto-player produced repeatedly that the parser silently
    /// dropped because the first-person guard caught them first.
    #[test]
    fn test_local_parse_first_person_movement_intents() {
        let cases = [
            ("I'll make for the Crossroads", "the Crossroads"),
            ("I'll be making for the Hedge School", "the Hedge School"),
            ("I'll venture to the Letter Office", "the Letter Office"),
            ("I'll head to the pub", "the pub"),
            ("I'll go to the mill", "the mill"),
            ("I'll walk to the church", "the church"),
            ("I'll wander to the crossroads", "the crossroads"),
            ("I'll stroll to the green", "the green"),
            ("I'll make my way to the shop", "the shop"),
            ("I'll head over to the Letter Office", "the Letter Office"),
            ("I'll be on me way to the Crossroads", "the Crossroads"),
            ("I'll be on my way to the Crossroads", "the Crossroads"),
            ("I'll be off to the pub", "the pub"),
            ("I'll be heading to the mill", "the mill"),
            ("I'll be walking to the well", "the well"),
            ("I'm off to the Letter Office", "the Letter Office"),
            ("I'm heading to the green", "the green"),
            ("Off I go to the Letter Office", "the Letter Office"),
        ];
        for (input, target) in cases {
            let intent =
                parse_intent_local(input).unwrap_or_else(|| panic!("no intent parsed: {input}"));
            assert_eq!(
                intent.intent,
                IntentKind::Move,
                "{input} must parse as Move (got {:?}): {intent:?}",
                intent.intent
            );
            assert_eq!(
                intent.target.as_deref(),
                Some(target),
                "{input} target mismatch"
            );
        }
    }

    /// Regression guard (fixed: #41): ensure the first-person narrative cases that
    /// must STAY Talk are unaffected by the new movement patterns.
    /// Without a movement verb, "I" / "I'm" / "I've" stay narrative.
    #[test]
    fn test_local_parse_first_person_narrative_still_talk_after_anchor_add() {
        let cases = [
            "I came from the coast",
            "I was at the shore yesterday",
            "I'm not from around here",
            "I've been to the pub before",
        ];
        for input in cases {
            let intent =
                parse_intent_local(input).unwrap_or_else(|| panic!("no intent parsed: {input}"));
            assert_eq!(
                intent.intent,
                IntentKind::Talk,
                "{input} must stay Talk: got {intent:?}"
            );
        }
    }

    /// Case-insensitivity regression guard (fixed: #41) for the new first-person move
    /// patterns.
    #[test]
    fn test_local_parse_first_person_movement_case_insensitive() {
        let intent = parse_intent_local("I'LL MAKE FOR THE CROSSROADS").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target.as_deref(), Some("THE CROSSROADS"));

        let intent = parse_intent_local("Off I Go To The Mill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target.as_deref(), Some("The Mill"));
    }

    /// Regression test (fixed: #53): modal first-person movement forms that the cycle 11
    /// auto-player produced repeatedly. Without these patterns the
    /// "might i" cases fell through the first-person Talk guard
    /// entirely and "i shall ..." was rewritten as Talk before any
    /// move match could fire.
    #[test]
    fn test_local_parse_modal_first_person_movement_intents() {
        let cases = [
            ("Might I venture to the Letter Office", "the Letter Office"),
            ("Might I go to the mill", "the mill"),
            ("Might I walk to the church", "the church"),
            ("Might I head to the crossroads", "the crossroads"),
            ("Might I make my way to the shop", "the shop"),
            ("Might I make for the green", "the green"),
            ("I shall go to the Letter Office", "the Letter Office"),
            ("I shall venture to the mill", "the mill"),
            ("I shall walk to the church", "the church"),
            ("I shall head to the crossroads", "the crossroads"),
            ("I shall make my way to the shop", "the shop"),
            ("I shall make for the green", "the green"),
        ];
        for (input, target) in cases {
            let intent =
                parse_intent_local(input).unwrap_or_else(|| panic!("no intent parsed: {input}"));
            assert_eq!(
                intent.intent,
                IntentKind::Move,
                "{input} must parse as Move (got {:?}): {intent:?}",
                intent.intent
            );
            assert_eq!(
                intent.target.as_deref(),
                Some(target),
                "{input} target mismatch"
            );
        }

        // Case-insensitivity guard for the new modal patterns.
        let upper = parse_intent_local("MIGHT I VENTURE TO THE MILL").unwrap();
        assert_eq!(upper.intent, IntentKind::Move);
        assert_eq!(upper.target.as_deref(), Some("THE MILL"));
        let mixed = parse_intent_local("I Shall Go To The Pub").unwrap();
        assert_eq!(mixed.intent, IntentKind::Move);
        assert_eq!(mixed.target.as_deref(), Some("The Pub"));
    }

    /// Regression guard (fixed: #53): modal openings that do NOT contain a recognised
    /// move verb must NOT be parsed as Move. `Might I ask…` and
    /// `I shall ponder…` are conversational openings and must continue
    /// to fall through to the Talk path.
    #[test]
    fn test_local_parse_modal_without_move_verb_stays_conversational() {
        // "Might I ask ..." does not match any modal-move pattern.
        // It is not first-person ("might" prefix), so the Talk guard
        // does not fire either — parse_intent_local returns None and
        // the input falls through to the LLM intent classifier, which
        // is the correct behaviour for an open-ended question.
        assert!(parse_intent_local("Might I ask about the harvest").is_none());
        assert!(parse_intent_local("Might I inquire after the priest").is_none());

        // "I shall ponder ..." starts with "i " so the first-person
        // Talk guard catches it as Talk — the correct outcome for a
        // reflective statement that mentions no destination.
        let intent = parse_intent_local("I shall ponder it a while").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);
        let intent = parse_intent_local("I shall think on that").unwrap();
        assert_eq!(intent.intent, IntentKind::Talk);
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
