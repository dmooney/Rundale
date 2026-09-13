//! Look command — renders the current location description with NPC names and exits.

/// The desktop IPC path retains its historical import path while delegating to
/// the dependency-light renderer shared with embedded clients.
pub use crate::portable_look::render_look_text;
