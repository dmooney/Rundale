//! Player intent and input classification result types.
//!
//! These types describe the output of intent parsing and the
//! command/free-text classification step.

use serde::Deserialize;

use crate::commands::Command;

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
