//! Parsers for save/branch commands: `/fork`, `/load`.

use crate::commands::{Command, validate_branch_name};

pub(super) fn parse_fork_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Help) // bare /fork → show help
    } else {
        match validate_branch_name(rest) {
            Ok(()) => Some(Command::Fork(rest.to_string())),
            Err(msg) => Some(Command::InvalidBranchName(msg)),
        }
    }
}

pub(super) fn parse_load_command(_trimmed: &str, rest: &str) -> Option<Command> {
    if rest.is_empty() {
        Some(Command::Load(String::new())) // empty string = show save picker
    } else {
        match validate_branch_name(rest) {
            Ok(()) => Some(Command::Load(rest.to_string())),
            Err(msg) => Some(Command::InvalidBranchName(msg)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::Command;

    #[test]
    fn test_parse_fork() {
        assert_eq!(
            crate::parser::parse_system_command("/fork main"),
            Some(Command::Fork("main".to_string()))
        );
        assert_eq!(
            crate::parser::parse_system_command("/fork  my save "),
            Some(Command::Fork("my save".to_string()))
        );
    }
    #[test]
    fn test_parse_load() {
        assert_eq!(
            crate::parser::parse_system_command("/load main"),
            Some(Command::Load("main".to_string()))
        );
    }
    #[test]
    fn test_parse_fork_empty_name() {
        // Bare /fork with only whitespace returns Help command
        assert_eq!(
            crate::parser::parse_system_command("/fork "),
            Some(Command::Help)
        );
        assert_eq!(
            crate::parser::parse_system_command("/fork   "),
            Some(Command::Help)
        );
    }
    // --- /load edge cases ---
    #[test]
    fn test_parse_load_empty_shows_picker() {
        assert_eq!(
            crate::parser::parse_system_command("/load"),
            Some(Command::Load(String::new()))
        );
        assert_eq!(
            crate::parser::parse_system_command("/load  "),
            Some(Command::Load(String::new()))
        );
    }
    #[test]
    fn test_fork_with_invalid_branch_name() {
        assert_eq!(
            crate::parser::parse_system_command("/fork ../../etc"),
            Some(Command::InvalidBranchName(
                "Branch names may only contain letters, numbers, spaces, underscores, and hyphens."
                    .to_string()
            ))
        );
    }
    #[test]
    fn test_load_with_invalid_branch_name() {
        assert_eq!(
            crate::parser::parse_system_command("/load ../../etc"),
            Some(Command::InvalidBranchName(
                "Branch names may only contain letters, numbers, spaces, underscores, and hyphens."
                    .to_string()
            ))
        );
    }
}
