//! Real-loop proof that objects the player describes are remembered across
//! turns (#1871) without the model's reply being checked against them.
//!
//! Drives `execute_via_real_loop`, mocking only completed provider responses.

use limerick_core::npc::NpcId;
use limerick_core::npc::types::NpcState;
use limerick_engine::testing::GameTestHarness;

fn harness_with_speaker(name: &str) -> (GameTestHarness, NpcId, String) {
    let mut harness = GameTestHarness::new();
    let player_location = harness.app.world.player_location;
    let speaker_id = harness
        .app
        .npc_manager
        .all_npcs()
        .find(|npc| npc.name == name)
        .map(|npc| npc.id)
        .unwrap_or_else(|| panic!("Rundale contains {name}"));
    harness
        .app
        .npc_manager
        .get_mut(speaker_id)
        .expect("speaker exists")
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
    (harness, speaker_id, name.to_string())
}

fn streamed_text(events: &[(String, serde_json::Value)]) -> String {
    events
        .iter()
        .filter(|(name, _)| name == "stream-token")
        .filter_map(|(_, payload)| payload.get("token").and_then(serde_json::Value::as_str))
        .collect()
}

fn reply(dialogue: &str) -> String {
    serde_json::json!({
        "dialogue": dialogue,
        "action": "",
        "mood": "content",
        "assigned_task": null
    })
    .to_string()
}

#[test]
fn player_established_object_material_survives_a_multiturn_real_loop() {
    let (mut harness, speaker_id, speaker) = harness_with_speaker("Padraig Darcy");
    harness.mock().push_json_for(
        &speaker,
        reply("Aye, a red wool ribbon with one blue stitch; I have it in mind."),
    );
    let first = harness.execute_via_real_loop(&format!(
        "@{speaker} The red wool ribbon has one blue stitch through its centre."
    ));
    assert!(streamed_text(&first).contains("red wool ribbon"));

    // The model's next reply contradicts the player's account (silk, not
    // wool). It is committed as written; the player's own facts are kept.
    let contradicting =
        "A small mark like that turns a plain scrap of silk into a whole life's remembrance.";
    harness.mock().push_json_for(&speaker, reply(contradicting));
    let second =
        harness.execute_via_real_loop(&format!("@{speaker} What did I tell you about the ribbon?"));
    assert_eq!(streamed_text(&second), contradicting);

    let facts = harness
        .app
        .world
        .conversation_log
        .remembered_object_facts(speaker_id, harness.app.world.player_location);
    assert_eq!(facts.len(), 1);
    assert!(
        facts[0]
            .attributes
            .iter()
            .any(|attribute| attribute.value == "wool"),
        "the player's wool stays on record: {facts:?}"
    );
}
