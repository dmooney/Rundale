//! System command parsing and input classification.
//!
//! Translates raw `/`-prefixed input strings into [`Command`] values
//! and routes anything else to free-form game input.

use parish_config::InferenceCategory;
use parish_types::GameSpeed;

use crate::commands::{
    Command, FlagSubcommand, InferenceLogSub, validate_branch_name, validate_flag_name,
};
use crate::intent_types::InputResult;

const SPINNER_DEFAULT_SECS: u64 = 30;
const SPINNER_MAX_SECS: u64 = 300;

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
        ("/fork", parse_fork_command),
        ("/load", parse_load_command),
        ("/map", parse_map_command),
        ("/wait", parse_wait_command),
        ("/theme", parse_theme_command),
        ("/unexplored", parse_unexplored_command),
        ("/preset", parse_preset_command),
        ("/provider", parse_provider_command),
        ("/model", parse_model_command),
        ("/key", parse_key_command),
        ("/spinner", parse_spinner_command),
        ("/debug", parse_debug_command),
        ("/speed", parse_speed_command),
        ("/cloud", parse_cloud_command),
        ("/weather", parse_weather_command),
        ("/flag", parse_flag_command),
        ("/inference-log", parse_inference_log_command),
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
        return parse_category_command(trimmed, &lower);
    }

    None
}

/// Parses `/inference-log [on|off|status|path]`.
fn parse_inference_log_command(_trimmed: &str, rest: &str) -> Option<Command> {
    let sub = match rest.to_lowercase().as_str() {
        "on" | "enable" | "start" => InferenceLogSub::On,
        "off" | "disable" | "stop" => InferenceLogSub::Off,
        "path" | "where" => InferenceLogSub::Path,
        // bare command and "status" both report status
        _ => InferenceLogSub::Status,
    };
    Some(Command::InferenceLog(sub))
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

fn parse_fork_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Help) // bare /fork → show help
    } else {
        match validate_branch_name(rest) {
            Ok(valid) => Some(Command::Fork(valid)),
            Err(msg) => Some(Command::InvalidBranchName(msg)),
        }
    }
}

fn parse_load_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Load(String::new())) // empty string = show save picker
    } else {
        match validate_branch_name(rest) {
            Ok(valid) => Some(Command::Load(valid)),
            Err(msg) => Some(Command::InvalidBranchName(msg)),
        }
    }
}

fn parse_map_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Map(None))
    } else {
        Some(Command::Map(Some(rest.to_string())))
    }
}

fn parse_wait_command(_trimmed: &str, rest: &str) -> Option<Command> {
    let mins = rest.parse::<u32>().unwrap_or(15);
    Some(Command::Wait(mins))
}

fn parse_theme_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Theme(None))
    } else {
        Some(Command::Theme(Some(rest.to_string())))
    }
}

fn parse_unexplored_command(_trimmed: &str, rest: &str) -> Option<Command> {
    let arg = rest.to_lowercase();
    match arg.as_str() {
        "reveal" | "show" | "on" => Some(Command::Unexplored(Some(true))),
        "hide" | "off" => Some(Command::Unexplored(Some(false))),
        _ => Some(Command::Unexplored(None)),
    }
}

fn parse_preset_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowPreset)
    } else {
        Some(Command::ApplyPreset(rest.to_string()))
    }
}

fn parse_provider_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowProvider)
    } else {
        Some(Command::SetProvider(rest.to_string()))
    }
}

fn parse_model_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowModel)
    } else {
        Some(Command::SetModel(rest.to_string()))
    }
}

fn parse_key_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowKey)
    } else {
        Some(Command::SetKey(rest.to_string()))
    }
}

fn parse_spinner_command(_trimmed: &str, rest: &str) -> Option<Command> {
    let secs = rest
        .parse::<u64>()
        .unwrap_or(SPINNER_DEFAULT_SECS)
        .min(SPINNER_MAX_SECS);
    Some(Command::Spinner(secs))
}

fn parse_debug_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Debug(None))
    } else {
        Some(Command::Debug(Some(rest.to_string())))
    }
}

fn parse_speed_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowSpeed)
    } else {
        match GameSpeed::from_name(rest) {
            Some(speed) => Some(Command::SetSpeed(speed)),
            None => Some(Command::InvalidSpeed(rest.to_string())),
        }
    }
}

fn parse_cloud_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowCloud)
    } else {
        parse_cloud_subcommand(rest)
    }
}

fn parse_weather_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Weather(None))
    } else {
        Some(Command::Weather(Some(rest.to_string())))
    }
}

fn parse_flag_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() || rest.to_lowercase() == "list" {
        Some(Command::Flag(FlagSubcommand::List))
    } else {
        parse_flag_subcommand(rest)
    }
}

/// Parses `/cloud <subcommand>` arguments.
fn parse_cloud_subcommand(rest: &str) -> Option<Command> {
    let rest_lower = rest.to_lowercase();

    // Split subcommand keyword from its argument.
    let (sub_kw, sub_arg) = match rest_lower.find(' ') {
        Some(pos) => (&rest_lower[..pos], rest[pos..].trim()),
        None => (rest_lower.as_str(), ""),
    };

    match sub_kw {
        "provider" => {
            if sub_arg.is_empty() {
                Some(Command::ShowCloud)
            } else {
                Some(Command::SetCloudProvider(sub_arg.to_string()))
            }
        }
        "model" => {
            if sub_arg.is_empty() {
                Some(Command::ShowCloudModel)
            } else {
                Some(Command::SetCloudModel(sub_arg.to_string()))
            }
        }
        "key" => {
            if sub_arg.is_empty() {
                Some(Command::ShowCloudKey)
            } else {
                Some(Command::SetCloudKey(sub_arg.to_string()))
            }
        }
        _ => Some(Command::ShowCloud),
    }
}

/// Parses `/flag <subcommand>` arguments (enable/disable/list).
fn parse_flag_subcommand(rest: &str) -> Option<Command> {
    let rest_lower = rest.to_lowercase();

    let (sub_kw, sub_arg) = match rest_lower.find(' ') {
        Some(pos) => (&rest_lower[..pos], rest[pos..].trim()),
        None => (rest_lower.as_str(), ""),
    };

    match sub_kw {
        "enable" => {
            if sub_arg.is_empty() {
                // `/flag enable` with no name → show list
                Some(Command::Flag(FlagSubcommand::List))
            } else {
                match validate_flag_name(sub_arg) {
                    Ok(valid) => Some(Command::Flag(FlagSubcommand::Enable(valid))),
                    Err(msg) => Some(Command::InvalidFlagName(msg)),
                }
            }
        }
        "disable" => {
            if sub_arg.is_empty() {
                Some(Command::Flag(FlagSubcommand::List))
            } else {
                match validate_flag_name(sub_arg) {
                    Ok(valid) => Some(Command::Flag(FlagSubcommand::Disable(valid))),
                    Err(msg) => Some(Command::InvalidFlagName(msg)),
                }
            }
        }
        "list" => Some(Command::Flag(FlagSubcommand::List)),
        _ => Some(Command::InvalidFlagName(format!(
            "Unknown flag sub-command '{}'. Use: /flag enable <name>, /flag disable <name>, /flag list",
            rest
        ))),
    }
}

/// Parses dot-notation per-category commands like `/model.dialogue`, `/provider.intent`.
///
/// Returns `Some(Command)` if the input matches a `/<base>.<category>` pattern
/// where base is `model`, `provider`, or `key`, and category is `dialogue`,
/// `simulation`, or `intent`.
fn parse_category_command(trimmed: &str, lower: &str) -> Option<Command> {
    for (prefix, show_fn, set_fn) in &[
        (
            "/model.",
            Command::ShowCategoryModel as fn(InferenceCategory) -> Command,
            Command::SetCategoryModel as fn(InferenceCategory, String) -> Command,
        ),
        (
            "/provider.",
            Command::ShowCategoryProvider as fn(InferenceCategory) -> Command,
            Command::SetCategoryProvider as fn(InferenceCategory, String) -> Command,
        ),
        (
            "/key.",
            Command::ShowCategoryKey as fn(InferenceCategory) -> Command,
            Command::SetCategoryKey as fn(InferenceCategory, String) -> Command,
        ),
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let (cat_str, arg) = match rest.find(' ') {
                Some(pos) => (&rest[..pos], trimmed[prefix.len() + pos..].trim()),
                None => (rest, ""),
            };
            let category = InferenceCategory::from_name(cat_str)?;
            if arg.is_empty() {
                return Some(show_fn(category));
            } else {
                return Some(set_fn(category, arg.to_string()));
            }
        }
    }
    None
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
    use crate::commands::{Command, FlagSubcommand};
    use crate::intent_types::InputResult;
    use parish_config::InferenceCategory;
    use parish_types::GameSpeed;

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
    fn test_parse_fork_empty_name() {
        // Bare /fork with only whitespace returns Help command
        assert_eq!(parse_system_command("/fork "), Some(Command::Help));
        assert_eq!(parse_system_command("/fork   "), Some(Command::Help));
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
    fn test_parse_map_command() {
        assert_eq!(parse_system_command("/map"), Some(Command::Map(None)));
        assert_eq!(parse_system_command("/map   "), Some(Command::Map(None)));
        assert_eq!(
            parse_system_command("/map osm"),
            Some(Command::Map(Some("osm".to_string())))
        );
        assert_eq!(
            parse_system_command("/map historic"),
            Some(Command::Map(Some("historic".to_string())))
        );
    }
    #[test]
    fn test_parse_map_command_case_insensitive() {
        assert_eq!(parse_system_command("/MAP"), Some(Command::Map(None)));
        assert_eq!(
            parse_system_command("/MAP OSM"),
            Some(Command::Map(Some("OSM".to_string())))
        );
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
    fn test_parse_wait_command() {
        assert_eq!(parse_system_command("/wait"), Some(Command::Wait(15)));
        assert_eq!(parse_system_command("/wait 60"), Some(Command::Wait(60)));
        assert_eq!(parse_system_command("/wait abc"), Some(Command::Wait(15)));
    }
    #[test]
    fn test_parse_wait_large_input_fallback() {
        // Values too large for u32 fall back to the default of 15
        assert_eq!(
            parse_system_command("/wait 999999999999999999999"),
            Some(Command::Wait(15))
        );
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
    fn test_parse_theme_command() {
        assert_eq!(parse_system_command("/theme"), Some(Command::Theme(None)));
        assert_eq!(
            parse_system_command("/theme default"),
            Some(Command::Theme(Some("default".to_string())))
        );
        assert_eq!(
            parse_system_command("/theme solarized"),
            Some(Command::Theme(Some("solarized".to_string())))
        );
        assert_eq!(
            parse_system_command("/theme solarized light"),
            Some(Command::Theme(Some("solarized light".to_string())))
        );
        assert_eq!(
            parse_system_command("/theme solarized dark"),
            Some(Command::Theme(Some("solarized dark".to_string())))
        );
        assert_eq!(
            parse_system_command("/theme solarized auto"),
            Some(Command::Theme(Some("solarized auto".to_string())))
        );
        assert_eq!(
            parse_system_command("/THEME Solarized Dark"),
            Some(Command::Theme(Some("Solarized Dark".to_string())))
        );
    }
    #[test]
    fn test_parse_unexplored_command() {
        assert_eq!(
            parse_system_command("/unexplored"),
            Some(Command::Unexplored(None))
        );
        assert_eq!(
            parse_system_command("/unexplored reveal"),
            Some(Command::Unexplored(Some(true)))
        );
        assert_eq!(
            parse_system_command("/unexplored hide"),
            Some(Command::Unexplored(Some(false)))
        );
        assert_eq!(
            parse_system_command("/unexplored on"),
            Some(Command::Unexplored(Some(true)))
        );
        assert_eq!(
            parse_system_command("/unexplored off"),
            Some(Command::Unexplored(Some(false)))
        );
        assert_eq!(
            parse_system_command("/unexplored whatever"),
            Some(Command::Unexplored(None))
        );
    }
    #[test]
    fn test_parse_provider_show() {
        assert_eq!(
            parse_system_command("/provider"),
            Some(Command::ShowProvider)
        );
        assert_eq!(
            parse_system_command("/provider   "),
            Some(Command::ShowProvider)
        );
    }
    #[test]
    fn test_parse_provider_set() {
        assert_eq!(
            parse_system_command("/provider openrouter"),
            Some(Command::SetProvider("openrouter".to_string()))
        );
        assert_eq!(
            parse_system_command("/provider  ollama "),
            Some(Command::SetProvider("ollama".to_string()))
        );
    }
    #[test]
    fn test_parse_model_show() {
        assert_eq!(parse_system_command("/model"), Some(Command::ShowModel));
    }
    #[test]
    fn test_parse_model_set() {
        assert_eq!(
            parse_system_command("/model google/gemma-3-1b-it:free"),
            Some(Command::SetModel("google/gemma-3-1b-it:free".to_string()))
        );
    }
    #[test]
    fn test_parse_key_show() {
        assert_eq!(parse_system_command("/key"), Some(Command::ShowKey));
    }
    #[test]
    fn test_parse_key_set() {
        assert_eq!(
            parse_system_command("/key sk-or-v1-abc123"),
            Some(Command::SetKey("sk-or-v1-abc123".to_string()))
        );
    }
    #[test]
    fn test_parse_preset_show_bare() {
        assert_eq!(parse_system_command("/preset"), Some(Command::ShowPreset));
        assert_eq!(
            parse_system_command("/preset   "),
            Some(Command::ShowPreset)
        );
    }
    #[test]
    fn test_parse_preset_apply() {
        assert_eq!(
            parse_system_command("/preset anthropic"),
            Some(Command::ApplyPreset("anthropic".to_string()))
        );
        assert_eq!(
            parse_system_command("/preset  ollama "),
            Some(Command::ApplyPreset("ollama".to_string()))
        );
    }
    #[test]
    fn test_parse_preset_case_insensitive() {
        // The /preset prefix is matched case-insensitively, but the argument
        // is preserved verbatim — Provider::from_str_loose handles casing.
        assert_eq!(
            parse_system_command("/PRESET Anthropic"),
            Some(Command::ApplyPreset("Anthropic".to_string()))
        );
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
    #[test]
    fn test_parse_provider_case_insensitive() {
        assert_eq!(
            parse_system_command("/PROVIDER"),
            Some(Command::ShowProvider)
        );
        assert_eq!(
            parse_system_command("/Provider OpenRouter"),
            Some(Command::SetProvider("OpenRouter".to_string()))
        );
    }
    #[test]
    fn test_parse_cloud_show() {
        assert_eq!(parse_system_command("/cloud"), Some(Command::ShowCloud));
    }
    #[test]
    fn test_parse_cloud_provider_set() {
        assert_eq!(
            parse_system_command("/cloud provider openrouter"),
            Some(Command::SetCloudProvider("openrouter".to_string()))
        );
    }
    #[test]
    fn test_parse_cloud_model_show() {
        assert_eq!(
            parse_system_command("/cloud model"),
            Some(Command::ShowCloudModel)
        );
    }
    #[test]
    fn test_parse_cloud_model_set() {
        assert_eq!(
            parse_system_command("/cloud model anthropic/claude-sonnet-4-20250514"),
            Some(Command::SetCloudModel(
                "anthropic/claude-sonnet-4-20250514".to_string()
            ))
        );
    }
    #[test]
    fn test_parse_cloud_key_show() {
        assert_eq!(
            parse_system_command("/cloud key"),
            Some(Command::ShowCloudKey)
        );
    }
    #[test]
    fn test_parse_cloud_key_set() {
        assert_eq!(
            parse_system_command("/cloud key sk-test-key"),
            Some(Command::SetCloudKey("sk-test-key".to_string()))
        );
    }
    #[test]
    fn test_parse_cloud_unknown_subcommand() {
        // Unknown subcommands show cloud status
        assert_eq!(
            parse_system_command("/cloud foobar"),
            Some(Command::ShowCloud)
        );
    }
    #[test]
    fn test_parse_speed_show() {
        assert_eq!(parse_system_command("/speed"), Some(Command::ShowSpeed));
    }
    #[test]
    fn test_parse_speed_set_variants() {
        assert_eq!(
            parse_system_command("/speed slow"),
            Some(Command::SetSpeed(GameSpeed::Slow))
        );
        assert_eq!(
            parse_system_command("/speed normal"),
            Some(Command::SetSpeed(GameSpeed::Normal))
        );
        assert_eq!(
            parse_system_command("/speed fast"),
            Some(Command::SetSpeed(GameSpeed::Fast))
        );
        assert_eq!(
            parse_system_command("/speed fastest"),
            Some(Command::SetSpeed(GameSpeed::Fastest))
        );
    }
    #[test]
    fn test_parse_speed_case_insensitive() {
        assert_eq!(
            parse_system_command("/speed FAST"),
            Some(Command::SetSpeed(GameSpeed::Fast))
        );
        assert_eq!(
            parse_system_command("/speed Slow"),
            Some(Command::SetSpeed(GameSpeed::Slow))
        );
        assert_eq!(
            parse_system_command("/SPEED normal"),
            Some(Command::SetSpeed(GameSpeed::Normal))
        );
    }
    #[test]
    fn test_parse_speed_invalid_shows_error() {
        assert_eq!(
            parse_system_command("/speed bogus"),
            Some(Command::InvalidSpeed("bogus".to_string()))
        );
    }
    #[test]
    fn test_parse_speed_whitespace_shows_current() {
        assert_eq!(parse_system_command("/speed   "), Some(Command::ShowSpeed));
    }
    // --- /debug command tests ---
    #[test]
    fn test_parse_debug_bare() {
        assert_eq!(parse_system_command("/debug"), Some(Command::Debug(None)));
    }
    #[test]
    fn test_parse_debug_with_subcommand() {
        assert_eq!(
            parse_system_command("/debug npcs"),
            Some(Command::Debug(Some("npcs".to_string())))
        );
        assert_eq!(
            parse_system_command("/debug memory Padraig"),
            Some(Command::Debug(Some("memory Padraig".to_string())))
        );
    }
    #[test]
    fn test_parse_debug_with_empty_trailing_space() {
        assert_eq!(
            parse_system_command("/debug   "),
            Some(Command::Debug(None))
        );
    }
    #[test]
    fn test_parse_debug_case_insensitive() {
        assert_eq!(parse_system_command("/DEBUG"), Some(Command::Debug(None)));
        assert_eq!(
            parse_system_command("/DEBUG npcs"),
            Some(Command::Debug(Some("npcs".to_string())))
        );
    }
    // --- /spinner command tests ---
    #[test]
    fn test_parse_spinner_bare() {
        assert_eq!(parse_system_command("/spinner"), Some(Command::Spinner(30)));
    }
    #[test]
    fn test_parse_spinner_with_duration() {
        assert_eq!(
            parse_system_command("/spinner 10"),
            Some(Command::Spinner(10))
        );
        assert_eq!(
            parse_system_command("/spinner 120"),
            Some(Command::Spinner(120))
        );
    }
    #[test]
    fn test_parse_spinner_invalid_duration() {
        // Non-numeric falls back to 30
        assert_eq!(
            parse_system_command("/spinner abc"),
            Some(Command::Spinner(30))
        );
    }
    #[test]
    fn test_parse_spinner_clamped_to_max() {
        // Values above SPINNER_MAX_SECS (300) are clamped
        assert_eq!(
            parse_system_command("/spinner 999"),
            Some(Command::Spinner(300))
        );
        assert_eq!(
            parse_system_command("/spinner 301"),
            Some(Command::Spinner(300))
        );
    }
    // --- category command tests ---
    //
    // Table-driven: all InferenceCategory variants × three verbs (model, provider, key)
    // × two operations (show, set). Iterates InferenceCategory::ALL so new categories
    // are tested automatically.
    #[test]
    fn test_parse_category_all_show_and_set() {
        type ShowFn = fn(InferenceCategory) -> Command;
        type SetFn = fn(InferenceCategory, String) -> Command;

        let verbs: &[(&str, ShowFn, SetFn)] = &[
            (
                "model",
                Command::ShowCategoryModel as ShowFn,
                Command::SetCategoryModel as SetFn,
            ),
            (
                "provider",
                Command::ShowCategoryProvider as ShowFn,
                Command::SetCategoryProvider as SetFn,
            ),
            (
                "key",
                Command::ShowCategoryKey as ShowFn,
                Command::SetCategoryKey as SetFn,
            ),
        ];

        for cat in InferenceCategory::ALL {
            let slug = cat.name();
            for (verb, show_fn, set_fn) in verbs {
                // show (bare command)
                let show_input = format!("/{}.{}", verb, slug);
                assert_eq!(
                    parse_system_command(&show_input),
                    Some(show_fn(cat)),
                    "show failed for {}.{}",
                    verb,
                    slug
                );

                // set (command with argument)
                let set_input = format!("/{}.{} test-value", verb, slug);
                assert_eq!(
                    parse_system_command(&set_input),
                    Some(set_fn(cat, "test-value".to_string())),
                    "set failed for {}.{}",
                    verb,
                    slug
                );
            }
        }
    }
    // show (bare command)
    // set (command with argument)
    #[test]
    fn test_parse_category_invalid_category_returns_none() {
        // Invalid category name should not match
        assert_eq!(parse_system_command("/model.bogus"), None);
        assert_eq!(parse_system_command("/provider.bogus"), None);
        assert_eq!(parse_system_command("/key.bogus"), None);
    }
    // --- /load edge cases ---
    #[test]
    fn test_parse_load_empty_shows_picker() {
        assert_eq!(
            parse_system_command("/load"),
            Some(Command::Load(String::new()))
        );
        assert_eq!(
            parse_system_command("/load  "),
            Some(Command::Load(String::new()))
        );
    }
    // --- /cloud edge cases ---
    #[test]
    fn test_parse_cloud_provider_show_bare() {
        // "/cloud provider" without a name shows cloud info
        assert_eq!(
            parse_system_command("/cloud provider"),
            Some(Command::ShowCloud)
        );
    }
    #[test]
    fn test_parse_cloud_provider_empty_name() {
        // "/cloud provider  " with only whitespace shows cloud info
        assert_eq!(
            parse_system_command("/cloud provider  "),
            Some(Command::ShowCloud)
        );
    }
    #[test]
    fn test_parse_cloud_model_empty_name() {
        assert_eq!(
            parse_system_command("/cloud model  "),
            Some(Command::ShowCloudModel)
        );
    }
    #[test]
    fn test_parse_cloud_key_empty_name() {
        assert_eq!(
            parse_system_command("/cloud key  "),
            Some(Command::ShowCloudKey)
        );
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
    // --- /weather command tests ---
    #[test]
    fn test_parse_weather_bare() {
        assert_eq!(
            parse_system_command("/weather"),
            Some(Command::Weather(None))
        );
        assert_eq!(
            parse_system_command("/weather  "),
            Some(Command::Weather(None))
        );
    }
    #[test]
    fn test_parse_weather_set() {
        assert_eq!(
            parse_system_command("/weather clear"),
            Some(Command::Weather(Some("clear".to_string())))
        );
        assert_eq!(
            parse_system_command("/weather light rain"),
            Some(Command::Weather(Some("light rain".to_string())))
        );
    }
    #[test]
    fn test_parse_weather_case_insensitive() {
        assert_eq!(
            parse_system_command("/WEATHER"),
            Some(Command::Weather(None))
        );
        assert_eq!(
            parse_system_command("/WEATHER FOG"),
            Some(Command::Weather(Some("FOG".to_string())))
        );
    }
    // --- /flag command tests ---
    #[test]
    fn test_parse_flag_bare_shows_list() {
        assert_eq!(
            parse_system_command("/flag"),
            Some(Command::Flag(FlagSubcommand::List))
        );
        assert_eq!(
            parse_system_command("/flag  "),
            Some(Command::Flag(FlagSubcommand::List))
        );
    }
    #[test]
    fn test_parse_flag_list() {
        assert_eq!(
            parse_system_command("/flag list"),
            Some(Command::Flag(FlagSubcommand::List))
        );
        assert_eq!(
            parse_system_command("/flag LIST"),
            Some(Command::Flag(FlagSubcommand::List))
        );
    }
    #[test]
    fn test_parse_flag_enable() {
        assert_eq!(
            parse_system_command("/flag enable experimental"),
            Some(Command::Flag(FlagSubcommand::Enable(
                "experimental".to_string()
            )))
        );
        assert_eq!(
            parse_system_command("/flag disable experimental"),
            Some(Command::Flag(FlagSubcommand::Disable(
                "experimental".to_string()
            )))
        );
    }
    #[test]
    fn test_parse_flag_enable_bare_shows_list() {
        assert_eq!(
            parse_system_command("/flag enable"),
            Some(Command::Flag(FlagSubcommand::List))
        );
        assert_eq!(
            parse_system_command("/flag disable"),
            Some(Command::Flag(FlagSubcommand::List))
        );
    }
    #[test]
    fn test_parse_flag_invalid_subcommand() {
        assert_eq!(
            parse_system_command("/flag bogus"),
            Some(Command::InvalidFlagName(
                "Unknown flag sub-command 'bogus'. Use: /flag enable <name>, /flag disable <name>, /flag list".to_string()
            ))
        );
    }
    #[test]
    fn test_parse_flags_alias() {
        assert_eq!(parse_system_command("/flags"), Some(Command::Flags));
    }
    #[test]
    fn test_parse_flag_invalid_name() {
        assert_eq!(
            parse_system_command("/flag enable bad/name"),
            Some(Command::InvalidFlagName(
                "Flag names may only contain letters, digits, hyphens, and underscores."
                    .to_string()
            ))
        );
    }
    // --- /speed ludicrous ---
    #[test]
    fn test_parse_speed_ludicrous() {
        assert_eq!(
            parse_system_command("/speed ludicrous"),
            Some(Command::SetSpeed(GameSpeed::Ludicrous))
        );
    }

    #[test]
    fn test_fork_with_invalid_branch_name() {
        assert_eq!(
            parse_system_command("/fork ../../etc"),
            Some(Command::InvalidBranchName(
                "Branch names may only contain letters, numbers, spaces, underscores, and hyphens."
                    .to_string()
            ))
        );
    }

    #[test]
    fn test_load_with_invalid_branch_name() {
        assert_eq!(
            parse_system_command("/load ../../etc"),
            Some(Command::InvalidBranchName(
                "Branch names may only contain letters, numbers, spaces, underscores, and hyphens."
                    .to_string()
            ))
        );
    }
}
