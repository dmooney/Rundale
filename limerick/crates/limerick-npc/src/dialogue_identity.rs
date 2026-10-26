//! Speaker self-identification in delivered dialogue.
//!
//! Whether an NPC has told the player who they are is authoritative game
//! state (it reveals the NPC's name in the notebook and UI). It is derived from
//! the final delivered line and the authored roster only, never from model
//! metadata.

/// Normalizes authored identity text for case-insensitive comparisons while
/// retaining apostrophes inside names and occupations.
fn normalize_identity_text(value: &str) -> String {
    value
        .to_lowercase()
        .replace('\u{2019}', "'")
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '\'' {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns the authored personal first name, skipping a supported honorific.
fn personal_first_name(name: &str) -> Option<&str> {
    let mut parts = name.split_whitespace();
    let first = parts.next()?;
    if matches!(
        first.trim_end_matches('.').to_ascii_lowercase().as_str(),
        "fr" | "father" | "mr" | "mrs" | "miss" | "dr"
    ) {
        parts.next()
    } else {
        Some(first)
    }
}

fn first_name_counts(roster: &[(String, String)]) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for (name, _) in roster {
        if let Some(first) = personal_first_name(name) {
            *counts.entry(normalize_identity_text(first)).or_insert(0) += 1;
        }
    }
    counts
}

fn direct_identity_preface(preface: &str) -> bool {
    let normalized = normalize_identity_text(preface);
    normalized.is_empty()
        || normalized.split_whitespace().all(|word| {
            matches!(
                word,
                "aye" | "well" | "faith" | "bedad" | "plainly" | "then" | "sure" | "mhuise"
            )
        })
}

/// Returns `true` when one spoken clause directly claims `name` in first
/// person. Reported speech and possessive/negated forms are intentionally not
/// identity claims.
fn clause_claims_identity(clause: &str, name: &str) -> bool {
    let lower = clause.to_lowercase().replace('\u{2019}', "'");
    let name = name.to_lowercase().replace('\u{2019}', "'");
    const PREFIXES: &[&str] = &[
        "i'm ",
        "i am ",
        "the name's ",
        "the name is ",
        "my name's ",
        "my name is ",
    ];

    for prefix in PREFIXES {
        let pattern = format!("{prefix}{name}");
        let mut search_from = 0;
        while let Some(relative) = lower[search_from..].find(&pattern) {
            let start = search_from + relative;
            let end = start + pattern.len();
            let boundary_is_valid = lower[end..]
                .chars()
                .next()
                .is_none_or(|next| !next.is_alphanumeric() && next != '\'');
            if boundary_is_valid && direct_identity_preface(&lower[..start]) {
                return true;
            }
            search_from = end;
        }
    }

    // Hiberno-English/postfixed forms put the name before the identifying
    // phrase: "Peig Hannigan's the name" / "Peig Hannigan is the name".
    for suffix in ["'s the name", " is the name"] {
        let pattern = format!("{name}{suffix}");
        if let Some(start) = lower.find(&pattern)
            && direct_identity_preface(&lower[..start])
        {
            return true;
        }
    }
    false
}

/// Returns `true` when `dialogue` contains a direct first-person identity claim
/// for the given `name`.
///
/// Matching is case-insensitive. This helper is used to detect BOTH:
///   1. Legitimate self-introductions by the speaker (name == speaker_name).
///   2. Wrong-identity claims by the speaker (name == other roster member's name).
///
/// The caller decides which case applies based on whose name matched.
fn dialogue_claims_identity(dialogue: &str, name: &str) -> bool {
    let name_lower = name.to_lowercase().replace('\u{2019}', "'");
    let mut name_aliases = vec![name_lower.clone()];
    if let Some(rest) = name_lower
        .strip_prefix("fr. ")
        .or_else(|| name_lower.strip_prefix("fr "))
    {
        name_aliases.push(format!("father {rest}"));
    }
    dialogue.split(['.', '?', '!', ';', '\n']).any(|clause| {
        name_aliases
            .iter()
            .any(|name| clause_claims_identity(clause, name))
    })
}

fn clause_contains_occupation(clause: &str, occupation: &str) -> bool {
    let clause = format!(" {} ", normalize_identity_text(clause));
    let occupation = normalize_identity_text(occupation);
    !occupation.is_empty() && clause.contains(&format!(" {occupation} "))
}

/// Returns whether the delivered dialogue explicitly establishes the
/// speaker's canonical identity.
///
/// The full authored name may be spoken directly. A first-name-only claim is
/// accepted only when that personal first name is unique across the complete
/// authored roster and the same clause states the correct authored occupation
/// (#1842). Metadata, an exchange occurring, or a third-person mention is not
/// enough to reveal an anonymous NPC's name (#1776).
pub fn dialogue_self_identifies_speaker(
    dialogue: &str,
    speaker_name: &str,
    speaker_occupation: &str,
    roster: &[(String, String)],
) -> bool {
    if dialogue_claims_identity(dialogue, speaker_name) {
        return true;
    }

    let Some(first_name) = personal_first_name(speaker_name) else {
        return false;
    };
    let first_name_key = normalize_identity_text(first_name);
    if first_name_key.is_empty()
        || first_name_counts(roster).get(&first_name_key).copied() != Some(1)
    {
        return false;
    }

    dialogue.split(['.', '?', '!', ';', '\n']).any(|clause| {
        clause_claims_identity(clause, first_name)
            && clause_contains_occupation(clause, speaker_occupation)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivered_full_name_marks_a_real_self_introduction() {
        let roster = vec![
            ("Peig Hannigan".to_string(), "Widow".to_string()),
            (
                "Fr. Declan Tierney".to_string(),
                "Parish Priest".to_string(),
            ),
        ];
        assert!(dialogue_self_identifies_speaker(
            "Peig Hannigan's the name. What brings ye here?",
            "Peig Hannigan",
            "Widow",
            &roster,
        ));
        assert!(dialogue_self_identifies_speaker(
            "I'm Peig Hannigan. What brings ye here?",
            "Peig Hannigan",
            "Widow",
            &roster,
        ));
        assert!(!dialogue_self_identifies_speaker(
            "Good morning. What brings ye here?",
            "Peig Hannigan",
            "Widow",
            &roster,
        ));
        assert!(!dialogue_self_identifies_speaker(
            "Peig Hannigan lives beyond the crossroads.",
            "Peig Hannigan",
            "Widow",
            &roster,
        ));
        assert!(!dialogue_self_identifies_speaker(
            "I'm Peig Hannigan's cousin from beyond the crossroads.",
            "Peig Hannigan",
            "Widow",
            &roster,
        ));
        assert!(dialogue_self_identifies_speaker(
            "I'm Father Declan Tierney, the parish priest.",
            "Fr. Declan Tierney",
            "Parish Priest",
            &roster,
        ));
    }

    fn identity_roster() -> Vec<(String, String)> {
        vec![
            ("Seamus Gallagher".to_string(), "Blacksmith".to_string()),
            ("Peig Hannigan".to_string(), "Widow".to_string()),
            (
                "Fr. Declan Tierney".to_string(),
                "Parish Priest".to_string(),
            ),
        ]
    }

    #[test]
    fn unique_first_name_and_authored_occupation_reveal_identity() {
        let roster = identity_roster();
        assert!(dialogue_self_identifies_speaker(
            "Aye, I'm Seamus, the blacksmith. What can I do for ye?",
            "Seamus Gallagher",
            "Blacksmith",
            &roster,
        ));
        assert!(dialogue_self_identifies_speaker(
            "I am Declan, the parish priest.",
            "Fr. Declan Tierney",
            "Parish Priest",
            &roster,
        ));
    }

    #[test]
    fn first_name_identity_requires_unique_roster_name_and_correct_occupation() {
        let mut ambiguous = identity_roster();
        ambiguous.push(("Seamus Brennan".to_string(), "Farmer".to_string()));
        assert!(!dialogue_self_identifies_speaker(
            "I'm Seamus, the blacksmith.",
            "Seamus Gallagher",
            "Blacksmith",
            &ambiguous,
        ));
        assert!(!dialogue_self_identifies_speaker(
            "I'm Seamus.",
            "Seamus Gallagher",
            "Blacksmith",
            &identity_roster(),
        ));
        assert!(!dialogue_self_identifies_speaker(
            "I'm Seamus, the farmer.",
            "Seamus Gallagher",
            "Blacksmith",
            &identity_roster(),
        ));
    }

    #[test]
    fn identity_detection_rejects_indirect_possessive_negated_and_reported_forms() {
        let roster = identity_roster();
        for dialogue in [
            "Seamus is the blacksmith.",
            "I know Seamus, the blacksmith.",
            "I'm Seamus's brother, the blacksmith's apprentice.",
            "I'm not Seamus, the blacksmith.",
            "Peig told me, I'm Seamus, the blacksmith.",
            "He said: I'm Seamus, the blacksmith.",
        ] {
            assert!(
                !dialogue_self_identifies_speaker(
                    dialogue,
                    "Seamus Gallagher",
                    "Blacksmith",
                    &roster,
                ),
                "indirect identity form must not reveal the speaker: {dialogue:?}"
            );
        }
    }
}
