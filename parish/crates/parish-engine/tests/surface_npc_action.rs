//! Real-loop integration tests for NPC action narration surfacing (#1490).
//!
//! These tests drive the **real** `parish_core::game_loop` via
//! `execute_via_real_loop` with a mock inference client that injects JSON
//! responses containing `action` fields, then assert the NPC action is
//! surfaced to the player as a `text-log` event with `subtype: "action"`.
//!
//! # Why `execute_via_real_loop` and not a plain unit test?
//!
//! The action-narration emission is at line `run_npc_turn` in
//! `parish_core::game_loop::npc_turn`, which is reached via
//! `handle_game_input` → `handle_npc_conversation` → `run_npc_turn`.
//! The legacy `execute()` path and `--script` fixtures bypass this function
//! entirely. `execute_via_real_loop` routes through the real game-loop path,
//! so the emission IS exercised here.

use parish_core::npc::types::NpcState;
use parish_engine::testing::GameTestHarness;

/// Sets up a harness with exactly one NPC co-located with the player and
/// marked as introduced so dialogue routing resolves deterministically.
/// Returns `(harness, npc_name)`.
fn harness_with_one_npc() -> (GameTestHarness, String) {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    let speaker_id = h
        .app
        .npc_manager
        .all_npcs()
        .map(|n| n.id)
        .next()
        .expect("harness loads at least one NPC");

    let speaker_name = {
        let npc = h
            .app
            .npc_manager
            .get_mut(speaker_id)
            .expect("speaker exists");
        npc.set_location_and_state(player_loc, NpcState::Present);
        npc.name.clone()
    };
    h.app.npc_manager.mark_introduced(speaker_id);

    // Move every other NPC away so routing is deterministic.
    let other_loc = h
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|l| *l != player_loc)
        .expect("graph has at least two locations");
    let others: Vec<_> = h
        .app
        .npc_manager
        .all_npcs()
        .filter(|n| n.location() == player_loc && n.id != speaker_id)
        .map(|n| n.id)
        .collect();
    for id in others {
        if let Some(n) = h.app.npc_manager.get_mut(id) {
            n.set_location(other_loc);
        }
    }

    (h, speaker_name)
}

// ── AC-1: action present → stage-direction event emitted ─────────────────────

/// AC-1 (#1490, game-loop integration): When the mock model returns a JSON
/// response with a non-empty `action` field, `run_npc_turn` must emit a
/// `text-log` event with `subtype: "action"` whose `content` wraps the
/// formatted stage direction in asterisks.
#[test]
fn real_loop_npc_action_is_surfaced_as_stage_direction() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // JSON response with both dialogue and a non-verbal action.
    // Use `push_json_for` to inject a raw JSON object so the action field is
    // preserved; `push_for` wraps the text in the plain dialogue envelope and
    // always sets action to "".
    let json_reply =
        r#"{"dialogue": "A fine morning to ye.", "action": "nods curtly", "mood": "sharp"}"#;
    h.mock().push_json_for(&speaker_name, json_reply);

    let events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));

    // Collect all text-log events.
    let text_logs: Vec<(Option<String>, String)> = events
        .iter()
        .filter(|(name, _)| name == "text-log")
        .map(|(_, payload)| {
            let subtype = payload
                .get("subtype")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let content = payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            (subtype, content)
        })
        .collect();

    // There must be at least one text-log overall (dialogue was emitted).
    assert!(
        !text_logs.is_empty(),
        "expected at least one text-log event from NPC turn; got events: {events:?}"
    );

    // At least one text-log entry must carry subtype "action".
    let action_entries: Vec<_> = text_logs
        .iter()
        .filter(|(subtype, _)| subtype.as_deref() == Some("action"))
        .collect();

    assert!(
        !action_entries.is_empty(),
        "expected a text-log with subtype \"action\" for NPC action narration (#1490); \
         got text-logs: {text_logs:?}"
    );

    // The action content must contain the action text wrapped in asterisks.
    let action_content = &action_entries[0].1;
    assert!(
        action_content.contains("nods curtly"),
        "action stage-direction must contain the action text \"nods curtly\"; \
         got: {action_content:?}"
    );
    assert!(
        action_content.starts_with('*'),
        "action stage-direction must be wrapped in asterisks for italic rendering; \
         got: {action_content:?}"
    );
    assert!(
        action_content.ends_with('*'),
        "action stage-direction must be wrapped in asterisks for italic rendering; \
         got: {action_content:?}"
    );
    // The NPC name must appear in the stage direction.
    assert!(
        action_content.contains(&speaker_name),
        "action stage-direction must include the NPC name; \
         got: {action_content:?}, speaker: {speaker_name:?}"
    );
}

// ── AC-2: action absent → no spurious stage-direction event ──────────────────

/// AC-2 (#1490, game-loop integration): When the mock model returns a JSON
/// response with an empty `action` field (or no metadata), no `text-log`
/// event with `subtype: "action"` must be emitted.
#[test]
fn real_loop_empty_action_emits_no_stage_direction() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // JSON response with empty action field.
    let json_reply = r#"{"dialogue": "Good day to ye.", "action": "", "mood": "neutral"}"#;
    h.mock().push_for(&speaker_name, json_reply);

    let events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));

    let action_entries: Vec<_> = events
        .iter()
        .filter(|(name, payload)| {
            name == "text-log" && payload.get("subtype").and_then(|v| v.as_str()) == Some("action")
        })
        .collect();

    assert!(
        action_entries.is_empty(),
        "no text-log with subtype \"action\" must be emitted when action field is empty; \
         got: {action_entries:?}"
    );
}

// ── AC-3: kill-switch flag disables narration ─────────────────────────────────

/// AC-3 (#1490, game-loop integration): When the `npc-action-narration` flag
/// is explicitly disabled, no `text-log` with `subtype: "action"` is emitted
/// even when the model supplies a non-empty `action` field.
#[test]
fn real_loop_action_narration_suppressed_when_flag_disabled() {
    let (mut h, speaker_name) = harness_with_one_npc();

    // Disable the kill-switch flag. The real-loop pulls config from
    // `app.snapshot_config()` which clones `app.flags`, so mutating
    // `app.flags` before the call propagates into `ctx.config.flags`.
    h.app
        .flags
        .disable(parish_core::game_loop::npc_turn::NPC_ACTION_NARRATION_FLAG);

    let json_reply =
        r#"{"dialogue": "Aye, working hard.", "action": "wipes brow", "mood": "tired"}"#;
    h.mock().push_json_for(&speaker_name, json_reply);

    let events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));

    let action_entries: Vec<_> = events
        .iter()
        .filter(|(name, payload)| {
            name == "text-log" && payload.get("subtype").and_then(|v| v.as_str()) == Some("action")
        })
        .collect();

    assert!(
        action_entries.is_empty(),
        "no text-log with subtype \"action\" must be emitted when npc-action-narration is disabled; \
         got: {action_entries:?}"
    );
}
