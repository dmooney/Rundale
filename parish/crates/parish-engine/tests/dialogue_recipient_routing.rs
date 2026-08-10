//! Real-loop regression coverage for explicit dialogue recipients.

use parish_core::npc::types::NpcState;
use parish_core::world::events::GameEvent;
use parish_engine::testing::GameTestHarness;

fn drain(rx: &mut tokio::sync::broadcast::Receiver<GameEvent>) -> Vec<GameEvent> {
    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(_) => return events,
        }
    }
}

fn setup(
    padraig_present: bool,
) -> (
    GameTestHarness,
    parish_core::npc::NpcId,
    parish_core::npc::NpcId,
) {
    let mut harness = GameTestHarness::new();
    let player_location = harness.app.world.player_location;
    let elsewhere = harness
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|location| *location != player_location)
        .expect("Rundale has more than one location");

    let seamus_id = harness
        .app
        .npc_manager
        .all_npcs()
        .find(|npc| npc.name == "Seamus Gallagher")
        .map(|npc| npc.id)
        .expect("Rundale contains Seamus Gallagher");
    let padraig_id = harness
        .app
        .npc_manager
        .all_npcs()
        .find(|npc| npc.name == "Padraig Darcy")
        .map(|npc| npc.id)
        .expect("Rundale contains Padraig Darcy");

    let npc_ids = harness
        .app
        .npc_manager
        .all_npcs()
        .map(|npc| npc.id)
        .collect::<Vec<_>>();
    for npc_id in npc_ids {
        let npc = harness
            .app
            .npc_manager
            .get_mut(npc_id)
            .expect("NPC id remains valid");
        if npc_id == seamus_id || padraig_present && npc_id == padraig_id {
            npc.set_location_and_state(player_location, NpcState::Present);
        } else {
            npc.set_location(elsewhere);
        }
    }
    harness.app.npc_manager.mark_introduced(seamus_id);
    harness.app.npc_manager.mark_introduced(padraig_id);

    (harness, seamus_id, padraig_id)
}

#[test]
fn explicit_talk_recipient_is_not_confused_with_absent_question_subject() {
    let (mut harness, seamus_id, padraig_id) = setup(false);

    harness.mock().push_for(
        "Seamus Gallagher",
        "Aye, Padraig keeps the public house at the crossroads.".to_string(),
    );
    let mut receiver = harness.app.world.event_bus.subscribe();

    let emitted =
        harness.execute_via_real_loop("talk to Seamus Gallagher about Where is Padraig Darcy?");
    let game_events = drain(&mut receiver);

    assert!(
        game_events.iter().any(|event| matches!(
            event,
            GameEvent::DialogueOccurred { npc_id, .. } if *npc_id == seamus_id
        )),
        "the explicit recipient should answer; emitted={emitted:?}, game_events={game_events:?}"
    );
    assert!(
        !game_events.iter().any(|event| matches!(
            event,
            GameEvent::AddressedAbsentNpc { name, .. } if name == "Padraig Darcy"
        )),
        "the question's subject is not an addressed recipient; game_events={game_events:?}"
    );
    assert!(
        !emitted.iter().any(|(name, payload)| {
            name == "text-log"
                && payload
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|content| content == "Padraig Darcy is not here.")
        }),
        "the player must not be told that the question's subject was addressed; emitted={emitted:?}"
    );

    assert_ne!(seamus_id, padraig_id);
}

#[test]
fn explicit_talk_recipient_does_not_make_present_question_subject_reply() {
    let (mut harness, seamus_id, padraig_id) = setup(true);
    harness.mock().push_for(
        "Seamus Gallagher",
        "Aye, Padraig keeps the public house at the crossroads.".to_string(),
    );
    harness.mock().push_for(
        "Padraig Darcy",
        "I am standing right here, if it is me ye mean.".to_string(),
    );
    let mut receiver = harness.app.world.event_bus.subscribe();

    let emitted =
        harness.execute_via_real_loop("talk to Seamus Gallagher about Where is Padraig Darcy?");
    let game_events = drain(&mut receiver);
    let speakers = game_events
        .iter()
        .filter_map(|event| match event {
            GameEvent::DialogueOccurred { npc_id, .. } => Some(*npc_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        speakers,
        vec![seamus_id],
        "only the explicit recipient should answer; emitted={emitted:?}, game_events={game_events:?}"
    );
    assert!(
        !speakers.contains(&padraig_id),
        "a person mentioned as the question's subject must not become a second speaker"
    );
}
