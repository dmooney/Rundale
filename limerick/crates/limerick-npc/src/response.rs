//! Parsing LLM responses into dialogue + metadata, with truncation recovery.

use super::*;

/// How a Tier-1 model response crossed the production parsing boundary.
///
/// Promotion soaks distinguish contract-valid JSON from diagnostic recovery.
/// The canonical apply seam rejects both recovery dispositions (the turn fails
/// with no effects), and a model that relies on them does not satisfy the
/// contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpcResponseParseDisposition {
    /// The complete response deserialized as the authored JSON object.
    FullJson,
    /// Full JSON failed, but the truncated `dialogue` field was recovered.
    RecoveredDialogue,
    /// Neither JSON path worked; the raw response became player text.
    RawText,
}

impl NpcResponseParseDisposition {
    /// Stable machine-readable value used by runtime quality telemetry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FullJson => "full_json",
            Self::RecoveredDialogue => "recovered_dialogue",
            Self::RawText => "raw_text",
        }
    }
}

/// The structural response contract checked at the commit boundary.
///
/// A candidate is committable only when it parsed as the complete authored JSON
/// object and carries non-empty spoken dialogue. This is the only check applied
/// to a reply: nothing inspects, rewrites, or replaces what the model said.
/// Callers fail the turn, with no memory, event, UI, or state effects, when it
/// returns `false`.
pub fn dialogue_candidate_is_complete(
    candidate: &NpcStreamResponse,
    disposition: NpcResponseParseDisposition,
) -> bool {
    disposition == NpcResponseParseDisposition::FullJson && !candidate.dialogue.trim().is_empty()
}

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
/// 2. **Heuristic `dialogue` extraction** — diagnostic recovery when the stream is truncated
///    mid-emit (max_tokens cutoff, network blip), the JSON won't close
///    but the `"dialogue": "..."` prefix is intact. Regex-extract the
///    inner string instead of letting the raw `{"dialogue": "..."}`
///    wrapper pollute telemetry. Canonical production apply rejects this disposition.
/// 3. **Raw text** — diagnostic recovery for non-JSON providers or empty responses;
///    canonical production apply rejects this disposition.
pub fn parse_npc_stream_response(full_text: &str) -> NpcStreamResponse {
    parse_npc_stream_response_with_disposition(full_text).0
}

/// Parses a complete NPC response and reports which production parser path won.
///
/// Keep callers that only need the response on [`parse_npc_stream_response`].
/// Runtime quality measurement uses this variant so reliability evidence comes
/// from the exact parser shown to players rather than a benchmark-side mirror.
pub fn parse_npc_stream_response_with_disposition(
    full_text: &str,
) -> (NpcStreamResponse, NpcResponseParseDisposition) {
    let trimmed = full_text.trim();
    let stripped = strip_json_fence(trimmed);

    if let Ok(json_resp) = serde_json::from_str::<NpcJsonResponse>(stripped) {
        // The reply is the model's own words; only surrounding whitespace goes.
        let dialogue = json_resp.dialogue.trim().to_string();
        let metadata = Some(NpcMetadata {
            action: json_resp.action,
            mood: json_resp.mood,
            internal_thought: json_resp.internal_thought,
            language_hints: json_resp.language_hints,
            mentioned_people: json_resp.mentioned_people,
            assigned_task: bounded_assigned_task(json_resp.assigned_task),
        });
        return (
            NpcStreamResponse { dialogue, metadata },
            NpcResponseParseDisposition::FullJson,
        );
    }

    // Heuristic recovery for truncated / malformed JSON: extract the
    // inner string from a `"dialogue": "..."` pair. Tolerates an
    // unclosed JSON object (Brendan + Cormac at The Mill, 2026-05-17
    // demo).
    if let Some(dlg) = extract_dialogue_field_heuristic(stripped) {
        return (
            NpcStreamResponse {
                dialogue: dlg.trim().to_string(),
                metadata: None,
            },
            NpcResponseParseDisposition::RecoveredDialogue,
        );
    }

    // Raw-text fallback: non-JSON provider or hopelessly malformed response.
    (
        NpcStreamResponse {
            dialogue: trimmed.to_string(),
            metadata: None,
        },
        NpcResponseParseDisposition::RawText,
    )
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
            .take(limerick_types::MAX_TASK_DESCRIPTION_CHARS)
            .collect(),
    )
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
    } else {
        let rest = after_colon.strip_prefix('\'')?;
        (rest, '\'')
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

    #[test]
    fn parse_disposition_distinguishes_contract_and_fallback_paths() {
        let (full, full_kind) =
            parse_npc_stream_response_with_disposition(r#"{"dialogue":"Aye."}"#);
        assert_eq!(full.dialogue, "Aye.");
        assert_eq!(full_kind, NpcResponseParseDisposition::FullJson);

        let (recovered, recovered_kind) =
            parse_npc_stream_response_with_disposition(r#"{"dialogue":"Still usable"#);
        assert_eq!(recovered.dialogue, "Still usable");
        assert_eq!(
            recovered_kind,
            NpcResponseParseDisposition::RecoveredDialogue
        );

        let (raw, raw_kind) = parse_npc_stream_response_with_disposition("Plain words.");
        assert_eq!(raw.dialogue, "Plain words.");
        assert_eq!(raw_kind, NpcResponseParseDisposition::RawText);
    }

    /// The valid-JSON path hands back the model's dialogue as written: a
    /// trailing capitalised word or a stray closing quote is not trimmed off.
    #[test]
    fn parsed_dialogue_is_not_rewritten() {
        for dialogue in [
            "Who sent you? Seamus",
            "Aye, workin' metal, ain't it? Nod",
            "What brings ye to the forge today? \u{201D}",
            "Good day to ye. '",
        ] {
            let json =
                serde_json::json!({"dialogue": dialogue, "action": "", "mood": "calm"}).to_string();
            let (parsed, disposition) = parse_npc_stream_response_with_disposition(&json);
            assert_eq!(disposition, NpcResponseParseDisposition::FullJson);
            assert_eq!(parsed.dialogue, dialogue);
        }
    }

    fn candidate(dialogue: &str) -> NpcStreamResponse {
        NpcStreamResponse {
            dialogue: dialogue.to_string(),
            metadata: None,
        }
    }

    /// The commit boundary applies the structural contract only: a complete
    /// JSON reply with non-empty dialogue is committable whatever it says.
    #[test]
    fn candidate_contract_is_structural_only() {
        use NpcResponseParseDisposition::{FullJson, RawText, RecoveredDialogue};

        assert!(dialogue_candidate_is_complete(
            &candidate("Aye, Seamus the cartwright sent ye? A grand man."),
            FullJson
        ));
        assert!(!dialogue_candidate_is_complete(
            &candidate("  \n"),
            FullJson
        ));
        assert!(!dialogue_candidate_is_complete(
            &candidate("Aye."),
            RecoveredDialogue
        ));
        assert!(!dialogue_candidate_is_complete(&candidate("Aye."), RawText));
    }
}
