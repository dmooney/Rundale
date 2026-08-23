//! Real-loop regression proof for the #1834 dialogue candidate quarantine.
//!
//! Evidence type: game-loop integration test. These tests drive
//! `execute_via_real_loop`, mocking only the provider completion boundary.

use parish_core::npc::types::NpcState;
use parish_engine::testing::GameTestHarness;

const SAFE_RECOVERY: &str = parish_core::game_loop::npc_turn::DIALOGUE_RETRY_MESSAGE;

fn harness_with_peig() -> (GameTestHarness, parish_core::npc::NpcId, String) {
    let mut harness = GameTestHarness::new();
    let location = harness.app.world.player_location;
    let peig_id = harness
        .app
        .npc_manager
        .all_npcs()
        .find(|npc| npc.name == "Peig Hannigan")
        .map(|npc| npc.id)
        .expect("Rundale contains Peig Hannigan");
    harness
        .app
        .npc_manager
        .get_mut(peig_id)
        .expect("Peig exists")
        .set_location_and_state(location, NpcState::Present);
    harness.app.npc_manager.mark_introduced(peig_id);
    (harness, peig_id, "Peig Hannigan".to_string())
}

fn streamed_text(events: &[(String, serde_json::Value)]) -> String {
    events
        .iter()
        .filter(|(name, _)| name == "stream-token")
        .filter_map(|(_, payload)| payload.get("token").and_then(serde_json::Value::as_str))
        .collect()
}

fn assert_failed_without_candidate_effects(events: &[(String, serde_json::Value)]) {
    assert!(streamed_text(events).is_empty());
    let terminal = events
        .iter()
        .find(|(name, _)| name == "stream-turn-end")
        .map(|(_, payload)| payload)
        .expect("rejected candidate should terminate the stream");
    assert_eq!(
        terminal.get("status").and_then(serde_json::Value::as_str),
        Some("failed")
    );
    assert_eq!(
        terminal
            .get("recovery_message")
            .and_then(serde_json::Value::as_str),
        Some(SAFE_RECOVERY)
    );
    assert!(terminal.get("final_text").is_none());
}

#[test]
fn forbidden_full_json_candidate_has_no_raw_text_or_metadata_effects() {
    let (mut harness, peig_id, speaker) = harness_with_peig();
    let original_mood = harness.app.npc_manager.get(peig_id).unwrap().mood.clone();
    let original_task_count = harness.app.world.player_progress.tasks().len();
    let raw_line = "Council says the planning board has set tongues.";
    let raw_json = format!(
        r#"{{"dialogue":"{raw_line}","action":"waves the committee notice","mood":"delighted","assigned_task":"Attend the agricultural show committee"}}"#
    );
    harness.mock().push_json_for(&speaker, &raw_json);
    let mut game_events = harness.app.world.event_bus.subscribe();

    let ui_events = harness.execute_via_real_loop(&format!("talk to {speaker}"));

    assert_failed_without_candidate_effects(&ui_events);
    let serialized_ui = serde_json::to_string(&ui_events).unwrap();
    for forbidden in [raw_line, "committee notice", "Attend the agricultural show"] {
        assert!(
            !serialized_ui.contains(forbidden),
            "raw candidate effect escaped: {forbidden}"
        );
    }
    assert!(!ui_events.iter().any(|(name, payload)| {
        name == "text-log"
            && payload.get("subtype").and_then(serde_json::Value::as_str) == Some("action")
    }));
    assert_eq!(
        harness.app.npc_manager.get(peig_id).unwrap().mood,
        original_mood
    );
    assert_eq!(
        harness.app.world.player_progress.tasks().len(),
        original_task_count
    );
    assert!(
        harness
            .app
            .npc_manager
            .get(peig_id)
            .unwrap()
            .memory
            .recent(8)
            .iter()
            .all(|memory| !memory.content.contains(raw_line))
    );

    assert!(
        game_events.try_recv().is_err(),
        "rejected candidate must not publish a canonical dialogue event"
    );
}

#[test]
fn malformed_and_raw_provider_modes_have_identical_canonical_output() {
    for candidate in [
        r#"{"dialogue":"The recovered planning board line""#,
        "The agricultural show committee has very strong opinions.",
    ] {
        let (mut harness, _peig_id, speaker) = harness_with_peig();
        harness.mock().push_json_for(&speaker, candidate);
        let events = harness.execute_via_real_loop(&format!("talk to {speaker}"));

        assert_failed_without_candidate_effects(&events);
        assert!(!serde_json::to_string(&events).unwrap().contains(candidate));
    }
}
