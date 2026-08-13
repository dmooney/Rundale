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
        ("/url", inference::parse_baseurl_command),
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

    // Dot-notation per-category commands: /model.<cat>, /provider.<cat>,
    // /key.<cat>, /url.<cat>
    if keyword.starts_with("/model.")
        || keyword.starts_with("/provider.")
        || keyword.starts_with("/key.")
        || keyword.starts_with("/url.")
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
        "/pause-silent" => Some(Command::PauseSilent),
        "/resume-silent" => Some(Command::ResumeSilent),
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
        "/listen" => Some(Command::Listen),
        "/omen" => Some(Command::Omen),
        "/folklore" => Some(Command::Folklore),
        "/byok" | "/onboard" | "/setup" => Some(Command::ResetByok),
        _ => None,
    }
}

/// Deterministic pre-classifier for exact natural command forms.
///
/// Small quantised intent models (e.g. Qwen2.5-1.5B-4bit on :8001) occasionally
/// misclassify unambiguous command forms such as `wait` and `listen carefully` as
/// player dialogue, causing the text to appear as a player utterance and NPCs to
/// react to it (#1351). This function short-circuits those exact forms to their
/// canonical `SystemCommand` equivalents before the LLM intent parser is ever
/// consulted.
///
/// # Scope
///
/// Only **exact** matches are intercepted (case-insensitive, with surrounding
/// whitespace trimmed). Extra words (e.g. `wait 30` or `listen to Mary`) are
/// intentionally **not** intercepted so the LLM can still parse natural-language
/// actions and dialogue that happen to start with one of these words.
///
/// # Token list rationale
///
/// - `wait` — maps to `Command::Wait(15)` (same default as `/wait`). A bare
///   "wait" with no number is unambiguous: pause time for 15 minutes.
/// - `status` — maps to `Command::Status`. A single bare word has no plausible
///   conversational meaning distinct from the `/status` command.
/// - Four deliberately enumerated `listen` forms map to `Command::Listen`;
///   `listen for an omen` maps to `Command::Omen`. Keeping the list closed
///   prevents arbitrary `listen ...` sentences from bypassing intent
///   classification.
pub(crate) fn parse_natural_command_intercept(input: &str) -> Option<Command> {
    match input.trim().to_lowercase().as_str() {
        "wait" => Some(Command::Wait(15)),
        "status" => Some(Command::Status),
        "listen" | "listen carefully" | "listen to the place" | "listen to the land" => {
            Some(Command::Listen)
        }
        "listen for an omen" => Some(Command::Omen),
        _ => None,
    }
}

/// Classifies raw input as either a system command or game input.
///
/// Classification order:
/// 1. `/`-prefixed system commands via [`parse_system_command`].
/// 2. Exact un-prefixed command forms via [`parse_natural_command_intercept`]
///    — deterministic short-circuit that prevents the LLM intent parser from
///    seeing unambiguous command forms and nondeterministically classifying
///    them as player dialogue (#1351). This short-circuit is skipped when the
///    player explicitly selected an addressee: in that context, natural text
///    is dialogue and only an explicit slash command can override it (#1450).
/// 3. Everything else → [`InputResult::GameInput`] for LLM intent parsing.
pub fn classify_input(raw: &str) -> InputResult {
    classify_input_with_context(raw, false)
}

/// Returns whether a request contains at least one meaningful NPC addressee.
///
/// Keeping this semantic check in the input crate ensures classification and
/// dialogue presentation agree about whitespace-only chip payloads.
pub fn has_explicit_addressee(addressed_to: &[String]) -> bool {
    addressed_to.iter().any(|name| !name.trim().is_empty())
}

/// Classifies input using the NPC addressees carried by a runtime request.
///
/// An addressee is explicit only when at least one entry contains non-whitespace
/// text. The request payload itself is not normalized here: runtimes still pass
/// the original list to game-input handling after classification.
pub fn classify_input_with_addressees(raw: &str, addressed_to: &[String]) -> InputResult {
    classify_input_with_context(raw, has_explicit_addressee(addressed_to))
}

/// Classifies input while preserving explicitly addressed dialogue.
///
/// Slash commands are always commands. Natural command shorthands are commands
/// only when no NPC addressee is selected, so phrases such as `listen carefully`
/// can still be spoken to an explicitly addressed character.
pub fn classify_input_with_context(raw: &str, has_explicit_addressee: bool) -> InputResult {
    let trimmed = raw.trim();
    if let Some(cmd) = parse_system_command(trimmed) {
        return InputResult::SystemCommand(cmd);
    }
    if !has_explicit_addressee && let Some(cmd) = parse_natural_command_intercept(trimmed) {
        return InputResult::SystemCommand(cmd);
    }
    InputResult::GameInput(trimmed.to_string())
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
    /// AC5 — /pause-silent and /resume-silent parse to the silent variants.
    #[test]
    fn test_parse_silent_pause_resume() {
        assert_eq!(
            parse_system_command("/pause-silent"),
            Some(Command::PauseSilent)
        );
        assert_eq!(
            parse_system_command("/resume-silent"),
            Some(Command::ResumeSilent)
        );
        // Case-insensitive.
        assert_eq!(
            parse_system_command("/PAUSE-SILENT"),
            Some(Command::PauseSilent)
        );
        assert_eq!(
            parse_system_command("/RESUME-SILENT"),
            Some(Command::ResumeSilent)
        );
        // Trailing text must not match (zero-arg policy).
        assert_eq!(parse_system_command("/pause-silent now"), None);
        assert_eq!(parse_system_command("/resume-silent please"), None);
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
        assert_eq!(parse_system_command("/listen"), Some(Command::Listen));
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
        assert_eq!(parse_system_command("/listen carefully"), None);
        assert_eq!(parse_system_command("/omen now"), None);
        assert_eq!(parse_system_command("/folklore please"), None);
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

    #[test]
    fn distinct_place_attention_commands_parse_separately() {
        assert_eq!(parse_system_command("/listen"), Some(Command::Listen));
        assert_eq!(parse_system_command("/omen"), Some(Command::Omen));
        assert_eq!(parse_system_command("/folklore"), Some(Command::Folklore));
    }

    #[test]
    fn distinct_place_attention_commands_are_case_insensitive_and_trimmed() {
        assert_eq!(parse_system_command("/LISTEN"), Some(Command::Listen));
        assert_eq!(parse_system_command("  /OmEn  "), Some(Command::Omen));
        assert_eq!(
            parse_system_command("\t/FOLKLORE\n"),
            Some(Command::Folklore)
        );
    }

    #[test]
    fn distinct_place_attention_commands_reject_trailing_text() {
        assert_eq!(parse_system_command("/listen carefully"), None);
        assert_eq!(parse_system_command("/omen now"), None);
        assert_eq!(parse_system_command("/folklore please"), None);
    }

    // ── Bare-command intercept (#1351) ────────────────────────────────────────

    /// Bare `wait` must route to `Command::Wait(15)` without reaching the LLM.
    #[test]
    fn bare_wait_is_intercepted_as_system_command() {
        assert_eq!(
            classify_input("wait"),
            InputResult::SystemCommand(Command::Wait(15))
        );
        assert_eq!(
            classify_input("WAIT"),
            InputResult::SystemCommand(Command::Wait(15))
        );
        assert_eq!(
            classify_input("  wait  "),
            InputResult::SystemCommand(Command::Wait(15))
        );
    }

    /// Bare `status` must route to `Command::Status` without reaching the LLM.
    #[test]
    fn bare_status_is_intercepted_as_system_command() {
        assert_eq!(
            classify_input("status"),
            InputResult::SystemCommand(Command::Status)
        );
        assert_eq!(
            classify_input("STATUS"),
            InputResult::SystemCommand(Command::Status)
        );
        assert_eq!(
            classify_input("  status  "),
            InputResult::SystemCommand(Command::Status)
        );
    }

    /// `wait` with an argument (e.g. "wait 30" or "wait for Mary") is NOT
    /// intercepted — it goes to `GameInput` so natural-language phrasing is
    /// still handled by the intent parser.
    #[test]
    fn wait_with_argument_is_game_input() {
        assert_eq!(
            classify_input("wait 30"),
            InputResult::GameInput("wait 30".to_string())
        );
        assert_eq!(
            classify_input("wait for Mary"),
            InputResult::GameInput("wait for Mary".to_string())
        );
    }

    /// `status` with trailing text is not intercepted — it falls through to
    /// game input where the LLM can handle the natural-language variant.
    #[test]
    fn status_with_argument_is_game_input() {
        assert_eq!(
            classify_input("status report"),
            InputResult::GameInput("status report".to_string())
        );
    }

    #[test]
    fn exact_natural_listen_forms_are_intercepted() {
        let listen_forms = [
            "listen",
            "listen carefully",
            "listen to the place",
            "listen to the land",
        ];

        for form in listen_forms {
            assert_eq!(
                classify_input(form),
                InputResult::SystemCommand(Command::Listen),
                "natural listen form '{form}' was not intercepted"
            );
            assert_eq!(
                classify_input(&format!("  {}  ", form.to_uppercase())),
                InputResult::SystemCommand(Command::Listen),
                "natural listen form '{form}' was not case-insensitive and trimmed"
            );
        }

        assert_eq!(
            classify_input("listen for an omen"),
            InputResult::SystemCommand(Command::Omen)
        );
        assert_eq!(
            classify_input("  LISTEN FOR AN OMEN  "),
            InputResult::SystemCommand(Command::Omen)
        );
    }

    #[test]
    fn near_miss_listen_sentences_remain_game_input() {
        let near_misses = [
            "listen to Mary",
            "listen carefully to Mary",
            "listen to the place for birds",
            "listen to the landscape",
            "listen for an omen in the trees",
            "please listen",
            "listen!",
        ];

        for input in near_misses {
            assert_eq!(
                classify_input(input),
                InputResult::GameInput(input.to_string()),
                "near-miss sentence '{input}' must reach intent classification"
            );
        }
    }

    #[test]
    fn explicit_addressee_preserves_natural_listen_as_dialogue() {
        assert_eq!(
            classify_input_with_context("listen carefully", true),
            InputResult::GameInput("listen carefully".to_string())
        );
        assert_eq!(
            classify_input_with_context("listen to the land", true),
            InputResult::GameInput("listen to the land".to_string())
        );
        assert_eq!(
            classify_input_with_context("/listen", true),
            InputResult::SystemCommand(Command::Listen),
            "an explicit slash command must override addressee context"
        );
    }

    #[test]
    fn request_addressees_require_at_least_one_nonblank_name() {
        for addressed_to in [vec![], vec![String::new()], vec!["  \t\n ".to_string()]] {
            assert_eq!(
                classify_input_with_addressees("listen carefully", &addressed_to),
                InputResult::SystemCommand(Command::Listen),
                "blank addressees must not suppress natural command shortcuts"
            );
        }

        assert_eq!(
            classify_input_with_addressees(
                "listen carefully",
                &["  ".to_string(), " Siobhan Murphy ".to_string()]
            ),
            InputResult::GameInput("listen carefully".to_string())
        );
        assert_eq!(
            classify_input_with_addressees("/listen", &["Siobhan Murphy".to_string()]),
            InputResult::SystemCommand(Command::Listen),
            "slash commands remain commands with a real addressee"
        );
    }

    /// All forms in `parse_natural_command_intercept` must return `Some`.
    /// Regression guard for the deliberately closed deterministic match list.
    #[test]
    fn all_natural_intercept_forms_return_some() {
        let forms = [
            "wait",
            "status",
            "listen",
            "listen carefully",
            "listen to the place",
            "listen to the land",
            "listen for an omen",
        ];
        for form in forms {
            assert!(
                super::parse_natural_command_intercept(form).is_some(),
                "natural intercept form '{form}' returned None"
            );
            assert!(
                super::parse_natural_command_intercept(&form.to_uppercase()).is_some(),
                "natural intercept form '{form}' (uppercase) returned None"
            );
        }
    }
}
