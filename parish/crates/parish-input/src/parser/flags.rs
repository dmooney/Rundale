//! Parsers for feature-flag commands: `/flag`.

use crate::commands::{Command, FlagSubcommand, validate_flag_name};

pub(super) fn parse_flag_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() || rest.to_lowercase() == "list" {
        Some(Command::Flag(FlagSubcommand::List))
    } else {
        parse_flag_subcommand(rest)
    }
}

/// Parses `/flag <subcommand>` arguments (enable/disable/list).
pub(super) fn parse_flag_subcommand(rest: &str) -> Option<Command> {
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

#[cfg(test)]
mod tests {
    use crate::commands::{Command, FlagSubcommand};

    // --- /flag command tests ---
    #[test]
    fn test_parse_flag_bare_shows_list() {
        assert_eq!(
            crate::parser::parse_system_command("/flag"),
            Some(Command::Flag(FlagSubcommand::List))
        );
        assert_eq!(
            crate::parser::parse_system_command("/flag  "),
            Some(Command::Flag(FlagSubcommand::List))
        );
    }
    #[test]
    fn test_parse_flag_list() {
        assert_eq!(
            crate::parser::parse_system_command("/flag list"),
            Some(Command::Flag(FlagSubcommand::List))
        );
        assert_eq!(
            crate::parser::parse_system_command("/flag LIST"),
            Some(Command::Flag(FlagSubcommand::List))
        );
    }
    #[test]
    fn test_parse_flag_enable() {
        assert_eq!(
            crate::parser::parse_system_command("/flag enable experimental"),
            Some(Command::Flag(FlagSubcommand::Enable(
                "experimental".to_string()
            )))
        );
        assert_eq!(
            crate::parser::parse_system_command("/flag disable experimental"),
            Some(Command::Flag(FlagSubcommand::Disable(
                "experimental".to_string()
            )))
        );
    }
    #[test]
    fn test_parse_flag_enable_bare_shows_list() {
        assert_eq!(
            crate::parser::parse_system_command("/flag enable"),
            Some(Command::Flag(FlagSubcommand::List))
        );
        assert_eq!(
            crate::parser::parse_system_command("/flag disable"),
            Some(Command::Flag(FlagSubcommand::List))
        );
    }
    #[test]
    fn test_parse_flag_invalid_subcommand() {
        assert_eq!(
            crate::parser::parse_system_command("/flag bogus"),
            Some(Command::InvalidFlagName(
                "Unknown flag sub-command 'bogus'. Use: /flag enable <name>, /flag disable <name>, /flag list".to_string()
            ))
        );
    }
    #[test]
    fn test_parse_flags_alias() {
        assert_eq!(
            crate::parser::parse_system_command("/flags"),
            Some(Command::Flags)
        );
    }
    #[test]
    fn test_parse_flag_invalid_name() {
        assert_eq!(
            crate::parser::parse_system_command("/flag enable bad/name"),
            Some(Command::InvalidFlagName(
                "Flag names may only contain letters, digits, hyphens, and underscores."
                    .to_string()
            ))
        );
    }
}
