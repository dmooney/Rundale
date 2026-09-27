//! Parsers for world/movement commands: `/map`, `/wait`, `/unexplored`, `/weather`.

use crate::commands::Command;

pub(super) fn parse_map_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Map(None))
    } else {
        Some(Command::Map(Some(rest.to_string())))
    }
}

pub(super) fn parse_wait_command(_trimmed: &str, rest: &str) -> Option<Command> {
    let mins = rest.parse::<u32>().unwrap_or(15);
    Some(Command::Wait(mins))
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
        assert_eq!(
            crate::parser::parse_system_command("/wait abc"),
            Some(Command::Wait(15))
        );
    }
    #[test]
    fn test_parse_wait_large_input_fallback() {
        // Values too large for u32 fall back to the default of 15
        assert_eq!(
            crate::parser::parse_system_command("/wait 999999999999999999999"),
            Some(Command::Wait(15))
        );
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
