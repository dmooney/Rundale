//! Player–NPC emoji reaction system.
//!
//! Supports three flows:
//! 1. Player reacts to NPC messages (stored in [`ReactionLog`], injected into prompts)
//! 2. NPCs react to player messages (rule-based keyword matching)
//! 3. NPC-to-NPC reactions (future, via Tier 2 ticks)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The canonical reaction palette mapping emoji to natural-language descriptions.
///
/// Period-appropriate gestures for an 1820s Irish parish. The UI shows emoji;
/// NPC context receives the description string.
pub const REACTION_PALETTE: &[(&str, &str)] = &[
    ("😊", "smiled warmly"),
    ("😠", "looked angry"),
    ("😢", "looked sorrowful"),
    ("😳", "looked startled"),
    ("🤔", "looked thoughtful"),
    ("😏", "smirked knowingly"),
    ("👀", "raised an eyebrow"),
    ("🤫", "made a hushing gesture"),
    ("😂", "laughed heartily"),
    ("🙄", "rolled their eyes"),
    ("🍺", "raised a glass"),
    ("✝️", "crossed themselves"),
];

/// Look up the natural-language description for a reaction emoji.
///
/// Returns `None` if the emoji is not in the palette.
pub fn reaction_description(emoji: &str) -> Option<&'static str> {
    REACTION_PALETTE
        .iter()
        .find(|(e, _)| *e == emoji)
        .map(|(_, desc)| *desc)
}

/// A single reaction entry recording a player's nonverbal response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionEntry {
    /// The emoji used.
    pub emoji: String,
    /// Natural-language description (e.g. "looked angry").
    pub description: String,
    /// Truncated context — what the NPC said that was reacted to.
    pub context: String,
    /// When the reaction occurred.
    pub timestamp: DateTime<Utc>,
}

/// Ring buffer of recent player reactions toward an NPC.
///
/// Stores the last [`MAX_ENTRIES`] reactions and formats them as prompt
/// context so the NPC is aware of the player's nonverbal feedback.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReactionLog {
    entries: Vec<ReactionEntry>,
}

/// Maximum number of reaction entries to retain.
const MAX_ENTRIES: usize = 10;

impl ReactionLog {
    /// Adds a player reaction, evicting the oldest if at capacity.
    ///
    /// Only adds the reaction if the emoji is in the canonical palette.
    pub fn add(&mut self, emoji: &str, context: &str, timestamp: DateTime<Utc>) {
        if let Some(desc) = reaction_description(emoji) {
            self.entries.push(ReactionEntry {
                emoji: emoji.to_string(),
                description: desc.to_string(),
                context: context.chars().take(80).collect(),
                timestamp,
            });
            if self.entries.len() > MAX_ENTRIES {
                self.entries.remove(0);
            }
        }
    }

    /// Formats the `n` most recent reactions as prompt context.
    ///
    /// Returns an empty string if there are no reactions.
    pub fn context_string(&self, n: usize) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let lines: Vec<String> = self
            .entries
            .iter()
            .rev()
            .take(n)
            .map(|e| {
                format!(
                    "- The player {} when you said \"{}\"",
                    e.description, e.context
                )
            })
            .collect();
        format!(
            "Recent nonverbal reactions from the player:\n{}",
            lines.join("\n")
        )
    }

    /// Returns the number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn reaction_description_known_emoji() {
        assert_eq!(reaction_description("😊"), Some("smiled warmly"));
        assert_eq!(reaction_description("😠"), Some("looked angry"));
        assert_eq!(reaction_description("✝️"), Some("crossed themselves"));
    }

    #[test]
    fn reaction_description_unknown_emoji() {
        assert_eq!(reaction_description("💀"), None);
        assert_eq!(reaction_description("hello"), None);
    }

    #[test]
    fn reaction_log_add_and_len() {
        let mut log = ReactionLog::default();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);

        log.add(
            "😊",
            "Hello there",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        assert_eq!(log.len(), 1);
        assert!(!log.is_empty());
    }

    #[test]
    fn reaction_log_ignores_unknown_emoji() {
        let mut log = ReactionLog::default();
        log.add(
            "💀",
            "test",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        assert!(log.is_empty());
    }

    #[test]
    fn reaction_log_caps_at_max_entries() {
        let mut log = ReactionLog::default();
        for i in 0..15 {
            log.add(
                "😊",
                &format!("message {}", i),
                Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            );
        }
        assert_eq!(log.len(), MAX_ENTRIES);
        // Oldest entries should be evicted
        assert!(log.entries[0].context.contains("message 5"));
    }

    #[test]
    fn reaction_log_truncates_context() {
        let mut log = ReactionLog::default();
        let long_context = "a".repeat(200);
        log.add(
            "😊",
            &long_context,
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        assert_eq!(log.entries[0].context.len(), 80);
    }

    #[test]
    fn reaction_log_context_string_empty() {
        let log = ReactionLog::default();
        assert_eq!(log.context_string(5), "");
    }

    #[test]
    fn reaction_log_context_string_formats_correctly() {
        let mut log = ReactionLog::default();
        log.add(
            "😠",
            "The rent was raised",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        log.add(
            "😊",
            "Welcome to the pub",
            Utc.with_ymd_and_hms(1820, 3, 20, 11, 0, 0).unwrap(),
        );

        let ctx = log.context_string(5);
        assert!(ctx.contains("Recent nonverbal reactions from the player:"));
        assert!(ctx.contains("smiled warmly"));
        assert!(ctx.contains("looked angry"));
        assert!(ctx.contains("The rent was raised"));
    }

    #[test]
    fn reaction_log_context_string_respects_limit() {
        let mut log = ReactionLog::default();
        for i in 0..5 {
            log.add(
                "😊",
                &format!("msg {}", i),
                Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            );
        }
        let ctx = log.context_string(2);
        // Should only contain the 2 most recent
        assert!(ctx.contains("msg 4"));
        assert!(ctx.contains("msg 3"));
        assert!(!ctx.contains("msg 2"));
    }

    #[test]
    fn reaction_log_serde_round_trip() {
        let mut log = ReactionLog::default();
        log.add(
            "😊",
            "test message",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );

        let json = serde_json::to_string(&log).unwrap();
        let deser: ReactionLog = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.len(), 1);
        assert_eq!(deser.entries[0].emoji, "😊");
    }

    #[test]
    fn generate_rule_reaction_keyword_match() {
        // Deterministic variant always returns on match
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
    fn palette_has_expected_size() {
        assert_eq!(REACTION_PALETTE.len(), 12);
    }
}
