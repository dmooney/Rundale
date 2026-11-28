//! Parsers for world/movement commands: `/map`, `/wait`, `/unexplored`, `/weather`.

use std::num::IntErrorKind;

use crate::commands::Command;

pub(super) fn parse_map_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Map(None))
    } else {
        Some(Command::Map(Some(rest.to_string())))
    }
}

pub(super) fn parse_wait_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        return Some(Command::Wait(DEFAULT_WAIT_MINUTES));
    }
    match rest.parse::<u32>() {
        Ok(mins) => Some(Command::Wait(mins)),
        Err(err) => Some(Command::InvalidWait(wait_error_message(rest, err.kind()))),
    }
}

/// Minutes a bare `/wait` lets pass.
const DEFAULT_WAIT_MINUTES: u32 = 15;

/// Player-facing reason a `/wait` argument was refused. The clock does not
/// move on a refusal, so the message says what to type instead.
fn wait_error_message(arg: &str, kind: &IntErrorKind) -> String {
    let reason = match kind {
        IntErrorKind::PosOverflow => format!("'{arg}' is too many minutes"),
        _ if arg.starts_with('-') => format!("'{arg}' is a negative time"),
        _ => format!("'{arg}' is not a number of minutes"),
    };
    format!(
        "You can't wait: {reason}. Try /wait on its own for the usual wait, \
         or /wait followed by a whole number of minutes, such as /wait 30."
    )
}

pub(super) fn parse_unexplored_command(_trimmed: &str, rest: &str) -> Option<Command> {
    let arg = rest.to_lowercase();
    match arg.as_str() {
        "reveal" | "show" | "on" => Some(Command::Unexplored(Some(true))),
        "hide" | "off" => Some(Command::Unexplored(Some(false))),
        _ => Some(Command::Unexplored(None)),
    }
}

pub(super) fn parse_weather_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Weather(None))
    } else {
        Some(Command::Weather(Some(rest.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::Command;

    #[test]
    fn test_parse_map_command() {
        assert_eq!(
            crate::parser::parse_system_command("/map"),
            Some(Command::Map(None))
        );
        assert_eq!(
            crate::parser::parse_system_command("/map   "),
            Some(Command::Map(None))
        );
        assert_eq!(
            crate::parser::parse_system_command("/map osm"),
            Some(Command::Map(Some("osm".to_string())))
        );
        assert_eq!(
            crate::parser::parse_system_command("/map historic"),
            Some(Command::Map(Some("historic".to_string())))
        );
    }
    #[test]
    fn test_parse_map_command_case_insensitive() {
        assert_eq!(
            crate::parser::parse_system_command("/MAP"),
            Some(Command::Map(None))
        );
        assert_eq!(
            crate::parser::parse_system_command("/MAP OSM"),
            Some(Command::Map(Some("OSM".to_string())))
        );
    }
    #[test]
    fn test_parse_wait_command() {
        assert_eq!(
            crate::parser::parse_system_command("/wait"),
            Some(Command::Wait(15))
        );
        assert_eq!(
            crate::parser::parse_system_command("/wait 60"),
            Some(Command::Wait(60))
        );
    }

    fn assert_invalid_wait(input: &str, expected_reason: &str) {
        match crate::parser::parse_system_command(input) {
            Some(Command::InvalidWait(msg)) => {
                assert!(msg.contains(expected_reason), "{input}: {msg}");
                assert!(msg.contains("/wait 30"), "{input}: {msg}");
            }
            other => panic!("{input} must be refused, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_wait_zero_and_u32_max_are_valid() {
        assert_eq!(
            crate::parser::parse_system_command("/wait 0"),
            Some(Command::Wait(0))
        );
        assert_eq!(
            crate::parser::parse_system_command("/wait 4294967295"),
            Some(Command::Wait(u32::MAX))
        );
    }

    #[test]
    fn test_parse_wait_rejects_negative() {
        assert_invalid_wait("/wait -1", "negative");
        assert_invalid_wait("/wait -0", "negative");
    }

    #[test]
    fn test_parse_wait_rejects_non_numeric() {
        assert_invalid_wait("/wait abc", "not a number");
        assert_invalid_wait("/wait bananas", "not a number");
        assert_invalid_wait("/wait 1.5", "not a number");
        assert_invalid_wait("/wait 30 minutes", "not a number");
    }

    #[test]
    fn test_parse_wait_rejects_overflow() {
        assert_invalid_wait("/wait 4294967296", "too many");
        assert_invalid_wait("/wait 999999999999999999999999999999999", "too many");
    }

    #[test]
    fn test_parse_unexplored_command() {
        assert_eq!(
            crate::parser::parse_system_command("/unexplored"),
            Some(Command::Unexplored(None))
        );
        assert_eq!(
            crate::parser::parse_system_command("/unexplored reveal"),
            Some(Command::Unexplored(Some(true)))
        );
        assert_eq!(
            crate::parser::parse_system_command("/unexplored hide"),
            Some(Command::Unexplored(Some(false)))
        );
        assert_eq!(
            crate::parser::parse_system_command("/unexplored on"),
            Some(Command::Unexplored(Some(true)))
        );
        assert_eq!(
            crate::parser::parse_system_command("/unexplored off"),
            Some(Command::Unexplored(Some(false)))
        );
        assert_eq!(
            crate::parser::parse_system_command("/unexplored whatever"),
            Some(Command::Unexplored(None))
        );
    }
    // --- /weather command tests ---
    #[test]
    fn test_parse_weather_bare() {
        assert_eq!(
            crate::parser::parse_system_command("/weather"),
            Some(Command::Weather(None))
        );
        assert_eq!(
            crate::parser::parse_system_command("/weather  "),
            Some(Command::Weather(None))
        );
    }
    #[test]
    fn test_parse_weather_set() {
        assert_eq!(
            crate::parser::parse_system_command("/weather clear"),
            Some(Command::Weather(Some("clear".to_string())))
        );
        assert_eq!(
            crate::parser::parse_system_command("/weather light rain"),
            Some(Command::Weather(Some("light rain".to_string())))
        );
    }
    #[test]
    fn test_parse_weather_case_insensitive() {
        assert_eq!(
            crate::parser::parse_system_command("/WEATHER"),
            Some(Command::Weather(None))
        );
        assert_eq!(
            crate::parser::parse_system_command("/WEATHER FOG"),
            Some(Command::Weather(Some("FOG".to_string())))
        );
    }
}
