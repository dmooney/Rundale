//! Real-loop proof for typed dialogue grounding (#1839, #1840, #1841).
//!
//! Drives `execute_via_real_loop`, mocking only completed provider responses.

use parish_core::npc::NpcId;
use parish_core::npc::types::NpcState;
use parish_engine::testing::GameTestHarness;

const SAFE_FALLBACK: &str = parish_core::npc::INVALID_DIALOGUE_FALLBACK;

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

fn dialogue_json(dialogue: &str) -> String {
    serde_json::json!({
        "dialogue": dialogue,
        "action": "points down the road",
        "mood": "certain",
        "assigned_task": "Follow the invented directions"
    })
    .to_string()
}

fn streamed_text(events: &[(String, serde_json::Value)]) -> String {
    events
        .iter()
        .filter(|(name, _)| name == "stream-token")
        .filter_map(|(_, payload)| payload.get("token").and_then(serde_json::Value::as_str))
        .collect()
}

fn force_turn(harness: &mut GameTestHarness, speaker: &str, input: &str, reply: &str) -> String {
    harness.mock().push_json_for(speaker, dialogue_json(reply));
    let events = harness.execute_via_real_loop(&format!("talk to {speaker} about {input}"));
    let rendered = streamed_text(&events);
    let serialized = serde_json::to_string(&events).expect("UI events serialize");
    assert!(
        !serialized.contains(reply),
        "rejected provider candidate must never enter UI events: {serialized}"
    );
    rendered
}

#[test]
fn cormac_and_ruined_abbey_followups_remain_quarantined_across_turns() {
    let (mut harness, speaker_id, speaker) = harness_with_speaker("Padraig Darcy");
    let original_mood = harness
        .app
        .npc_manager
        .get(speaker_id)
        .unwrap()
        .mood
        .clone();
    let original_tasks = harness.app.world.player_progress.tasks().len();

    assert_eq!(
        force_turn(
            &mut harness,
            &speaker,
            "Have you seen my cousin Cormac Finn?",
            "Aye, I've seen yer cousin. He was here earlier."
        ),
        SAFE_FALLBACK
    );
    assert_eq!(
        force_turn(
            &mut harness,
            &speaker,
            "Where did he go?",
            "He made for the crossroads, as if in a hurry."
        ),
        SAFE_FALLBACK
    );

    harness.mock().push_json_for(
        &speaker,
        serde_json::json!({
            "dialogue": "I know of no such abbey in this parish.",
            "action": "",
            "mood": original_mood.clone(),
            "assigned_task": null
        })
        .to_string(),
    );
    let denial = harness.execute_via_real_loop(&format!(
        "talk to {speaker} about Is there a ruined abbey nearby?"
    ));
    assert!(streamed_text(&denial).contains("no such abbey"));
    assert_eq!(
        force_turn(
            &mut harness,
            &speaker,
            "How do I reach it?",
            "The ruins are but a walk to the south past the old church; keep your eyes open for the stones swallowed by ivy."
        ),
        SAFE_FALLBACK
    );

    assert_eq!(
        harness.app.npc_manager.get(speaker_id).unwrap().mood,
        original_mood
    );
    assert_eq!(
        harness.app.world.player_progress.tasks().len(),
        original_tasks
    );
    let memories = harness
        .app
        .npc_manager
        .get(speaker_id)
        .unwrap()
        .memory
        .recent(20);
    assert!(memories.iter().all(|memory| {
        !memory.content.contains("seen yer cousin")
            && !memory.content.contains("made for the crossroads")
            && !memory.content.contains("stones swallowed")
            && !memory.content.contains("points down the road")
            && !memory.content.contains("invented directions")
    }));
}

#[test]
fn festival_role_and_geography_completions_are_rejected_by_real_loop() {
    let (mut harness, _speaker_id, speaker) = harness_with_speaker("Peig Hannigan");
    for (input, reply) in [
        (
            "Is the well blessed?",
            "'Tis said 'tis blessed on this day, Saint Brigid's feast, and can heal sore eyes and more.",
        ),
        (
            "Where is the blacksmith?",
            "Ye want the blacksmith, go the lane to the forge. Ye'll find Padraig Darcy there.",
        ),
        (
            "Where is Darcy's Pub?",
            "Ye'll find it at Darcy's Pub, in Curraghboy Village.",
        ),
    ] {
        assert_eq!(
            force_turn(&mut harness, &speaker, input, reply),
            SAFE_FALLBACK
        );
    }
}
