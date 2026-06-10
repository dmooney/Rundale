//! Parsing LLM responses into dialogue + metadata, with truncation recovery.

use super::*;

/// Parsed result from an NPC LLM response.
///
/// Contains the player-visible dialogue/action text and the optional
/// metadata parsed from the JSON response.
#[derive(Debug, Clone)]
pub struct NpcStreamResponse {
    /// The dialogue and action text shown to the player.
    pub dialogue: String,
    /// Parsed metadata from the JSON response, if present.
    pub metadata: Option<NpcMetadata>,
}

/// Full JSON response from an NPC interaction (Tier 1).
///
/// The LLM returns this as a complete JSON object via `response_format: json_object`.
/// Contains both the player-visible dialogue and simulation metadata in a single
/// structured response, eliminating the need for separator-based parsing.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcJsonResponse {
    /// The NPC's spoken words and actions, as shown to the player.
    #[serde(default)]
    pub dialogue: String,
    /// What the NPC physically does (e.g. "speaks warmly", "nods", "sighs").
    #[serde(default)]
    pub action: String,
    /// The NPC's mood after this interaction.
    #[serde(default)]
    pub mood: String,
    /// Internal thought (not shown to player, used for simulation).
    #[serde(default)]
    pub internal_thought: Option<String>,
    /// Pronunciation hints for any secondary-language words used in dialogue.
    #[serde(default, alias = "irish_words")]
    pub language_hints: Vec<LanguageHint>,
    /// People the NPC mentioned by name in their dialogue (self-declared by the LLM).
    #[serde(default)]
    pub mentioned_people: Vec<String>,
}

/// Metadata block from an NPC response.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcMetadata {
    /// What the NPC physically does.
    #[serde(default)]
    pub action: String,
    /// The NPC's mood after this interaction.
    #[serde(default)]
    pub mood: String,
    /// Internal thought (not shown to player).
    #[serde(default)]
    pub internal_thought: Option<String>,
    /// Pronunciation hints for any secondary-language words used in dialogue.
    #[serde(default, alias = "irish_words")]
    pub language_hints: Vec<LanguageHint>,
    /// People the NPC mentioned by name in their dialogue (self-declared by the LLM).
    #[serde(default)]
    pub mentioned_people: Vec<String>,
}

/// Parses a complete NPC response (JSON format) into dialogue and metadata.
///
/// Expects a JSON object with a `dialogue` field and metadata fields.
/// Strips Markdown code fences (`` ```json ... ``` ``) that some providers
/// (notably Anthropic) occasionally wrap around JSON output.
///
/// Three-tier fallback when full JSON parse fails:
///
/// 1. **Full JSON parse** — preferred path, captures dialogue + metadata.
/// 2. **Heuristic `dialogue` extraction** — when the stream is truncated
///    mid-emit (max_tokens cutoff, network blip), the JSON won't close
///    but the `"dialogue": "..."` prefix is intact. Regex-extract the
///    inner string instead of letting the raw `{"dialogue": "..."}`
///    wrapper render as user-visible text.
/// 3. **Raw text** — for non-JSON providers or empty responses.
pub fn parse_npc_stream_response(full_text: &str) -> NpcStreamResponse {
    let trimmed = full_text.trim();
    let stripped = strip_json_fence(trimmed);

    if let Ok(json_resp) = serde_json::from_str::<NpcJsonResponse>(stripped) {
        let dialogue = strip_trailing_action_token(&json_resp.dialogue);
        let metadata = Some(NpcMetadata {
            action: json_resp.action,
            mood: json_resp.mood,
            internal_thought: json_resp.internal_thought,
            language_hints: json_resp.language_hints,
            mentioned_people: json_resp.mentioned_people,
        });
        return NpcStreamResponse { dialogue, metadata };
    }

    // Heuristic recovery for truncated / malformed JSON: extract the
    // inner string from a `"dialogue": "..."` pair. Tolerates an
    // unclosed JSON object (Brendan + Cormac at The Mill, 2026-05-17
    // demo).
    if let Some(dlg) = extract_dialogue_field_heuristic(stripped) {
        return NpcStreamResponse {
            dialogue: strip_trailing_action_token(&dlg),
            metadata: None,
        };
    }

    NpcStreamResponse {
        dialogue: strip_trailing_action_token(trimmed),
        metadata: None,
    }
}

/// Strips a trailing bare action token from the dialogue string.
///
/// Small models (notably Qwen2.5-14B) sometimes append a stage-direction
/// token — a lone capitalised word like `Nod`, `Smile`, `Laugh`, `Shrug` —
/// at the very end of the `dialogue` field instead of emitting it in `action`.
/// This is a parse-boundary fix: we remove the token here so it never reaches
/// the player (fixes #1374).
///
/// Only strips the last word when:
/// - it is a single word (no spaces),
/// - it starts with an ASCII uppercase letter, and
/// - the preceding text ends with sentence-ending punctuation (`.`, `!`, `?`).
///
/// This avoids over-stripping names or title-case sentence endings.
pub(crate) fn strip_trailing_action_token(dialogue: &str) -> String {
    let text = dialogue.trim();
    if text.is_empty() {
        return String::new();
    }

    // Split at the last whitespace boundary.
    if let Some(last_space) = text.rfind(|c: char| c.is_whitespace()) {
        let before = text[..last_space].trim_end();
        let last_word = text[last_space + 1..].trim();

        // Last word must be a single capitalised word (no digits, no punctuation).
        let is_capitalised_word = last_word
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
            && last_word.chars().all(|c| c.is_alphabetic());

        // The text before the last word must end with sentence-ending punctuation.
        let before_ends_with_sentence_punct = before
            .chars()
            .last()
            .map(|c| matches!(c, '.' | '!' | '?'))
            .unwrap_or(false);

        if is_capitalised_word && before_ends_with_sentence_punct {
            return before.to_string();
        }
    }

    text.to_string()
}

/// Extracts the value of a `"dialogue": "..."` JSON field from a possibly
/// truncated / malformed object. Returns `None` if the field is not present
/// or the value is empty after JSON-escape decoding.
fn extract_dialogue_field_heuristic(text: &str) -> Option<String> {
    let t = text.trim_start_matches(|c: char| c.is_whitespace() || c == '{');
    let after_key = t.strip_prefix("\"dialogue\"").or_else(|| {
        t.strip_prefix("'dialogue'")
            .or_else(|| t.strip_prefix("dialogue"))
    })?;
    let after_colon = after_key.trim_start();
    let after_colon = after_colon.strip_prefix(':')?.trim_start();
    let (inner, opener) = if let Some(rest) = after_colon.strip_prefix('"') {
        (rest, '"')
    } else if let Some(rest) = after_colon.strip_prefix('\'') {
        (rest, '\'')
    } else {
        return None;
    };

    // Walk the string body, honoring JSON-style backslash escapes. Stop
    // at the first unescaped quote that MATCHES the opener; if the stream
    // ran out (truncated), take everything we have. Tracking the matching
    // closer is important — a single-quoted body containing `"` should
    // not terminate early, and vice versa.
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => break,
            },
            c if c == opener => break,
            other => out.push(other),
        }
    }

    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Strips Markdown code-fence wrappers that some models emit around JSON.
pub(crate) fn strip_json_fence(raw: &str) -> &str {
    let t = raw.trim();
    if let Some(inner) = t.strip_prefix("```json") {
        return inner
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    if let Some(inner) = t.strip_prefix("```") {
        return inner
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_trailing_action_token ───────────────────────────────────────

    #[test]
    fn action_tag_nod_stripped_from_dialogue() {
        // AC-5 (fix #1374): "Nod" after a sentence-ending `.` must be stripped.
        assert_eq!(
            strip_trailing_action_token("Aye, fine so. Nod"),
            "Aye, fine so."
        );
    }

    #[test]
    fn action_tag_various_tokens_stripped() {
        assert_eq!(
            strip_trailing_action_token("Off with ye. Smile"),
            "Off with ye."
        );
        assert_eq!(strip_trailing_action_token("Indeed? Laugh"), "Indeed?");
        assert_eq!(
            strip_trailing_action_token("I'll think on it. Shrug"),
            "I'll think on it."
        );
    }

    #[test]
    fn action_tag_not_stripped_when_no_sentence_punct_before() {
        // If what precedes the last word isn't sentence-ending punctuation,
        // don't strip — it may be a proper name or title-case word mid-sentence.
        assert_eq!(
            strip_trailing_action_token("Good morning Padraig"),
            "Good morning Padraig"
        );
    }

    #[test]
    fn action_tag_not_stripped_for_lowercase_last_word() {
        // Lowercase last words are dialogue, not action tags.
        assert_eq!(
            strip_trailing_action_token("Come in, friend."),
            "Come in, friend."
        );
    }

    #[test]
    fn action_tag_not_stripped_for_multi_word_suffix() {
        // A multi-word action tag (two words) should not be stripped by this function.
        // We only strip a single bare word.
        let input = "Right so. Waves hand";
        assert_eq!(strip_trailing_action_token(input), input);
    }

    #[test]
    fn action_tag_not_stripped_when_last_word_has_digits() {
        // Action tokens are pure alpha. A word with digits is not a tag.
        assert_eq!(
            strip_trailing_action_token("Come back at 7."),
            "Come back at 7."
        );
    }

    #[test]
    fn action_tag_empty_string_returns_empty() {
        assert_eq!(strip_trailing_action_token(""), "");
        assert_eq!(strip_trailing_action_token("   "), "");
    }

    // ── parse_npc_stream_response with action-tag ─────────────────────────

    #[test]
    fn parse_response_strips_trailing_action_token() {
        let json = r#"{"dialogue": "Aye, workin' metal, ain't it? Nod", "action": "nods", "mood": "cheerful"}"#;
        let resp = parse_npc_stream_response(json);
        assert_eq!(
            resp.dialogue, "Aye, workin' metal, ain't it?",
            "trailing 'Nod' must be stripped from dialogue"
        );
        // Metadata still parsed normally
        let meta = resp.metadata.expect("metadata must parse");
        assert_eq!(meta.action, "nods");
        assert_eq!(meta.mood, "cheerful");
    }

    #[test]
    fn parse_response_no_action_token_unchanged() {
        let json = r#"{"dialogue": "A fine morning to ye.", "action": "nods", "mood": "calm"}"#;
        let resp = parse_npc_stream_response(json);
        assert_eq!(resp.dialogue, "A fine morning to ye.");
    }
}
