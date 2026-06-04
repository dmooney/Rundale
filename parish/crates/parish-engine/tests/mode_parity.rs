//! Mode-parity golden test (#1172).
//!
//! A single dialogue turn must publish an identical `GameEvent` stream no matter
//! which turn path drives it. This guards the shared
//! `parish_core::game_session::apply_npc_dialogue_turn` seam (#1173) against the
//! harness/headless drift that produced #1028, #1035 and #1077/#1079 — where the
//! legacy "talk to <name>" path applied Tier-1 state but never published
//! `DialogueOccurred`.
//!
//! The test drives the *same* deterministic input through two real turn paths —
//! the legacy `GameTestHarness` router (`execute`) and the real
//! `parish_core::game_loop` (`execute_via_real_loop`, with a mock-backed
//! inference worker) — collects the `DialogueOccurred` events each publishes on
//! `world.event_bus`, and asserts they match modulo incidental ids/timestamps.
//! The third path (headless `apply_npc_response`) routes through the same seam
//! function; it is exercised live by `play_1172-1173-dialogue-seam.txt`.

use std::collections::BTreeSet;

use parish_core::npc::NpcId;
use parish_core::npc::types::NpcState;
use parish_core::persistence::snapshot::GameSnapshot;
use parish_core::world::LocationId;
use parish_core::world::events::GameEvent;
use parish_engine::testing::GameTestHarness;

/// Drains every event currently buffered on a broadcast receiver. Lagged events
/// are skipped (the buffer never overflows for a single turn); Empty/Closed end
/// the drain.
fn drain(rx: &mut tokio::sync::broadcast::Receiver<GameEvent>) -> Vec<GameEvent> {
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(ev) => out.push(ev),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
    out
}

/// Recursively strips run-to-run incidental keys so two semantically-equal
/// events compare equal.
fn strip_incidental(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            map.remove("timestamp");
            map.remove("request_id");
            for child in map.values_mut() {
                strip_incidental(child);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_incidental),
        _ => {}
    }
}

/// The set of `DialogueOccurred` events in an event stream, normalized to
/// incidental-free JSON. A `BTreeSet` makes the comparison order-independent and
/// idempotent — so the test is immune to the shadow wrapper (when
/// `PARISH_HARNESS_SHADOW` is set) replaying an identical event during
/// `execute`.
fn dialogue_events(events: &[GameEvent]) -> BTreeSet<String> {
    events
        .iter()
        .filter(|e| matches!(e, GameEvent::DialogueOccurred { .. }))
        .map(|e| {
            let mut v = serde_json::to_value(e).expect("GameEvent serializes");
            strip_incidental(&mut v);
            serde_json::to_string(&v).expect("value serializes")
        })
        .collect()
}

fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or(s)
}

#[test]
fn dialogue_turn_publishes_identical_event_across_legacy_and_real_loop() {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    // Co-locate exactly one known NPC with the player, Present, so both paths
    // resolve the same speaker deterministically regardless of the mod schedule.
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
        npc.location = player_loc;
        npc.state = NpcState::Present;
        npc.name.clone()
    };
    h.app.npc_manager.mark_introduced(speaker_id);

    // Move every other co-located NPC away so "talk to <name>" (and any
    // no-target fallback) can only resolve to our speaker.
    let other_loc = h
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|l| *l != player_loc)
        .expect("graph has at least two locations");
    let others: Vec<NpcId> = h
        .app
        .npc_manager
        .all_npcs()
        .filter(|n| n.location == player_loc && n.id != speaker_id)
        .map(|n| n.id)
        .collect();
    for id in others {
        if let Some(n) = h.app.npc_manager.get_mut(id) {
            n.location = other_loc;
        }
    }

    let input = format!("talk to {speaker_name} about the harvest");
    let reply = "A fair evening to ye, and the harvest looks kind this year.";

    // Shared pre-state so both paths run from byte-identical world + NPC state.
    let pre = GameSnapshot::capture(&h.app.world, &h.app.npc_manager);

    // --- Path A: legacy harness router (the addressed `talk to` path) ---
    h.add_canned_response(&speaker_name, reply);
    let mut rx_a = h.app.world.event_bus.subscribe();
    let _ = h.execute(&input);
    let legacy = dialogue_events(&drain(&mut rx_a));

    // Restore the pre-state so the real loop sees the same world.
    pre.restore(&mut h.app.world, &mut h.app.npc_manager);

    // --- Path B: real game_loop, mock-backed inference ---
    h.mock().push_for(first_word(&speaker_name), reply);
    let mut rx_b = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop(&input);
    let real = dialogue_events(&drain(&mut rx_b));

    assert!(
        !real.is_empty(),
        "real game_loop must publish a DialogueOccurred for `{input}`; got none"
    );
    assert!(
        real.iter().any(|e| e.contains("harvest looks kind")),
        "the real loop's dialogue event must carry the scripted reply (not a \
         fallback/empty turn); got {real:?}"
    );
    assert_eq!(
        legacy, real,
        "legacy harness and real game_loop must publish the same DialogueOccurred \
         stream for `{input}`\n  legacy = {legacy:?}\n  real   = {real:?}"
    );
}

/// C6: the comparison the parity test relies on must flag a path that drops the
/// `DialogueOccurred` event (the exact pre-fix addressed-path bug), and must NOT
/// flag two events that differ only in incidental ids/timestamps.
#[test]
fn parity_comparison_catches_a_dropped_dialogue_event() {
    let ts = chrono::Utc::now();
    let event = |req_id, summary: &str| GameEvent::DialogueOccurred {
        npc_id: NpcId(1),
        location: LocationId(1),
        summary: summary.to_string(),
        player_said: Some("talk to Peig about the harvest".to_string()),
        npc_said: Some(summary.to_string()),
        request_id: Some(req_id),
        timestamp: ts,
    };

    let with_event = vec![event(7, "A fair evening to ye.")];
    let without_event: Vec<GameEvent> = vec![];

    assert_ne!(
        dialogue_events(&with_event),
        dialogue_events(&without_event),
        "a path that drops DialogueOccurred must register as a divergence"
    );

    // Same content, different incidental request_id → must normalize equal so the
    // parity test never flags a false positive on the live req-id counter.
    let same_content_other_id = vec![event(999, "A fair evening to ye.")];
    assert_eq!(
        dialogue_events(&with_event),
        dialogue_events(&same_content_other_id),
        "events differing only in request_id/timestamp must compare equal"
    );
}
