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
        let dialogue = json_resp.dialogue.clone();
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
            dialogue: dlg,
            metadata: None,
        };
    }

    NpcStreamResponse {
        dialogue: trimmed.to_string(),
        metadata: None,
    }
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
