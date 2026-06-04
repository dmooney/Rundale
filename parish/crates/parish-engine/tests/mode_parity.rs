//! Golden tests for mode-parity of dialogue turn side effects.
//!
//! These tests compare the deterministic script-harness dialogue path against
//! the real `parish_core::game_loop` path with inference pinned to
//! `MockClient`. They focus on semantic `GameEvent`s and shared state, not UI
//! stream-token transport details.

use parish_engine::testing::{ActionResult, GameTestHarness};
use parish_engine::world::events::GameEvent;
use parish_types::ConversationLog;
use serde_json::Value;

const PLAYER_INPUT: &str = "talk to Padraig Darcy about my name is Brigid.";
const PADRAIG_REPLY: &str = "A fair welcome to you, Brigid.";

fn harness_at_pub() -> GameTestHarness {
    let mut h = GameTestHarness::new();
    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");
    assert_eq!(h.player_location(), "Darcy's Pub");
    let npcs = h.npcs_here();
    assert!(
        npcs.contains(&"Padraig Darcy"),
        "Padraig should be present at the pub for the parity fixture; got {npcs:?}"
    );
    h
}

fn drain_dialogue_events(rx: &mut tokio::sync::broadcast::Receiver<GameEvent>) -> Vec<GameEvent> {
    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(event @ GameEvent::DialogueOccurred { .. }) => events.push(event),
            Ok(_) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
        }
    }
    events
}

fn canonical_dialogue_event(event: &GameEvent) -> Value {
    let mut value = serde_json::to_value(event).expect("GameEvent serializes");
    if let Value::Object(map) = &mut value {
        map.remove("request_id");
        map.remove("timestamp");
    }
    value
}

fn canonical_dialogue_events(events: &[GameEvent]) -> Vec<Value> {
    events.iter().map(canonical_dialogue_event).collect()
}

fn strip_incidental_timestamp(value: &mut Value) {
    if let Value::Object(map) = value {
        map.remove("timestamp");
    }
}

fn canonical_conversation_log(log: &ConversationLog) -> Vec<Value> {
    log.all()
        .map(|exchange| {
            let mut value =
                serde_json::to_value(exchange).expect("ConversationExchange serializes");
            strip_incidental_timestamp(&mut value);
            value
        })
        .collect()
}

fn canonical_memories(
    entries: impl Iterator<Item = parish_engine::npc::memory::MemoryEntry>,
) -> Vec<Value> {
    entries
        .map(|entry| {
            let mut value = serde_json::to_value(entry).expect("MemoryEntry serializes");
            strip_incidental_timestamp(&mut value);
            value
        })
        .collect()
}

#[test]
fn mode_parity_dialogue_event_stream_matches_across_harness_and_real_loop() {
    let mut legacy = harness_at_pub();
    legacy.add_canned_response("Padraig Darcy", PADRAIG_REPLY);
    let mut legacy_rx = legacy.app.world.event_bus.subscribe();

    let legacy_result = legacy.execute(PLAYER_INPUT);
    assert!(
        matches!(legacy_result, ActionResult::NpcResponse { .. }),
        "legacy harness should produce a canned NPC response, got {legacy_result:?}"
    );
    let legacy_events = drain_dialogue_events(&mut legacy_rx);

    let mut real = harness_at_pub();
    real.mock().push_for("Padraig Darcy", PADRAIG_REPLY);
    let mut real_rx = real.app.world.event_bus.subscribe();

    let emitted = real.execute_via_real_loop(PLAYER_INPUT);
    assert!(
        emitted.iter().any(|(name, payload)| {
            (name == "text-log" || name == "stream-token")
                && ["content", "token"].iter().any(|field| {
                    payload
                        .get(*field)
                        .and_then(|content| content.as_str())
                        .is_some_and(|content| content.contains(PADRAIG_REPLY))
                })
        }),
        "real loop should emit the scripted dialogue reply; got {emitted:?}"
    );
    let real_events = drain_dialogue_events(&mut real_rx);

    assert_eq!(
        canonical_dialogue_events(&legacy_events),
        canonical_dialogue_events(&real_events),
        "DialogueOccurred event stream diverged between harness and real loop\nlegacy: {legacy_events:#?}\nreal: {real_events:#?}"
    );
}

#[test]
fn mode_parity_dialogue_state_matches_across_harness_and_real_loop() {
    let mut legacy = harness_at_pub();
    legacy.add_canned_response("Padraig Darcy", PADRAIG_REPLY);
    let _ = legacy.execute(PLAYER_INPUT);

    let mut real = harness_at_pub();
    real.mock().push_for("Padraig Darcy", PADRAIG_REPLY);
    let _ = real.execute_via_real_loop(PLAYER_INPUT);

    assert_eq!(legacy.app.world.player_name, real.app.world.player_name);
    assert_eq!(
        canonical_conversation_log(&legacy.app.world.conversation_log),
        canonical_conversation_log(&real.app.world.conversation_log),
        "conversation log diverged between harness and real loop"
    );

    let legacy_padraig = legacy
        .app
        .npc_manager
        .all_npcs()
        .find(|npc| npc.name == "Padraig Darcy")
        .expect("legacy Padraig exists");
    let real_padraig = real
        .app
        .npc_manager
        .all_npcs()
        .find(|npc| npc.name == "Padraig Darcy")
        .expect("real Padraig exists");

    assert_eq!(legacy_padraig.mood, real_padraig.mood);
    assert_eq!(legacy_padraig.memory.len(), real_padraig.memory.len());
    assert_eq!(
        canonical_memories(legacy_padraig.memory.entries().cloned()),
        canonical_memories(real_padraig.memory.entries().cloned()),
        "speaker memory diverged between harness and real loop"
    );
}
