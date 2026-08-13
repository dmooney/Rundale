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
pub use intent_llm::parse_intent;
pub use intent_local::{
    detect_atmospheric_topic, is_physical_action_shaped, is_player_dialogue,
    is_player_dialogue_with_addressees, parse_intent_local,
};
pub use intent_types::{AtmosphericTopic, InputResult, IntentKind, PlayerIntent};
pub use mention::{MentionExtraction, extract_mention};
pub use parser::{
    classify_input, classify_input_with_addressees, classify_input_with_context,
    has_explicit_addressee, parse_system_command,
};
