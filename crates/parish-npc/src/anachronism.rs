//! Anachronism detection for player input in the 1820 Ireland setting.
//!
//! Scans player text for words, phrases, and concepts that would not exist
//! in 1820 Ireland, then produces a context alert that can be injected into
//! the NPC's LLM prompt so the character can respond in-period.
//!
//! The checker uses a static dictionary of anachronistic terms organized by
//! category (technology, language, concepts, etc.) with invention/origin dates.
//! It applies word-boundary matching to minimize false positives.

use std::fmt;

/// A single anachronistic term detected in player input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anachronism {
    /// The anachronistic word or phrase that was matched.
    pub term: String,
    /// The category of anachronism (e.g. "technology", "slang").
    pub category: AnachronismCategory,
    /// Approximate year the term/concept originated or became common.
    pub origin_year: u16,
    /// A brief note on why this is anachronistic.
    pub note: &'static str,
}

/// Categories of anachronistic terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnachronismCategory {
    /// Post-1820 technology (telegraph, train, etc.).
    Technology,
    /// Modern slang or idiom not in use in 1820.
    Slang,
    /// Concepts, institutions, or movements that postdate 1820.
    Concept,
    /// Products, brands, or materials not yet available.
    Material,
    /// Units, measurements, or standards not yet established.
    Measurement,
}

impl fmt::Display for AnachronismCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnachronismCategory::Technology => write!(f, "technology"),
            AnachronismCategory::Slang => write!(f, "slang"),
            AnachronismCategory::Concept => write!(f, "concept"),
            AnachronismCategory::Material => write!(f, "material"),
            AnachronismCategory::Measurement => write!(f, "measurement"),
        }
    }
}

/// A dictionary entry for an anachronistic term.
struct DictEntry {
    /// The term to match (lowercase).
    term: &'static str,
    category: AnachronismCategory,
    origin_year: u16,
    note: &'static str,
}

/// Static dictionary of anachronistic terms for 1820 Ireland.
///
/// Each entry specifies a term, its category, approximate origin year,
/// and a brief explanation. Terms are matched with word boundaries to
/// avoid false positives (e.g. "train" in "training" is not flagged
/// because "train" as a vehicle didn't exist yet, but the word "train"
/// meaning "to instruct" did — so we only flag "railway" and "railroad").
const ANACHRONISM_DICT: &[DictEntry] = &[
    // === Technology ===
    DictEntry {
        term: "telephone",
        category: AnachronismCategory::Technology,
        origin_year: 1876,
        note: "invented by Bell in 1876",
    },
    DictEntry {
        term: "phone",
        category: AnachronismCategory::Technology,
        origin_year: 1876,
        note: "short for telephone, 1876",
    },
    DictEntry {
        term: "telegraph",
        category: AnachronismCategory::Technology,
        origin_year: 1837,
        note: "electric telegraph from the 1830s",
    },
    DictEntry {
        term: "railway",
        category: AnachronismCategory::Technology,
        origin_year: 1825,
        note: "first public railway opened 1825",
    },
    DictEntry {
        term: "railroad",
        category: AnachronismCategory::Technology,
        origin_year: 1825,
        note: "first public railway opened 1825",
    },
    DictEntry {
        term: "locomotive",
        category: AnachronismCategory::Technology,
        origin_year: 1825,
        note: "Stephenson's Rocket era, 1825+",
    },
    DictEntry {
        term: "electricity",
        category: AnachronismCategory::Technology,
        origin_year: 1880,
        note: "electric power distribution from 1880s",
    },
    DictEntry {
        term: "electric",
        category: AnachronismCategory::Technology,
        origin_year: 1880,
        note: "practical electric power from 1880s",
    },
    DictEntry {
        term: "lightbulb",
        category: AnachronismCategory::Technology,
        origin_year: 1879,
        note: "Edison's lightbulb, 1879",
    },
    DictEntry {
        term: "photograph",
        category: AnachronismCategory::Technology,
        origin_year: 1839,
        note: "daguerreotype from 1839",
    },
    DictEntry {
        term: "camera",
        category: AnachronismCategory::Technology,
        origin_year: 1839,
        note: "photographic camera from 1839",
    },
    DictEntry {
        term: "bicycle",
        category: AnachronismCategory::Technology,
        origin_year: 1860,
        note: "velocipede/bicycle from 1860s",
    },
    DictEntry {
        term: "automobile",
        category: AnachronismCategory::Technology,
        origin_year: 1886,
        note: "Benz patent motorcar, 1886",
    },
    DictEntry {
        term: "motorcar",
        category: AnachronismCategory::Technology,
        origin_year: 1886,
        note: "Benz patent motorcar, 1886",
    },
    DictEntry {
        term: "airplane",
        category: AnachronismCategory::Technology,
        origin_year: 1903,
        note: "Wright brothers, 1903",
    },
    DictEntry {
        term: "aeroplane",
        category: AnachronismCategory::Technology,
        origin_year: 1903,
        note: "Wright brothers, 1903",
    },
    DictEntry {
        term: "radio",
        category: AnachronismCategory::Technology,
        origin_year: 1895,
        note: "Marconi's wireless telegraphy, 1895",
    },
    DictEntry {
        term: "television",
        category: AnachronismCategory::Technology,
        origin_year: 1927,
        note: "first electronic television, 1927",
    },
    DictEntry {
        term: "computer",
        category: AnachronismCategory::Technology,
        origin_year: 1940,
        note: "electronic computers from 1940s",
    },
    DictEntry {
        term: "internet",
        category: AnachronismCategory::Technology,
        origin_year: 1969,
        note: "ARPANET, 1969",
    },
    DictEntry {
        term: "smartphone",
        category: AnachronismCategory::Technology,
        origin_year: 2007,
        note: "iPhone launched 2007",
    },
    DictEntry {
        term: "tractor",
        category: AnachronismCategory::Technology,
        origin_year: 1892,
        note: "gasoline tractor from 1892",
    },
    DictEntry {
        term: "dynamite",
        category: AnachronismCategory::Technology,
        origin_year: 1867,
        note: "Nobel patented dynamite in 1867",
    },
    DictEntry {
        term: "machine gun",
        category: AnachronismCategory::Technology,
        origin_year: 1862,
        note: "Gatling gun, 1862",
    },
    DictEntry {
        term: "revolver",
        category: AnachronismCategory::Technology,
        origin_year: 1836,
        note: "Colt revolver patented 1836",
    },
    DictEntry {
        term: "typewriter",
        category: AnachronismCategory::Technology,
        origin_year: 1868,
        note: "practical typewriter from 1868",
    },
    DictEntry {
        term: "gramophone",
        category: AnachronismCategory::Technology,
        origin_year: 1887,
        note: "gramophone from 1887",
    },
    DictEntry {
        term: "phonograph",
        category: AnachronismCategory::Technology,
        origin_year: 1877,
        note: "Edison's phonograph, 1877",
    },
    DictEntry {
        term: "cinema",
        category: AnachronismCategory::Technology,
        origin_year: 1895,
        note: "Lumière brothers, 1895",
    },
    DictEntry {
        term: "movie",
        category: AnachronismCategory::Technology,
        origin_year: 1895,
        note: "motion pictures from 1895",
    },
    // === Slang / Modern Language ===
    DictEntry {
        term: "okay",
        category: AnachronismCategory::Slang,
        origin_year: 1839,
        note: "first recorded use 1839",
    },
    DictEntry {
        term: "cool",
        category: AnachronismCategory::Slang,
        origin_year: 1930,
        note: "slang sense from 1930s jazz era",
    },
    DictEntry {
        term: "awesome",
        category: AnachronismCategory::Slang,
        origin_year: 1960,
        note: "slang sense from 1960s",
    },
    DictEntry {
        term: "dude",
        category: AnachronismCategory::Slang,
        origin_year: 1883,
        note: "first attested 1883",
    },
    DictEntry {
        term: "selfie",
        category: AnachronismCategory::Slang,
        origin_year: 2002,
        note: "term coined early 2000s",
    },
    DictEntry {
        term: "hashtag",
        category: AnachronismCategory::Slang,
        origin_year: 2007,
        note: "Twitter hashtags from 2007",
    },
    DictEntry {
        term: "vibe",
        category: AnachronismCategory::Slang,
        origin_year: 1940,
        note: "slang sense from 1940s jazz",
    },
    DictEntry {
        term: "chill",
        category: AnachronismCategory::Slang,
        origin_year: 1970,
        note: "slang sense from 1970s",
    },
    DictEntry {
        term: "bro",
        category: AnachronismCategory::Slang,
        origin_year: 1970,
        note: "modern slang from 1970s",
    },
    // === Concepts / Institutions ===
    DictEntry {
        term: "communism",
        category: AnachronismCategory::Concept,
        origin_year: 1848,
        note: "Communist Manifesto published 1848",
    },
    DictEntry {
        term: "socialism",
        category: AnachronismCategory::Concept,
        origin_year: 1830,
        note: "term in common use from 1830s",
    },
    DictEntry {
        term: "darwinism",
        category: AnachronismCategory::Concept,
        origin_year: 1859,
        note: "Origin of Species published 1859",
    },
    DictEntry {
        term: "evolution",
        category: AnachronismCategory::Concept,
        origin_year: 1859,
        note: "Darwinian evolution from 1859",
    },
    DictEntry {
        term: "feminism",
        category: AnachronismCategory::Concept,
        origin_year: 1837,
        note: "term coined in 1837",
    },
    DictEntry {
        term: "famine",
        category: AnachronismCategory::Concept,
        origin_year: 1845,
        note: "the Great Famine began in 1845 — hasn't happened yet in 1820",
    },
    DictEntry {
        term: "home rule",
        category: AnachronismCategory::Concept,
        origin_year: 1870,
        note: "Irish Home Rule movement from 1870s",
    },
    DictEntry {
        term: "fenian",
        category: AnachronismCategory::Concept,
        origin_year: 1858,
        note: "Fenian Brotherhood founded 1858",
    },
    DictEntry {
        term: "sinn fein",
        category: AnachronismCategory::Concept,
        origin_year: 1905,
        note: "Sinn Féin founded 1905",
    },
    DictEntry {
        term: "republic",
        category: AnachronismCategory::Concept,
        origin_year: 1916,
        note: "Irish Republic proclaimed 1916",
    },
    // === Materials / Products ===
    DictEntry {
        term: "plastic",
        category: AnachronismCategory::Material,
        origin_year: 1907,
        note: "Bakelite, first synthetic plastic, 1907",
    },
    DictEntry {
        term: "rubber",
        category: AnachronismCategory::Material,
        origin_year: 1844,
        note: "vulcanized rubber from 1844",
    },
    DictEntry {
        term: "aspirin",
        category: AnachronismCategory::Material,
        origin_year: 1899,
        note: "Bayer aspirin from 1899",
    },
    DictEntry {
        term: "penicillin",
        category: AnachronismCategory::Material,
        origin_year: 1928,
        note: "discovered by Fleming in 1928",
    },
    DictEntry {
        term: "diesel",
        category: AnachronismCategory::Material,
        origin_year: 1893,
        note: "Rudolf Diesel's engine, 1893",
    },
    DictEntry {
        term: "petrol",
        category: AnachronismCategory::Material,
        origin_year: 1860,
        note: "petroleum fuel from 1860s",
    },
    DictEntry {
        term: "gasoline",
        category: AnachronismCategory::Material,
        origin_year: 1860,
        note: "petroleum fuel from 1860s",
    },
    DictEntry {
        term: "cement",
        category: AnachronismCategory::Material,
        origin_year: 1824,
        note: "Portland cement patented 1824",
    },
    // === Measurements ===
    DictEntry {
        term: "celsius",
        category: AnachronismCategory::Measurement,
        origin_year: 1948,
        note: "renamed from centigrade in 1948",
    },
    DictEntry {
        term: "watt",
        category: AnachronismCategory::Measurement,
        origin_year: 1882,
        note: "adopted as unit in 1882",
    },
    DictEntry {
        term: "volt",
        category: AnachronismCategory::Measurement,
        origin_year: 1881,
        note: "adopted as unit in 1881",
    },
    DictEntry {
        term: "kilowatt",
        category: AnachronismCategory::Measurement,
        origin_year: 1882,
        note: "electric power unit, 1882",
    },
];

/// Checks player input for anachronistic terms.
///
/// Returns a list of detected anachronisms. Uses case-insensitive matching
/// with word-boundary detection to avoid false positives (e.g. "train"
/// inside "training" won't match the "train" entry, because we check
/// for whole-word matches).
///
/// # Examples
///
/// ```
/// use parish_npc::anachronism::check_input;
///
/// let hits = check_input("Can I take the telephone to call someone?");
/// assert_eq!(hits.len(), 1);
/// assert_eq!(hits[0].term, "telephone");
/// ```
pub fn check_input(input: &str) -> Vec<Anachronism> {
    let lower = input.to_lowercase();
    let mut results = Vec::new();

    for entry in ANACHRONISM_DICT {
        if has_word_match(&lower, entry.term) {
            results.push(Anachronism {
                term: entry.term.to_string(),
                category: entry.category,
                origin_year: entry.origin_year,
                note: entry.note,
            });
        }
    }

    results
}

/// Checks whether `haystack` contains `needle` as a whole word or phrase.
///
/// A match is considered "whole word" if the characters immediately before
/// and after the match are not alphanumeric (or the match is at the
/// start/end of the string). This prevents "phone" from matching inside
/// "saxophone" or "train" inside "training".
fn has_word_match(haystack: &str, needle: &str) -> bool {
    let needle_bytes = needle.as_bytes();
    let hay_bytes = haystack.as_bytes();

    if needle_bytes.len() > hay_bytes.len() {
        return false;
    }

    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs_pos = start + pos;
        let end_pos = abs_pos + needle_bytes.len();

        let before_ok = abs_pos == 0 || !haystack.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
        let after_ok = end_pos >= hay_bytes.len() || !hay_bytes[end_pos].is_ascii_alphanumeric();

        if before_ok && after_ok {
            return true;
        }

        // Advance past this match to avoid infinite loop
        start = abs_pos + 1;
        if start >= haystack.len() {
            break;
        }
    }

    false
}

/// Formats detected anachronisms into a context alert for injection into
/// the NPC's LLM prompt.
///
/// The alert instructs the NPC to respond in-character to the anachronistic
/// input — e.g., expressing confusion about unfamiliar concepts rather than
/// breaking immersion by using the anachronistic term.
///
/// Returns `None` if no anachronisms were detected.
///
/// # Examples
///
/// ```
/// use parish_npc::anachronism::{check_input, format_context_alert};
///
/// let hits = check_input("I want to take a photograph");
/// let alert = format_context_alert(&hits);
/// assert!(alert.is_some());
/// assert!(alert.unwrap().contains("photograph"));
/// ```
pub fn format_context_alert(anachronisms: &[Anachronism]) -> Option<String> {
    if anachronisms.is_empty() {
        return None;
    }

    let mut alert = String::from(
        "\nANACHRONISM ALERT: The player used words/concepts that do not exist in 1820. \
         Respond in character — your character would NOT understand these references. \
         React with authentic confusion, curiosity, or folk interpretation. Do NOT use \
         the anachronistic terms yourself. Specific anachronisms detected:\n",
    );

    for a in anachronisms {
        alert.push_str(&format!(
            "- \"{}\": {} (not until ~{})\n",
            a.term, a.note, a.origin_year
        ));
    }

    alert.push_str(
        "\nStay in character. Express genuine bewilderment at unfamiliar words. \
         You may guess at meaning from context, mishear the word as something period-appropriate, \
         or ask the player to explain in plainer terms.",
    );

    Some(alert)
}

/// A term detected by the data-driven anachronism checker.
///
/// Unlike [`Anachronism`] which references the static dictionary, this
/// type owns all its strings and works with mod-loaded data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedTerm {
    /// The anachronistic word or phrase that was matched.
    pub term: String,
    /// A brief note on why this is anachronistic.
    pub reason: String,
}

/// Checks player input against mod-loaded anachronism entries.
///
/// Works identically to [`check_input`] but uses data from a
/// [`GameMod`](parish_core::game_mod::GameMod) instead of the static dictionary.
pub fn check_input_from_mod_data(
    input: &str,
    entries: &[parish_types::AnachronismEntry],
) -> Vec<DetectedTerm> {
    let lower = input.to_lowercase();
    let mut results = Vec::new();

    for entry in entries {
        if has_word_match(&lower, &entry.term) {
            results.push(DetectedTerm {
                term: entry.term.clone(),
                reason: entry.note.clone(),
            });
        }
    }

    results
}

/// Formats detected anachronisms using mod-provided alert prefix and suffix.
///
/// Works identically to [`format_context_alert`] but uses text from the mod's
/// `anachronisms.json` instead of hardcoded strings.
pub fn format_context_alert_from_mod_data(
    detected: &[DetectedTerm],
    prefix: &str,
    suffix: &str,
) -> Option<String> {
    if detected.is_empty() {
        return None;
    }

    let mut alert = format!("\n{}\n", prefix);

    for d in detected {
        alert.push_str(&format!("- \"{}\": {}\n", d.term, d.reason));
    }

    alert.push_str(&format!("\n{}", suffix));

    Some(alert)
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Detection tests ===

    #[test]
    fn test_detect_telephone() {
        let hits = check_input("Can I use the telephone?");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "telephone");
        assert_eq!(hits[0].category, AnachronismCategory::Technology);
        assert_eq!(hits[0].origin_year, 1876);
    }

    #[test]
    fn test_detect_multiple() {
        let hits = check_input("Let me take a photograph with my smartphone");
        assert!(hits.len() >= 2);
        let terms: Vec<&str> = hits.iter().map(|a| a.term.as_str()).collect();
        assert!(terms.contains(&"photograph"));
        assert!(terms.contains(&"smartphone"));
    }

    #[test]
    fn test_detect_case_insensitive() {
        let hits = check_input("Have you seen the TELEGRAPH office?");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "telegraph");
    }

    #[test]
    fn test_detect_railway() {
        let hits = check_input("When does the railway arrive?");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "railway");
    }

    #[test]
    fn test_detect_electricity() {
        let hits = check_input("Do you have electricity here?");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "electricity");
    }

    #[test]
    fn test_detect_slang_okay() {
        let hits = check_input("Okay, I understand");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "okay");
        assert_eq!(hits[0].category, AnachronismCategory::Slang);
    }

    #[test]
    fn test_detect_concept_famine() {
        let hits = check_input("Will there be a famine soon?");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "famine");
        assert_eq!(hits[0].category, AnachronismCategory::Concept);
    }

    #[test]
    fn test_detect_material_plastic() {
        let hits = check_input("Is that made of plastic?");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "plastic");
        assert_eq!(hits[0].category, AnachronismCategory::Material);
    }

    #[test]
    fn test_detect_multi_word_phrase() {
        let hits = check_input("Is that a machine gun?");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "machine gun");
    }

    // === False positive avoidance ===

    #[test]
    fn test_no_false_positive_period_appropriate() {
        let hits = check_input("Good morning to ye! Fine day for walking.");
        assert!(hits.is_empty());
    }

    #[test]
    fn test_no_false_positive_common_words() {
        let hits = check_input("I'll have a pint of porter and some bread");
        assert!(hits.is_empty());
    }

    #[test]
    fn test_no_false_positive_phone_in_word() {
        // "phone" should not match inside "saxophone" or similar
        let hits = check_input("He plays the saxophone beautifully");
        assert!(
            hits.is_empty(),
            "Should not match 'phone' inside 'saxophone'"
        );
    }

    #[test]
    fn test_no_false_positive_cool_temperature() {
        // "cool" as slang is tricky — our checker flags it.
        // This is a known trade-off; the alert helps the LLM decide.
        let hits = check_input("The evening air grows cool");
        // We accept that "cool" matches — the LLM context alert
        // will still produce good results since the NPC prompt
        // is instructed to interpret based on context.
        assert_eq!(
            hits.len(),
            1,
            "cool matches even in temperature sense — known trade-off"
        );
    }

    #[test]
    fn test_no_false_positive_republic_in_sentence() {
        // "republic" should match as a standalone word
        let hits = check_input("We need a republic!");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn test_no_false_positive_empty_input() {
        let hits = check_input("");
        assert!(hits.is_empty());
    }

    #[test]
    fn test_no_false_positive_irish_words() {
        let hits = check_input("Dia dhuit! Conas atá tú?");
        assert!(hits.is_empty());
    }

    #[test]
    fn test_no_false_positive_period_items() {
        let hits = check_input("Pass me the candle and flint, would ye?");
        assert!(hits.is_empty());
    }

    // === Word boundary tests ===

    #[test]
    fn test_word_boundary_start_of_string() {
        let hits = check_input("telephone is ringing");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn test_word_boundary_end_of_string() {
        let hits = check_input("I need a telephone");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn test_word_boundary_with_punctuation() {
        let hits = check_input("Where's the telephone?");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn test_word_boundary_comma_separated() {
        let hits = check_input("A telephone, a camera, and a bicycle");
        assert_eq!(hits.len(), 3);
    }

    // === format_context_alert tests ===

    #[test]
    fn test_alert_none_for_empty() {
        let alert = format_context_alert(&[]);
        assert!(alert.is_none());
    }

    #[test]
    fn test_alert_contains_term() {
        let hits = check_input("Where is the telephone?");
        let alert = format_context_alert(&hits).unwrap();
        assert!(alert.contains("telephone"));
        assert!(alert.contains("1876"));
        assert!(alert.contains("ANACHRONISM ALERT"));
    }

    #[test]
    fn test_alert_contains_multiple_terms() {
        let hits = check_input("Take a photograph and send it by telegraph");
        let alert = format_context_alert(&hits).unwrap();
        assert!(alert.contains("photograph"));
        assert!(alert.contains("telegraph"));
    }

    #[test]
    fn test_alert_instructs_confusion() {
        let hits = check_input("I'll check the internet");
        let alert = format_context_alert(&hits).unwrap();
        assert!(alert.contains("bewilderment"));
        assert!(alert.contains("Stay in character"));
    }

    // === has_word_match unit tests ===

    #[test]
    fn test_word_match_basic() {
        assert!(has_word_match("hello world", "hello"));
        assert!(has_word_match("hello world", "world"));
    }

    #[test]
    fn test_word_match_not_substring() {
        assert!(!has_word_match("saxophone", "phone"));
        assert!(!has_word_match("training", "rain"));
    }

    #[test]
    fn test_word_match_with_punctuation() {
        assert!(has_word_match("hello, world!", "world"));
        assert!(has_word_match("(hello)", "hello"));
    }

    #[test]
    fn test_word_match_exact() {
        assert!(has_word_match("phone", "phone"));
    }

    #[test]
    fn test_word_match_empty_needle() {
        // Empty needle doesn't produce a word-boundary match.
        // This is fine — we never have empty entries in the dictionary.
        assert!(!has_word_match("anything", ""));
    }

    #[test]
    fn test_word_match_empty_haystack() {
        assert!(!has_word_match("", "phone"));
    }

    // === Data-driven anachronism tests ===

    #[test]
    fn test_check_input_from_mod_data() {
        let entries = vec![
            parish_types::AnachronismEntry {
                term: "telephone".to_string(),
                note: "invented 1876".to_string(),
                category: None,
                origin_year: None,
            },
            parish_types::AnachronismEntry {
                term: "internet".to_string(),
                note: "developed 1960s".to_string(),
                category: None,
                origin_year: None,
            },
        ];

        let hits = check_input_from_mod_data("Where is the telephone?", &entries);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "telephone");
        assert_eq!(hits[0].reason, "invented 1876");
    }

    #[test]
    fn test_check_input_from_mod_data_no_match() {
        let entries = vec![parish_types::AnachronismEntry {
            term: "telephone".to_string(),
            note: "invented 1876".to_string(),
            category: None,
            origin_year: None,
        }];

        let hits = check_input_from_mod_data("Good morning!", &entries);
        assert!(hits.is_empty());
    }

    #[test]
    fn test_format_context_alert_from_mod_data() {
        let detected = vec![DetectedTerm {
            term: "telephone".to_string(),
            reason: "invented 1876".to_string(),
        }];

        let alert =
            format_context_alert_from_mod_data(&detected, "ALERT: anachronisms!", "Stay in role.")
                .unwrap();
        assert!(alert.contains("ALERT: anachronisms!"));
        assert!(alert.contains("telephone"));
        assert!(alert.contains("Stay in role."));
    }

    #[test]
    fn test_format_context_alert_from_mod_data_empty() {
        let alert =
            format_context_alert_from_mod_data(&[], "ALERT: anachronisms!", "Stay in role.");
        assert!(alert.is_none());
    }

    // === AnachronismCategory display ===

    #[test]
    fn test_category_display() {
        assert_eq!(AnachronismCategory::Technology.to_string(), "technology");
        assert_eq!(AnachronismCategory::Slang.to_string(), "slang");
        assert_eq!(AnachronismCategory::Concept.to_string(), "concept");
        assert_eq!(AnachronismCategory::Material.to_string(), "material");
        assert_eq!(AnachronismCategory::Measurement.to_string(), "measurement");
    }

    // === Dictionary coverage ===

    #[test]
    fn test_dictionary_not_empty() {
        assert!(
            !ANACHRONISM_DICT.is_empty(),
            "dictionary should have entries"
        );
    }

    #[test]
    fn test_all_entries_post_1820() {
        for entry in ANACHRONISM_DICT {
            assert!(
                entry.origin_year > 1820 || entry.category == AnachronismCategory::Concept,
                "Entry '{}' has origin_year {} which should be after 1820 \
                 (or be a Concept category for events that haven't happened yet)",
                entry.term,
                entry.origin_year
            );
        }
    }

    #[test]
    fn test_all_entries_have_notes() {
        for entry in ANACHRONISM_DICT {
            assert!(
                !entry.note.is_empty(),
                "Entry '{}' should have a non-empty note",
                entry.term
            );
        }
    }
}
