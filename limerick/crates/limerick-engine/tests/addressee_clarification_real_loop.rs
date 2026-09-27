//! Real-loop coverage for addressee clarification (#2034): an explicit
//! addressee that matches several people present is asked about instead of
//! being reported absent, and a leading vocative addresses that person
//! instead of whoever is first.

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

#[test]
fn an_ambiguous_explicit_addressee_is_asked_about_not_reported_absent() {
    let (mut harness, _, duffy) = harness_with_two_micheals();
    let mut receiver = harness.app.world.event_bus.subscribe();

    let emitted = harness.execute_via_real_loop("talk to Mícheál about the harvest");
    let events = drain(&mut receiver);

    let lines = system_lines(&emitted);
    assert!(
        lines
            .contains(&"Which Mícheál do you mean: Mícheál Connolly or Mícheál Duffy?".to_string()),
        "the question and its choices are shown; emitted={emitted:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("not here")),
        "an ambiguous addressee is not reported absent; lines={lines:?}"
    );
    assert!(speakers(&events).is_empty(), "no one speaks: {events:?}");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameEvent::AddressedAbsentNpc { .. }))
    );

    // Asking does not leave the conversation claimed (#1379): the next,
    // unambiguous address is answered.
    harness
        .mock()
        .push_for("Mícheál Duffy", "The harvest is middling.".to_string());
    let emitted = harness.execute_via_real_loop("talk to Mícheál Duffy about the harvest");
    let events = drain(&mut receiver);
    assert_eq!(speakers(&events), vec![duffy], "emitted={emitted:?}");
}

#[test]
fn a_leading_role_vocative_addresses_that_person_not_whoever_is_first() {
    let (mut harness, ids) = harness_with(&["Padraig Darcy", "Peig Hannigan"]);
    let (padraig, peig) = (ids[0], ids[1]);
    harness
        .mock()
        .push_for("Peig Hannigan", "Divil a bit of news, a stór.".to_string());
    harness
        .mock()
        .push_for("Padraig Darcy", "Ask the widow, not me.".to_string());
    let mut receiver = harness.app.world.event_bus.subscribe();

    let emitted = harness.execute_via_real_loop("Widow, any news?");
    let events = drain(&mut receiver);

    assert_eq!(
        speakers(&events),
        vec![peig],
        "the widow answers; emitted={emitted:?}"
    );
    assert!(!speakers(&events).contains(&padraig));
}

#[test]
fn with_the_flag_off_the_previous_behaviour_is_kept() {
    let (mut harness, _, _) = harness_with_two_micheals();
    harness.app.flags.disable(ADDRESSEE_CLARIFICATION_FLAG);

    let emitted = harness.execute_via_real_loop("talk to Mícheál about the harvest");

    let lines = system_lines(&emitted);
    assert!(
        lines.contains(&"Mícheál is not here.".to_string()),
        "lines={lines:?}"
    );
    assert!(!lines.iter().any(|line| line.starts_with("Which ")));
}
