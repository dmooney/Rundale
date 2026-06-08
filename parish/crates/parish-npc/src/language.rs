//! Locale directive construction and curated secondary-language phrase guides.

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

const GA_IE_PHRASE_GUIDE: &str = "\n\
    Preferred ga-IE phrases (use these where natural; do not confabulate \
    other Irish): \
    Greetings: \"Dia dhuit\" (hello), \"Dia is Muire dhuit\" (reply), \
    \"Conas atá tú?\" (how are you), \"Slán\" (goodbye), \
    \"Slán abhaile\" (safe home). \
    Blessings / thanks: \"Go raibh maith agat\" (thank you), \
    \"Le cúnamh Dé\" (with God's help), \"Buíochas le Dia\" (thank God), \
    \"Beannacht Dé ort\" (God bless you), \"Go n-éirí leat\" (good luck to you). \
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
