//! Unit tests for pure-logic helpers extracted from Tauri command handlers.
//!
//! # Coverage scope — partial fix for #706
//!
//! These tests cover the command-layer validation functions that can be
//! exercised without constructing `AppState` or booting the Tauri runtime.
//! They complement the compile-time symbol checks in `command_registry.rs`
//! and the addressed_to tests in `input_validation.rs`.
//!
//! ## Commands covered (3 of 32)
//!
//! | Command / helper           | Tests         | Reason                          |
//! |----------------------------|---------------|---------------------------------|
//! | `submit_input` (length)    | 4             | #752 — text length cap          |
//! | `react_to_message` (emoji) | 3             | #687 — unknown emoji rejection  |
//! | `react_to_message` (snip)  | 6             | #687 — injection char filter    |
//!
//! ## Commands deferred (29 of 32)
//!
//! All remaining commands bind `tauri::State<Arc<AppState>>` at their call
//! boundary and require either the Tauri runtime or a non-trivial mock.
//! Deferred until #706 provides a lightweight `AppState` test fixture:
//!
//! `get_world_snapshot`, `get_map`, `get_npcs_here`, `get_theme`,
//! `get_debug_snapshot`, `get_ui_config`, `discover_save_files`,
//! `save_game`, `load_branch`, `create_branch`, `new_save_file`,
//! `new_game`, `get_save_state`, `get_demo_config`, `get_demo_context`,
//! `get_llm_player_action`, `react_to_message` (state side effects),
//! all 12 `editor_*` commands (delegated to `parish_core::ipc::editor`
//! which has its own test suite).

use parish_tauri_lib::commands::{is_snippet_injection_char, validate_input_text};

// ── submit_input — text length cap (#752) ────────────────────────────────────

/// Empty string (after trimming) is returned as-is — callers short-circuit.
#[test]
fn validate_input_text_empty_string_is_ok() {
    let result = validate_input_text("");
    assert_eq!(result, Ok(String::new()));
}

/// Whitespace-only trims to empty and is OK.
#[test]
fn validate_input_text_whitespace_only_trims_to_empty() {
    let result = validate_input_text("   \t\n   ");
    assert_eq!(result, Ok(String::new()));
}

/// Text of exactly 2000 characters (after trim) is accepted.
#[test]
fn validate_input_text_exactly_2000_chars_is_ok() {
    let text = "a".repeat(2000);
    let result = validate_input_text(&text);
    assert_eq!(result.map(|s| s.len()), Ok(2000));
}

/// Text of 2001 characters must be rejected.
#[test]
fn validate_input_text_2001_chars_is_rejected() {
    let text = "a".repeat(2001);
    let result = validate_input_text(&text);
    assert!(result.is_err(), "2001-char input should be rejected");
    let err = result.unwrap_err();
    assert!(
        err.contains("2000"),
        "error message should mention the limit: {err}"
    );
}

// ── react_to_message — emoji validation (#687) ───────────────────────────────

/// A valid palette emoji is recognised by the palette guard.
///
/// The Tauri command calls `reactions::reaction_description` directly;
/// this test verifies the same function returns Some for a known emoji.
#[test]
fn valid_palette_emoji_is_recognised() {
    use parish_core::npc::reactions::reaction_description;
    assert!(
        reaction_description("😊").is_some(),
        "😊 should be a known palette emoji"
    );
}

/// An arbitrary string that is not in the palette is rejected.
#[test]
fn unknown_emoji_string_is_not_in_palette() {
    use parish_core::npc::reactions::reaction_description;
    assert!(
        reaction_description("NOTANEMOJI").is_none(),
        "arbitrary string should not be in the palette"
    );
}

/// Unicode thumbs-up (not in the 1820s parish palette) is rejected.
#[test]
fn thumbs_up_emoji_not_in_palette() {
    use parish_core::npc::reactions::reaction_description;
    // 👍 is a common emoji but not in the period-appropriate palette.
    assert!(reaction_description("👍").is_none());
}

// ── react_to_message — snippet injection filter (#687) ───────────────────────

/// Plain ASCII prose is allowed.
#[test]
fn plain_ascii_snippet_has_no_injection_chars() {
    let snippet = "The weaver looked tired today.";
    assert!(
        !snippet.chars().any(is_snippet_injection_char),
        "plain ASCII should contain no injection chars"
    );
}

/// Double-quote is rejected (could break JSON / prompt string boundaries).
#[test]
fn double_quote_is_injection_char() {
    assert!(
        is_snippet_injection_char('"'),
        "double-quote must be flagged as injection char"
    );
}

/// Backslash is rejected (escape vector).
#[test]
fn backslash_is_injection_char() {
    assert!(
        is_snippet_injection_char('\\'),
        "backslash must be flagged as injection char"
    );
}

/// U+2028 LINE SEPARATOR is rejected (JSON/JS injection vector).
#[test]
fn unicode_line_separator_is_injection_char() {
    assert!(
        is_snippet_injection_char('\u{2028}'),
        "U+2028 must be flagged as injection char"
    );
}

/// U+2029 PARAGRAPH SEPARATOR is rejected.
#[test]
fn unicode_paragraph_separator_is_injection_char() {
    assert!(
        is_snippet_injection_char('\u{2029}'),
        "U+2029 must be flagged as injection char"
    );
}

/// Newline (control char) is rejected.
#[test]
fn newline_is_injection_char() {
    assert!(
        is_snippet_injection_char('\n'),
        "newline (control char) must be flagged as injection char"
    );
}
