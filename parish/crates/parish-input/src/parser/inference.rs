//! Parsers for inference/configuration commands:
//! `/preset`, `/provider`, `/model`, `/key`, `/cloud`, `/inference-log`,
//! and dot-notation per-category variants (`/model.<cat>`, etc.).

use parish_config::InferenceCategory;

use crate::commands::{Command, InferenceLogSub};

pub(super) fn parse_inference_log_command(_trimmed: &str, rest: &str) -> Option<Command> {
    let sub = match rest.to_lowercase().as_str() {
        "on" | "enable" | "start" => InferenceLogSub::On,
        "off" | "disable" | "stop" => InferenceLogSub::Off,
        "path" | "where" => InferenceLogSub::Path,
        // bare command and "status" both report status
        _ => InferenceLogSub::Status,
    };
    Some(Command::InferenceLog(sub))
}

pub(super) fn parse_preset_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowPreset)
    } else {
        Some(Command::ApplyPreset(rest.to_string()))
    }
}

pub(super) fn parse_provider_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowProvider)
    } else {
        Some(Command::SetProvider(rest.to_string()))
    }
}

pub(super) fn parse_model_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowModel)
    } else {
        Some(Command::SetModel(rest.to_string()))
    }
}

pub(super) fn parse_key_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowKey)
    } else {
        Some(Command::SetKey(rest.to_string()))
    }
}

pub(super) fn parse_cloud_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowCloud)
    } else {
        parse_cloud_subcommand(rest)
    }
}

/// Parses `/cloud <subcommand>` arguments.
pub(super) fn parse_cloud_subcommand(rest: &str) -> Option<Command> {
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

/// Parses dot-notation per-category commands like `/model.dialogue`, `/provider.intent`.
///
/// Returns `Some(Command)` if the input matches a `/<base>.<category>` pattern
/// where base is `model`, `provider`, or `key`, and category is `dialogue`,
/// `simulation`, or `intent`.
pub(super) fn parse_category_command(trimmed: &str, lower: &str) -> Option<Command> {
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

#[cfg(test)]
mod tests {
    use crate::commands::{Command, InferenceLogSub};
    use parish_config::InferenceCategory;

    #[test]
    fn test_parse_provider_show() {
        assert_eq!(
            crate::parser::parse_system_command("/provider"),
            Some(Command::ShowProvider)
        );
        assert_eq!(
            crate::parser::parse_system_command("/provider   "),
            Some(Command::ShowProvider)
        );
    }
    #[test]
    fn test_parse_provider_set() {
        assert_eq!(
            crate::parser::parse_system_command("/provider openrouter"),
            Some(Command::SetProvider("openrouter".to_string()))
        );
        assert_eq!(
            crate::parser::parse_system_command("/provider  ollama "),
            Some(Command::SetProvider("ollama".to_string()))
        );
    }
    #[test]
    fn test_parse_model_show() {
        assert_eq!(
            crate::parser::parse_system_command("/model"),
            Some(Command::ShowModel)
        );
    }
    #[test]
    fn test_parse_model_set() {
        assert_eq!(
            crate::parser::parse_system_command("/model google/gemma-3-1b-it:free"),
            Some(Command::SetModel("google/gemma-3-1b-it:free".to_string()))
        );
    }
    #[test]
    fn test_parse_key_show() {
        assert_eq!(
            crate::parser::parse_system_command("/key"),
            Some(Command::ShowKey)
        );
    }
    #[test]
    fn test_parse_key_set() {
        assert_eq!(
            crate::parser::parse_system_command("/key sk-or-v1-abc123"),
            Some(Command::SetKey("sk-or-v1-abc123".to_string()))
        );
    }
    #[test]
    fn test_parse_preset_show_bare() {
        assert_eq!(
            crate::parser::parse_system_command("/preset"),
            Some(Command::ShowPreset)
        );
        assert_eq!(
            crate::parser::parse_system_command("/preset   "),
            Some(Command::ShowPreset)
        );
    }
    #[test]
    fn test_parse_preset_apply() {
        assert_eq!(
            crate::parser::parse_system_command("/preset anthropic"),
            Some(Command::ApplyPreset("anthropic".to_string()))
        );
        assert_eq!(
            crate::parser::parse_system_command("/preset  ollama "),
            Some(Command::ApplyPreset("ollama".to_string()))
        );
    }
    #[test]
    fn test_parse_preset_case_insensitive() {
        // The /preset prefix is matched case-insensitively, but the argument
        // is preserved verbatim — Provider::from_str_loose handles casing.
        assert_eq!(
            crate::parser::parse_system_command("/PRESET Anthropic"),
            Some(Command::ApplyPreset("Anthropic".to_string()))
        );
    }
    #[test]
    fn test_parse_provider_case_insensitive() {
        assert_eq!(
            crate::parser::parse_system_command("/PROVIDER"),
            Some(Command::ShowProvider)
        );
        assert_eq!(
            crate::parser::parse_system_command("/Provider OpenRouter"),
            Some(Command::SetProvider("OpenRouter".to_string()))
        );
    }
    #[test]
    fn test_parse_cloud_show() {
        assert_eq!(
            crate::parser::parse_system_command("/cloud"),
            Some(Command::ShowCloud)
        );
    }
    #[test]
    fn test_parse_cloud_provider_set() {
        assert_eq!(
            crate::parser::parse_system_command("/cloud provider openrouter"),
            Some(Command::SetCloudProvider("openrouter".to_string()))
        );
    }
    #[test]
    fn test_parse_cloud_model_show() {
        assert_eq!(
            crate::parser::parse_system_command("/cloud model"),
            Some(Command::ShowCloudModel)
        );
    }
    #[test]
    fn test_parse_cloud_model_set() {
        assert_eq!(
            crate::parser::parse_system_command("/cloud model anthropic/claude-sonnet-4-20250514"),
            Some(Command::SetCloudModel(
                "anthropic/claude-sonnet-4-20250514".to_string()
            ))
        );
    }
    #[test]
    fn test_parse_cloud_key_show() {
        assert_eq!(
            crate::parser::parse_system_command("/cloud key"),
            Some(Command::ShowCloudKey)
        );
    }
    #[test]
    fn test_parse_cloud_key_set() {
        assert_eq!(
            crate::parser::parse_system_command("/cloud key sk-test-key"),
            Some(Command::SetCloudKey("sk-test-key".to_string()))
        );
    }
    #[test]
    fn test_parse_cloud_unknown_subcommand() {
        // Unknown subcommands show cloud status
        assert_eq!(
            crate::parser::parse_system_command("/cloud foobar"),
            Some(Command::ShowCloud)
        );
    }
    // --- /cloud edge cases ---
    #[test]
    fn test_parse_cloud_provider_show_bare() {
        // "/cloud provider" without a name shows cloud info
        assert_eq!(
            crate::parser::parse_system_command("/cloud provider"),
            Some(Command::ShowCloud)
        );
    }
    #[test]
    fn test_parse_cloud_provider_empty_name() {
        // "/cloud provider  " with only whitespace shows cloud info
        assert_eq!(
            crate::parser::parse_system_command("/cloud provider  "),
            Some(Command::ShowCloud)
        );
    }
    #[test]
    fn test_parse_cloud_model_empty_name() {
        assert_eq!(
            crate::parser::parse_system_command("/cloud model  "),
            Some(Command::ShowCloudModel)
        );
    }
    #[test]
    fn test_parse_cloud_key_empty_name() {
        assert_eq!(
            crate::parser::parse_system_command("/cloud key  "),
            Some(Command::ShowCloudKey)
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
                    crate::parser::parse_system_command(&show_input),
                    Some(show_fn(cat)),
                    "show failed for {}.{}",
                    verb,
                    slug
                );

                // set (command with argument)
                let set_input = format!("/{}.{} test-value", verb, slug);
                assert_eq!(
                    crate::parser::parse_system_command(&set_input),
                    Some(set_fn(cat, "test-value".to_string())),
                    "set failed for {}.{}",
                    verb,
                    slug
                );
            }
        }
    }
    #[test]
    fn test_parse_category_invalid_category_returns_none() {
        // Invalid category name should not match
        assert_eq!(crate::parser::parse_system_command("/model.bogus"), None);
        assert_eq!(crate::parser::parse_system_command("/provider.bogus"), None);
        assert_eq!(crate::parser::parse_system_command("/key.bogus"), None);
    }
    // --- /inference-log command tests ---
    #[test]
    fn test_parse_inference_log_on() {
        assert_eq!(
            crate::parser::parse_system_command("/inference-log on"),
            Some(Command::InferenceLog(InferenceLogSub::On))
        );
        assert_eq!(
            crate::parser::parse_system_command("/inference-log enable"),
            Some(Command::InferenceLog(InferenceLogSub::On))
        );
        assert_eq!(
            crate::parser::parse_system_command("/inference-log start"),
            Some(Command::InferenceLog(InferenceLogSub::On))
        );
    }
    #[test]
    fn test_parse_inference_log_off() {
        assert_eq!(
            crate::parser::parse_system_command("/inference-log off"),
            Some(Command::InferenceLog(InferenceLogSub::Off))
        );
        assert_eq!(
            crate::parser::parse_system_command("/inference-log disable"),
            Some(Command::InferenceLog(InferenceLogSub::Off))
        );
        assert_eq!(
            crate::parser::parse_system_command("/inference-log stop"),
            Some(Command::InferenceLog(InferenceLogSub::Off))
        );
    }
    #[test]
    fn test_parse_inference_log_path() {
        assert_eq!(
            crate::parser::parse_system_command("/inference-log path"),
            Some(Command::InferenceLog(InferenceLogSub::Path))
        );
        assert_eq!(
            crate::parser::parse_system_command("/inference-log where"),
            Some(Command::InferenceLog(InferenceLogSub::Path))
        );
    }
    #[test]
    fn test_parse_inference_log_status() {
        assert_eq!(
            crate::parser::parse_system_command("/inference-log status"),
            Some(Command::InferenceLog(InferenceLogSub::Status))
        );
        assert_eq!(
            crate::parser::parse_system_command("/inference-log"),
            Some(Command::InferenceLog(InferenceLogSub::Status))
        );
    }
}
