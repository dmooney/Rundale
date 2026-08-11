//! Real-loop proof for explicit current-turn dialogue obligations (#1832).
//!
//! The provider boundary is mocked; prompt preparation, parsing, canonical
//! validation/apply, event publication, and UI streaming are production code.

use parish_core::npc::NpcId;
use parish_core::npc::types::NpcState;
use parish_engine::testing::GameTestHarness;

const ISSUE_INPUT: &str = "Good morning, Father. Peig Hannigan sent me. I'm Aiden Carney, seeking honest work and somewhere dry to sleep.";
const BAD_REPLY: &str = "'Tis a fine morning indeed. Ye've come to the right place for a brief moment of peace. What brings ye to this church?";

fn harness_with_priest() -> (GameTestHarness, NpcId, String) {
    let mut harness = GameTestHarness::new();
    let player_location = harness.app.world.player_location;
    let speaker_id = harness
        .app
        .npc_manager
        .all_npcs()
        .find(|npc| npc.name == "Fr. Declan Tierney")
        .map(|npc| npc.id)
        .expect("Rundale contains Fr. Declan Tierney");
    harness
        .app
        .npc_manager
        .get_mut(speaker_id)
        .expect("priest exists")
        .set_location_and_state(player_location, NpcState::Present);
    harness.app.npc_manager.mark_introduced(speaker_id);
    let elsewhere = harness
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|location| *location != player_location)
        .expect("world has another location");
    let others: Vec<NpcId> = harness
        .app
        .npc_manager
        .all_npcs()
        .filter(|npc| npc.id != speaker_id && npc.location() == player_location)
        .map(|npc| npc.id)
        .collect();
    for id in others {
        harness
            .app
            .npc_manager
            .get_mut(id)
            .expect("other NPC exists")
            .set_location(elsewhere);
    }
    (harness, speaker_id, "Fr. Declan Tierney".to_string())
}

fn streamed_text(events: &[(String, serde_json::Value)]) -> String {
    events
        .iter()
        .filter(|(name, _)| name == "stream-token")
        .filter_map(|(_, payload)| payload.get("token").and_then(serde_json::Value::as_str))
        .collect()
}

fn candidate(dialogue: &str) -> String {
    serde_json::json!({
        "dialogue": dialogue,
        "action": "",
        "mood": "solemn",
        "assigned_task": null
    })
    .to_string()
}

#[test]
fn exact_issue_completion_is_replaced_before_real_loop_ui_and_state_effects() {
    let (mut harness, speaker_id, speaker) = harness_with_priest();
    let original_mood = harness
        .app
        .npc_manager
        .get(speaker_id)
        .unwrap()
        .mood
        .clone();
    let original_tasks = harness.app.world.player_progress.tasks().len();
    let bad_candidate = serde_json::json!({
        "dialogue": BAD_REPLY,
        "action": "offers a room key",
        "mood": "delighted",
        "assigned_task": "Start work at the rectory"
    })
    .to_string();
    harness.mock().push_json_for(&speaker, bad_candidate);

    let events = harness.execute_via_real_loop(&format!("talk to {speaker} about {ISSUE_INPUT}"));
    let rendered = streamed_text(&events);
    let serialized = serde_json::to_string(&events).expect("UI events serialize");

    for required in ["Peig Hannigan", "Aiden Carney", "work", "lodging"] {
        assert!(
            rendered.contains(required),
            "missing {required}: {rendered}"
        );
    }
    for rejected in [BAD_REPLY, "offers a room key", "Start work at the rectory"] {
        assert!(
            !serialized.contains(rejected),
            "candidate escaped: {rejected}"
        );
    }
    assert!(rendered.contains("cannot promise"));
    assert_eq!(
        harness.app.npc_manager.get(speaker_id).unwrap().mood,
        original_mood
    );
    assert_eq!(
        harness.app.world.player_progress.tasks().len(),
        original_tasks
    );
    assert!(
        harness
            .app
            .npc_manager
            .all_npcs()
            .flat_map(|npc| npc.memory.entries())
            .all(|memory| !memory.content.contains(BAD_REPLY))
    );
}

#[test]
fn complete_noncommittal_completion_survives_the_real_loop() {
    let (mut harness, _speaker_id, speaker) = harness_with_priest();
    let good =
        "I hear Peig sent you, Aiden. I cannot promise work or a bed, but I understand both needs.";
    harness.mock().push_json_for(&speaker, candidate(good));

    let events = harness.execute_via_real_loop(&format!("talk to {speaker} about {ISSUE_INPUT}"));

    assert_eq!(streamed_text(&events), good);
}
