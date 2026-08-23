//! LLM-informed player-message emoji reactions and keyword-based rule reactions.
//!
//! # Emoji Reactions (player ↔ NPC)
//!
//! Supports three flows:
//! 1. Player reacts to NPC messages (stored in [`ReactionLog`], injected into prompts)
//! 2. NPCs react to player messages (rule-based keyword matching)
//! 3. NPC-to-NPC reactions (future, via Tier 2 ticks)

use crate::reactions::reaction_description;
use crate::{LanguageSettings, Npc};
use parish_inference::{AnyClient, GenerateParams};

/// Sampling temperature for the player-message reaction inference call.
///
/// Bumped from `None` (provider default, effectively 0 on most backends)
/// to 1.0 in issue #995 so small-model reaction sampling explores beyond
/// the most-likely-safe choice (`🤔` for questions, `😊` for friendly).
/// The output schema is locked to a single palette emoji so widening the
/// distribution cannot break correctness — only diversify it.
pub const REACTION_INFERENCE_TEMPERATURE: f32 = 1.0;

/// Keyword groups that trigger NPC reactions, with the corresponding emoji.
///
/// Coverage was widened in #982 after a five-turn demo run produced zero
/// reactions: the demo prompt steers the player into chitchat ("greet
/// people", "ask about lives, land, events") which never tripped the old
/// charged-topic set. The additions are everyday parish-life cues —
/// greetings, weather, work, family, news, prayer, music — mapped to
/// emoji that are already members of [`crate::reactions::REACTION_PALETTE`]
/// so the palette stays the canonical 12-entry set used by the UI, the LLM
/// validator, and the reaction-log context renderer.
/// The 60% probabilistic gate is intentionally preserved so reactions
/// remain a sparing accent.
const KEYWORD_REACTIONS: &[(&[&str], &str)] = &[
    // Charged topics — original set.
    (&["death", "died", "killed", "murder"], "😢"),
    (&["fairy", "fairies", "púca", "banshee", "sidhe"], "✝️"),
    (&["drink", "whiskey", "poitín", "ale", "stout"], "🍺"),
    (&["joke", "funny", "laugh", "haha"], "😂"),
    (&["secret", "don't tell", "between us", "confidence"], "🤫"),
    (&["rent", "evict", "landlord", "agent", "tithe"], "😠"),
    (&["gold", "treasure", "fortune", "money", "reward"], "👀"),
    (&["strange", "ghost", "haunted", "spirit"], "😳"),
    // Everyday chitchat — added in #982. All emoji are palette-resident.
    (
        &[
            "hello",
            "hallo",
            "good morning",
            "good day",
            "good evening",
            "dia duit",
            "fáilte",
        ],
        "😊",
    ),
    (
        &[
            "weather", "rain", "raining", "sunny", "storm", "cold", "frost", "wind", "harvest",
            "crop", "potato", "praties", "field", "plough", "turf", "bog",
        ],
        "🤔",
    ),
    (
        &[
            "parish",
            "village",
            "townland",
            "neighbour",
            "neighbor",
            "kilteevan",
            "family",
            "mother",
            "father",
            "child",
            "son",
            "daughter",
            "music",
            "song",
            "fiddle",
            "tune",
            "dance",
        ],
        "😊",
    ),
    (&["news", "story", "tell me", "heard", "rumour"], "👀"),
    (
        &["pray", "prayer", "priest", "mass", "blessing", "holy well"],
        "✝️",
    ),
];

/// Normalises an input line for whole-word keyword matching.
///
/// Lowercases, replaces every non-alphanumeric character (apart from `'`,
/// which carries meaning in cues like `don't tell`) with a space, then
/// surrounds the result with sentinel spaces so a caller can check
/// `normalised.contains(&format!(" {kw} "))` to get word-boundary semantics
/// without pulling in `regex` for the hot path. Multi-word cues like
/// `"good morning"` or `"holy well"` still match — the helper preserves
/// internal spaces in keywords.
fn normalise_for_keyword_match(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 2);
    out.push(' ');
    for c in input.chars() {
        if c.is_alphanumeric() || c == '\'' {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
        } else {
            out.push(' ');
        }
    }
    out.push(' ');
    out
}

/// Whole-word keyword match.
///
/// Avoids the substring false positives that the original
/// `input_lower.contains(kw)` produced — e.g. `son` matching `person`,
/// `pray` matching `spray`, `mass` matching `massage` (#982 review).
fn input_contains_keyword(normalised: &str, kw: &str) -> bool {
    let needle = format!(" {kw} ");
    normalised.contains(&needle)
}

/// Generates a rule-based NPC reaction to player input.
///
/// Returns `Some(emoji)` if a keyword match triggers a reaction (60% chance),
/// or `None` if no reaction is generated.
pub fn generate_rule_reaction(player_input: &str) -> Option<String> {
    let normalised = normalise_for_keyword_match(player_input);

    for (keywords, emoji) in KEYWORD_REACTIONS {
        if keywords
            .iter()
            .any(|kw| input_contains_keyword(&normalised, kw))
        {
            // 60% chance to react — not every NPC reacts every time
            if rand::random::<f64>() < 0.6 {
                return Some((*emoji).to_string());
            }
        }
    }

    None
}

/// Deterministic variant for testing — always returns a reaction if keywords match.
#[cfg(test)]
fn generate_rule_reaction_deterministic(player_input: &str) -> Option<String> {
    let normalised = normalise_for_keyword_match(player_input);

    for (keywords, emoji) in KEYWORD_REACTIONS {
        if keywords
            .iter()
            .any(|kw| input_contains_keyword(&normalised, kw))
        {
            return Some((*emoji).to_string());
        }
    }

    None
}

// ── LLM-informed player-message reactions ────────────────────────────────────

/// Structured output returned by the player-message reaction inference call.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LlmReactionDecision {
    /// Emoji from [`REACTION_PALETTE`], or `None` for no visible reaction.
    #[serde(default)]
    pub emoji: Option<String>,
}

/// Strips Markdown JSON code fences (`` ```json `` or `` ``` ``) and trims whitespace.
fn strip_code_fence(raw: &str) -> &str {
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

/// Extracts the first complete `{...}` JSON object substring from `s`.
///
/// Used to tolerate trailing text after the closing brace — a common
/// Qwen2.5-1.5B deviation from the expected schema.
fn extract_first_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let mut depth: usize = 0;
    for (i, ch) in s[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Lenient parser for raw LLM output from the reaction model.
///
/// Handles two characteristic Qwen2.5-1.5B failure modes:
/// 1. `{"emoji": {"value": "😊"}}` — nested map where a bare string is expected.
/// 2. `{"emoji": "😊"} some extra text` — trailing characters after the JSON object.
///
/// Falls back to treating the entire cleaned string as a bare emoji when no
/// JSON object is found at all. Returns `None` only when nothing can be
/// extracted.
pub fn parse_reaction_decision(raw: &str) -> Option<LlmReactionDecision> {
    let cleaned = strip_code_fence(raw);

    // Try to find and parse the first `{...}` block (handles trailing chars).
    let json_str = extract_first_json_object(cleaned).unwrap_or(cleaned);

    match serde_json::from_str::<LlmReactionDecision>(json_str) {
        Ok(decision) => return Some(decision),
        Err(e) => {
            let msg = e.to_string();
            // Qwen2.5-1.5B sometimes nests the emoji as a map, e.g.
            // {"emoji": {"value": "😊"}} or {"emoji": {"emoji": "😊"}}.
            // Extract any string value from that nested object.
            if msg.contains("invalid type: map")
                && let Ok(outer) = serde_json::from_str::<serde_json::Value>(json_str)
                && let Some(inner_obj) = outer.get("emoji").and_then(|v| v.as_object())
            {
                for v in inner_obj.values() {
                    if let Some(s) = v.as_str() {
                        return Some(LlmReactionDecision {
                            emoji: Some(s.to_string()),
                        });
                    }
                }
            }
            // Anything else (or extraction failed): fall through.
            tracing::debug!(error = %e, raw = raw, "reaction decision parse failed; trying bare-string fallback");
        }
    }

    // Last resort: treat the whole cleaned output as a bare emoji string.
    // This catches models that return just the emoji character without any JSON.
    // Only fire when the output contains no ASCII letters/digits (to avoid treating
    // verbose prose or error messages as emoji).
    let bare = cleaned.trim();
    let looks_like_emoji =
        !bare.is_empty() && !bare.contains('{') && !bare.chars().any(|c| c.is_ascii_alphanumeric());
    if looks_like_emoji {
        return Some(LlmReactionDecision {
            emoji: Some(bare.to_string()),
        });
    }

    None
}

/// Builds the system and user prompts used to infer an NPC emoji reaction to
/// a player message.
///
/// The system prompt enumerates the full [`REACTION_PALETTE`] and the legacy
/// keyword cues as weak few-shot examples. The user prompt contains the NPC's
/// name, occupation, mood, and personality snippet followed by the player
/// message.
pub fn build_player_message_reaction_prompt(
    npc: &Npc,
    player_input: &str,
    _language: &LanguageSettings,
) -> (String, String) {
    use crate::reactions::REACTION_PALETTE;

    let palette_lines: Vec<String> = REACTION_PALETTE
        .iter()
        .map(|(emoji, desc)| format!("- {emoji}: {desc}"))
        .chain(std::iter::once("- null: no visible reaction".to_string()))
        .collect();

    let keyword_examples = KEYWORD_REACTIONS
        .iter()
        .map(|(group, emoji)| format!("- {:?} -> {}", group, emoji))
        .collect::<Vec<_>>()
        .join("\n");

    let system = format!(
        "You decide whether a single NPC would visibly react to a player's spoken line.\n\
         Return STRICT JSON only with schema: {{\"emoji\": <emoji-or-null>}}.\n\
         If unsure or neutral, return {{\"emoji\": null}}.\n\
         Never invent new emoji.\n\
         Available palette:\n{}\n\
         Legacy keyword cues (weak examples only; use full meaning and tone):\n{}",
        palette_lines.join("\n"),
        keyword_examples,
    );

    let personality_snippet: String = npc.personality.chars().take(300).collect();
    let user = format!(
        "NPC:\n\
         - Name: {}\n\
         - Occupation: {}\n\
         - Mood: {}\n\
         - Personality: {}\n\n\
         Player message:\n\
         \"{}\"\n\n\
         Choose one emoji or null.",
        npc.name, npc.occupation, npc.mood, personality_snippet, player_input,
    );

    (system, user)
}

/// Uses the LLM to infer an NPC emoji reaction for a player message.
///
/// Returns `Some(emoji)` only when:
/// - inference succeeds within `timeout`,
/// - the output emoji is in [`REACTION_PALETTE`], and
/// - the 60% probabilistic gate fires (same rate as rule-based reactions).
///
/// Returns `None` for all errors, unknown emoji, explicit null output, or when
/// the probabilistic gate does not fire. This function never panics.
pub async fn infer_player_message_reaction(
    client: &AnyClient,
    model: &str,
    npc: &Npc,
    player_input: &str,
    timeout: std::time::Duration,
) -> Option<String> {
    infer_player_message_reaction_with_profile(
        client,
        model,
        npc,
        player_input,
        timeout,
        parish_config::InferenceProfile::for_subrole(
            parish_config::InferenceSubrole::MessageReaction,
        ),
    )
    .await
}

/// Infers an emoji reaction using the profile resolved from runtime config.
pub async fn infer_player_message_reaction_with_profile(
    client: &AnyClient,
    model: &str,
    npc: &Npc,
    player_input: &str,
    timeout: std::time::Duration,
    profile: parish_config::InferenceProfile,
) -> Option<String> {
    infer_player_message_reaction_with_profile_and_audit(
        client,
        model,
        npc,
        player_input,
        timeout,
        profile,
        None,
    )
    .await
}

/// Infers an emoji reaction with resolved tuning and common audit sinks.
pub async fn infer_player_message_reaction_with_profile_and_audit(
    client: &AnyClient,
    model: &str,
    npc: &Npc,
    player_input: &str,
    timeout: std::time::Duration,
    profile: parish_config::InferenceProfile,
    audit_sink: Option<parish_inference::InferenceAuditSink>,
) -> Option<String> {
    let lang = LanguageSettings::english_only();
    let (system, prompt) = build_player_message_reaction_prompt(npc, player_input, &lang);
    // 80-token floor (#984 follow-up): the JSON envelope `{"emoji": "<glyph>"}`
    // fits in ~15 tokens for ASCII palette entries, but multi-codepoint glyphs
    // (e.g. ✝️ as `U+271D U+FE0F`, country-flag pairs, ZWJ family clusters)
    // tokenise to 3-6 BPE tokens each. Combined with optional reasoning prefix
    // tokens emitted by some local models, the previous 40-token cap could
    // truncate the JSON before the closing brace, producing an empty parse and
    // an invisible reaction. The shared Reaction profile leaves enough room
    // for Gemini's hidden thought budget while the schema still constrains
    // visible output to a single palette entry.
    //
    // Issue #995: small-model reaction inference collapses onto one or two
    // safe emoji at temp=0. An explicit 1.0 widens the sampling distribution
    // so a 1.5B-class model picks across the full palette rather than always
    // returning 🤔 / 😊. The output schema is constrained to one of the
    // palette entries, so the higher temperature does not break correctness.
    let params = GenerateParams {
        max_tokens: Some(profile.max_output_tokens),
        temperature: Some(REACTION_INFERENCE_TEMPERATURE),
        frequency_penalty: None,
        enable_thinking: None,
        reasoning_effort: None,
        thinking_level: Some(profile.thinking_level),
        service_tier: Some(profile.service_tier),
        reasoning_intent: (profile.configuration_epoch > 0).then_some(profile.reasoning_intent),
        reasoning_dialect: profile.reasoning_dialect,
    };
    let audit = parish_inference::DirectInferenceAudit::new(
        audit_sink,
        model,
        &prompt,
        Some(&system),
        parish_config::InferenceSubrole::MessageReaction,
        false,
        params.max_tokens,
        params.thinking_level,
        params.service_tier,
        params.temperature,
        parish_inference::InferencePriority::Interactive,
    );
    let call = client.generate_detailed_with_format(model, &prompt, Some(&system), None, params);

    let detailed = match tokio::time::timeout(timeout, call).await {
        Ok(result) => result,
        Err(_) => Err(parish_inference::ProviderCallError {
            message: format!("reaction inference timed out after {timeout:?}"),
            partial_text: String::new(),
            metadata: Box::new(parish_inference::ProviderMetadata::unavailable(model)),
        }),
    };
    let validated = detailed.and_then(|result| match parse_reaction_decision(&result.text) {
        Some(parsed) => Ok((result, parsed)),
        None => Err(parish_inference::ProviderCallError {
            message: "reaction JSON parse failed: no valid reaction decision".to_string(),
            partial_text: result.text,
            metadata: Box::new(result.metadata),
        }),
    });
    let response = match validated {
        Ok((raw, parsed)) => match audit.record(Ok(raw)).await {
            Ok(_) => parsed,
            Err(error) => {
                tracing::debug!(?error, "inference audit unexpectedly changed a success");
                return None;
            }
        },
        Err(error) => {
            let error = audit
                .record(Err(error))
                .await
                .expect_err("auditing must preserve provider errors");
            tracing::debug!(
                ?error,
                "inference call failed in infer_player_message_reaction"
            );
            return None;
        }
    };
    let emoji = response.emoji?;
    reaction_description(&emoji)?;
    if rand::random::<f64>() >= 0.6 {
        return None;
    }
    Some(emoji)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactions::REACTION_PALETTE;
    use crate::test_helpers::make_named_occupation_npc as test_npc;
    use parish_types::LocationId;

    #[test]
    fn generate_rule_reaction_keyword_match() {
        assert_eq!(
            generate_rule_reaction_deterministic("The fairy fort is cursed"),
            Some("✝️".to_string())
        );
        assert_eq!(
            generate_rule_reaction_deterministic("Let's have a drink of poitín"),
            Some("🍺".to_string())
        );
        assert_eq!(
            generate_rule_reaction_deterministic("The rent is too high"),
            Some("😠".to_string())
        );
    }

    #[test]
    fn generate_rule_reaction_no_match() {
        assert_eq!(
            generate_rule_reaction_deterministic("Just walking by here"),
            None
        );
    }

    /// Regression test for the substring false positive that
    /// `input.contains(kw)` produced before #982 added whole-word matching.
    /// Words that *contain* a keyword as a fragment must not trigger a
    /// reaction — only standalone occurrences should.
    #[test]
    fn generate_rule_reaction_rejects_substring_false_positives() {
        // `son` is a keyword, but "person", "lesson", "comparison" are not.
        assert_eq!(
            generate_rule_reaction_deterministic("A person walked past"),
            None
        );
        assert_eq!(
            generate_rule_reaction_deterministic("That was a lesson learned"),
            None
        );
        // `pray` is a keyword, but "spray" is not.
        assert_eq!(
            generate_rule_reaction_deterministic("There was sea spray on the air"),
            None
        );
        // `mass` is a keyword, but "massage" is not.
        assert_eq!(
            generate_rule_reaction_deterministic("She wanted a massage"),
            None
        );
        // `rain` is a keyword, but "train" / "brain" are not.
        assert_eq!(
            generate_rule_reaction_deterministic("He took the train"),
            None
        );
        // `tune` is a keyword, but "tuning" / "tuned" are not.
        assert_eq!(
            generate_rule_reaction_deterministic("He was tuning the cart wheel"),
            None
        );
    }

    /// Whole-word matching must still strike on real keywords surrounded by
    /// punctuation or appearing in multi-word cues.
    #[test]
    fn generate_rule_reaction_handles_punctuation_and_multiword_keywords() {
        // Trailing question mark / comma — non-alphanumeric chars are
        // normalised to spaces.
        assert!(generate_rule_reaction_deterministic("What news from the market?").is_some());
        assert!(generate_rule_reaction_deterministic("Hello, friend!").is_some());
        // Multi-word keyword "good morning".
        assert!(generate_rule_reaction_deterministic("A good morning to ye").is_some());
        // Multi-word keyword "tell me" with an apostrophe nearby.
        assert!(generate_rule_reaction_deterministic("Tell me, what's news?").is_some());
    }

    #[test]
    fn llm_reaction_decision_allows_null() {
        let parsed: LlmReactionDecision = serde_json::from_str(r#"{"emoji":null}"#).unwrap();
        assert!(parsed.emoji.is_none());
    }

    #[test]
    fn llm_reaction_decision_accepts_missing_field() {
        let parsed: LlmReactionDecision = serde_json::from_str(r#"{}"#).unwrap();
        assert!(parsed.emoji.is_none());
    }

    #[test]
    fn llm_reaction_decision_non_null_emoji() {
        let parsed: LlmReactionDecision = serde_json::from_str(r#"{"emoji":"test"}"#).unwrap();
        assert_eq!(parsed.emoji.as_deref(), Some("test"));
    }

    #[test]
    fn build_player_message_reaction_prompt_contains_palette_and_npc_name() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let lang = LanguageSettings::english_only();
        let (system, user) =
            build_player_message_reaction_prompt(&npc, "The landlord is coming.", &lang);

        assert!(system.contains("Available palette"));
        assert!(system.contains("null: no visible reaction"));
        assert!(system.contains("Legacy keyword cues"));
        assert!(user.contains("Padraig Darcy"));
        assert!(user.contains("Player message"));
        assert!(user.contains("landlord"));
    }

    #[test]
    fn palette_has_expected_size() {
        assert_eq!(REACTION_PALETTE.len(), 12);
    }

    #[test]
    fn build_player_message_reaction_prompt_contains_palette_and_npc() {
        let npc = test_npc(1, "Padraig Darcy", "Publican", Some(LocationId(2)));
        let lang = LanguageSettings::english_only();
        let (system, user) = build_player_message_reaction_prompt(
            &npc,
            "Your landlord's agent is at the door.",
            &lang,
        );

        assert!(system.contains("Available palette"));
        assert!(system.contains("null: no visible reaction"));
        assert!(system.contains("Legacy keyword cues"));
        assert!(system.contains("landlord"));
        assert!(system.contains("STRICT JSON"));

        assert!(user.contains("Padraig Darcy"));
        assert!(user.contains("Publican"));
        assert!(user.contains("Player message"));
        assert!(user.contains("landlord's agent"));
    }

    #[test]
    fn build_player_message_reaction_prompt_truncates_long_personality() {
        let mut npc = test_npc(2, "Brigid", "Healer", None);
        npc.personality = "A".repeat(500);
        let lang = LanguageSettings::english_only();
        let (_system, user) = build_player_message_reaction_prompt(&npc, "Hello", &lang);
        let after_personality = user
            .split("- Personality: ")
            .nth(1)
            .unwrap_or("")
            .split('\n')
            .next()
            .unwrap_or("");
        assert!(after_personality.chars().count() <= 300);
    }

    #[tokio::test]
    async fn infer_player_message_reaction_simulator_returns_palette_emoji() {
        use parish_inference::AnyClient;

        let client = AnyClient::simulator();
        let npc = test_npc(3, "Seán Brennan", "Farmer", None);

        let result = infer_player_message_reaction(
            &client,
            "any-model",
            &npc,
            "The landlord is coming to collect rent.",
            std::time::Duration::from_secs(5),
        )
        .await;

        if let Some(ref emoji) = result {
            assert!(
                crate::reactions::reaction_description(emoji).is_some(),
                "returned emoji {emoji:?} not in palette"
            );
        }
    }

    #[tokio::test]
    async fn infer_player_message_reaction_timeout_returns_none() {
        use parish_inference::AnyClient;

        let client = AnyClient::simulator();
        let npc = test_npc(99, "Timeout Npc", "Farmer", None);

        let result = infer_player_message_reaction(
            &client,
            "any-model",
            &npc,
            "Will this time out?",
            std::time::Duration::ZERO,
        )
        .await;

        assert!(result.is_none());
    }

    #[test]
    fn generate_rule_reaction_deterministic_matches_known_keywords() {
        assert!(generate_rule_reaction_deterministic("rent and landlord").is_some());
        assert!(generate_rule_reaction_deterministic("strange ghost").is_some());
        assert!(generate_rule_reaction_deterministic("Just walking by here").is_none());
    }

    // ── parse_reaction_decision unit tests ──────────────────────────────────

    /// Happy path: well-formed JSON with a string emoji value.
    #[test]
    fn parse_reaction_decision_happy_path() {
        let result = parse_reaction_decision(r#"{"emoji": "😊"}"#);
        let d = result.expect("should parse happy-path JSON");
        assert_eq!(d.emoji.as_deref(), Some("😊"));
    }

    /// Qwen2.5-1.5B failure mode 1: emoji is a nested JSON map instead of a
    /// bare string — `{"emoji": {"value": "😊"}}`.
    /// Must parse without error and extract the emoji string from the map.
    #[test]
    fn parse_reaction_decision_map_where_string_expected() {
        let result = parse_reaction_decision(r#"{"emoji": {"value": "😊"}}"#);
        let d = result.expect("should recover emoji from nested map");
        assert_eq!(d.emoji.as_deref(), Some("😊"));
    }

    /// Qwen2.5-1.5B failure mode 1 (alternate key): nested map uses a
    /// different key name — `{"emoji": {"emoji": "😊"}}`.
    #[test]
    fn parse_reaction_decision_map_alternate_key() {
        let result = parse_reaction_decision(r#"{"emoji": {"emoji": "😊"}}"#);
        let d = result.expect("should recover emoji from nested map with alternate key");
        assert_eq!(d.emoji.as_deref(), Some("😊"));
    }

    /// Qwen2.5-1.5B failure mode 2: trailing text after the closing brace.
    /// Must extract the first `{...}` block and parse it successfully.
    #[test]
    fn parse_reaction_decision_trailing_characters() {
        let result = parse_reaction_decision(r#"{"emoji": "😊"} some extra text"#);
        let d = result.expect("should parse despite trailing characters");
        assert_eq!(d.emoji.as_deref(), Some("😊"));
    }

    /// Bare emoji string fallback: no JSON object present at all.
    /// Must return a decision with the emoji set to the raw string.
    #[test]
    fn parse_reaction_decision_bare_emoji_string() {
        let result = parse_reaction_decision("😊");
        let d = result.expect("should treat bare string as emoji fallback");
        assert_eq!(d.emoji.as_deref(), Some("😊"));
    }

    /// Total garbage: neither parseable JSON nor a bare emoji.
    /// Must return `None` without panicking.
    #[test]
    fn parse_reaction_decision_garbage_returns_none() {
        assert!(parse_reaction_decision("not json at all and not an emoji").is_none());
    }

    /// Explicit null in well-formed JSON → `emoji` is `None`.
    #[test]
    fn parse_reaction_decision_explicit_null() {
        let result = parse_reaction_decision(r#"{"emoji": null}"#);
        let d = result.expect("should parse null decision");
        assert!(d.emoji.is_none());
    }

    /// Code-fence-wrapped JSON is handled.
    #[test]
    fn parse_reaction_decision_code_fence() {
        let raw = "```json\n{\"emoji\": \"😊\"}\n```";
        let result = parse_reaction_decision(raw);
        let d = result.expect("should strip code fence and parse");
        assert_eq!(d.emoji.as_deref(), Some("😊"));
    }
}
