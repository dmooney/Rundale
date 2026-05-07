//! Registry of the Tauri command names that `lib.rs` registers via
//! `tauri::generate_handler!`.
//!
//! The list here must stay in sync with the `invoke_handler` block in
//! `lib.rs`.  The integration test `tests/command_registry.rs` uses it as a
//! compile-time and length assertion so that additions or deletions in one
//! place are caught before they ship.

/// Canonical list of Tauri command names exposed by this crate, in the same
/// order they appear in the `generate_handler!` block in `lib.rs`.
pub const EXPECTED_COMMANDS: &[&str] = &[
    // ── core game commands ────────────────────────────────────────────────
    "get_world_snapshot",
    "get_map",
    "get_npcs_here",
    "get_theme",
    "get_ui_config",
    "get_debug_snapshot",
    "get_setup_snapshot",
    "submit_input",
    "discover_save_files",
    "save_game",
    "load_branch",
    "create_branch",
    "new_save_file",
    "new_game",
    "get_save_state",
    "react_to_message",
    // ── demo commands ──────────────────────────────────────────────────────
    "get_demo_config",
    "get_demo_context",
    "get_llm_player_action",
    // ── editor commands ───────────────────────────────────────────────────
    "editor_list_mods",
    "editor_open_mod",
    "editor_get_snapshot",
    "editor_validate",
    "editor_update_npcs",
    "editor_update_locations",
    "editor_save",
    "editor_reload",
    "editor_close",
    "editor_list_saves",
    "editor_list_branches",
    "editor_list_snapshots",
    "editor_read_snapshot",
];
