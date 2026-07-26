//! Tauri command handlers for the Parish desktop frontend.
//!
//! Each public function here is registered with `tauri::generate_handler!` and
//! becomes callable from the Svelte frontend via `invoke("command_name", args)`.
//!
//! This file is the Rust-2018 hub: it declares the command-family submodules
//! and re-exports every `#[tauri::command]` and public helper so `lib.rs`,
//! the `invoke_handler` registration, and all external `use` paths are unchanged.
//!
//! Submodule layout:
//! - `snapshot`  — read-only world/map/NPC/theme/debug snapshot commands
//! - `setup`     — BYOK onboarding and local inference setup commands
//! - `input`     — player input submission, validation, and dispatch
//! - `saves`     — save/load/branch/new-game persistence commands
//! - `admin`     — debug snapshot builder, inference rebuild, inactivity tick, bug report
//! - `screenshot`— screenshot capture, storage, and round-trip callbacks
//! - `reactions` — emoji reactions and background NPC reaction emission
//! - `demo`      — demo/auto-player commands and prompt helpers
//! - `cmd_tests` — shared test helpers (test-only, cfg(test))

pub mod admin;
pub(crate) mod cmd_tests;
pub mod demo;
pub mod input;
pub mod reactions;
pub mod saves;
pub mod screenshot;
pub mod setup;
pub mod snapshot;

// ── Re-exports: snapshot ──────────────────────────────────────────────────────
pub use snapshot::{
    get_debug_snapshot, get_engine_state, get_map, get_npcs_here, get_setup_snapshot, get_theme,
    get_ui_config, get_world_snapshot, get_world_snapshot_inner, toggle_fullscreen,
};

// ── Re-exports: setup ─────────────────────────────────────────────────────────
pub use setup::{
    LocalSetupArgs, OnboardingOptions, clear_provider_config, get_onboarding_options,
    get_provider_config, list_available_providers, list_byok_env_keys, list_preset_models,
    set_provider_config, start_local_inference_setup, validate_provider_config,
};
pub(crate) use setup::{do_get_onboarding_options, do_start_local_inference_setup};

// ── Re-exports: input ─────────────────────────────────────────────────────────
pub(crate) use input::do_submit_input;
pub use input::{submit_input, validate_addressed_to, validate_input_text};

// ── Re-exports: saves ─────────────────────────────────────────────────────────
pub use saves::{
    create_branch, discover_save_files, do_create_branch, do_load_branch, do_new_game,
    get_save_state, load_branch, new_game, new_save_file, save_game,
};
pub(crate) use saves::{do_branch_log_text, do_list_branches_text, do_save_game};

// ── Re-exports: admin ─────────────────────────────────────────────────────────
pub(crate) use admin::{do_submit_bug_report, tick_inactivity};
pub use admin::{open_url, rebuild_inference_inner, submit_bug_report};

// ── Re-exports: screenshot ────────────────────────────────────────────────────
pub use screenshot::{
    GraphicalReadiness, ScreenshotInfo, decode_data_url_png, get_graphical_readiness,
    get_latest_screenshot, notify_screenshot_captured, notify_screenshot_error,
    notify_screenshot_started, report_graphical_error, report_graphical_ready,
    report_graphical_unready, save_screenshot, take_screenshot, write_screenshot_to_disk,
};
pub(crate) use screenshot::{
    PendingScreenshot, do_get_latest_screenshot, do_take_screenshot, graphical_readiness,
};

// ── Re-exports: reactions ─────────────────────────────────────────────────────
pub use reactions::{is_snippet_injection_char, react_to_message};

// ── Re-exports: demo ──────────────────────────────────────────────────────────
pub use demo::{
    DemoAdjacentLocation, DemoConfigPayload, DemoContextSnapshot, DemoNpcInfo, build_demo_context,
    get_demo_config, get_demo_context, get_llm_player_action,
};
