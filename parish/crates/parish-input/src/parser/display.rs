//! Parsers for display/UI commands: `/theme`, `/debug`, `/spinner`, `/speed`.

use parish_types::GameSpeed;

use crate::commands::Command;

pub(super) const SPINNER_DEFAULT_SECS: u64 = 30;
pub(super) const SPINNER_MAX_SECS: u64 = 300;

pub(super) fn parse_theme_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Theme(None))
    } else {
        Some(Command::Theme(Some(rest.to_string())))
    }
}

pub(super) fn parse_debug_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Debug(None))
    } else {
        Some(Command::Debug(Some(rest.to_string())))
    }
}

pub(super) fn parse_spinner_command(_trimmed: &str, rest: &str) -> Option<Command> {
    let secs = rest
        .parse::<u64>()
        .unwrap_or(SPINNER_DEFAULT_SECS)
        .min(SPINNER_MAX_SECS);
    Some(Command::Spinner(secs))
}

pub(super) fn parse_speed_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::ShowSpeed)
    } else {
        match GameSpeed::from_name(rest) {
            Some(speed) => Some(Command::SetSpeed(speed)),
            None => Some(Command::InvalidSpeed(rest.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::Command;
    use parish_types::GameSpeed;

    #[test]
    fn test_parse_theme_command() {
        assert_eq!(
            crate::parser::parse_system_command("/theme"),
            Some(Command::Theme(None))
        );
        assert_eq!(
            crate::parser::parse_system_command("/theme default"),
            Some(Command::Theme(Some("default".to_string())))
        );
        assert_eq!(
            crate::parser::parse_system_command("/theme solarized"),
            Some(Command::Theme(Some("solarized".to_string())))
        );
        assert_eq!(
            crate::parser::parse_system_command("/theme solarized light"),
            Some(Command::Theme(Some("solarized light".to_string())))
        );
        assert_eq!(
            crate::parser::parse_system_command("/theme solarized dark"),
            Some(Command::Theme(Some("solarized dark".to_string())))
        );
        assert_eq!(
            crate::parser::parse_system_command("/theme solarized auto"),
            Some(Command::Theme(Some("solarized auto".to_string())))
        );
        assert_eq!(
            crate::parser::parse_system_command("/THEME Solarized Dark"),
            Some(Command::Theme(Some("Solarized Dark".to_string())))
        );
    }
    // --- /debug command tests ---
    #[test]
    fn test_parse_debug_bare() {
        assert_eq!(
            crate::parser::parse_system_command("/debug"),
            Some(Command::Debug(None))
        );
    }
    #[test]
    fn test_parse_debug_with_subcommand() {
        assert_eq!(
            crate::parser::parse_system_command("/debug npcs"),
            Some(Command::Debug(Some("npcs".to_string())))
        );
        assert_eq!(
            crate::parser::parse_system_command("/debug memory Padraig"),
            Some(Command::Debug(Some("memory Padraig".to_string())))
        );
    }
    #[test]
    fn test_parse_debug_with_empty_trailing_space() {
        assert_eq!(
            crate::parser::parse_system_command("/debug   "),
            Some(Command::Debug(None))
        );
    }
    #[test]
    fn test_parse_debug_case_insensitive() {
        assert_eq!(
            crate::parser::parse_system_command("/DEBUG"),
            Some(Command::Debug(None))
        );
        assert_eq!(
            crate::parser::parse_system_command("/DEBUG npcs"),
            Some(Command::Debug(Some("npcs".to_string())))
        );
    }
    // --- /spinner command tests ---
    #[test]
    fn test_parse_spinner_bare() {
        assert_eq!(
            crate::parser::parse_system_command("/spinner"),
            Some(Command::Spinner(30))
        );
    }
    #[test]
    fn test_parse_spinner_with_duration() {
        assert_eq!(
            crate::parser::parse_system_command("/spinner 10"),
            Some(Command::Spinner(10))
        );
        assert_eq!(
            crate::parser::parse_system_command("/spinner 120"),
            Some(Command::Spinner(120))
        );
    }
    #[test]
    fn test_parse_spinner_invalid_duration() {
        // Non-numeric falls back to 30
        assert_eq!(
            crate::parser::parse_system_command("/spinner abc"),
            Some(Command::Spinner(30))
        );
    }
    #[test]
    fn test_parse_spinner_clamped_to_max() {
        // Values above SPINNER_MAX_SECS (300) are clamped
        assert_eq!(
            crate::parser::parse_system_command("/spinner 999"),
            Some(Command::Spinner(300))
        );
        assert_eq!(
            crate::parser::parse_system_command("/spinner 301"),
            Some(Command::Spinner(300))
        );
    }
    #[test]
    fn test_parse_speed_show() {
        assert_eq!(
            crate::parser::parse_system_command("/speed"),
            Some(Command::ShowSpeed)
        );
    }
    #[test]
    fn test_parse_speed_set_variants() {
        assert_eq!(
            crate::parser::parse_system_command("/speed slow"),
            Some(Command::SetSpeed(GameSpeed::Slow))
        );
        assert_eq!(
            crate::parser::parse_system_command("/speed normal"),
            Some(Command::SetSpeed(GameSpeed::Normal))
        );
        assert_eq!(
            crate::parser::parse_system_command("/speed fast"),
            Some(Command::SetSpeed(GameSpeed::Fast))
        );
        assert_eq!(
            crate::parser::parse_system_command("/speed fastest"),
            Some(Command::SetSpeed(GameSpeed::Fastest))
        );
    }
    #[test]
    fn test_parse_speed_case_insensitive() {
        assert_eq!(
            crate::parser::parse_system_command("/speed FAST"),
            Some(Command::SetSpeed(GameSpeed::Fast))
        );
        assert_eq!(
            crate::parser::parse_system_command("/speed Slow"),
            Some(Command::SetSpeed(GameSpeed::Slow))
        );
        assert_eq!(
            crate::parser::parse_system_command("/SPEED normal"),
            Some(Command::SetSpeed(GameSpeed::Normal))
        );
    }
    #[test]
    fn test_parse_speed_invalid_shows_error() {
        assert_eq!(
            crate::parser::parse_system_command("/speed bogus"),
            Some(Command::InvalidSpeed("bogus".to_string()))
        );
    }
    #[test]
    fn test_parse_speed_whitespace_shows_current() {
        assert_eq!(
            crate::parser::parse_system_command("/speed   "),
            Some(Command::ShowSpeed)
        );
    }
    // --- /speed ludicrous ---
    #[test]
    fn test_parse_speed_ludicrous() {
        assert_eq!(
            crate::parser::parse_system_command("/speed ludicrous"),
            Some(Command::SetSpeed(GameSpeed::Ludicrous))
        );
    }
}
