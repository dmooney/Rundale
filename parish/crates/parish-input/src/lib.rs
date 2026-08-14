//! Player input parsing and command detection.
//!
//! System commands use `/` prefix (e.g., `/quit`, `/save`).
//! All other input is natural language sent to the LLM for
//! intent parsing (move, talk, look, interact, examine).

mod commands;
mod intent_llm;
mod intent_local;
mod intent_types;
mod mention;
mod parser;

pub use commands::{Command, FlagSubcommand, InferenceLogSub, validate_branch_name};
pub use intent_llm::{
    parse_intent, parse_intent_with_profile, parse_intent_with_profile_and_audit,
};
pub use intent_local::{
    is_directed_instruction_dialogue, is_physical_action_shaped, is_player_dialogue,
    parse_intent_local,
};
pub use intent_types::{InputResult, IntentKind, PlayerIntent};
pub use mention::{MentionExtraction, extract_mention};
pub use parser::{classify_input, parse_system_command};
