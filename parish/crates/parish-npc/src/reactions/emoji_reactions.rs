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
use parish_inference::AnyClient;

/// Keyword groups that trigger NPC reactions, with the corresponding emoji.
const KEYWORD_REACTIONS: &[(&[&str], &str)] = &[
    (&["death", "died", "killed", "murder"], "😢"),
    (&["fairy", "fairies", "púca", "banshee", "sidhe"], "✝️"),
    (&["drink", "whiskey", "poitín", "ale", "stout"], "🍺"),
    (&["joke", "funny", "laugh", "haha"], "😂"),
    (&["secret", "don't tell", "between us", "confidence"], "🤫"),
    (&["rent", "evict", "landlord", "agent", "tithe"], "😠"),
    (&["gold", "treasure", "fortune", "money", "reward"], "👀"),
    (&["strange", "ghost", "haunted", "spirit"], "😳"),
];

/// Generates a rule-based NPC reaction to player input.
///
/// Returns `Some(emoji)` if a keyword match triggers a reaction (60% chance),
/// or `None` if no reaction is generated.
pub fn generate_rule_reaction(player_input: &str) -> Option<String> {
    let input_lower = player_input.to_lowercase();

    for (keywords, emoji) in KEYWORD_REACTIONS {
        if keywords.iter().any(|kw| input_lower.contains(kw)) {
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
    let input_lower = player_input.to_lowercase();

    for (keywords, emoji) in KEYWORD_REACTIONS {
        if keywords.iter().any(|kw| input_lower.contains(kw)) {
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
    pub emoji: Option<String>,
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
    let lang = LanguageSettings::english_only();
    let (system, prompt) = build_player_message_reaction_prompt(npc, player_input, &lang);
    let call =
        client.generate_json::<LlmReactionDecision>(model, &prompt, Some(&system), Some(40), None);

    let response = match tokio::time::timeout(timeout, call).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            tracing::error!(error = ?e, "inference call failed in infer_player_message_reaction");
            return None;
        }
        Err(_) => {
            tracing::warn!(
                "inference call timed out after {:?} in infer_player_message_reaction",
                timeout
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
    use crate::types::Intelligence;
    use parish_types::LocationId;

    fn test_npc(id: u32, name: &str, occupation: &str, workplace: Option<LocationId>) -> Npc {
        let mut npc = crate::test_helpers::make_test_npc(id, workplace.unwrap_or(LocationId(1)).0);
        npc.name = name.to_string();
        npc.brief_description = format!("a {}", occupation.to_lowercase());
        npc.age = 40;
        npc.occupation = occupation.to_string();
        npc.personality = "A friendly person.".to_string();
        npc.intelligence = Intelligence {
            verbal: 3,
            analytical: 3,
            emotional: 3,
            practical: 3,
            wisdom: 3,
            creative: 3,
        };
        npc.workplace = workplace;
        npc
    }

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
            generate_rule_reaction_deterministic("Good morning to you"),
            None
        );
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
        assert!(generate_rule_reaction_deterministic("perfectly normal day").is_none());
    }
}
