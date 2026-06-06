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
//! - `movement`  — handle_movement, handle_look
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
pub(crate) mod movement;
pub mod reactions;
pub mod saves;
pub mod screenshot;
pub mod setup;
pub mod snapshot;

// ── Re-exports: snapshot ──────────────────────────────────────────────────────
pub use snapshot::get_world_snapshot_inner;

// ── Re-exports: setup ─────────────────────────────────────────────────────────
pub use setup::{LocalSetupArgs, OnboardingOptions};
pub(crate) use setup::{do_get_onboarding_options, do_start_local_inference_setup};

// ── Re-exports: input ─────────────────────────────────────────────────────────
pub use input::{validate_addressed_to, validate_input_text};
pub(crate) use input::{MAX_ADDRESSED_TO, MAX_TARGETS, do_submit_input, handle_game_input};

// ── Re-exports: saves ─────────────────────────────────────────────────────────
pub use saves::{do_create_branch, do_load_branch, do_new_game};
pub(crate) use saves::{do_branch_log_text, do_list_branches_text, do_save_game};

// ── Re-exports: admin ─────────────────────────────────────────────────────────
pub use admin::rebuild_inference_inner;
pub(crate) use admin::{build_app_debug_snapshot, do_submit_bug_report, tick_inactivity};

// ── Re-exports: screenshot ────────────────────────────────────────────────────
pub use screenshot::{ScreenshotInfo, decode_data_url_png, write_screenshot_to_disk};
pub(crate) use screenshot::{do_get_latest_screenshot, do_save_screenshot, do_take_screenshot};

// ── Re-exports: reactions ─────────────────────────────────────────────────────
pub use reactions::is_snippet_injection_char;

// ── Re-exports: demo ──────────────────────────────────────────────────────────
pub use demo::{DemoAdjacentLocation, DemoConfigPayload, DemoContextSnapshot, DemoNpcInfo, build_demo_context};
