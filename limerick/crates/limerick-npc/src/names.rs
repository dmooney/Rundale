//! Player-name detection and mentioned-people / greeting validation.

use super::*;

/// Detects if the player is introducing themselves by name.
///
/// Matches patterns like "My name is Ciaran", "I'm Ciaran", "Call me Ciaran".
/// Returns the extracted name if found.
pub fn detect_player_name(input: &str) -> Option<String> {
    use regex::Regex;
    use std::sync::LazyLock;

    static NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)(?:my name(?:'s| is)|I'm|I am|they call me|call me|the name's|name is)\s+(?-i:([A-Z][a-zA-Z']+(?:\s+[A-Z][a-zA-Z']+)?))",
        )
        .unwrap()
    });

    NAME_RE.captures(input).and_then(|caps| -> Option<String> {
        let name = caps.get(1)?.as_str().to_string();
        // Reject very short names (likely false positives)
        if name.len() < 2 {
            return None;
        }
        Some(name)
    })
}

/// Validates the people mentioned in an NPC's dialogue against a known roster.
///
/// Returns a list of hallucinated names — names that appear in `mentioned`
/// but don't match any entry in the roster, the player's name, or known
/// location names.
pub fn validate_mentioned_people(
    mentioned: &[String],
    known_roster: &[(NpcId, String, String)],
    player_name: Option<&str>,
) -> Vec<String> {
    if mentioned.is_empty() {
        return Vec::new();
    }

    let mut hallucinated = Vec::new();
    for name in mentioned {
        let lower = name.to_lowercase();
        // Skip empty names
        if lower.is_empty() {
            continue;
        }

        // Check against player name
        if player_name.is_some_and(|pn| pn.to_lowercase() == lower) {
            continue;
        }

        // Check against roster (full name or first name match)
        let in_roster = known_roster.iter().any(|(_, roster_name, _)| {
            let roster_lower = roster_name.to_lowercase();
            roster_lower == lower
                || roster_lower
                    .split_whitespace()
                    .next()
                    .is_some_and(|first| first == lower)
        });

        if !in_roster {
            hallucinated.push(name.clone());
        }
    }
    hallucinated
}

/// Returns a directive forbidding greetings that are wrong for the current time of day.
///
/// Small models sometimes ignore the affirmative "greet accordingly" cue and
/// fall back to the training-data majority ("good morning"). A negative
/// constraint paired with the positive cue is more reliable (#1225).
///
/// Only emits a directive when the time of day is clearly NOT morning —
/// Morning and Midday have no forbidden-greeting because "good morning" and
/// "good day" are appropriate for those buckets.
pub fn forbidden_greeting_directive(time_of_day: TimeOfDay) -> Option<&'static str> {
    match time_of_day {
        TimeOfDay::Dawn => Some(
            "Do NOT say 'good morning' — it is only just dawning. \
             A simple 'Dia dhuit' or 'good day to ye' is right.",
        ),
        TimeOfDay::Afternoon => Some(
            "Do NOT say 'good morning' — it is the afternoon. \
             'Good afternoon' or 'good day' is fitting.",
        ),
        TimeOfDay::Dusk => Some(
            "Do NOT say 'good morning' or 'good day' — it is dusk, \
             the sun is low. 'Good evening' or 'good e'en to ye' is right.",
        ),
        TimeOfDay::Night => Some(
            "Do NOT say 'good morning' or 'good day' — it is night. \
             'Good evening' or 'good night' is fitting.",
        ),
        TimeOfDay::Midnight => Some(
            "Do NOT say 'good morning' — it is well past midnight. \
             A hushed greeting or simple nod is in order.",
        ),
        // Morning and Midday: "good morning" / "good day" are appropriate.
        TimeOfDay::Morning | TimeOfDay::Midday => None,
    }
}

/// `text` folded for matching names the player types: lowercased, with
/// accents removed from precomposed Latin letters and combining marks
/// dropped. "Roisin", "róisín" and "Róisín" typed with combining fadas all
/// fold to "roisin" (design `docs/design/dynamic-npc-context.md` §3.5,
/// #2167). Every name matcher and the `/debug` lookup fold with this one
/// function.
pub fn fold_name(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !matches!(*c, '\u{0300}'..='\u{036F}'))
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
            'ď' => 'd',
            'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
            'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
            'ĥ' => 'h',
            'í' | 'ì' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
            'ĵ' => 'j',
            'ķ' => 'k',
            'ĺ' | 'ļ' | 'ľ' => 'l',
            'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ō' | 'ŏ' | 'ő' => 'o',
            'ŕ' | 'ŗ' | 'ř' => 'r',
            'ś' | 'ŝ' | 'ş' | 'š' => 's',
            'ţ' | 'ť' => 't',
            'ú' | 'ù' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
            'ŵ' => 'w',
            'ý' | 'ÿ' | 'ŷ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod fold_tests {
    use super::fold_name;

    #[test]
    fn precomposed_and_combining_fadas_fold_to_the_same_name() {
        assert_eq!(fold_name("Róisín"), "roisin");
        assert_eq!(fold_name("ROISIN"), "roisin");
        assert_eq!(fold_name("Ro\u{301}isi\u{301}n"), "roisin");
        assert_eq!(fold_name("Mícheál Connolly"), "micheal connolly");
        assert_eq!(fold_name("Mi\u{301}chea\u{301}l"), "micheal");
        assert_eq!(fold_name("Seán"), "sean");
    }
}
