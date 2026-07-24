//! Real-loop integration tests for the P2 cleanup batch (#1476, #1477, #1478,
//! #1491, #1493).
//!
//! Each test drives the **real** `parish_core::game_loop` via
//! [`execute_via_real_loop`] with a mock inference client, then asserts the
//! post-generation guards produce correct player-visible output.
//!
//! # Why `execute_via_real_loop` and not a plain unit test
//!
//! The guards are wired into `run_npc_turn` and `handle_npc_conversation`, which
//! are only called through the real game-loop path. A plain unit test only
//! exercises the guard function itself; these tests verify the full
//! `handle_game_input → handle_npc_conversation → run_npc_turn` chain.

use parish_core::npc::types::NpcState;
use parish_core::world::events::GameEvent;
use parish_engine::testing::GameTestHarness;

/// Helper: drain all events from a broadcast receiver.
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

/// Sets up a harness with exactly one NPC co-located with the player, all
/// other NPCs moved away. Returns `(harness, npc_id, npc_name)`.
fn harness_with_one_npc() -> (GameTestHarness, parish_core::npc::NpcId, String) {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

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

    let other_loc = h
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|l| *l != player_loc)
        .expect("graph has at least two locations");
    let others: Vec<_> = h
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

    (h, speaker_id, speaker_name)
}

// ── #1491 — mood-aware sentence cap ──────────────────────────────────────────

/// AC-1 (#1491, real-loop): When the NPC's canonical starting mood is "busy"
/// and the mock model returns 5 sentences, the player-visible output must be
/// capped at 2 sentences by `guard_verbosity_runons_with_mood`.
///
/// The model's self-reported JSON mood is not authoritative for the current
/// spoken turn (#1779).
#[test]
fn real_loop_busy_mood_caps_at_two_sentences() {
    let (mut h, speaker_id, speaker_name) = harness_with_one_npc();
    h.app
        .npc_manager
        .get_mut(speaker_id)
        .expect("speaker exists")
        .mood = "busy".to_string();

    // Five distinct sentences. The JSON mood deliberately disagrees so this
    // test proves the authored pre-turn state wins.
    let json_reply = r#"{"dialogue": "Aye, I heard ye. The rents are fierce high. The harvest was poor. The landlord takes no pity. God help us all.", "action": "wipes hands on apron", "mood": "friendly", "internal_thought": null, "language_hints": []}"#;
    h.mock().push_json_for(&speaker_name, json_reply);

    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));

    let dialogue_events = drain(&mut rx);
    let shown: Vec<String> = dialogue_events
        .iter()
        .filter_map(|ev| match ev {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.clone(),
            _ => None,
        })
        .collect();

    assert!(
        !shown.is_empty(),
        "expected DialogueOccurred for the NPC turn"
    );

    // Count terminal sentence marks in the player-visible output.
    let joined = shown.join(" ");
    let sentence_marks = joined
        .chars()
        .filter(|c| matches!(c, '.' | '!' | '?'))
        .count();

    // 5-sentence input with busy mood → must be capped at 2 (so ≤ 2 terminal marks).
    assert!(
        sentence_marks <= 2,
        "busy NPC must be capped at 2 sentences in real-loop (#1491); \
         got {sentence_marks} terminal marks: {joined:?}"
    );

    // First sentence survives.
    assert!(
        joined.to_lowercase().contains("heard ye") || joined.to_lowercase().contains("aye"),
        "first sentence must survive the 2-sentence cap: {joined:?}"
    );
}

#[test]
fn real_loop_bitter_mood_is_expressed_in_exact_harness_reply() {
    let (mut h, speaker_id, speaker_name) = harness_with_one_npc();
    h.app
        .npc_manager
        .get_mut(speaker_id)
        .expect("speaker exists")
        .mood = "bitter".to_string();

    let json_reply = r#"{"dialogue": "Aye. Stick to Siobhan's lead, she knows the patch better'n I do. What's your trade?", "action": "nods", "mood": "friendly", "internal_thought": null, "language_hints": []}"#;
    h.mock().push_json_for(&speaker_name, json_reply);

    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));
    let joined = drain(&mut rx)
        .iter()
        .filter_map(|event| match event {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.as_deref(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        joined.starts_with("If ye must know—"),
        "authored bitter mood must be audible even when model metadata says friendly: {joined:?}"
    );
    assert!(
        joined.contains("Stick to Siobhan's lead"),
        "mood correction must preserve the substantive answer: {joined:?}"
    );
}

// ── #1477 — wrong-location reference guard ────────────────────────────────────

/// AC-1 (#1477, real-loop): When the mock model names the wrong settlement in
/// "here in X" collocation, `guard_wrong_location_reference` inside
/// `run_npc_turn` replaces it with the correct location name.
#[test]
fn real_loop_wrong_location_reference_is_corrected() {
    let (mut h, _, speaker_name) = harness_with_one_npc();

    // Get the actual player location name from the world graph.
    let correct_loc = h
        .app
        .world
        .graph
        .get(h.app.world.player_location)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Kilteevan".to_string());

    // Only run the guard assertion when the correct location has a distinct name.
    let wrong_place = if correct_loc.contains("Kilteevan") {
        "Strokestown"
    } else {
        "SomeFabricatedVillage"
    };

    // JSON response naming the wrong settlement.
    let json_reply = format!(
        r#"{{"dialogue": "Aye, here in {wrong_place}, we know how to work the land.", "action": "nods", "mood": "neutral", "internal_thought": null, "language_hints": []}}"#
    );
    h.mock().push_json_for(&speaker_name, json_reply);

    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(&format!("talk to {speaker_name}"));

    let dialogue_events = drain(&mut rx);
    let shown: Vec<String> = dialogue_events
        .iter()
        .filter_map(|ev| match ev {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.clone(),
            _ => None,
        })
        .collect();

    assert!(
        !shown.is_empty(),
        "expected DialogueOccurred for the NPC turn"
    );

    let joined = shown.join(" ");

    assert!(
        !joined.contains(wrong_place),
        "wrong settlement name must be removed from dialogue by guard (#1477); \
         wrong={wrong_place:?}, got: {joined:?}"
    );
    assert!(
        joined.contains(&correct_loc),
        "correct location must appear after guard correction (#1477); \
         correct={correct_loc:?}, got: {joined:?}"
    );
}

// ── #1478 — routing-after-denial guard ────────────────────────────────────────

/// AC-1 (#1478, real-loop): When the mock model denies knowing a fabricated
/// person but then adds a routing phrase, `guard_fabricated_person_routing`
/// inside `run_npc_turn` replaces the full response with a clean decline.
#[test]
fn real_loop_routing_after_denial_is_stripped() {
    let (mut h, _, speaker_name) = harness_with_one_npc();

    // Fabricated full name — not in any Rundale NPC roster.
    let fabricated = "Cormac Donaghue";

    // JSON response: denial + routing phrase (the bug pattern).
    let json_reply = format!(
        r#"{{"dialogue": "I know no such person as {fabricated} in these parts, but you might find him at the old mill.", "action": "shrugs", "mood": "uncertain", "internal_thought": null, "language_hints": []}}"#
    );
    h.mock().push_json_for(&speaker_name, json_reply);

    let mut rx = h.app.world.event_bus.subscribe();
    let _events = h.execute_via_real_loop(&format!("talk to {speaker_name} about {fabricated}"));

    let dialogue_events = drain(&mut rx);
    let shown: Vec<String> = dialogue_events
        .iter()
        .filter_map(|ev| match ev {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.clone(),
            _ => None,
        })
        .collect();

    assert!(
        !shown.is_empty(),
        "expected DialogueOccurred for the NPC turn"
    );

    let joined = shown.join(" ").to_lowercase();

    // "might find him" routing phrase must be gone.
    assert!(
        !joined.contains("might find him"),
        "routing phrase after denial must be stripped by guard (#1478); got: {joined:?}"
    );
    // "old mill" fabricated destination must be gone.
    assert!(
        !joined.contains("old mill"),
        "routing destination must be stripped by guard (#1478); got: {joined:?}"
    );
}

// ── #1493 — addressed farewell when NPC has departed ─────────────────────────

/// AC-1 (#1493, real-loop): When the player addresses a farewell to an NPC
/// who has already departed, the text-log must surface the player's line or
/// a graceful system message — not a silent void.
///
/// Setup: start with one NPC co-located, then move them away, then send a
/// farewell addressed to them by name (which routes them to the `absent` list).
#[test]
fn real_loop_farewell_to_absent_npc_emits_player_line() {
    let (mut h, speaker_id, speaker_name) = harness_with_one_npc();

    // Move the NPC away — they are now absent.
    let other_loc = h
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|l| *l != h.app.world.player_location)
        .expect("graph has at least two locations");
    if let Some(npc) = h.app.npc_manager.get_mut(speaker_id) {
        npc.location = other_loc;
    }

    // Player says goodbye to the departed NPC by name.
    let farewell = format!("Goodbye, {speaker_name}");
    let events = h.execute_via_real_loop(&farewell);

    // Collect text-log content strings.
    let text_lines: Vec<String> = events
        .iter()
        .filter(|(name, _)| name == "text-log")
        .filter_map(|(_, payload)| {
            payload
                .get("content")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();

    // After the fix (#1493): the player's line (contains "Goodbye") or a
    // graceful system message ("already gone" / "not here") must appear.
    let has_farewell_or_graceful = text_lines.iter().any(|t| {
        let l = t.to_lowercase();
        l.contains("goodbye") || l.contains("already gone") || l.contains("not here")
    });

    assert!(
        has_farewell_or_graceful,
        "player farewell or graceful message must appear in text-log when NPC departed (#1493); \
         got text lines: {text_lines:?}"
    );
}

// ── #1476 — first-person physical actions route to Interact ──────────────────

/// AC-1 (#1476, real-loop): "I pick up a stone" must NOT route to NPC
/// conversation; it must produce an action narration log entry ("You pick up a
/// stone.") with no NPC inference triggered.
///
/// No mock response is queued. If this wrongly routes to NPC conversation, the
/// harness returns the inference-not-available fallback and nothing action-like.
#[test]
fn real_loop_first_person_pick_up_routes_to_interact_not_talk() {
    let (mut h, _, _) = harness_with_one_npc();

    // "interact-narration" flag is default-on.
    // No mock queued — NPC dialogue won't fire.
    let events = h.execute_via_real_loop("I pick up a stone");

    let text_lines: Vec<String> = events
        .iter()
        .filter(|(name, _)| name == "text-log")
        .filter_map(|(_, payload)| {
            payload
                .get("content")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();

    // An interact narration containing "pick up" must appear.
    let has_action = text_lines
        .iter()
        .any(|t| t.to_lowercase().contains("pick up"));

    assert!(
        has_action,
        "first-person physical action must produce an action narration (#1476); \
         got text lines: {text_lines:?}"
    );
}
