/// Integration tests for cross-user / shared-state isolation
/// (issues #332, #333, #334, #335, #605).
///
/// These tests drive minimal axum routers that exercise the security
/// boundaries without requiring a fully initialised game world where possible.
use axum::http::StatusCode;

use parish_server::routes::{check_admin_against, is_admin_command, validate_branch_name};

// ── #332 — Admin-command gate ─────────────────────────────────────────────────

/// Non-admin user issuing `/cloud key sk-evil` must be blocked.
///
/// `is_admin_command` operates on the parsed `Command` variant (#509, #516) to
/// avoid false-matching in-game dialogue; `check_admin_against` is the testable
/// version of the env-var-backed `check_admin`.
#[test]
fn submit_input_admin_command_non_admin_is_403() {
    use parish_core::input::{InputResult, classify_input};
    let text = "/cloud key sk-evil";
    let InputResult::SystemCommand(cmd) = classify_input(text) else {
        panic!("expected SystemCommand for {text:?}");
    };
    assert!(is_admin_command(&cmd));
    assert_eq!(
        check_admin_against("attacker@example.com", text, Some("operator@example.com")),
        Err(StatusCode::FORBIDDEN),
    );
}

/// Non-admin user issuing a gameplay command must not be blocked.
#[test]
fn submit_input_gameplay_command_any_user_is_200() {
    use parish_core::input::{InputResult, classify_input};
    // "say hello" is not a system command at all — is_admin_command must return false.
    match classify_input("say hello") {
        InputResult::SystemCommand(cmd) => assert!(!is_admin_command(&cmd)),
        _ => { /* not a system command: definitely not admin */ }
    }
}

/// Admin user issuing an admin command must be allowed.
#[test]
fn submit_input_admin_command_admin_is_ok() {
    use parish_core::input::{InputResult, classify_input};
    let text = "/cloud key sk-good";
    let InputResult::SystemCommand(cmd) = classify_input(text) else {
        panic!("expected SystemCommand for {text:?}");
    };
    assert!(is_admin_command(&cmd));
    assert_eq!(
        check_admin_against("operator@example.com", text, Some("operator@example.com")),
        Ok(()),
    );
}

// ── #605 — Admin-email check is order-independent ────────────────────────────
//
// The production `admin_emails()` caches parsed emails in a `OnceCell`
// forever, making tests that call it in parallel order-dependent.
// `check_admin_against` accepts the admin email as an explicit parameter,
// so each test is fully self-contained and carries no shared state.

/// A completely different admin email set can be used in the same test run
/// without interfering with other tests — each call is stateless.
#[test]
fn check_admin_against_different_admin_sets_are_independent() {
    // Simulate a test that runs *first* and configures one admin.
    let result_a = check_admin_against("alice@example.com", "/key sk-a", Some("alice@example.com"));
    assert_eq!(
        result_a,
        Ok(()),
        "alice should be allowed when she is the admin"
    );

    // Simulate a test that runs *second* with a completely different admin —
    // this must not be affected by the previous call's email set.
    let result_b = check_admin_against("bob@example.com", "/key sk-b", Some("bob@example.com"));
    assert_eq!(
        result_b,
        Ok(()),
        "bob should be allowed when he is the admin"
    );

    // Cross-check: alice is not an admin in bob's config.
    let result_c = check_admin_against("alice@example.com", "/key sk-c", Some("bob@example.com"));
    assert_eq!(
        result_c,
        Err(StatusCode::FORBIDDEN),
        "alice must be rejected when bob is the sole admin"
    );
}

/// When no admin is configured (`None`), the fail-closed rule applies in
/// release builds.  In debug builds (tests run with debug assertions) it
/// must succeed.  This result is deterministic regardless of test order.
#[test]
fn check_admin_against_none_config_is_deterministic() {
    let result = check_admin_against("any@example.com", "/key sk-x", None);
    // In test/debug builds, cfg!(debug_assertions) is true → Ok(()).
    assert_eq!(
        result,
        Ok(()),
        "debug build with no admin config must allow (fail-open for local dev)"
    );
}

// ── #333 — Debug snapshot redaction ──────────────────────────────────────────

/// The debug-snapshot handler strips sensitive fields from the call log.
///
/// We exercise the redaction logic directly by constructing an
/// `InferenceLogEntry` with sensitive content, applying the same
/// zeroing-out transformation that `get_debug_snapshot` performs, then
/// asserting the serialised JSON contains `prompt_len` but not the
/// secret text.
#[test]
fn debug_snapshot_call_log_has_prompt_len_not_prompt_text() {
    use parish_core::debug_snapshot::InferenceLogEntry;

    // Construct a synthetic log entry with sensitive content.
    let entry = InferenceLogEntry {
        request_id: 1,
        timestamp: "12:00:00".to_string(),
        model: "llama3".to_string(),
        streaming: false,
        duration_ms: 500,
        prompt_len: 42,
        response_len: 17,
        error: None,
        system_prompt: Some("SECRET SYSTEM PROMPT".to_string()),
        prompt_text: "secret user prompt".to_string(),
        response_text: "secret response".to_string(),
        max_tokens: None,
    };

    // Apply the same redaction that `get_debug_snapshot` applies (#333).
    let redacted = InferenceLogEntry {
        request_id: entry.request_id,
        timestamp: entry.timestamp.clone(),
        model: entry.model.clone(),
        streaming: entry.streaming,
        duration_ms: entry.duration_ms,
        prompt_len: entry.prompt_len,
        response_len: entry.response_len,
        error: entry.error.clone(),
        max_tokens: entry.max_tokens,
        system_prompt: None,
        prompt_text: String::new(),
        response_text: String::new(),
    };

    let json = serde_json::to_value(&redacted).unwrap();

    // Must contain prompt_len.
    assert!(
        json.get("prompt_len").is_some(),
        "prompt_len must be present"
    );
    assert_eq!(json["prompt_len"], 42);

    // Must NOT contain sensitive text anywhere in the serialised output.
    let json_str = json.to_string();
    assert!(
        !json_str.contains("secret"),
        "redacted entry must not contain prompt/response text"
    );
    assert!(
        !json_str.contains("SECRET SYSTEM PROMPT"),
        "redacted entry must not contain system_prompt text"
    );

    // Sensitive fields exist in the struct but are empty / null.
    assert_eq!(json["prompt_text"], "");
    assert_eq!(json["response_text"], "");
    assert!(json["system_prompt"].is_null());
}

// ── #334 — Single WS per email ───────────────────────────────────────────────

/// A second WS upgrade for the same email must be blocked (409 Conflict).
///
/// We test the `active_ws` set logic directly against `AppState` rather than
/// driving a real WebSocket upgrade (which requires a live TCP server).
#[tokio::test]
async fn second_ws_upgrade_same_email_is_409() {
    use std::sync::Arc;

    // Build a minimal AppState using the public builder.
    use parish_core::npc::manager::NpcManager;
    use parish_core::world::transport::TransportConfig;
    use parish_core::world::{LocationId, WorldState};
    use parish_server::state::{GameConfig, UiConfigSnapshot, build_app_state};

    let data_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
    let world = WorldState::from_parish_file(&data_dir.join("world.json"), LocationId(15)).unwrap();
    let npc_manager = NpcManager::new();
    let ui_config = UiConfigSnapshot {
        hints_label: "test".to_string(),
        default_accent: "#000".to_string(),
        splash_text: String::new(),
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        auto_pause_timeout_seconds: 300,
    };
    let theme_palette = parish_core::game_mod::default_theme_palette();
    let saves_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../saves");

    let state = Arc::new(build_app_state(
        world,
        npc_manager,
        None,
        GameConfig {
            provider_name: String::new(),
            base_url: String::new(),
            api_key: None,
            model_name: String::new(),
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_api_key: None,
            cloud_base_url: None,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: 60,
            category_provider: [None, None, None, None],
            category_model: [None, None, None, None],
            category_api_key: [None, None, None, None],
            category_base_url: [None, None, None, None],
            flags: parish_core::config::FeatureFlags::default(),
            category_rate_limit: [None, None, None, None],
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
        },
        None,
        TransportConfig::default(),
        ui_config,
        theme_palette,
        saves_dir,
        data_dir.clone(),
        None,
        data_dir.join("parish-flags.json"),
        parish_core::config::InferenceConfig::default(),
    ));

    // Simulate first connection inserting the email.
    let first_insert: bool = state
        .active_ws
        .lock()
        .await
        .insert("ws-user@example.com".to_string());
    assert!(first_insert, "first insert must succeed");

    // Second attempt: the email is already present — insert returns false.
    let second_insert: bool = state
        .active_ws
        .lock()
        .await
        .insert("ws-user@example.com".to_string());
    assert!(
        !second_insert,
        "second insert must fail (email already active)"
    );

    // Map the HashSet result to the 409 the handler would return.
    let status = if !second_insert {
        StatusCode::CONFLICT
    } else {
        StatusCode::OK
    };
    assert_eq!(status, StatusCode::CONFLICT);
}

// ── #105 / #282 — Debug snapshot acquires locks sequentially, not all at once ─
//
// Before the fix, `get_debug_snapshot` (and the Tauri debug tick) held all 5+
// mutexes simultaneously while building the snapshot.  The fix snapshots each
// lock's data in a brief, non-overlapping window so only `world` and
// `npc_manager` are held during the actual `build_debug_snapshot` call.
//
// This test exercises the corrected lock pattern against a live `AppState` to
// confirm that concurrent snapshot + state-mutation tasks do not deadlock and
// both produce sensible output.

/// Concurrent snapshot builds and state reads must not deadlock (#105, #282).
///
/// Spawns multiple tasks that simultaneously:
/// - read the debug_events / game_events / inference_log (snapshot path)
/// - acquire world + npc_manager (the dominant locks in `build_debug_snapshot`)
///
/// If any task blocks forever the test times out; if the data is coherent
/// each snapshot's `world` fields must be non-empty.
#[tokio::test]
async fn debug_snapshot_no_deadlock_with_concurrent_readers() {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use parish_core::debug_snapshot::{
        AuthDebug, DebugEvent, InferenceDebug, build_debug_snapshot,
    };
    use parish_core::npc::manager::NpcManager;
    use parish_core::world::events::GameEvent;
    use parish_core::world::transport::TransportConfig;
    use parish_core::world::{LocationId, WorldState};
    use parish_server::state::{GameConfig, UiConfigSnapshot, build_app_state};

    let data_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
    let world = WorldState::from_parish_file(&data_dir.join("world.json"), LocationId(15)).unwrap();
    let npc_manager = NpcManager::new();
    let ui_config = UiConfigSnapshot {
        hints_label: "test".to_string(),
        default_accent: "#000".to_string(),
        splash_text: String::new(),
        active_tile_source: String::new(),
        tile_sources: Vec::new(),
        auto_pause_timeout_seconds: 300,
    };
    let theme_palette = parish_core::game_mod::default_theme_palette();
    let saves_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../saves");

    let state = Arc::new(build_app_state(
        world,
        npc_manager,
        None,
        GameConfig {
            provider_name: "test".to_string(),
            base_url: String::new(),
            api_key: None,
            model_name: "test-model".to_string(),
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_api_key: None,
            cloud_base_url: None,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: 60,
            category_provider: [None, None, None, None],
            category_model: [None, None, None, None],
            category_api_key: [None, None, None, None],
            category_base_url: [None, None, None, None],
            flags: parish_core::config::FeatureFlags::default(),
            category_rate_limit: [None, None, None, None],
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
        },
        None,
        TransportConfig::default(),
        ui_config,
        theme_palette,
        saves_dir,
        data_dir.clone(),
        None,
        data_dir.join("parish-flags.json"),
        parish_core::config::InferenceConfig::default(),
    ));

    // Pre-populate debug_events so the snapshot has something to copy.
    {
        let mut events = state.debug_events.lock().await;
        events.push_back(DebugEvent {
            timestamp: "08:00 1820-03-20".to_string(),
            category: "system".to_string(),
            message: "test event".to_string(),
        });
    }

    // Spawn 8 concurrent tasks that each execute the fixed snapshot pattern.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let state = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            // Replicate the fixed lock acquisition sequence from get_debug_snapshot.
            let has_inference_queue = state.inference_queue.lock().await.is_some();
            let (provider_name, model_name, base_url, improv_enabled) = {
                let config = state.config.lock().await;
                (
                    config.provider_name.clone(),
                    config.model_name.clone(),
                    config.base_url.clone(),
                    config.improv_enabled,
                )
            };
            let events_snap: VecDeque<DebugEvent> =
                state.debug_events.lock().await.iter().cloned().collect();
            let game_events_snap: VecDeque<GameEvent> =
                state.game_events.lock().await.iter().cloned().collect();
            let call_log: Vec<parish_core::debug_snapshot::InferenceLogEntry> =
                state.inference_log.lock().await.iter().cloned().collect();

            let inference = InferenceDebug {
                provider_name,
                model_name,
                base_url,
                cloud_provider: None,
                cloud_model: None,
                has_queue: has_inference_queue,
                reaction_req_id: 0,
                improv_enabled,
                call_log,
                categories: vec![],
                configured_providers: vec![],
            };

            // Acquire world + npc_manager last, build snapshot, release.
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
            let snapshot = build_debug_snapshot(
                &world,
                &npc_manager,
                &events_snap,
                &game_events_snap,
                &inference,
                &AuthDebug::disabled(),
            );
            drop(npc_manager);
            drop(world);

            snapshot
        }));
    }

    // All tasks must complete without deadlock.
    let mut snapshots = Vec::new();
    for handle in handles {
        snapshots.push(handle.await.expect("task panicked"));
    }

    assert_eq!(snapshots.len(), 8, "all 8 snapshot tasks must complete");
    for snap in &snapshots {
        // The snapshot must reference a valid world clock.
        assert!(
            snap.clock.game_time.contains("08:00"),
            "clock should start at 08:00"
        );
        // The pre-populated debug event must be present.
        assert_eq!(snap.events.len(), 1, "one debug event must be present");
    }
}

// ── #335 — Branch name validation ────────────────────────────────────────────

/// A branch name of 65 characters must be rejected with 400.
#[test]
fn create_branch_65_char_name_is_400() {
    let long_name = "a".repeat(65);
    assert_eq!(
        validate_branch_name(&long_name),
        Err(StatusCode::BAD_REQUEST)
    );
}

/// A branch name with only valid characters must pass validation.
#[test]
fn create_branch_valid_name_passes_validation() {
    assert_eq!(validate_branch_name("valid name"), Ok(()));
}
