//! Player–NPC emoji reaction system and NPC arrival reaction system.
//!
//! # Emoji Reactions
//!
//! The canonical palette ([`REACTION_PALETTE`]), the [`ReactionLog`] ring buffer,
//! keyword-based rule reactions, and an LLM-informed reaction path are split
//! across three modules. This root module defines the shared palette and log;
//! [`emoji_reactions`] holds the keyword and LLM reaction logic;
//! [`arrival_reactions`] holds the NPC arrival greeting system.
//!
//! # Arrival Reactions
//!
//! When the player arrives at a location, NPCs present may react —
//! greeting, nodding, welcoming, introducing themselves, or ignoring
//! the player entirely.

use chrono::{DateTime, Utc};
use parish_types::ReactionDirection;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub(crate) mod arrival_reactions;
pub(crate) mod emoji_reactions;

// Re-export public items from sub-modules.
pub use arrival_reactions::{
    ArrivalContext, GreetingsByTime, IntroductionTemplates, LlmGreetingParams, NpcReaction,
    OccupationGreetings, ReactionKind, ReactionTemplates, WelcomesByOccupation,
    build_reaction_prompt, generate_arrival_reactions, reaction_threshold, resolve_llm_greeting,
};
pub use emoji_reactions::{
    LlmReactionDecision, build_player_message_reaction_prompt, generate_rule_reaction,
    infer_player_message_reaction, infer_player_message_reaction_with_profile,
    infer_player_message_reaction_with_profile_and_audit,
};

// ── Emoji reaction system ────────────────────────────────────────────────────

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

/// A single directional nonverbal reaction entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReactionEntry {
    /// Who performed the reaction.
    ///
    /// Legacy entries predate automatic NPC reactions and therefore default
    /// conservatively to the original player→NPC meaning.
    #[serde(default)]
    pub direction: ReactionDirection,
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
///
/// Uses a [`VecDeque`] internally so eviction of the oldest entry is O(1)
/// rather than the O(n) shift that a plain `Vec` would require. Fixes #284.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ReactionLog {
    entries: VecDeque<ReactionEntry>,
}

/// Maximum number of reaction entries to retain per NPC.
///
/// 20 entries covers the tail of a long conversation session without
/// accumulating unbounded memory. Older entries are evicted with O(1)
/// [`VecDeque::pop_front`]. Fixes #284.
const MAX_ENTRIES: usize = 20;

impl ReactionLog {
    fn push_entry(
        &mut self,
        direction: ReactionDirection,
        emoji: &str,
        context: &str,
        timestamp: DateTime<Utc>,
    ) {
        if let Some(desc) = reaction_description(emoji) {
            self.entries.push_back(ReactionEntry {
                direction,
                emoji: emoji.to_string(),
                description: desc.to_string(),
                context: context.chars().take(80).collect(),
                timestamp,
            });
            if self.entries.len() > MAX_ENTRIES {
                self.entries.pop_front();
            }
        }
    }

    /// Adds a player→NPC reaction (player reacts to something the NPC said).
    ///
    /// Evicts the oldest entry if at capacity. Only adds when the emoji is in
    /// the canonical palette. `context` is what the NPC said.
    pub fn add(&mut self, emoji: &str, context: &str, timestamp: DateTime<Utc>) {
        self.push_entry(ReactionDirection::PlayerToNpc, emoji, context, timestamp);
    }

    /// Adds an NPC→player-message reaction (NPC reacts to a player's spoken line).
    ///
    /// Evicts the oldest entry if at capacity. Only adds when the emoji is in
    /// the canonical palette. `player_message` is the player's message that
    /// triggered the reaction.
    pub fn add_player_message_reaction(
        &mut self,
        emoji: &str,
        player_message: &str,
        timestamp: DateTime<Utc>,
    ) {
        self.push_entry(
            ReactionDirection::NpcToPlayer,
            emoji,
            player_message,
            timestamp,
        );
    }

    /// Adds a reaction whose direction has already been validated.
    pub fn add_directional(
        &mut self,
        direction: ReactionDirection,
        emoji: &str,
        context: &str,
        timestamp: DateTime<Utc>,
    ) {
        self.push_entry(direction, emoji, context, timestamp);
    }

    /// Formats the `n` most recent player→NPC reactions as prompt context.
    ///
    /// Each line reads: "- The player [description] when you said [context]".
    /// Returns an empty string if there are no reactions.
    pub fn context_string(&self, n: usize) -> String {
        let lines = self
            .entries
            .iter()
            .rev()
            .filter(|entry| entry.direction == ReactionDirection::PlayerToNpc)
            .take(n)
            .map(|entry| {
                format!(
                    "- The player {} when you said \"{}\"",
                    entry.description, entry.context
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if lines.is_empty() {
            return String::new();
        }
        format!("Recent nonverbal reactions from the player:\n{lines}")
    }

    /// Formats the `n` most recent NPC→player-message reactions as prompt context.
    ///
    /// Each line reads: "- You [description] in response to the player saying [message]".
    /// Returns an empty string if there are no entries.
    pub fn npc_context_string(&self, n: usize) -> String {
        let lines = self
            .entries
            .iter()
            .rev()
            .filter(|entry| entry.direction == ReactionDirection::NpcToPlayer)
            .take(n)
            .map(|entry| {
                format!(
                    "- You {} in response to the player saying \"{}\"",
                    entry.description, entry.context
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if lines.is_empty() {
            return String::new();
        }
        format!("Recent nonverbal reactions you showed to the player:\n{lines}")
    }

    /// Formats both directional histories for the production NPC prompt.
    pub fn prompt_context_string(&self, n: usize) -> String {
        let mut player_lines = Vec::new();
        let mut npc_lines = Vec::new();
        for entry in self.entries.iter().rev().take(n) {
            match entry.direction {
                ReactionDirection::PlayerToNpc => player_lines.push(format!(
                    "- The player {} when you said \"{}\"",
                    entry.description, entry.context
                )),
                ReactionDirection::NpcToPlayer => npc_lines.push(format!(
                    "- You {} in response to the player saying \"{}\"",
                    entry.description, entry.context
                )),
            }
        }
        let mut blocks = Vec::new();
        if !player_lines.is_empty() {
            blocks.push(format!(
                "Recent nonverbal reactions from the player:\n{}",
                player_lines.join("\n")
            ));
        }
        if !npc_lines.is_empty() {
            blocks.push(format!(
                "Recent nonverbal reactions you showed to the player:\n{}",
                npc_lines.join("\n")
            ));
        }
        blocks.join("\n\n")
    }

    /// Returns the number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns an iterator over entries in chronological order (oldest first).
    ///
    /// The iterator is double-ended, so callers can call `.rev()` to walk
    /// newest-first without an intermediate allocation.
    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &ReactionEntry> + ExactSizeIterator {
        self.entries.iter()
    }
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
        for i in 0..25 {
            log.add(
                "😊",
                &format!("message {}", i),
                Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            );
        }
        assert_eq!(log.len(), MAX_ENTRIES);
        assert!(log.entries[0].context.contains("message 5"));
    }

    #[test]
    fn reaction_log_pop_front_is_o1() {
        let mut log = ReactionLog::default();
        for i in 0..=MAX_ENTRIES {
            log.add(
                "😊",
                &format!("ctx {}", i),
                Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
            );
        }
        assert_eq!(log.len(), MAX_ENTRIES);
        assert!(
            log.entries()
                .last()
                .unwrap()
                .context
                .contains(&format!("ctx {}", MAX_ENTRIES))
        );
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
        assert_eq!(deser.entries[0].direction, ReactionDirection::PlayerToNpc);
    }

    #[test]
    fn legacy_entry_without_direction_defaults_to_player_reaction() {
        let json = r#"{"entries":[{"emoji":"😊","description":"smiled warmly","context":"Welcome","timestamp":"1820-03-20T10:00:00Z"}]}"#;
        let log: ReactionLog = serde_json::from_str(json).unwrap();
        assert_eq!(
            log.entries().next().unwrap().direction,
            ReactionDirection::PlayerToNpc
        );
        assert!(log.context_string(5).contains("The player smiled warmly"));
        assert!(log.npc_context_string(5).is_empty());
    }

    #[test]
    fn npc_reaction_log_context_string_correct_format() {
        let mut log = ReactionLog::default();
        log.add_player_message_reaction(
            "😠",
            "The rent is too high",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );

        let ctx = log.npc_context_string(5);
        assert!(ctx.contains("You looked angry"));
        assert!(ctx.contains("the player saying"));
        assert!(ctx.contains("The rent is too high"));
        assert!(!ctx.contains("The player"));
    }

    #[test]
    fn player_reaction_log_context_string_unchanged() {
        let mut log = ReactionLog::default();
        log.add(
            "😊",
            "Good morning to you",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );

        let ctx = log.context_string(5);
        assert!(ctx.contains("The player smiled warmly"));
        assert!(ctx.contains("when you said"));
    }

    #[test]
    fn npc_reaction_log_ignores_unknown_emoji() {
        let mut log = ReactionLog::default();
        log.add_player_message_reaction(
            "💀",
            "some message",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        assert!(log.is_empty());
    }

    #[test]
    fn npc_reaction_log_context_string_empty() {
        let log = ReactionLog::default();
        assert_eq!(log.npc_context_string(5), "");
    }

    #[test]
    fn mixed_prompt_context_preserves_both_reaction_actors() {
        let mut log = ReactionLog::default();
        log.add(
            "😊",
            "Welcome to the pub",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
        );
        log.add_player_message_reaction(
            "👀",
            "I have no money",
            Utc.with_ymd_and_hms(1820, 3, 20, 10, 1, 0).unwrap(),
        );

        let context = log.prompt_context_string(5);
        assert!(context.contains("The player smiled warmly when you said \"Welcome to the pub\""));
        assert!(context.contains(
            "You raised an eyebrow in response to the player saying \"I have no money\""
        ));
        assert!(!context.contains("The player raised an eyebrow"));
        assert!(!context.contains("You smiled warmly in response"));
    }

    #[test]
    fn mixed_prompt_context_limit_applies_across_both_directions() {
        let mut log = ReactionLog::default();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        log.add("😊", "old player reaction", now);
        log.add_player_message_reaction("👀", "new npc reaction", now);

        let context = log.prompt_context_string(1);
        assert!(context.contains("new npc reaction"));
        assert!(!context.contains("old player reaction"));
    }
}
