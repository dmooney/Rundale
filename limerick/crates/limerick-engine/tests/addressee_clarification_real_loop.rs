//! Real-loop coverage for recipient resolution on the desktop's in-process
//! path (design `docs/design/dynamic-npc-context.md` §3, #2221): with no
//! System 1 answer, unaddressed speech goes to the partner or asks who it is
//! for, an @tag picks the recipient, and no line names anyone absent.

use limerick_core::game_loop::ADDRESSEE_CLARIFICATION_FLAG;
use limerick_core::npc::NpcId;
use limerick_core::npc::types::NpcState;
use limerick_core::world::events::GameEvent;
use limerick_engine::testing::GameTestHarness;

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

/// Leaves exactly the named NPCs at the player's location.
fn harness_with(names: &[&str]) -> (GameTestHarness, Vec<NpcId>) {
    let mut harness = GameTestHarness::new();
    let here = harness.app.world.player_location;
    let elsewhere = harness
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|location| *location != here)
        .expect("Rundale has more than one location");
    let ids: Vec<NpcId> = names
        .iter()
        .map(|name| {
            harness
                .app
                .npc_manager
                .all_npcs()
                .find(|npc| npc.name == *name)
                .unwrap_or_else(|| panic!("Rundale contains {name}"))
                .id
        })
        .collect();
    let all: Vec<NpcId> = harness
        .app
        .npc_manager
        .all_npcs()
        .map(|npc| npc.id)
        .collect();
    for id in all {
        let npc = harness.app.npc_manager.get_mut(id).unwrap();
        if ids.contains(&id) {
            npc.set_location_and_state(here, NpcState::Present);
        } else {
            npc.set_location(elsewhere);
        }
    }
    (harness, ids)
}

/// Two introduced people present named Mícheál: (Connolly, Duffy).
fn harness_with_two_micheals() -> (GameTestHarness, NpcId, NpcId) {
    let (mut harness, ids) = harness_with(&["Roisin Connolly", "Cormac Duffy"]);
    harness.app.npc_manager.get_mut(ids[0]).unwrap().name = "Mícheál Connolly".to_string();
    harness.app.npc_manager.get_mut(ids[1]).unwrap().name = "Mícheál Duffy".to_string();
    harness.app.npc_manager.mark_introduced(ids[0]);
    harness.app.npc_manager.mark_introduced(ids[1]);
    (harness, ids[0], ids[1])
}

fn system_lines(emitted: &[(String, serde_json::Value)]) -> Vec<String> {
    emitted
        .iter()
        .filter(|(name, payload)| {
            name == "text-log"
                && payload.get("source").and_then(serde_json::Value::as_str) == Some("system")
        })
        .filter_map(|(_, payload)| payload.get("content")?.as_str().map(str::to_string))
        .collect()
}

fn speakers(events: &[GameEvent]) -> Vec<NpcId> {
    events
        .iter()
        .filter_map(|event| match event {
            GameEvent::DialogueOccurred { npc_id, .. } => Some(*npc_id),
            _ => None,
        })
        .collect()
}

/// Design §3.1 and §3.3 on the desktop's in-process path, which has no
/// System 1 answer: unaddressed speech with two people present and no
/// partner asks who it is for, an @tag picks one person, who becomes the
/// partner, and the next unaddressed line goes to the partner.
#[test]
fn unaddressed_speech_asks_and_a_tagged_person_becomes_the_partner() {
    let (mut harness, _, duffy) = harness_with_two_micheals();
    let mut receiver = harness.app.world.event_bus.subscribe();

    let emitted = harness.execute_via_real_loop("What news of the harvest?");
    let events = drain(&mut receiver);

    let lines = system_lines(&emitted);
    assert!(
        lines.contains(
            &"Who are you speaking to: Mícheál Connolly, Mícheál Duffy or All?".to_string()
        ),
        "the question and its choices are shown; emitted={emitted:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("not here")),
        "lines={lines:?}"
    );
    assert!(speakers(&events).is_empty(), "no one speaks: {events:?}");

    // Asking does not leave the conversation claimed (#1379): the next,
    // explicit address is answered.
    harness
        .mock()
        .push_for("Mícheál Duffy", "The harvest is middling.".to_string());
    let emitted = harness.execute_via_real_loop("@Mícheál Duffy what of the harvest?");
    let events = drain(&mut receiver);
    assert_eq!(speakers(&events), vec![duffy], "emitted={emitted:?}");
    assert_eq!(harness.app.npc_manager.partner(), Some(duffy));

    harness
        .mock()
        .push_for("Mícheál Duffy", "Middling, as I said.".to_string());
    let emitted = harness.execute_via_real_loop("And the oats?");
    let events = drain(&mut receiver);
    assert_eq!(
        speakers(&events),
        vec![duffy],
        "the partner answers an unaddressed line; emitted={emitted:?}"
    );
}

#[test]
fn with_the_flag_off_a_line_that_would_ask_reaches_everyone_present() {
    let (mut harness, connolly, duffy) = harness_with_two_micheals();
    harness.app.flags.disable(ADDRESSEE_CLARIFICATION_FLAG);
    harness
        .mock()
        .push_for("Mícheál Connolly", "Fair enough.".to_string());
    harness
        .mock()
        .push_for("Mícheál Duffy", "Middling.".to_string());
    let mut receiver = harness.app.world.event_bus.subscribe();

    let emitted = harness.execute_via_real_loop("What news of the harvest?");
    let events = drain(&mut receiver);

    let lines = system_lines(&emitted);
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("not here") || line.starts_with("Who ")),
        "lines={lines:?}"
    );
    assert_eq!(speakers(&events), vec![connolly, duffy]);
}
