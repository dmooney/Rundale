//! Player-name detection, greeting directives, and name folding.

use super::*;

/// A player's self-introduction found in a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerIntroduction {
    /// The name the player gave.
    pub name: String,
    /// Whether the line names the player outright ("my name is", "call
    /// me", "the name's"), rather than with "I'm"/"I am", which also
    /// introduces nationalities and moods ("I'm Irish", "I am Sorry").
    pub explicit: bool,
}

/// Capitalised words that follow "I'm"/"I am" without being a name.
const NOT_A_NAME_AFTER_I_AM: &[&str] = &[
    "Irish",
    "English",
    "British",
    "Scottish",
    "Scots",
    "Welsh",
    "American",
    "French",
    "Spanish",
    "German",
    "Catholic",
    "Protestant",
    "Christian",
    "Sorry",
    "Fine",
    "Grand",
    "Glad",
    "Afraid",
    "Sure",
    "Not",
    "Just",
    "Only",
    "Here",
    "Back",
    "From",
    "Going",
    "Looking",
    "Well",
    "Tired",
    "Lost",
    "New",
    "Happy",
    "Delighted",
    "Very",
    "So",
    "A",
    "An",
    "The",
    "Your",
    "His",
    "Her",
    "Their",
    "Our",
    "My",
    "Mr",
    "Mrs",
    "Miss",
];

/// Detects if the player is introducing themselves by name.
///
/// Matches patterns like "My name is Ciaran", "I'm Ciaran", "Call me Ciaran".
/// Names may carry accented letters ("Seán", "Tomás", precomposed or with a
/// combining accent) and the typographic apostrophe an iPhone keyboard
/// types ("I’m"). Someone else's name ("his name is Daniel") is not the
/// player's, and "I'm"/"I am" before a nationality or mood ("I'm Irish")
/// is not a name. Returns the first introduction found.
pub fn detect_player_introduction(input: &str) -> Option<PlayerIntroduction> {
    use regex::Regex;
    use std::sync::LazyLock;

    static NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(?:(?:(?P<owner>my|the|his|her|their|your|whose|its)\s+)?name(?:['’]s|\s+is)|(?P<iam>I['’]m|I\s+am)|(?:they\s+)?call\s+me)\s+(?-i:(?P<name>\p{Lu}[\p{L}\p{M}'’]+(?:\s+\p{Lu}[\p{L}\p{M}'’]+)?))",
        )
        .unwrap()
    });

    NAME_RE.captures_iter(input).find_map(|caps| {
        if let Some(owner) = caps.name("owner") {
            let owner = owner.as_str().to_lowercase();
            if owner != "my" && owner != "the" {
                return None;
            }
        }
        let name = caps.name("name")?.as_str();
        let explicit = caps.name("iam").is_none();
        if !explicit {
            let first = name.split_whitespace().next().unwrap_or(name);
            if NOT_A_NAME_AFTER_I_AM.contains(&first) {
                return None;
            }
        }
        // "I'm Mícheál's neighbour" names someone else.
        if name.ends_with("'s") || name.ends_with("’s") {
            return None;
        }
        // Reject very short names (likely false positives)
        if name.chars().count() < 2 {
            return None;
        }
        Some(PlayerIntroduction {
            name: name.to_string(),
            explicit,
        })
    })
}

/// The name in the player's self-introduction, if the line has one. See
/// [`detect_player_introduction`].
pub fn detect_player_name(input: &str) -> Option<String> {
    detect_player_introduction(input).map(|intro| intro.name)
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
