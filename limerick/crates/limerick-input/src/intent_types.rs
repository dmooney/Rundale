//! Player intent and input classification result types.
//!
//! These types describe the output of intent parsing and the
//! command/free-text classification step.

use serde::Deserialize;

use crate::commands::Command;

/// An atmospheric subject present in the player's natural-language input.
///
/// This supplements, rather than replaces, the primary [`IntentKind`]. The
/// runtime uses it to enrich conversational routing; it does not independently
/// turn movement, examination, or interaction into an atmospheric action.
/// Explicitly addressing an NPC can make otherwise action-shaped text
/// conversational, in which case this topic may still supply an atmospheric
/// cue alongside that conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AtmosphericTopic {
    /// Listening to the wider place or living world.
    Listen,
    /// Omens, portents, or deliberately seeking a supernatural sign.
    Omen,
    /// Folklore, local legends, or old/traditional tales.
    Folklore,
}

/// The kind of player action parsed from natural language input.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntentKind {
    /// Move to a location.
    Move,
    /// Talk to an NPC.
    Talk,
    /// Look around or at something.
    Look,
    /// Interact with an object or NPC.
    Interact,
    /// Examine something closely.
    Examine,
    /// Intent could not be determined.
    Unknown,
}

/// A parsed player intent derived from natural language input.
///
/// Created by LLM-based intent parsing of the player's raw text.
#[derive(Debug, Clone)]
pub struct PlayerIntent {
    /// The kind of action the player wants to take.
    pub intent: IntentKind,
    /// The target of the action (e.g. an NPC name, location, object).
    pub target: Option<String>,
    /// Dialogue text if the player is speaking.
    pub dialogue: Option<String>,
    /// Optional atmospheric subject available when the input routes as
    /// conversation.
    pub atmosphere: Option<AtmosphericTopic>,
    /// The original raw input text.
    pub raw: String,
}

/// The result of classifying raw player input.
///
/// Input is either a system command (prefixed with `/`) or free-form
/// game input to be parsed by the LLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputResult {
    /// A recognized system command.
    SystemCommand(Command),
    /// Free-form game input for LLM parsing.
    GameInput(String),
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_kind_deserialize() {
        let json = r#""move""#;
        let kind: IntentKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, IntentKind::Move);

        let json = r#""talk""#;
        let kind: IntentKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, IntentKind::Talk);

        let json = r#""unknown""#;
        let kind: IntentKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, IntentKind::Unknown);
    }

    #[test]
    fn atmospheric_topic_deserializes_and_is_copyable() {
        let topic: AtmosphericTopic = serde_json::from_str(r#""folklore""#).unwrap();
        let copied = topic;
        assert_eq!(copied, AtmosphericTopic::Folklore);
    }
}
