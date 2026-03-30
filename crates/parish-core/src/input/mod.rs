//! Player input parsing and command detection.
//!
//! System commands use `/` prefix (e.g., `/quit`, `/save`).
//! All other input is natural language sent to the LLM for
//! intent parsing (move, talk, look, interact, examine).

use crate::config::InferenceCategory;
use crate::error::ParishError;
use crate::inference::openai_client::OpenAiClient;
use crate::world::time::GameSpeed;
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
    /// Toggle the Irish pronunciation sidebar.
    ToggleSidebar,
    /// Toggle improv craft mode for NPC dialogue.
    ToggleImprov,
    /// Show current LLM provider.
    ShowProvider,
    /// Change LLM provider at runtime.
    SetProvider(String),
    /// Show current model name.
    ShowModel,
    /// Change model name at runtime.
    SetModel(String),
    /// Show current API key (masked).
    ShowKey,
    /// Set API key at runtime.
    SetKey(String),
    /// Debug command with optional subcommand.
    Debug(Option<String>),
    /// Show the loading spinner for a duration (seconds).
    Spinner(u64),
    /// Show cloud provider info.
    ShowCloud,
    /// Change cloud provider at runtime.
    SetCloudProvider(String),
    /// Show cloud model name.
    ShowCloudModel,
    /// Change cloud model at runtime.
    SetCloudModel(String),
    /// Show cloud API key (masked).
    ShowCloudKey,
    /// Set cloud API key at runtime.
    SetCloudKey(String),
    /// Show current game speed.
    ShowSpeed,
    /// Set game speed to a named preset.
    SetSpeed(GameSpeed),
    /// Invalid speed preset was requested.
    InvalidSpeed(String),
    /// Show provider for a specific inference category.
    ShowCategoryProvider(InferenceCategory),
    /// Set provider for a specific inference category.
    SetCategoryProvider(InferenceCategory, String),
    /// Show model for a specific inference category.
    ShowCategoryModel(InferenceCategory),
    /// Set model for a specific inference category.
    SetCategoryModel(InferenceCategory, String),
    /// Show API key for a specific inference category (masked).
    ShowCategoryKey(InferenceCategory),
    /// Set API key for a specific inference category.
    SetCategoryKey(InferenceCategory, String),
    /// Show about / credits information.
    About,
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
    } else if lower == "/irish" {
        Some(Command::ToggleSidebar)
    } else if lower == "/improv" {
        Some(Command::ToggleImprov)
    } else if lower == "/about" {
        Some(Command::About)
    } else if let Some(cmd) = parse_category_command(trimmed, &lower) {
        Some(cmd)
    } else if lower == "/provider" {
        Some(Command::ShowProvider)
    } else if lower.starts_with("/provider ") {
        let name = trimmed[10..].trim().to_string();
        if name.is_empty() {
            Some(Command::ShowProvider)
        } else {
            Some(Command::SetProvider(name))
        }
    } else if lower == "/model" {
        Some(Command::ShowModel)
    } else if lower.starts_with("/model ") {
        let name = trimmed[7..].trim().to_string();
        if name.is_empty() {
            Some(Command::ShowModel)
        } else {
            Some(Command::SetModel(name))
        }
    } else if lower == "/key" {
        Some(Command::ShowKey)
    } else if lower.starts_with("/key ") {
        let value = trimmed[5..].trim().to_string();
        if value.is_empty() {
            Some(Command::ShowKey)
        } else {
            Some(Command::SetKey(value))
        }
    } else if lower == "/spinner" {
        Some(Command::Spinner(30))
    } else if lower.starts_with("/spinner ") {
        let secs = trimmed[9..].trim().parse::<u64>().unwrap_or(30);
        Some(Command::Spinner(secs))
    } else if lower == "/debug" {
        Some(Command::Debug(None))
    } else if lower.starts_with("/debug ") {
        let sub = trimmed[7..].trim().to_string();
        if sub.is_empty() {
            Some(Command::Debug(None))
        } else {
            Some(Command::Debug(Some(sub)))
        }
    } else if lower == "/speed" {
        Some(Command::ShowSpeed)
    } else if lower.starts_with("/speed ") {
        let arg = trimmed[7..].trim();
        match GameSpeed::from_name(arg) {
            Some(speed) => Some(Command::SetSpeed(speed)),
            None if arg.is_empty() => Some(Command::ShowSpeed),
            None => Some(Command::InvalidSpeed(arg.to_string())),
        }
    } else if lower == "/cloud" {
        Some(Command::ShowCloud)
    } else if lower.starts_with("/cloud ") {
        let rest = trimmed[7..].trim();
        let rest_lower = rest.to_lowercase();
        if rest_lower.starts_with("provider ") {
            let name = rest[9..].trim().to_string();
            if name.is_empty() {
                Some(Command::ShowCloud)
            } else {
                Some(Command::SetCloudProvider(name))
            }
        } else if rest_lower == "provider" {
            Some(Command::ShowCloud)
        } else if rest_lower.starts_with("model ") {
            let name = rest[6..].trim().to_string();
            if name.is_empty() {
                Some(Command::ShowCloudModel)
            } else {
                Some(Command::SetCloudModel(name))
            }
        } else if rest_lower == "model" {
            Some(Command::ShowCloudModel)
        } else if rest_lower.starts_with("key ") {
            let value = rest[4..].trim().to_string();
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

/// Attempts to parse intent locally using keyword matching.
///
/// Catches common movement and look phrases without requiring an LLM call.
/// Returns `None` if the input doesn't match any known pattern.
pub fn parse_intent_local(raw_input: &str) -> Option<PlayerIntent> {
    let lower = raw_input.trim().to_lowercase();

    // Movement patterns — multi-word phrases checked first (longest match wins),
    // then single-verb prefixes. Covers common, colloquial, and unusual verbs.
    let move_phrases = [
        "make my way to ",
        "make my way ",
        "head over to ",
        "head over ",
        "pop over to ",
        "pop over ",
        "nip to ",
        "swing by ",
        "go to ",
        "walk to ",
        "head to ",
        "move to ",
        "travel to ",
        "run to ",
        "jog to ",
        "dash to ",
        "hurry to ",
        "rush to ",
        "stroll to ",
        "saunter to ",
        "mosey to ",
        "wander to ",
        "amble to ",
        "trek to ",
        "hike to ",
        "proceed to ",
        "sprint to ",
        "march to ",
        "traipse to ",
        "meander to ",
        "trot to ",
        "stride to ",
        "creep to ",
        "sneak to ",
        "bolt to ",
        "scramble to ",
    ];

    // Single-verb prefixes (without "to") — "saunter pub", "go pub", etc.
    let move_verbs = [
        "go ",
        "walk ",
        "head ",
        "visit ",
        "run ",
        "jog ",
        "dash ",
        "hurry ",
        "rush ",
        "stroll ",
        "saunter ",
        "mosey ",
        "wander ",
        "amble ",
        "trek ",
        "hike ",
        "proceed ",
        "sprint ",
        "march ",
        "traipse ",
        "meander ",
        "trot ",
        "stride ",
        "creep ",
        "sneak ",
        "bolt ",
        "scramble ",
    ];

    // Try multi-word phrases first for longest-match semantics
    for prefix in &move_phrases {
        if let Some(target) = lower.strip_prefix(prefix) {
            let target = target.trim();
            if !target.is_empty() {
                return Some(PlayerIntent {
                    intent: IntentKind::Move,
                    target: Some(target.to_string()),
                    dialogue: None,
                    raw: raw_input.to_string(),
                });
            }
        }
    }

    // Then try bare verb + destination
    for prefix in &move_verbs {
        if let Some(target) = lower.strip_prefix(prefix) {
            let target = target.trim();
            if !target.is_empty() {
                return Some(PlayerIntent {
                    intent: IntentKind::Move,
                    target: Some(target.to_string()),
                    dialogue: None,
                    raw: raw_input.to_string(),
                });
            }
        }
    }

    // Look patterns
    let look_phrases = ["look", "look around", "l", "examine room", "where am i"];
    if look_phrases.contains(&lower.as_str()) {
        return Some(PlayerIntent {
            intent: IntentKind::Look,
            target: None,
            dialogue: None,
            raw: raw_input.to_string(),
        });
    }

    None
}

/// The result of extracting an `@mention` from player input.
///
/// Contains the mentioned name and the remaining input text with the
/// `@mention` stripped out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionExtraction {
    /// The name that was mentioned (without the `@` prefix).
    pub name: String,
    /// The remaining input text after stripping the mention.
    pub remaining: String,
}

/// Extracts an `@mention` from the beginning of player input.
///
/// Recognises `@Name` at the start of input. The name runs from the `@`
/// until the next punctuation, double-space, or end of string — so both
/// single-word names (`@Padraig`) and multi-word names (`@Padraig Darcy`)
/// are supported.
///
/// Returns `None` if the input does not start with `@`.
///
/// # Examples
///
/// ```
/// use parish_core::input::extract_mention;
///
/// let result = extract_mention("@Padraig hello there");
/// assert_eq!(result.unwrap().name, "Padraig");
///
/// let result = extract_mention("hello @Padraig");
/// assert!(result.is_none()); // only matches at start
/// ```
pub fn extract_mention(raw: &str) -> Option<MentionExtraction> {
    let trimmed = raw.trim();

    // Find `@` anywhere in the input (at start, or preceded by a space)
    let at_pos = trimmed.find('@')?;
    if at_pos > 0 && !trimmed.as_bytes()[at_pos - 1].is_ascii_whitespace() {
        return None;
    }

    let rest = &trimmed[at_pos + 1..];
    if rest.is_empty() || rest.starts_with(' ') {
        return None;
    }

    // Name runs until we hit a delimiter that signals end of the name portion.
    // Find where the name ends. Name = sequence of words where each word
    // starts with an uppercase letter or is a short connector (e.g., "O'Brien").
    // Once we hit a word starting with lowercase (and it's not a name particle),
    // that's the start of the remaining text.
    let words: Vec<&str> = rest.splitn(20, ' ').collect();
    let mut name_end = 0;

    for (i, word) in words.iter().enumerate() {
        let first_char = word.chars().next().unwrap_or(' ');
        if i == 0 {
            // First word is always part of the name
            name_end = 1;
            continue;
        }
        // If word starts with uppercase, it's likely part of the name
        if first_char.is_uppercase() {
            name_end = i + 1;
        } else {
            break;
        }
    }

    let name = words[..name_end].join(" ");
    // Remaining = text before the @mention + text after the name
    let before = trimmed[..at_pos].trim();
    let after = words[name_end..].join(" ");
    let remaining = match (before.is_empty(), after.trim().is_empty()) {
        (true, true) => String::new(),
        (true, false) => after.trim().to_string(),
        (false, true) => before.to_string(),
        (false, false) => format!("{} {}", before, after.trim()),
    };

    if name.is_empty() {
        return None;
    }

    Some(MentionExtraction { name, remaining })
}

/// Parses natural language input into a structured `PlayerIntent`.
///
/// First tries local keyword matching for common commands (movement, look).
/// Falls back to LLM for ambiguous input. If the LLM call fails,
/// returns `IntentKind::Unknown`.
pub async fn parse_intent(
    client: &OpenAiClient,
    raw_input: &str,
    model: &str,
) -> Result<PlayerIntent, ParishError> {
    // Try local parsing first — no LLM needed for obvious commands
    if let Some(intent) = parse_intent_local(raw_input) {
        return Ok(intent);
    }

    let result = client
        .generate_json::<IntentResponse>(model, raw_input, Some(INTENT_SYSTEM_PROMPT), None)
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

    #[test]
    fn test_local_parse_go_to() {
        let intent = parse_intent_local("go to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));
    }

    #[test]
    fn test_local_parse_walk_to() {
        let intent = parse_intent_local("walk to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));
    }

    #[test]
    fn test_local_parse_go_shorthand() {
        let intent = parse_intent_local("go pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }

    #[test]
    fn test_local_parse_head_to() {
        let intent = parse_intent_local("head to Murphy's Farm").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("murphy's farm".to_string()));
    }

    #[test]
    fn test_local_parse_visit() {
        let intent = parse_intent_local("visit the fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the fairy fort".to_string()));
    }

    #[test]
    fn test_local_parse_look() {
        let intent = parse_intent_local("look").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);

        let intent = parse_intent_local("look around").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);

        let intent = parse_intent_local("l").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);
    }

    #[test]
    fn test_local_parse_case_insensitive() {
        let intent = parse_intent_local("GO TO THE PUB").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("LOOK").unwrap();
        assert_eq!(intent.intent, IntentKind::Look);
    }

    #[test]
    fn test_local_parse_no_match() {
        assert!(parse_intent_local("tell Mary hello").is_none());
        assert!(parse_intent_local("pick up the stone").is_none());
        assert!(parse_intent_local("hello there").is_none());
    }

    #[test]
    fn test_local_parse_empty_target() {
        // "go to " with nothing after should match "go " prefix with target "to",
        // which is fine — the world graph won't find "to" and will say not found.
        // But bare "go" or "walk" with no target should not match.
        assert!(parse_intent_local("go").is_none());
        assert!(parse_intent_local("walk").is_none());
    }

    #[test]
    fn test_local_parse_saunter() {
        let intent = parse_intent_local("saunter to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("saunter pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }

    #[test]
    fn test_local_parse_mosey() {
        let intent = parse_intent_local("mosey to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("mosey church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("church".to_string()));
    }

    #[test]
    fn test_local_parse_wander() {
        let intent = parse_intent_local("wander to the crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the crossroads".to_string()));

        let intent = parse_intent_local("wander crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("crossroads".to_string()));
    }

    #[test]
    fn test_local_parse_stroll() {
        let intent = parse_intent_local("stroll to the fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the fairy fort".to_string()));

        let intent = parse_intent_local("stroll fairy fort").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("fairy fort".to_string()));
    }

    #[test]
    fn test_local_parse_amble() {
        let intent = parse_intent_local("amble to the village green").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the village green".to_string()));

        let intent = parse_intent_local("amble village green").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("village green".to_string()));
    }

    #[test]
    fn test_local_parse_trek_and_hike() {
        let intent = parse_intent_local("trek to the bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the bog".to_string()));

        let intent = parse_intent_local("hike to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));

        let intent = parse_intent_local("trek bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("bog".to_string()));
    }

    #[test]
    fn test_local_parse_run_jog_dash() {
        let intent = parse_intent_local("run to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("jog to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("dash to the crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the crossroads".to_string()));

        // Without "to"
        let intent = parse_intent_local("run pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }

    #[test]
    fn test_local_parse_hurry_rush() {
        let intent = parse_intent_local("hurry to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("rush to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("hurry pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));
    }

    #[test]
    fn test_local_parse_proceed() {
        let intent = parse_intent_local("proceed to the town square").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the town square".to_string()));

        let intent = parse_intent_local("proceed town square").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("town square".to_string()));
    }

    #[test]
    fn test_local_parse_multi_word_phrases() {
        let intent = parse_intent_local("make my way to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("make my way pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("pub".to_string()));

        let intent = parse_intent_local("head over to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("head over church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("church".to_string()));

        let intent = parse_intent_local("pop over to the shop").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the shop".to_string()));

        let intent = parse_intent_local("pop over shop").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("shop".to_string()));

        let intent = parse_intent_local("nip to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("swing by the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));
    }

    #[test]
    fn test_local_parse_sprint_march_traipse() {
        let intent = parse_intent_local("sprint to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("march to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("traipse to the bog").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the bog".to_string()));
    }

    #[test]
    fn test_local_parse_meander_trot_stride() {
        let intent = parse_intent_local("meander to the river").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the river".to_string()));

        let intent = parse_intent_local("trot to the farm").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the farm".to_string()));

        let intent = parse_intent_local("stride to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));
    }

    #[test]
    fn test_local_parse_creep_sneak_bolt_scramble() {
        let intent = parse_intent_local("creep to the graveyard").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the graveyard".to_string()));

        let intent = parse_intent_local("sneak to the pub").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("bolt to the church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("scramble to the hill").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the hill".to_string()));
    }

    #[test]
    fn test_local_parse_unusual_verbs_case_insensitive() {
        let intent = parse_intent_local("SAUNTER TO THE PUB").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the pub".to_string()));

        let intent = parse_intent_local("Mosey To The Church").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("the church".to_string()));

        let intent = parse_intent_local("WANDER crossroads").unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target, Some("crossroads".to_string()));
    }

    #[test]
    fn test_local_parse_bare_unusual_verbs_no_target() {
        // Bare verbs without a target should not match
        assert!(parse_intent_local("saunter").is_none());
        assert!(parse_intent_local("mosey").is_none());
        assert!(parse_intent_local("wander").is_none());
        assert!(parse_intent_local("stroll").is_none());
        assert!(parse_intent_local("amble").is_none());
        assert!(parse_intent_local("run").is_none());
        assert!(parse_intent_local("dash").is_none());
    }

    #[test]
    fn test_parse_irish_command() {
        let cmd = parse_system_command("/irish");
        assert_eq!(cmd, Some(Command::ToggleSidebar));
    }

    #[test]
    fn test_parse_irish_command_case_insensitive() {
        let cmd = parse_system_command("/IRISH");
        assert_eq!(cmd, Some(Command::ToggleSidebar));
    }

    #[test]
    fn test_classify_irish_command() {
        let result = classify_input("/irish");
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

    // --- extract_mention tests ---

    #[test]
    fn test_extract_mention_simple_name() {
        let result = extract_mention("@Padraig hello there").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello there");
    }

    #[test]
    fn test_extract_mention_full_name() {
        let result = extract_mention("@Padraig Darcy hello").unwrap();
        assert_eq!(result.name, "Padraig Darcy");
        assert_eq!(result.remaining, "hello");
    }

    #[test]
    fn test_extract_mention_name_only() {
        let result = extract_mention("@Padraig").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "");
    }

    #[test]
    fn test_extract_mention_no_at() {
        assert!(extract_mention("hello there").is_none());
    }

    #[test]
    fn test_extract_mention_at_mid_input() {
        let result = extract_mention("hello @Padraig").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello");
    }

    #[test]
    fn test_extract_mention_at_not_after_space() {
        assert!(extract_mention("email@Padraig").is_none());
    }

    #[test]
    fn test_extract_mention_bare_at() {
        assert!(extract_mention("@").is_none());
    }

    #[test]
    fn test_extract_mention_at_space() {
        assert!(extract_mention("@ hello").is_none());
    }

    #[test]
    fn test_extract_mention_with_sentence() {
        let result = extract_mention("@Siobhan how are you today?").unwrap();
        assert_eq!(result.name, "Siobhan");
        assert_eq!(result.remaining, "how are you today?");
    }

    #[test]
    fn test_extract_mention_whitespace_trimmed() {
        let result = extract_mention("  @Padraig  hello  ").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello");
    }

    #[test]
    fn test_extract_mention_mid_with_rest() {
        let result = extract_mention("hello @Padraig how are you").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello how are you");
    }
}
