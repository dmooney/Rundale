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
    /// Concrete work the NPC assigned in the spoken dialogue, if any.
    ///
    /// The model proposes only this natural-language description. The engine
    /// supplies and validates task identity, assigner, location, timestamps,
    /// and lifecycle state at the canonical apply seam.
    #[serde(default)]
    pub assigned_task: Option<String>,
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
    /// Bounded description of concrete work the NPC claims to have assigned.
    ///
    /// This remains untrusted until the canonical dialogue apply seam confirms
    /// that the final player-visible line actually conveys the assignment.
    #[serde(default)]
    pub assigned_task: Option<String>,
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
        let dialogue =
            strip_trailing_action_token(&strip_trailing_unmatched_quote(&json_resp.dialogue));
        let metadata = Some(NpcMetadata {
            action: json_resp.action,
            mood: json_resp.mood,
            internal_thought: json_resp.internal_thought,
            language_hints: json_resp.language_hints,
            mentioned_people: json_resp.mentioned_people,
            assigned_task: bounded_assigned_task(json_resp.assigned_task),
        });
        return NpcStreamResponse { dialogue, metadata };
    }

    // Heuristic recovery for truncated / malformed JSON: extract the
    // inner string from a `"dialogue": "..."` pair. Tolerates an
    // unclosed JSON object (Brendan + Cormac at The Mill, 2026-05-17
    // demo).
    if let Some(dlg) = extract_dialogue_field_heuristic(stripped) {
        return NpcStreamResponse {
            dialogue: strip_trailing_action_token(&strip_trailing_unmatched_quote(&dlg)),
            metadata: None,
        };
    }

    // Raw-text fallback: non-JSON provider or hopelessly malformed response.
    // Skip quote-stripping here — the text may be a raw JSON stub (e.g. a
    // truncated `{"dialogue": "`) where stripping the trailing `"` would
    // produce an even more garbled result.
    NpcStreamResponse {
        dialogue: strip_trailing_action_token(trimmed),
        metadata: None,
    }
}

fn bounded_assigned_task(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .chars()
            .take(parish_types::MAX_TASK_DESCRIPTION_CHARS)
            .collect(),
    )
}

/// Strips a trailing unmatched quote character from the end of a dialogue string.
///
/// Small models (notably Qwen2.5-14B) sometimes emit a stray closing-quote
/// character — straight `"`, smart `"` / `"`, or single `'` / `'` — at the
/// very end of the `dialogue` JSON field value. This is the JSON-field
/// closing-quote leaking through when the model wraps its reply in quotation
/// marks without balancing them, or when a truncated stream leaves a dangling
/// closer (fixes #1405).
///
/// A quote is considered "unmatched" when the last non-whitespace character is
/// a quote and no corresponding opening quote of the same kind appears earlier
/// in the text (simple open-count check: if the count of openers ≠ closers, the
/// trailing char is the orphan). Straight `"` uses a parity check; smart quotes
/// use open/close matching.
///
/// This is called *before* `strip_trailing_action_token` so both artifacts can
/// be removed in a single pass.
pub(crate) fn strip_trailing_unmatched_quote(dialogue: &str) -> String {
    let text = dialogue.trim();
    if text.is_empty() {
        return String::new();
    }

    // Candidate trailing quote characters (all closing / ambiguous variants).
    const TRAILING_QUOTES: &[char] = &[
        '"',        // U+0022 straight double quote (ambiguous open/close)
        '\u{201C}', // U+201C left double quotation mark "
        '\u{201D}', // U+201D right double quotation mark "
        '\'',       // U+0027 straight apostrophe / single quote
        '\u{2018}', // U+2018 left single quotation mark '
        '\u{2019}', // U+2019 right single quotation mark '
    ];

    let last_char = match text.chars().next_back() {
        Some(c) if TRAILING_QUOTES.contains(&c) => c,
        _ => return text.to_string(),
    };

    // Count openers vs. closers to decide if the trailing char is unmatched.
    let is_unmatched = match last_char {
        '"' => {
            // Straight quote is ambiguous: count all occurrences. If odd, the
            // last one is unmatched.
            let count = text.chars().filter(|&c| c == '"').count();
            count % 2 != 0
        }
        '\'' => {
            // Straight single quote is used heavily in Irish dialogue as an
            // apostrophe, so only strip when it appears after sentence-ending
            // punctuation (preceded by `.`, `!`, or `?` and optional whitespace).
            let before = text[..text.len() - '\''.len_utf8()].trim_end();
            before
                .chars()
                .last()
                .map(|c| matches!(c, '.' | '!' | '?'))
                .unwrap_or(false)
        }
        '\u{201D}' => {
            // Right double quotation mark is unambiguously a closer.
            // Unmatched when open-count != close-count.
            let opens = text.chars().filter(|&c| c == '\u{201C}').count();
            let closes = text.chars().filter(|&c| c == '\u{201D}').count();
            opens < closes
        }
        '\u{201C}' => {
            // Left double quotation mark as trailing char is unusual — treat as
            // unmatched only when it has no matching closer.
            let opens = text.chars().filter(|&c| c == '\u{201C}').count();
            let closes = text.chars().filter(|&c| c == '\u{201D}').count();
            opens > closes
        }
        '\u{2019}' => {
            // Right single quotation mark — same logic as right double.
            let opens = text.chars().filter(|&c| c == '\u{2018}').count();
            let closes = text.chars().filter(|&c| c == '\u{2019}').count();
            opens < closes
        }
        '\u{2018}' => {
            let opens = text.chars().filter(|&c| c == '\u{2018}').count();
            let closes = text.chars().filter(|&c| c == '\u{2019}').count();
            opens > closes
        }
        _ => false,
    };

    if is_unmatched {
        // Remove the trailing quote and any whitespace that preceded it.
        let without = &text[..text.len() - last_char.len_utf8()];
        without.trim_end().to_string()
    } else {
        text.to_string()
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
        let last_word = text[last_space..].trim_start();

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

    #[test]
    fn action_tag_multibyte_whitespace_no_panic() {
        // EM SPACE (U+2003) is a 3-byte UTF-8 character. Slicing at
        // `last_space + 1` would land mid-codepoint and panic; using
        // `text[last_space..].trim_start()` is always char-boundary-safe.
        let em_space = "\u{2003}";
        let input = format!("Right so.{em_space}Nod");
        assert_eq!(strip_trailing_action_token(&input), "Right so.");

        // Also verify it doesn't strip when the punctuation guard fails.
        let no_punct = format!("Good morning{em_space}Padraig");
        assert_eq!(strip_trailing_action_token(&no_punct), no_punct.trim());
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

    // ── strip_trailing_unmatched_quote ────────────────────────────────────

    #[test]
    fn trailing_straight_double_quote_stripped() {
        // AC-1: dangling straight closing-quote after sentence punctuation.
        assert_eq!(
            strip_trailing_unmatched_quote(
                "Sure enough, a leather-man would be well sought after. \""
            ),
            "Sure enough, a leather-man would be well sought after."
        );
    }

    #[test]
    fn trailing_right_double_quote_stripped() {
        // AC-2: U+201D dangling after text.
        assert_eq!(
            strip_trailing_unmatched_quote("What brings ye today? \u{201D}"),
            "What brings ye today?"
        );
    }

    #[test]
    fn trailing_left_double_quote_stripped() {
        // AC-3: U+201C appearing as trailing char with no matching closer.
        // e.g. model emitted an opener but truncated before the closer.
        assert_eq!(
            strip_trailing_unmatched_quote("Something odd happened. \u{201C}"),
            "Something odd happened."
        );
    }

    #[test]
    fn trailing_straight_single_quote_stripped_after_sentence_punct() {
        // AC-4: dangling straight single-quote after `.`.
        assert_eq!(
            strip_trailing_unmatched_quote("Good day to ye. '"),
            "Good day to ye."
        );
    }

    #[test]
    fn trailing_right_single_quote_stripped() {
        // AC-5: U+2019 unmatched at end.
        assert_eq!(
            strip_trailing_unmatched_quote("Off ye go now. \u{2019}"),
            "Off ye go now."
        );
    }

    #[test]
    fn balanced_smart_quotes_not_stripped() {
        // AC-6: proper open/close pair — leave unchanged.
        let input = "\u{201C}Tis a fine day.\u{201D}";
        assert_eq!(strip_trailing_unmatched_quote(input), input);
    }

    #[test]
    fn balanced_straight_quotes_not_stripped() {
        // AC-6: even count of straight double-quotes — leave unchanged.
        let input = "\"Tis a fine day.\"";
        assert_eq!(strip_trailing_unmatched_quote(input), input);
    }

    #[test]
    fn apostrophe_mid_word_not_stripped() {
        // AC-6 variant: straight single-quote used as apostrophe — not
        // preceded by sentence punctuation, so must not be stripped.
        let input = "Ye're a fine one, aren't ye?";
        assert_eq!(strip_trailing_unmatched_quote(input), input);
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(strip_trailing_unmatched_quote(""), "");
        assert_eq!(strip_trailing_unmatched_quote("   "), "");
    }

    #[test]
    fn parse_response_strips_trailing_stray_quote() {
        // AC-1 exercised through the full parse pipeline.
        let json = r#"{"dialogue": "What brings ye to the forge today? ”", "action": "nods", "mood": "friendly"}"#;
        let resp = parse_npc_stream_response(json);
        assert_eq!(resp.dialogue, "What brings ye to the forge today?");
    }

    #[test]
    fn parse_response_stray_quote_from_issue_1405() {
        // Exact artifact from the bug report: trailing ` "` (U+201D).
        let json = r#"{"dialogue": "Sure enough, a leather-man would be well sought after. Now, what brings ye to the forge today? ”", "action": "hammers", "mood": "busy"}"#;
        let resp = parse_npc_stream_response(json);
        assert!(
            !resp.dialogue.ends_with('\u{201D}'),
            "U+201D must be stripped: {:?}",
            resp.dialogue
        );
        assert!(
            !resp.dialogue.ends_with('"'),
            "straight quote must be stripped: {:?}",
            resp.dialogue
        );
    }
}
