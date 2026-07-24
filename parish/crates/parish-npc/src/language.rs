//! Locale directive construction and curated secondary-language phrase guides.

use std::collections::HashSet;

use parish_types::LanguageHint;

/// Language settings derived from the active mod manifest.
///
/// Carries the BCP 47 locale codes that are injected into every dialogue
/// prompt builder via [`language_directive`].
#[derive(Debug, Clone)]
pub struct LanguageSettings {
    /// BCP 47 tag for the primary dialogue language (e.g. `"en-IE"`).
    pub player: String,
    /// BCP 47 tag for the secondary code-switch language, if any (e.g. `"ga-IE"`).
    pub native: Option<String>,
}

impl LanguageSettings {
    /// Constructs a new `LanguageSettings` from a player language and an
    /// optional native language.
    pub fn new(player: impl Into<String>, native: Option<String>) -> Self {
        Self {
            player: player.into(),
            native,
        }
    }

    /// Convenience constructor for tests or monolingual fallbacks.
    pub fn english_only() -> Self {
        Self {
            player: "en".to_string(),
            native: None,
        }
    }
}

/// Curated ga-IE phrase list appended to the directive when native is
/// Irish. The May 2026 Opus-blind eval found that Qwen2.5 knows *where*
/// to drop a sprinkle but often picks ungrammatical or anachronistic
/// Irish — e.g. "Tá mo chuid lánaí eile" ("my share of other children"
/// literally). A small list of pre-vetted phrases lifts authenticity
/// without changing model.
///
/// Categories cover the situations most likely to come up in NPC
/// dialogue: greetings, blessings, exclamations, terms of endearment,
/// thanks, and a few period-appropriate plant/herb names. Each entry
/// includes a brief English gloss so the model can match phrase to
/// register.
/// Pig Latin phrase guide appended to the directive when native is `x-pig-lat`.
///
/// Gives the model the transformation rules and a small set of pre-verified
/// forms so NPCs produce recognisable Pig Latin rather than garbled output.
const PIG_LAT_PHRASE_GUIDE: &str = "\n\
    Pig Latin rules: move the initial consonant cluster to the end and add \
    \"ay\" (e.g. \"pig\" → \"igpay\", \"street\" → \"eetstray\", \
    \"there\" → \"erethay\"). \
    For words starting with a vowel, append \"way\" \
    (e.g. \"inside\" → \"insideway\", \"all\" → \"allway\"). \
    Pre-verified forms you may use: ellohay (hello), oodgay (good), \
    ayday (day), ankthay ouyay (thank you), easyplay (please), \
    eresway (where's), atwhay (what), oday ouyay (do you), \
    iendsfray (friends), omingcay (coming).";

#[derive(Clone, Copy)]
struct CanonicalLanguageHint {
    word: &'static str,
    pronunciation: &'static str,
    meaning: &'static str,
}

/// Canonical Irish hint records whose metadata may reach the player.
///
/// This is deliberately authored data rather than a language detector. The
/// model may nominate a phrase, but it cannot supply the pronunciation or
/// translation shown in the UI (#1789).
const GA_IE_CANONICAL_HINTS: &[CanonicalLanguageHint] = &[
    CanonicalLanguageHint {
        word: "Dia dhuit",
        pronunciation: "DEE-ah gwit",
        meaning: "hello",
    },
    CanonicalLanguageHint {
        word: "Dia is Muire dhuit",
        pronunciation: "DEE-ah iss MWIR-ah gwit",
        meaning: "hello in reply",
    },
    CanonicalLanguageHint {
        word: "Conas atá tú",
        pronunciation: "KUN-us ah-TAW too",
        meaning: "how are you",
    },
    CanonicalLanguageHint {
        word: "Slán",
        pronunciation: "slawn",
        meaning: "goodbye",
    },
    CanonicalLanguageHint {
        word: "Slán abhaile",
        pronunciation: "slawn ah-WAL-yah",
        meaning: "safe home",
    },
    CanonicalLanguageHint {
        word: "Slán leat",
        pronunciation: "slawn lat",
        meaning: "goodbye",
    },
    CanonicalLanguageHint {
        word: "Go raibh maith agat",
        pronunciation: "guh rev mah AH-gut",
        meaning: "thank you",
    },
    CanonicalLanguageHint {
        word: "Le cúnamh Dé",
        pronunciation: "leh KOO-nuv day",
        meaning: "with God's help",
    },
    CanonicalLanguageHint {
        word: "Buíochas le Dia",
        pronunciation: "BWEE-khus leh DEE-ah",
        meaning: "thank God",
    },
    CanonicalLanguageHint {
        word: "Beannacht Dé ort",
        pronunciation: "BAN-ukht day ort",
        meaning: "God bless you",
    },
    CanonicalLanguageHint {
        word: "Go n-éirí leat",
        pronunciation: "guh NAY-ree lat",
        meaning: "good luck to you",
    },
    CanonicalLanguageHint {
        word: "Céad míle fáilte",
        pronunciation: "kayd MEE-leh FAWL-cheh",
        meaning: "a hundred thousand welcomes",
    },
    CanonicalLanguageHint {
        word: "Sláinte",
        pronunciation: "SLAWN-cheh",
        meaning: "health; cheers",
    },
    CanonicalLanguageHint {
        word: "Mo ghrá",
        pronunciation: "muh ghraw",
        meaning: "my love",
    },
    CanonicalLanguageHint {
        word: "mo chara",
        pronunciation: "muh KHAR-ah",
        meaning: "my friend",
    },
    CanonicalLanguageHint {
        word: "A chroí",
        pronunciation: "ah khree",
        meaning: "dear; sweetheart",
    },
    CanonicalLanguageHint {
        word: "A stór",
        pronunciation: "ah stohr",
        meaning: "treasure; dear",
    },
    CanonicalLanguageHint {
        word: "A leanbh",
        pronunciation: "ah LAN-uv",
        meaning: "child",
    },
    CanonicalLanguageHint {
        word: "Mhuise",
        pronunciation: "MWISH-eh",
        meaning: "well; indeed",
    },
    CanonicalLanguageHint {
        word: "sídhe",
        pronunciation: "shee",
        meaning: "fairy folk",
    },
    CanonicalLanguageHint {
        word: "sí",
        pronunciation: "shee",
        meaning: "fairy mound",
    },
    CanonicalLanguageHint {
        word: "seanchaí",
        pronunciation: "SHAN-uh-khee",
        meaning: "storyteller",
    },
    CanonicalLanguageHint {
        word: "céilí",
        pronunciation: "KAY-lee",
        meaning: "gathering",
    },
    CanonicalLanguageHint {
        word: "poitín",
        pronunciation: "puh-CHEEN",
        meaning: "illicit spirits",
    },
    CanonicalLanguageHint {
        word: "piseog",
        pronunciation: "pish-OHG",
        meaning: "superstition",
    },
];

/// Canonical records for the test language's pre-verified Pig Latin forms.
const PIG_LAT_CANONICAL_HINTS: &[CanonicalLanguageHint] = &[
    CanonicalLanguageHint {
        word: "ellohay",
        pronunciation: "ell-oh-hay",
        meaning: "hello",
    },
    CanonicalLanguageHint {
        word: "oodgay",
        pronunciation: "ood-gay",
        meaning: "good",
    },
    CanonicalLanguageHint {
        word: "ayday",
        pronunciation: "ay-day",
        meaning: "day",
    },
    CanonicalLanguageHint {
        word: "ankthay ouyay",
        pronunciation: "ank-thay oo-yay",
        meaning: "thank you",
    },
    CanonicalLanguageHint {
        word: "easyplay",
        pronunciation: "eez-ee-play",
        meaning: "please",
    },
    CanonicalLanguageHint {
        word: "eresway",
        pronunciation: "airs-way",
        meaning: "where's",
    },
    CanonicalLanguageHint {
        word: "atwhay",
        pronunciation: "at-way",
        meaning: "what",
    },
    CanonicalLanguageHint {
        word: "oday ouyay",
        pronunciation: "oh-day oo-yay",
        meaning: "do you",
    },
    CanonicalLanguageHint {
        word: "iendsfray",
        pronunciation: "ends-fray",
        meaning: "friends",
    },
    CanonicalLanguageHint {
        word: "omingcay",
        pronunciation: "oh-ming-kay",
        meaning: "coming",
    },
];

const GA_IE_PHRASE_GUIDE: &str = "\n\
    Preferred ga-IE phrases (use these where natural; do not confabulate \
    other Irish): \
    Greetings: \"Dia dhuit\" (hello), \"Dia is Muire dhuit\" (reply), \
    \"Conas atá tú?\" (how are you), \"Slán\" (goodbye), \
    \"Slán abhaile\" (safe home), \"Slán leat\" (goodbye). \
    Blessings / thanks: \"Go raibh maith agat\" (thank you), \
    \"Le cúnamh Dé\" (with God's help), \"Buíochas le Dia\" (thank God), \
    \"Beannacht Dé ort\" (God bless you), \"Go n-éirí leat\" (good luck to you). \
    Welcomes / health: \"Céad míle fáilte\" (a hundred thousand welcomes), \
    \"Sláinte\" (health / cheers), \"mo chara\" (my friend). \
    Exclamations: \"Mo ghrá\" (my love), \"A chroí\" (dear, sweetheart), \
    \"A stór\" (treasure / dear), \"A leanbh\" (child), \"Mhuise\" (well, indeed), \
    \"Faith\", \"Bedad\", \"Bedambut\". \
    Concepts: \"sídhe\" (fairy folk), \"sí\" (fairy mound), \
    \"seanchaí\" (storyteller), \"céilí\" (gathering), \
    \"poitín\" (illicit spirits), \"piseog\" (superstition).";

/// Renders the locale directive injected into every dialogue system prompt.
///
/// Always emits a leading `LANGUAGE: Speak in {player}.` clause, plus
/// spelling-discipline guidance. When the player language is an English
/// variant other than `en-US`, the directive forbids en-US spellings.
/// When a `native` language is set, the directive instructs the model to
/// code-switch naturally and record secondary-language words in the
/// `language_hints` metadata array. When `native` is `ga-IE` specifically,
/// the directive appends a curated phrase list ([`GA_IE_PHRASE_GUIDE`])
/// so the model picks from grammatical, period-appropriate Irish rather
/// than confabulating its own.
///
/// Closes with a character-set guard that forbids non-Latin scripts
/// (Cyrillic, Han, Hiragana, Katakana, Hangul, Arabic, Hebrew, Greek,
/// Devanagari). Multilingual model weights — notably Qwen2.5 — drift
/// into Chinese or Russian mid-sentence on rural-Irish prompts at higher
/// temperatures; the explicit allow-list disciplines the output side.
pub fn language_directive(lang: &LanguageSettings) -> String {
    let player = &lang.player;
    let mut directive = format!(
        "LANGUAGE: Speak in {player}. \
        Use spelling, idioms, and conventions appropriate to that BCP 47 locale."
    );

    let player_lower = player.to_lowercase();
    if player_lower.starts_with("en") && player_lower != "en-us" {
        directive.push_str(&format!(
            " Never use en-US spellings such as \"color\", \"realize\", \
            \"favor\", \"neighbor\", or \"-ize\" verb endings \
            — use the spelling appropriate to {player}."
        ));
    }

    if let Some(native) = &lang.native {
        directive.push_str(&format!(
            " Where a native speaker would naturally code-switch, sprinkle words \
            and short phrases from {native} into your dialogue and record them \
            in the `language_hints` metadata array. \
            CRITICAL: {native} is a SPRINKLE only — at most one short phrase \
            (1-5 words) per reply, woven into otherwise-{player} prose. \
            {player} must carry the meaning of every sentence. \
            NEVER reply entirely in {native}, even if the player's question \
            seems to invite it. The player may not speak {native}; the meaning \
            of your reply must be clear to a {player} speaker who knows zero \
            {native}. \
            Use ONLY {player} and {native} — no other language under any \
            circumstances."
        ));
        if native.eq_ignore_ascii_case("ga-IE") || native.eq_ignore_ascii_case("ga") {
            directive.push_str(GA_IE_PHRASE_GUIDE);
        } else if native.eq_ignore_ascii_case("x-pig-lat") {
            directive.push_str(PIG_LAT_PHRASE_GUIDE);
        }
    } else {
        directive.push_str(&format!(
            " Stay in {player} — do not invent or import other languages."
        ));
    }

    directive.push_str(
        " Every character you emit must be Latin script (a-z, A-Z, accented \
        Latin such as á é í ó ú ü ñ ç ß) or standard punctuation. \
        Do NOT emit Cyrillic (Russian), Han (Chinese), Hiragana / Katakana \
        (Japanese), Hangul (Korean), Arabic, Hebrew, Greek, or Devanagari \
        characters — replace any tempted non-Latin word with its English or \
        native-language equivalent, or omit it.",
    );

    directive
}

/// Validates model-supplied secondary-language metadata against the canonical
/// dialogue actually delivered to the player.
///
/// A hint is accepted only when:
/// - the active setting configures a supported native language;
/// - its word/phrase is from that language's curated prompt inventory;
/// - that phrase appears in the final, post-guard dialogue.
///
/// The returned pronunciation and meaning always come from the canonical
/// record, never model metadata. Duplicate nominations are collapsed
/// case-insensitively and at most one survives, matching the prompt's
/// one-phrase sprinkle contract. Monolingual settings and native languages
/// without a curated inventory return no hints.
pub fn validate_language_hints(
    hints: &[LanguageHint],
    delivered_dialogue: &str,
    language: &LanguageSettings,
) -> Vec<LanguageHint> {
    let Some(native) = language.native.as_deref() else {
        return Vec::new();
    };
    let canonical = if native.eq_ignore_ascii_case("ga-IE") || native.eq_ignore_ascii_case("ga") {
        GA_IE_CANONICAL_HINTS
    } else if native.eq_ignore_ascii_case("x-pig-lat") {
        PIG_LAT_CANONICAL_HINTS
    } else {
        return Vec::new();
    };

    let delivered_lower = delivered_dialogue.to_lowercase();
    let mut seen = HashSet::new();
    hints
        .iter()
        .filter_map(|hint| {
            let nominated = hint.word.trim();
            let record = canonical
                .iter()
                .find(|record| record.word.eq_ignore_ascii_case(nominated))?;
            let key = record.word.to_lowercase();
            if !delivered_lower.contains(&key) || !seen.insert(key) {
                return None;
            }
            Some(LanguageHint {
                word: record.word.to_string(),
                pronunciation: record.pronunciation.to_string(),
                meaning: Some(record.meaning.to_string()),
            })
        })
        .take(1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(word: &str, pronunciation: &str) -> LanguageHint {
        LanguageHint {
            word: word.to_string(),
            pronunciation: pronunciation.to_string(),
            meaning: Some("meaning".to_string()),
        }
    }

    fn canonical_dia_dhuit() -> LanguageHint {
        LanguageHint {
            word: "Dia dhuit".to_string(),
            pronunciation: "DEE-ah gwit".to_string(),
            meaning: Some("hello".to_string()),
        }
    }

    #[test]
    fn ordinary_english_word_is_not_a_secondary_language_hint() {
        let language = LanguageSettings::new("en-IE", Some("ga-IE".to_string()));
        let result = validate_language_hints(
            &[hint("whispers", "WISP-urs")],
            "It is more the whispers ye hear here.",
            &language,
        );
        assert!(result.is_empty(), "ordinary English must be rejected");
    }

    #[test]
    fn approved_hint_must_appear_exactly_in_delivered_dialogue() {
        let language = LanguageSettings::new("en-IE", Some("ga-IE".to_string()));
        let result = validate_language_hints(
            &[hint("Dia dhuit", "DEE-ah GHWIT")],
            "Good day to ye.",
            &language,
        );
        assert!(
            result.is_empty(),
            "metadata for text removed by post-processing must be rejected"
        );
    }

    #[test]
    fn approved_delivered_hint_survives_and_duplicates_are_capped() {
        let language = LanguageSettings::new("en-IE", Some("ga-IE".to_string()));
        let result = validate_language_hints(
            &[
                hint("Dia dhuit", "DEE-ah GHWIT"),
                hint("Dia dhuit", "DEE-ah GHWIT"),
                hint("Sláinte", "SLAWN-cha"),
            ],
            "Dia dhuit, stranger. Sláinte.",
            &language,
        );
        assert_eq!(result, vec![canonical_dia_dhuit()]);
    }

    #[test]
    fn model_translation_and_pronunciation_are_replaced_with_canonical_data() {
        let bilingual = LanguageSettings::new("en-IE", Some("ga-IE".to_string()));
        let result = validate_language_hints(
            &[LanguageHint {
                word: "Dia dhuit".to_string(),
                pronunciation: "definitely wrong".to_string(),
                meaning: Some("an invented meaning".to_string()),
            }],
            "Dia dhuit, stranger.",
            &bilingual,
        );
        assert_eq!(result, vec![canonical_dia_dhuit()]);
    }

    #[test]
    fn monolingual_settings_reject_even_approved_hints() {
        assert!(
            validate_language_hints(
                &[hint("Sláinte", "SLAWN-cha")],
                "Sláinte.",
                &LanguageSettings::english_only(),
            )
            .is_empty()
        );
    }
}
