//! Player input parsing and command detection.
//!
//! System commands use `/` prefix (e.g., `/quit`, `/save`).
//! All other input is natural language sent to the LLM for
//! intent parsing (move, talk, look, interact, examine).

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
#[derive(Debug, Clone, PartialEq, Eq)]
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
