//! System command parsing and input classification.
//!
//! Translates raw `/`-prefixed input strings into [`Command`] values
//! and routes anything else to free-form game input.
//!
//! # Module layout
//!
//! | Submodule   | Commands handled                                      |
//! |-------------|-------------------------------------------------------|
//! | `save`      | `/fork`, `/load`                                      |
//! | `world`     | `/map`, `/wait`, `/unexplored`, `/weather`            |
//! | `display`   | `/theme`, `/debug`, `/spinner`, `/speed`              |
//! | `inference` | `/preset`, `/provider`, `/model`, `/key`, `/cloud`,   |
//! |             | `/inference-log`, dot-notation category variants      |
//! | `flags`     | `/flag`                                               |

mod display;
mod flags;
mod inference;
mod save;
mod world;

use crate::commands::Command;
use crate::intent_types::InputResult;

/// Handler signature for commands that accept arguments.
type ArgHandler = fn(trimmed: &str, rest: &str) -> Option<Command>;

/// Attempts to parse a system command from raw input.
///
/// Returns `Some(Command)` if the input matches a known `/` command,
/// `None` otherwise.
pub fn parse_system_command(input: &str) -> Option<Command> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // Split into the command keyword and the remainder argument string.
    // e.g. "/map clonalis" → keyword="/map", rest="clonalis"
    //      "/map"          → keyword="/map", rest=""
    let (keyword, rest_trimmed) = match lower.find(' ') {
        Some(pos) => (&lower[..pos], trimmed[pos..].trim()),
        None => (lower.as_str(), ""),
    };

    // Try zero-argument commands first
    if rest_trimmed.is_empty()
        && let Some(cmd) = parse_zero_arg_command(keyword)
    {
        return Some(cmd);
    }

    // Dispatch table for commands that accept arguments.
    let handlers: &[(&str, ArgHandler)] = &[
        ("/fork", save::parse_fork_command),
        ("/load", save::parse_load_command),
        ("/map", world::parse_map_command),
        ("/wait", world::parse_wait_command),
        ("/theme", display::parse_theme_command),
        ("/unexplored", world::parse_unexplored_command),
        ("/preset", inference::parse_preset_command),
        ("/provider", inference::parse_provider_command),
        ("/model", inference::parse_model_command),
        ("/key", inference::parse_key_command),
        ("/spinner", display::parse_spinner_command),
        ("/debug", display::parse_debug_command),
        ("/speed", display::parse_speed_command),
        ("/cloud", inference::parse_cloud_command),
        ("/weather", world::parse_weather_command),
        ("/flag", flags::parse_flag_command),
        ("/inference-log", inference::parse_inference_log_command),
    ];

    for (prefix, handler) in handlers {
        if keyword == *prefix {
            return handler(trimmed, rest_trimmed);
        }
    }

    // Dot-notation per-category commands: /model.<cat>, /provider.<cat>, /key.<cat>
    if keyword.starts_with("/model.")
        || keyword.starts_with("/provider.")
        || keyword.starts_with("/key.")
    {
        // Re-assemble the full trimmed string for parse_category_command since it
        // expects the original (potentially mixed-case) trimmed input alongside the
        // lowercase version for prefix-stripping.
        return inference::parse_category_command(trimmed, &lower);
    }

    None
}

/// Returns `Some(Command)` if `keyword` is a recognised zero-argument command.
fn parse_zero_arg_command(keyword: &str) -> Option<Command> {
    match keyword {
        "/pause" => Some(Command::Pause),
        "/resume" => Some(Command::Resume),
        "/quit" | "/exit" => Some(Command::Quit),
        "/save" => Some(Command::Save),
        "/branches" => Some(Command::Branches),
        "/log" => Some(Command::Log),
        "/status" | "/where" => Some(Command::Status),
        "/help" => Some(Command::Help),
        "/hints" => Some(Command::ToggleSidebar),
        "/improv" => Some(Command::ToggleImprov),
        "/about" => Some(Command::About),
        "/designer" => Some(Command::Designer),
        "/npcs" => Some(Command::NpcsHere),
        "/time" => Some(Command::Time),
        "/new" => Some(Command::NewGame),
        "/tick" => Some(Command::Tick),
        "/flags" => Some(Command::Flags),
        "/session" | "/tune" | "/music" | "/fiddle" | "/seisiun" => Some(Command::Session),
        "/byok" | "/onboard" | "/setup" => Some(Command::ResetByok),
        _ => None,
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
    use crate::commands::Command;
    use crate::intent_types::InputResult;

    #[test]
    fn test_parse_quit() {
        assert_eq!(parse_system_command("/quit"), Some(Command::Quit));
        assert_eq!(parse_system_command("/QUIT"), Some(Command::Quit));
        assert_eq!(parse_system_command("  /quit  "), Some(Command::Quit));
        assert_eq!(parse_system_command("/exit"), Some(Command::Quit));
        assert_eq!(parse_system_command("/EXIT"), Some(Command::Quit));
        assert_eq!(parse_system_command("  /exit  "), Some(Command::Quit));
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
    /// Zero-argument commands must NOT match when trailing text is present.
    /// Regression: the refactored match split on the first space, so
    /// `/pause foo` would match when it should not.
    #[test]
    fn test_zero_arg_commands_reject_trailing_text() {
        assert_eq!(parse_system_command("/pause foo"), None);
        assert_eq!(parse_system_command("/resume now"), None);
        assert_eq!(parse_system_command("/quit please"), None);
        assert_eq!(parse_system_command("/exit please"), None);
        assert_eq!(parse_system_command("/save me"), None);
        assert_eq!(parse_system_command("/branches list"), None);
        assert_eq!(parse_system_command("/log all"), None);
        assert_eq!(parse_system_command("/status detailed"), None);
        assert_eq!(parse_system_command("/where am I"), None);
        assert_eq!(parse_system_command("/help me"), None);
        assert_eq!(parse_system_command("/hints on"), None);
        assert_eq!(parse_system_command("/improv mode"), None);
        assert_eq!(parse_system_command("/about us"), None);
        assert_eq!(parse_system_command("/designer mode"), None);
        assert_eq!(parse_system_command("/npcs here"), None);
        assert_eq!(parse_system_command("/time now"), None);
        assert_eq!(parse_system_command("/new game"), None);
        assert_eq!(parse_system_command("/tick once"), None);
        assert_eq!(parse_system_command("/flags all"), None);
        assert_eq!(parse_system_command("/session start"), None);
    }
    #[test]
    fn test_parse_unknown_command() {
        assert_eq!(parse_system_command("/unknown"), None);
        assert_eq!(parse_system_command("quit"), None);
        assert_eq!(parse_system_command("go to pub"), None);
    }
    #[test]
    fn test_classify_system_command() {
        assert_eq!(
            classify_input("/quit"),
            InputResult::SystemCommand(Command::Quit)
        );
        assert_eq!(
            classify_input("/exit"),
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
    #[test]
    fn test_parse_hints_command() {
        let cmd = parse_system_command("/hints");
        assert_eq!(cmd, Some(Command::ToggleSidebar));
    }
    #[test]
    fn test_parse_hints_command_case_insensitive() {
        let cmd = parse_system_command("/HINTS");
        assert_eq!(cmd, Some(Command::ToggleSidebar));
    }
    #[test]
    fn test_classify_hints_command() {
        let result = classify_input("/hints");
        assert_eq!(result, InputResult::SystemCommand(Command::ToggleSidebar));
    }
    #[test]
    fn test_parse_improv_command() {
        let cmd = parse_system_command("/improv");
        assert_eq!(cmd, Some(Command::ToggleImprov));
    }
    #[test]
    fn test_parse_improv_command_case_insensitive() {
        let cmd = parse_system_command("/IMPROV");
        assert_eq!(cmd, Some(Command::ToggleImprov));
    }
    #[test]
    fn test_classify_improv_command() {
        let result = classify_input("/improv");
        assert_eq!(result, InputResult::SystemCommand(Command::ToggleImprov));
    }
    #[test]
    fn test_parse_about_command() {
        assert_eq!(parse_system_command("/about"), Some(Command::About));
    }
    #[test]
    fn test_parse_about_command_case_insensitive() {
        assert_eq!(parse_system_command("/ABOUT"), Some(Command::About));
    }
    #[test]
    fn test_classify_map_command() {
        let result = classify_input("/map");
        assert_eq!(result, InputResult::SystemCommand(Command::Map(None)));
    }
    #[test]
    fn test_parse_designer_command() {
        assert_eq!(parse_system_command("/designer"), Some(Command::Designer));
        assert_eq!(parse_system_command("/DESIGNER"), Some(Command::Designer));
        assert_eq!(
            parse_system_command("  /designer  "),
            Some(Command::Designer)
        );
    }
    #[test]
    fn test_parse_npcs_command() {
        assert_eq!(parse_system_command("/npcs"), Some(Command::NpcsHere));
    }
    #[test]
    fn test_parse_time_command() {
        assert_eq!(parse_system_command("/time"), Some(Command::Time));
    }
    #[test]
    fn test_parse_where_command() {
        assert_eq!(parse_system_command("/where"), Some(Command::Status));
    }
    #[test]
    fn test_parse_new_command() {
        assert_eq!(parse_system_command("/new"), Some(Command::NewGame));
        assert_eq!(parse_system_command("/NEW"), Some(Command::NewGame));
        assert_eq!(parse_system_command("  /new  "), Some(Command::NewGame));
    }
    #[test]
    fn test_new_game_alias_policy_keeps_new_canonical() {
        assert_eq!(parse_system_command("/new-game"), None);
        assert_eq!(parse_system_command("/NEW-GAME"), None);
        assert_eq!(parse_system_command("/new-game please"), None);
        assert_eq!(
            classify_input("/new-game"),
            InputResult::GameInput("/new-game".to_string())
        );
    }
    #[test]
    fn test_new_command_policy_documentation_stays_canonical() {
        let readme = include_str!("../README.md");
        assert!(
            readme.contains("/new` is the canonical player command"),
            "parish-input README must document /new as the canonical player command"
        );
        assert!(
            readme.contains("Do not add\n  `/new-game` as a slash-command alias"),
            "parish-input README must reject /new-game as a player slash-command alias"
        );
    }
    #[test]
    fn test_parse_tick_command() {
        assert_eq!(parse_system_command("/tick"), Some(Command::Tick));
    }
    #[test]
    fn test_parse_byok_reset_aliases() {
        assert_eq!(parse_system_command("/byok"), Some(Command::ResetByok));
        assert_eq!(parse_system_command("/onboard"), Some(Command::ResetByok));
        assert_eq!(parse_system_command("/setup"), Some(Command::ResetByok));
        assert_eq!(parse_system_command("/BYOK"), Some(Command::ResetByok));
        assert_eq!(parse_system_command("/OnBoard"), Some(Command::ResetByok));
        assert_eq!(parse_system_command("  /setup  "), Some(Command::ResetByok));
    }
    #[test]
    fn test_byok_reset_aliases_reject_trailing_text() {
        assert_eq!(parse_system_command("/byok again"), None);
        assert_eq!(parse_system_command("/onboard reset"), None);
        assert_eq!(parse_system_command("/setup provider"), None);
    }
    // --- /session and music alias tests ---
    #[test]
    fn test_parse_session_aliases() {
        assert_eq!(parse_system_command("/session"), Some(Command::Session));
        assert_eq!(parse_system_command("/tune"), Some(Command::Session));
        assert_eq!(parse_system_command("/music"), Some(Command::Session));
        assert_eq!(parse_system_command("/fiddle"), Some(Command::Session));
        assert_eq!(parse_system_command("/seisiun"), Some(Command::Session));
    }
    #[test]
    fn test_parse_session_case_insensitive() {
        assert_eq!(parse_system_command("/SESSION"), Some(Command::Session));
        assert_eq!(parse_system_command("/TUNE"), Some(Command::Session));
        assert_eq!(parse_system_command("/SEISIUN"), Some(Command::Session));
    }
}
