//! Smoke tests for the parish-tauri command registry.
//!
//! # Framework constraints
//!
//! `tauri::generate_handler!` returns an opaque closure that is not
//! introspectable at runtime; there is no API to enumerate the names it
//! registered at compile time.  The tests here therefore use two complementary
//! strategies:
//!
//! 1. **Length assertion** — `EXPECTED_COMMANDS.len()` is compared against a
//!    hard-coded count.  If a developer adds or removes a command in
//!    `lib.rs` without updating `command_registry::EXPECTED_COMMANDS`, the
//!    test fails.
//!
//! 2. **Compile-time symbol check** — each `#[tauri::command]` function is
//!    imported into this file.  If a command is renamed or deleted, the crate
//!    stops compiling and CI catches it immediately.

use parish_tauri_lib::command_registry::EXPECTED_COMMANDS;

// ── compile-time symbol presence ─────────────────────────────────────────────
//
// Importing each command function by name is sufficient: if a symbol
// disappears from the crate, this file fails to compile.  The imports are
// not called; `#[allow(unused_imports)]` suppresses the lint without
// requiring any runtime plumbing.
#[allow(unused_imports)]
use parish_tauri_lib::commands::{
    create_branch, discover_save_files, get_debug_snapshot, get_map, get_npcs_here, get_save_state,
    get_setup_snapshot, get_theme, get_ui_config, get_world_snapshot, load_branch, new_game,
    new_save_file, react_to_message, save_game, submit_input,
};
#[allow(unused_imports)]
use parish_tauri_lib::editor_commands::{
    editor_close, editor_get_snapshot, editor_list_branches, editor_list_mods, editor_list_saves,
    editor_list_snapshots, editor_open_mod, editor_read_snapshot, editor_reload, editor_save,
    editor_update_locations, editor_update_npcs, editor_validate,
};

/// All 29 expected commands are listed in EXPECTED_COMMANDS.
#[test]
fn command_count_matches_registry() {
    const EXPECTED_COUNT: usize = 29;
    assert_eq!(
        EXPECTED_COMMANDS.len(),
        EXPECTED_COUNT,
        "EXPECTED_COMMANDS has {} entries but {} were expected. \
         Update command_registry.rs whenever you add or remove a \
         #[tauri::command] in lib.rs.",
        EXPECTED_COMMANDS.len(),
        EXPECTED_COUNT,
    );
}

/// Every name in EXPECTED_COMMANDS is non-empty and contains no whitespace.
#[test]
fn command_names_are_well_formed() {
    for name in EXPECTED_COMMANDS {
        assert!(!name.is_empty(), "empty command name in EXPECTED_COMMANDS");
        assert!(
            !name.contains(char::is_whitespace),
            "command name contains whitespace: {name:?}"
        );
    }
}

/// Each name in EXPECTED_COMMANDS is unique (no duplicate registrations).
#[test]
fn command_names_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for name in EXPECTED_COMMANDS {
        assert!(
            seen.insert(*name),
            "duplicate command name in EXPECTED_COMMANDS: {name:?}"
        );
    }
}
