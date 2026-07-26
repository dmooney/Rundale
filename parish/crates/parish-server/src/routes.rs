//! HTTP route handlers for the Parish web server.
//!
//! Each route maps to a Tauri command, calling the shared handlers in
//! [`parish_core::ipc`] and returning JSON responses.
//!
//! Handlers are split into submodules by route family:
//! - [`world`]         — query endpoints (snapshot, map, npcs, theme, debug, bug report)
//! - [`input`]         — player input and game-loop adapters
//! - [`reactions`]     — NPC reaction recording and emission
//! - [`saves`]         — save-file and branch lifecycle
//! - [`admin`]         — admin guard, branch-name validation, addressed_to validation
//! - [`demo`]          — demo/screenshot stubs (desktop-only, returns 501)
//! - [`mods`]          — mod listing and switching
//! - [`session_token`] — WS session-token issuance

pub mod admin;
pub mod demo;
pub mod input;
pub mod mods;
pub mod reactions;
pub mod saves;
pub mod session_token;
pub mod turn;
pub mod world;

// ── Re-exports — keep lib.rs route registration unchanged ────────────────────

// world
pub use world::{
    build_full_debug_snapshot, get_app_icon, get_available_providers, get_debug_snapshot,
    get_engine_state, get_favicon, get_health, get_map, get_npcs_here, get_reconnect_state,
    get_setup_snapshot, get_theme, get_ui_config, get_world_snapshot, redact_call_log,
    serve_mod_icon, submit_bug_report,
};

// input
pub use input::{
    SubmitInputRequest, emit_world_update, handle_game_input, handle_system_command,
    rebuild_inference_inner, spawn_loading_animation, submit_input, tick_inactivity,
    touch_player_activity,
};

// turn
pub use turn::get_turn;

// reactions
pub use reactions::{emit_npc_reactions, react_to_message};

// saves
pub use saves::{
    CreateBranchRequest, LoadBranchRequest, create_branch, discover_save_files,
    do_branch_log_inner, do_fork_branch_inner, do_list_branches_inner, do_new_game_inner,
    do_save_game_inner, get_save_state, load_branch, load_branch_name, new_game, new_save_file,
    restore_snapshot_and_emit, save_game, validate_and_acquire_lock,
};

// admin
pub use admin::{
    admin_emails, check_admin, check_admin_against, check_admin_no_config, is_admin_command,
    parse_admin_emails, validate_addressed_to, validate_branch_name,
};

// demo
pub use demo::{
    get_demo_config, get_demo_context, get_latest_screenshot, get_llm_player_action,
    save_screenshot, take_screenshot,
};

// mods
pub use mods::{ModEntry, SwitchModBody, collect_base_mods, list_mods, switch_mod};

// session_token
pub use session_token::{SessionInitResponse, session_init};

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub mod tests;
