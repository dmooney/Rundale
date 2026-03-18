//! Player input parsing and command detection.
//!
//! System commands use `/` prefix (e.g., `/quit`, `/save`).
//! All other input is natural language sent to the LLM for
//! intent parsing (move, talk, look, interact, examine).

use crate::error::ParishError;
use crate::inference::client::OllamaClient;
use serde::Deserialize;

/// A system command entered by the player.
///
/// System commands use a `/` prefix and control game meta-operations
/// like saving, loading, and quitting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Freeze all simulation ticks.
    Pause,
    /// Unfreeze simulation.
    Resume,
    /// Persist state and exit.
    Quit,
    /// Manual snapshot to current branch.
    Save,
    /// Create a new named save branch.
    Fork(String),
    /// Load a named save branch.
    Load(String),
    /// List all save branches.
    Branches,
    /// Show history of current branch.
    Log,
    /// Show current game status.
    Status,
    /// Show help text.
    Help,
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

/// Attempts to parse a system command from raw input.
///
/// Returns `Some(Command)` if the input matches a known `/` command,
/// `None` otherwise.
pub fn parse_system_command(input: &str) -> Option<Command> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if lower == "/pause" {
        Some(Command::Pause)
    } else if lower == "/resume" {
        Some(Command::Resume)
    } else if lower == "/quit" {
        Some(Command::Quit)
    } else if lower == "/save" {
        Some(Command::Save)
    } else if lower.starts_with("/fork ") {
        let name = trimmed[6..].trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(Command::Fork(name))
        }
    } else if lower.starts_with("/load ") {
        let name = trimmed[6..].trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(Command::Load(name))
        }
    } else if lower == "/branches" {
        Some(Command::Branches)
    } else if lower == "/log" {
        Some(Command::Log)
    } else if lower == "/status" {
        Some(Command::Status)
    } else if lower == "/help" {
        Some(Command::Help)
    } else {
        None
    }
}

/// Classifies raw input as either a system command or game input.
///
/// If the input starts with `/` and matches a known command, returns
/// `InputResult::SystemCommand`. Otherwise returns `InputResult::GameInput`.
pub fn classify_input(raw: &str) -> InputResult {
    let trimmed = raw.trim();
    if let Some(cmd) = parse_system_command(trimmed) {
        InputResult::SystemCommand(cmd)
    } else {
        InputResult::GameInput(trimmed.to_string())
    }
}

/// Raw JSON response from the LLM intent parser.
#[derive(Deserialize)]
struct IntentResponse {
    #[serde(default)]
    intent: Option<IntentKind>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    dialogue: Option<String>,
}

/// The system prompt used for intent parsing.
const INTENT_SYSTEM_PROMPT: &str = "\
You are a text adventure input parser. Given the player's natural language input, \
determine their intent. Respond with valid JSON containing:\n\
- \"intent\": one of \"move\", \"talk\", \"look\", \"interact\", \"examine\", \"unknown\"\n\
- \"target\": what the action is directed at (string or null)\n\
- \"dialogue\": what the player is saying, if talking (string or null)\n\
\n\
Examples:\n\
Input: \"go to the pub\" → {\"intent\": \"move\", \"target\": \"the pub\", \"dialogue\": null}\n\
Input: \"talk to Mary\" → {\"intent\": \"talk\", \"target\": \"Mary\", \"dialogue\": null}\n\
Input: \"tell Padraig I saw his cow\" → {\"intent\": \"talk\", \"target\": \"Padraig\", \"dialogue\": \"I saw his cow\"}\n\
Input: \"look around\" → {\"intent\": \"look\", \"target\": null, \"dialogue\": null}\n\
Input: \"pick up the stone\" → {\"intent\": \"interact\", \"target\": \"the stone\", \"dialogue\": null}\n\
\n\
Respond ONLY with valid JSON. No explanation.";

/// Parses natural language input into a structured `PlayerIntent` via LLM.
///
/// Sends the player's input to Ollama for intent classification. Falls back
/// to `IntentKind::Unknown` if the LLM response cannot be parsed.
pub async fn parse_intent(
    client: &OllamaClient,
    raw_input: &str,
    model: &str,
) -> Result<PlayerIntent, ParishError> {
    let result = client
        .generate_json::<IntentResponse>(model, raw_input, Some(INTENT_SYSTEM_PROMPT))
        .await;

    match result {
        Ok(resp) => Ok(PlayerIntent {
            intent: resp.intent.unwrap_or(IntentKind::Unknown),
            target: resp.target,
            dialogue: resp.dialogue,
            raw: raw_input.to_string(),
        }),
        Err(_) => Ok(PlayerIntent {
            intent: IntentKind::Unknown,
            target: None,
            dialogue: None,
            raw: raw_input.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quit() {
        assert_eq!(parse_system_command("/quit"), Some(Command::Quit));
        assert_eq!(parse_system_command("/QUIT"), Some(Command::Quit));
        assert_eq!(parse_system_command("  /quit  "), Some(Command::Quit));
    }

    #[test]
    fn test_parse_fork() {
        assert_eq!(
            parse_system_command("/fork main"),
            Some(Command::Fork("main".to_string()))
        );
        assert_eq!(
            parse_system_command("/fork  my save "),
            Some(Command::Fork("my save".to_string()))
        );
    }

    #[test]
    fn test_parse_load() {
        assert_eq!(
            parse_system_command("/load main"),
            Some(Command::Load("main".to_string()))
        );
    }

    #[test]
    fn test_parse_all_commands() {
        assert_eq!(parse_system_command("/pause"), Some(Command::Pause));
        assert_eq!(parse_system_command("/resume"), Some(Command::Resume));
        assert_eq!(parse_system_command("/save"), Some(Command::Save));
        assert_eq!(parse_system_command("/branches"), Some(Command::Branches));
        assert_eq!(parse_system_command("/log"), Some(Command::Log));
        assert_eq!(parse_system_command("/status"), Some(Command::Status));
        assert_eq!(parse_system_command("/help"), Some(Command::Help));
    }

    #[test]
    fn test_parse_unknown_command() {
        assert_eq!(parse_system_command("/unknown"), None);
        assert_eq!(parse_system_command("quit"), None);
        assert_eq!(parse_system_command("go to pub"), None);
    }

    #[test]
    fn test_parse_fork_empty_name() {
        assert_eq!(parse_system_command("/fork "), None);
        assert_eq!(parse_system_command("/fork   "), None);
    }

    #[test]
    fn test_classify_system_command() {
        assert_eq!(
            classify_input("/quit"),
            InputResult::SystemCommand(Command::Quit)
        );
        assert_eq!(
            classify_input("/fork main"),
            InputResult::SystemCommand(Command::Fork("main".to_string()))
        );
    }

    #[test]
    fn test_classify_game_input() {
        assert_eq!(
            classify_input("go to the pub"),
            InputResult::GameInput("go to the pub".to_string())
        );
        assert_eq!(
            classify_input("tell Mary hello"),
            InputResult::GameInput("tell Mary hello".to_string())
        );
    }

    #[test]
    fn test_classify_unknown_slash_command() {
        // Unknown /commands fall through as game input
        assert_eq!(
            classify_input("/dance"),
            InputResult::GameInput("/dance".to_string())
        );
    }

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
    fn test_intent_response_deserialize() {
        let json = r#"{"intent": "move", "target": "the pub", "dialogue": null}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.intent, Some(IntentKind::Move));
        assert_eq!(resp.target, Some("the pub".to_string()));
        assert!(resp.dialogue.is_none());
    }

    #[test]
    fn test_intent_response_empty() {
        let json = r#"{}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert!(resp.intent.is_none());
        assert!(resp.target.is_none());
        assert!(resp.dialogue.is_none());
    }

    #[test]
    fn test_classify_whitespace() {
        assert_eq!(
            classify_input("  /quit  "),
            InputResult::SystemCommand(Command::Quit)
        );
        assert_eq!(
            classify_input("  hello  "),
            InputResult::GameInput("hello".to_string())
        );
    }
}
