//! System command parsing and input classification.
//!
//! Translates raw `/`-prefixed input strings into [`Command`] values
//! and routes anything else to free-form game input.

use parish_config::InferenceCategory;
use parish_types::GameSpeed;

use crate::commands::{Command, FlagSubcommand, validate_branch_name, validate_flag_name};
use crate::intent_types::InputResult;

const SPINNER_DEFAULT_SECS: u64 = 30;
const SPINNER_MAX_SECS: u64 = 300;

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
    } else if lower == "/fork" || lower.starts_with("/fork ") {
        let name = trimmed
            .get("/fork".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            Some(Command::Help) // bare /fork → show help
        } else {
            match validate_branch_name(&name) {
                Ok(valid) => Some(Command::Fork(valid)),
                Err(msg) => Some(Command::InvalidBranchName(msg)),
            }
        }
    } else if lower == "/load" || lower.starts_with("/load ") {
        let name = trimmed
            .get("/load".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            Some(Command::Load(String::new())) // empty string = show save picker
        } else {
            match validate_branch_name(&name) {
                Ok(valid) => Some(Command::Load(valid)),
                Err(msg) => Some(Command::InvalidBranchName(msg)),
            }
        }
    } else if lower == "/branches" {
        Some(Command::Branches)
    } else if lower == "/log" {
        Some(Command::Log)
    } else if lower == "/status" {
        Some(Command::Status)
    } else if lower == "/help" {
        Some(Command::Help)
    } else if lower == "/irish" {
        Some(Command::ToggleSidebar)
    } else if lower == "/improv" {
        Some(Command::ToggleImprov)
    } else if lower == "/about" {
        Some(Command::About)
    } else if lower == "/map" {
        Some(Command::Map(None))
    } else if lower.starts_with("/map ") {
        let arg = trimmed
            .get("/map ".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if arg.is_empty() {
            Some(Command::Map(None))
        } else {
            Some(Command::Map(Some(arg)))
        }
    } else if lower == "/designer" {
        Some(Command::Designer)
    } else if lower == "/npcs" {
        Some(Command::NpcsHere)
    } else if lower == "/standing" || lower == "/familiarity" {
        Some(Command::Standing)
    } else if lower == "/time" {
        Some(Command::Time)
    } else if lower == "/where" {
        Some(Command::Status)
    } else if lower == "/wait" {
        Some(Command::Wait(15))
    } else if lower.starts_with("/wait ") {
        let mins = trimmed[6..].trim().parse::<u32>().unwrap_or(15);
        Some(Command::Wait(mins))
    } else if lower == "/new" {
        Some(Command::NewGame)
    } else if lower == "/tick" {
        Some(Command::Tick)
    } else if lower == "/theme" {
        Some(Command::Theme(None))
    } else if lower.starts_with("/theme ") {
        let arg = trimmed
            .get("/theme ".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if arg.is_empty() {
            Some(Command::Theme(None))
        } else {
            Some(Command::Theme(Some(arg)))
        }
    } else if lower == "/unexplored" {
        Some(Command::Unexplored(None))
    } else if lower.starts_with("/unexplored ") {
        let arg = trimmed
            .get("/unexplored ".len()..)
            .unwrap_or("")
            .trim()
            .to_lowercase();
        match arg.as_str() {
            "reveal" | "show" | "on" => Some(Command::Unexplored(Some(true))),
            "hide" | "off" => Some(Command::Unexplored(Some(false))),
            _ => Some(Command::Unexplored(None)),
        }
    } else if let Some(cmd) = parse_category_command(trimmed, &lower) {
        Some(cmd)
    } else if lower == "/preset" {
        Some(Command::ShowPreset)
    } else if lower.starts_with("/preset ") {
        let name = trimmed
            .get("/preset ".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            Some(Command::ShowPreset)
        } else {
            Some(Command::ApplyPreset(name))
        }
    } else if lower == "/provider" {
        Some(Command::ShowProvider)
    } else if lower.starts_with("/provider ") {
        let name = trimmed
            .get("/provider ".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            Some(Command::ShowProvider)
        } else {
            Some(Command::SetProvider(name))
        }
    } else if lower == "/model" {
        Some(Command::ShowModel)
    } else if lower.starts_with("/model ") {
        let name = trimmed
            .get("/model ".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            Some(Command::ShowModel)
        } else {
            Some(Command::SetModel(name))
        }
    } else if lower == "/key" {
        Some(Command::ShowKey)
    } else if lower.starts_with("/key ") {
        let value = trimmed
            .get("/key ".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if value.is_empty() {
            Some(Command::ShowKey)
        } else {
            Some(Command::SetKey(value))
        }
    } else if lower == "/spinner" {
        Some(Command::Spinner(SPINNER_DEFAULT_SECS))
    } else if lower.starts_with("/spinner ") {
        let secs = trimmed
            .get("/spinner ".len()..)
            .unwrap_or("")
            .trim()
            .parse::<u64>()
            .unwrap_or(SPINNER_DEFAULT_SECS)
            .min(SPINNER_MAX_SECS);
        Some(Command::Spinner(secs))
    } else if lower == "/debug" {
        Some(Command::Debug(None))
    } else if lower.starts_with("/debug ") {
        let sub = trimmed
            .get("/debug ".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if sub.is_empty() {
            Some(Command::Debug(None))
        } else {
            Some(Command::Debug(Some(sub)))
        }
    } else if lower == "/speed" {
        Some(Command::ShowSpeed)
    } else if lower.starts_with("/speed ") {
        let arg = trimmed.get("/speed ".len()..).unwrap_or("").trim();
        match GameSpeed::from_name(arg) {
            Some(speed) => Some(Command::SetSpeed(speed)),
            None if arg.is_empty() => Some(Command::ShowSpeed),
            None => Some(Command::InvalidSpeed(arg.to_string())),
        }
    } else if lower == "/cloud" {
        Some(Command::ShowCloud)
    } else if lower.starts_with("/cloud ") {
        let rest = trimmed.get("/cloud ".len()..).unwrap_or("").trim();
        let rest_lower = rest.to_lowercase();
        if rest_lower.starts_with("provider ") {
            let name = rest
                .get("provider ".len()..)
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                Some(Command::ShowCloud)
            } else {
                Some(Command::SetCloudProvider(name))
            }
        } else if rest_lower == "provider" {
            Some(Command::ShowCloud)
        } else if rest_lower.starts_with("model ") {
            let name = rest.get("model ".len()..).unwrap_or("").trim().to_string();
            if name.is_empty() {
                Some(Command::ShowCloudModel)
            } else {
                Some(Command::SetCloudModel(name))
            }
        } else if rest_lower == "model" {
            Some(Command::ShowCloudModel)
        } else if rest_lower.starts_with("key ") {
            let value = rest.get("key ".len()..).unwrap_or("").trim().to_string();
            if value.is_empty() {
                Some(Command::ShowCloudKey)
            } else {
                Some(Command::SetCloudKey(value))
            }
        } else if rest_lower == "key" {
            Some(Command::ShowCloudKey)
        } else {
            Some(Command::ShowCloud)
        }
    } else if lower == "/flags" {
        Some(Command::Flags)
    } else if lower == "/weather" {
        Some(Command::Weather(None))
    } else if lower.starts_with("/weather ") {
        let arg = trimmed
            .get("/weather ".len()..)
            .unwrap_or("")
            .trim()
            .to_string();
        if arg.is_empty() {
            Some(Command::Weather(None))
        } else {
            Some(Command::Weather(Some(arg)))
        }
    } else if matches!(
        lower.as_str(),
        "/session" | "/tune" | "/music" | "/fiddle" | "/seisiun"
    ) {
        Some(Command::Session)
    } else if lower == "/flag" || lower == "/flag list" {
        Some(Command::Flag(FlagSubcommand::List))
    } else if lower.starts_with("/flag ") {
        let rest = trimmed.get("/flag ".len()..).unwrap_or("").trim();
        let rest_lower = rest.to_lowercase();
        if rest_lower.starts_with("enable ") {
            let name = rest.get("enable ".len()..).unwrap_or("").trim();
            match validate_flag_name(name) {
                Ok(valid) => Some(Command::Flag(FlagSubcommand::Enable(valid))),
                Err(msg) => Some(Command::InvalidFlagName(msg)),
            }
        } else if rest_lower.starts_with("disable ") {
            let name = rest.get("disable ".len()..).unwrap_or("").trim();
            match validate_flag_name(name) {
                Ok(valid) => Some(Command::Flag(FlagSubcommand::Disable(valid))),
                Err(msg) => Some(Command::InvalidFlagName(msg)),
            }
        } else if rest_lower == "enable" || rest_lower == "disable" || rest_lower == "list" {
            Some(Command::Flag(FlagSubcommand::List))
        } else {
            // `/flag <name>` without enable/disable — treat as usage error
            Some(Command::InvalidFlagName(format!(
                "Unknown flag sub-command '{}'. Use: /flag enable <name>, /flag disable <name>, /flag list",
                rest
            )))
        }
    } else {
        None
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
