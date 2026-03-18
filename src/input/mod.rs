//! Player input parsing and command detection.
//!
//! System commands use `/` prefix (e.g., `/quit`, `/save`).
//! All other input is natural language sent to the LLM for
//! intent parsing (move, talk, look, interact, examine).

// TODO: Command enum (Pause, Resume, Quit, Save, Fork, Load, etc.)
// TODO: Parse system commands from raw input
// TODO: PlayerIntent struct for LLM-parsed natural language
// TODO: Intent parsing via Ollama
