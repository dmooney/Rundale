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
use parish_types::TaskStatus;

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

fn player_task_events(events: &[GameEvent]) -> Vec<String> {
    let mut normalized: Vec<String> = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                GameEvent::PlayerTaskAssigned { .. } | GameEvent::PlayerTaskProgressed { .. }
            )
        })
        .map(|event| {
            let mut value = serde_json::to_value(event).expect("GameEvent serializes");
            strip_incidental(&mut value);
            serde_json::to_string(&value).expect("value serializes")
        })
        .collect();
    normalized.sort();
    normalized
}

fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or(s)
}

fn isolate_one_speaker(harness: &mut GameTestHarness) -> (NpcId, String) {
    let player_location = harness.app.world.player_location;
    let speaker_id = harness
        .app
        .npc_manager
        .all_npcs()
        .map(|npc| npc.id)
        .min_by_key(|id| id.0)
        .expect("harness loads at least one NPC");
    let speaker_name = {
        let npc = harness
            .app
            .npc_manager
            .get_mut(speaker_id)
            .expect("speaker exists");
        npc.set_location_and_state(player_location, NpcState::Present);
        npc.name.clone()
    };
    harness.app.npc_manager.mark_introduced(speaker_id);

    let other_location = harness
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|location| *location != player_location)
        .expect("graph has at least two locations");
    let others: Vec<NpcId> = harness
        .app
        .npc_manager
        .all_npcs()
        .filter(|npc| npc.location() == player_location && npc.id != speaker_id)
        .map(|npc| npc.id)
        .collect();
    for id in others {
        if let Some(npc) = harness.app.npc_manager.get_mut(id) {
            npc.set_location(other_location);
        }
    }

    (speaker_id, speaker_name)
}

fn isolate_one_unintroduced_speaker(harness: &mut GameTestHarness) -> (NpcId, String, String) {
    let player_location = harness.app.world.player_location;
    let speaker_id = harness
        .app
        .npc_manager
        .all_npcs()
        .map(|npc| npc.id)
        .min_by_key(|id| id.0)
        .expect("harness loads at least one NPC");
    let (speaker_name, occupation) = {
        let npc = harness
            .app
            .npc_manager
            .get_mut(speaker_id)
            .expect("speaker exists");
        npc.set_location_and_state(player_location, NpcState::Present);
        (npc.name.clone(), npc.occupation.clone())
    };

    let other_location = harness
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|location| *location != player_location)
        .expect("graph has at least two locations");
    let others: Vec<NpcId> = harness
        .app
        .npc_manager
        .all_npcs()
        .filter(|npc| npc.location() == player_location && npc.id != speaker_id)
        .map(|npc| npc.id)
        .collect();
    for id in others {
        if let Some(npc) = harness.app.npc_manager.get_mut(id) {
            npc.set_location(other_location);
        }
    }

    (speaker_id, speaker_name, occupation)
}

#[test]
fn dialogue_turn_publishes_identical_event_across_legacy_and_real_loop() {
    let mut h = GameTestHarness::new();
    let (_speaker_id, speaker_name) = isolate_one_speaker(&mut h);

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

#[test]
fn unique_first_name_and_occupation_reveals_identity_across_turn_modes() {
    let mut legacy_harness = GameTestHarness::new();
    let (legacy_id, speaker_name, occupation) =
        isolate_one_unintroduced_speaker(&mut legacy_harness);
    let first_name = first_word(&speaker_name);
    let input = format!("talk to {speaker_name} about the forge");
    let reply = format!("I'm {first_name}, the {occupation}.");
    let roster: Vec<(String, String)> = legacy_harness
        .app
        .npc_manager
        .all_npcs()
        .map(|npc| (npc.name.clone(), npc.occupation.clone()))
        .collect();
    assert!(parish_core::npc::dialogue_self_identifies_speaker(
        &reply,
        &speaker_name,
        &occupation,
        &roster,
    ));

    legacy_harness.add_canned_response(&speaker_name, &reply);
    let _ = legacy_harness.execute(&input);
    assert!(
        legacy_harness.app.npc_manager.is_introduced(legacy_id),
        "legacy harness route must commit the identity transition"
    );

    let mut real_harness = GameTestHarness::new();
    let (real_id, real_name, real_occupation) = isolate_one_unintroduced_speaker(&mut real_harness);
    assert_eq!(
        (real_name, real_occupation),
        (speaker_name.clone(), occupation)
    );
    real_harness.mock().push_for(first_name, &reply);
    let ui_events = real_harness.execute_via_real_loop(&input);
    assert!(
        real_harness.app.npc_manager.is_introduced(real_id),
        "real game-loop route must commit the same identity transition; UI events: {ui_events:?}"
    );
}

#[test]
fn rejected_anachronistic_candidate_has_identical_fallback_across_modes() {
    let mut harness = GameTestHarness::new();
    let (_speaker_id, speaker_name) = isolate_one_speaker(&mut harness);
    let input = "I'll take the work. What would you have me do first?".to_string();
    let response = serde_json::json!({
        "dialogue": "Council says the planning board has set tongues.",
        "action": "waves a notice",
        "mood": "delighted",
        "assigned_task": "Attend the agricultural show committee"
    })
    .to_string();
    let pre = GameSnapshot::capture(&harness.app.world, &harness.app.npc_manager);

    harness.add_canned_response(&speaker_name, &response);
    let mut legacy_rx = harness.app.world.event_bus.subscribe();
    let _ = harness.execute(&input);
    let legacy_stream = drain(&mut legacy_rx);
    let legacy = dialogue_events(&legacy_stream);
    let legacy_tasks = player_task_events(&legacy_stream);
    assert!(harness.app.world.player_progress.is_empty());

    pre.restore(&mut harness.app.world, &mut harness.app.npc_manager);
    harness
        .mock()
        .push_json_for(first_word(&speaker_name), &response);
    let mut real_rx = harness.app.world.event_bus.subscribe();
    let ui_events = harness.execute_via_real_loop(&input);
    let real_stream = drain(&mut real_rx);
    let real = dialogue_events(&real_stream);
    let real_tasks = player_task_events(&real_stream);

    assert_eq!(legacy, real);
    assert_eq!(legacy_tasks, real_tasks);
    assert!(real_tasks.is_empty());
    assert!(harness.app.world.player_progress.is_empty());
    assert!(real.iter().all(|event| !event.contains("planning board")));
    assert!(
        real.iter()
            .any(|event| event.contains("I beg your pardon; I lost the thread of that."))
    );
    assert!(
        serde_json::to_string(&ui_events)
            .unwrap()
            .find("planning board")
            .is_none()
    );
}

#[test]
fn authored_landmark_rejection_has_identical_fallback_across_modes() {
    let mut harness = GameTestHarness::new();
    let (speaker_id, speaker_name) = isolate_one_speaker(&mut harness);
    let kilteevan = harness
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|id| {
            harness
                .app
                .world
                .graph
                .get(*id)
                .is_some_and(|location| location.name == "Kilteevan Village")
        })
        .expect("Rundale contains Kilteevan Village");
    harness.app.world.player_location = kilteevan;
    harness
        .app
        .npc_manager
        .get_mut(speaker_id)
        .unwrap()
        .set_location_and_state(kilteevan, NpcState::Present);
    let input = format!(
        "talk to {speaker_name} about Is there an old bridge in Kilteevan Village?"
    );
    let raw = "There is no old bridge in Kilteevan that I have ever heard tell of.";
    let response = serde_json::json!({
        "dialogue": raw,
        "action": "points away from the stream",
        "mood": "certain",
        "assigned_task": "Search elsewhere"
    })
    .to_string();
    let pre = GameSnapshot::capture(&harness.app.world, &harness.app.npc_manager);

    harness.add_canned_response(&speaker_name, &response);
    let mut legacy_rx = harness.app.world.event_bus.subscribe();
    let _ = harness.execute(&input);
    let legacy = dialogue_events(&drain(&mut legacy_rx));

    pre.restore(&mut harness.app.world, &mut harness.app.npc_manager);
    harness
        .mock()
        .push_json_for(first_word(&speaker_name), &response);
    let mut real_rx = harness.app.world.event_bus.subscribe();
    let ui_events = harness.execute_via_real_loop(&input);
    let real = dialogue_events(&drain(&mut real_rx));

    assert_eq!(legacy, real);
    assert_eq!(real.len(), 1);
    let event = real.iter().next().unwrap();
    assert!(event.contains("I beg your pardon; I lost the thread of that."));
    for rejected in [raw, "points away", "Search elsewhere"] {
        assert!(!event.contains(rejected));
        assert!(!serde_json::to_string(&ui_events).unwrap().contains(rejected));
    }
}

#[test]
fn incomplete_multifacet_reply_has_identical_obligation_fallback_across_modes() {
    let mut harness = GameTestHarness::new();
    let (_speaker_id, speaker_name) = isolate_one_speaker(&mut harness);
    // Peig is a known authored parish person even when not co-located.
    let request =
        "Peig Hannigan sent me. I'm Aiden Carney, seeking honest work and somewhere dry to sleep.";
    let input = format!("talk to {speaker_name} about {request}");
    let raw = "'Tis a fine morning indeed. What brings ye here?";
    let response = serde_json::json!({
        "dialogue": raw,
        "action": "offers a room key",
        "mood": "delighted",
        "assigned_task": "Start work tomorrow"
    })
    .to_string();
    let pre = GameSnapshot::capture(&harness.app.world, &harness.app.npc_manager);

    harness.add_canned_response(&speaker_name, &response);
    let mut legacy_rx = harness.app.world.event_bus.subscribe();
    let _ = harness.execute(&input);
    let legacy = dialogue_events(&drain(&mut legacy_rx));

    pre.restore(&mut harness.app.world, &mut harness.app.npc_manager);
    harness
        .mock()
        .push_json_for(first_word(&speaker_name), &response);
    let mut real_rx = harness.app.world.event_bus.subscribe();
    let ui_events = harness.execute_via_real_loop(&input);
    let real = dialogue_events(&drain(&mut real_rx));

    assert_eq!(legacy, real);
    assert_eq!(real.len(), 1);
    let event = real.iter().next().unwrap();
    for required in ["Peig Hannigan", "Aiden Carney", "work", "lodging"] {
        assert!(event.contains(required), "missing {required}: {event}");
    }
    for rejected in [raw, "room key", "Start work tomorrow"] {
        assert!(!event.contains(rejected));
        assert!(
            !serde_json::to_string(&ui_events)
                .unwrap()
                .contains(rejected)
        );
    }
}

#[test]
fn typed_unknown_person_followup_has_legacy_real_loop_parity() {
    let mut legacy_harness = GameTestHarness::new();
    let (legacy_id, legacy_speaker) = isolate_one_speaker(&mut legacy_harness);
    let mut real_harness = GameTestHarness::new();
    let (real_id, real_speaker) = isolate_one_speaker(&mut real_harness);
    assert_eq!(legacy_speaker, real_speaker);

    let turns = [
        (
            "Have you seen my cousin Cormac Finn?",
            "Aye, I've seen yer cousin. He was here earlier.",
        ),
        (
            "Where did he go?",
            "He made for the crossroads, as if in a hurry.",
        ),
    ];
    let mut legacy_events = BTreeSet::new();
    let mut real_events = BTreeSet::new();
    for (input, reply) in turns {
        let legacy_location = legacy_harness.app.world.player_location;
        legacy_harness
            .app
            .npc_manager
            .get_mut(legacy_id)
            .expect("legacy speaker exists")
            .set_location_and_state(legacy_location, NpcState::Present);
        let real_location = real_harness.app.world.player_location;
        real_harness
            .app
            .npc_manager
            .get_mut(real_id)
            .expect("real speaker exists")
            .set_location_and_state(real_location, NpcState::Present);
        let response = serde_json::json!({
            "dialogue": reply,
            "action": "points away",
            "mood": "certain",
            "assigned_task": "Follow Cormac"
        })
        .to_string();

        legacy_harness.add_canned_response(&legacy_speaker, &response);
        let mut legacy_rx = legacy_harness.app.world.event_bus.subscribe();
        let _ = legacy_harness.execute(&format!("talk to {legacy_speaker} about {input}"));
        legacy_events.extend(dialogue_events(&drain(&mut legacy_rx)));

        real_harness
            .mock()
            .push_json_for(first_word(&real_speaker), &response);
        let mut real_rx = real_harness.app.world.event_bus.subscribe();
        let ui_events =
            real_harness.execute_via_real_loop(&format!("talk to {real_speaker} about {input}"));
        assert!(!serde_json::to_string(&ui_events).unwrap().contains(reply));
        real_events.extend(dialogue_events(&drain(&mut real_rx)));
    }

    assert_eq!(legacy_events, real_events);
    assert_eq!(legacy_events.len(), 2);
    assert!(legacy_events.iter().all(|event| {
        event.contains(parish_core::npc::INVALID_DIALOGUE_FALLBACK)
            && !event.contains("seen yer cousin")
            && !event.contains("made for the crossroads")
            && !event.contains("points away")
            && !event.contains("Follow Cormac\"")
    }));
}

#[test]
fn grounded_task_assignment_is_identical_in_legacy_and_real_loops() {
    let mut harness = GameTestHarness::new();
    let (speaker_id, speaker_name) = isolate_one_speaker(&mut harness);
    let input = "I'll take the work. What would you have me do first?".to_string();
    let response = serde_json::json!({
        "dialogue": "First, help with the potato patch — break the clods and plant seed.",
        "action": "points toward the field",
        "mood": "busy",
        "language_hints": [],
        "assigned_task": "Break the clods and plant seed in the potato patch.",
        "internal_thought": null
    })
    .to_string();
    let pre = GameSnapshot::capture(&harness.app.world, &harness.app.npc_manager);

    harness.add_canned_response(&speaker_name, &response);
    let mut legacy_rx = harness.app.world.event_bus.subscribe();
    let _ = harness.execute(&input);
    let legacy_events = player_task_events(&drain(&mut legacy_rx));
    let legacy_task = harness
        .app
        .world
        .player_progress
        .active_tasks()
        .next()
        .cloned()
        .expect("legacy path assigns the grounded task");
    assert_eq!(
        legacy_task.description,
        "Break the clods and plant seed in the potato patch."
    );
    assert_eq!(legacy_task.assigned_by, speaker_id);

    pre.restore(&mut harness.app.world, &mut harness.app.npc_manager);
    harness
        .mock()
        .push_json_for(first_word(&speaker_name), &response);
    let mut real_rx = harness.app.world.event_bus.subscribe();
    let _ = harness.execute_via_real_loop(&input);
    let real_events = player_task_events(&drain(&mut real_rx));
    let real_task = harness
        .app
        .world
        .player_progress
        .active_tasks()
        .next()
        .cloned()
        .expect("real loop assigns the grounded task");

    assert_eq!(
        legacy_events.len(),
        1,
        "legacy path must publish exactly one assignment event: {legacy_events:?}"
    );
    assert_eq!(legacy_task, real_task);
    assert_eq!(
        legacy_events, real_events,
        "legacy harness and real game loop must publish identical task assignment events"
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

#[test]
fn potato_patch_action_progresses_identically_in_legacy_and_real_loops() {
    const ACTION: &str = "I take up a spade, break the clods in the potato patch, and plant the seed as Siobhan instructed.";

    let mut harness = GameTestHarness::new();
    let location = harness.app.world.player_location;
    let assigned_at = harness.app.world.clock.now();
    let task_id = harness
        .app
        .world
        .player_progress
        .assign_task(
            "Break the clods and plant seed in the potato patch.",
            NpcId(7),
            location,
            assigned_at,
        )
        .expect("seed exact potato-patch assignment");
    let pre = GameSnapshot::capture(&harness.app.world, &harness.app.npc_manager);

    let mut legacy_rx = harness.app.world.event_bus.subscribe();
    let _ = harness.execute(ACTION);
    let legacy_events = player_task_events(&drain(&mut legacy_rx));
    assert_eq!(
        harness
            .app
            .world
            .player_progress
            .task(task_id)
            .expect("legacy task remains")
            .status,
        TaskStatus::InProgress,
        "legacy harness action path must start the assigned task"
    );

    pre.restore(&mut harness.app.world, &mut harness.app.npc_manager);
    let mut real_rx = harness.app.world.event_bus.subscribe();
    let _ = harness.execute_via_real_loop(ACTION);
    let real_events = player_task_events(&drain(&mut real_rx));
    assert_eq!(
        harness
            .app
            .world
            .player_progress
            .task(task_id)
            .expect("real-loop task remains")
            .status,
        TaskStatus::InProgress,
        "real game-loop action path must start the assigned task"
    );

    assert_eq!(
        legacy_events.len(),
        1,
        "legacy path must publish exactly one semantic task transition: {legacy_events:?}"
    );
    assert_eq!(
        legacy_events, real_events,
        "legacy harness and real game loop must publish identical task progression events"
    );
}
