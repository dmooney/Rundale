//! Game-loop integration tests for NPC verbosity / multi-question guards (#1505, #1492).
//!
//! These tests exercise the full `handle_npc_conversation` → `run_npc_turn`
//! pipeline via `execute_via_real_loop` with a scripted `MockClient`. They
//! prove that the deterministic post-generation guards wired into `npc_turn.rs`
//! fire on degenerate model output and leave clean output untouched.
//!
//! ## Evidence type
//!
//! `Evidence type: game-loop integration test` — these tests exercise
//! `execute_via_real_loop` which drives the real `parish_core::game_loop`
//! (including `run_npc_turn` and `guard_verbosity_runons`) with the LLM
//! boundary mocked by `MockClient`.
//!
//! ## #1505 — consecutive short-phrase near-repetition
//!
//! A 403-char NPC reply containing "tell me, tell me" should have the
//! repeated phrase removed by `strip_consecutive_short_phrase_repeat` inside
//! the `guard_verbosity_runons` pipeline.
//!
//! ## #1492 — cross-NPC stock opener across separate turns
//!
//! When two NPCs are addressed on separate turns at the same location and
//! both open with a near-identical stock phrase ("Ye've come to the right
//! place for X"), the second NPC's opener is stripped by
//! `dedupe_cross_npc_openers` using the session-level
//! `seen_openers_this_location` set on `ConversationRuntimeState`.

use parish_core::npc::types::NpcState;
use parish_core::world::events::GameEvent;
use parish_engine::testing::GameTestHarness;

/// Drains the broadcast receiver until empty, returning all events.
fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<GameEvent>) -> Vec<GameEvent> {
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

// ── #1505 — consecutive short-phrase repeat ("tell me, tell me") ─────────────

/// AC-1 (#1505): a scripted NPC reply containing "tell me, tell me" must have
/// the repeated phrase stripped by the verbosity guard before the
/// `DialogueOccurred` event is published.
///
/// This test mocks the LLM to return a reply containing the exact pattern
/// observed in the quality-harness run 15 bug report and asserts that the
/// guard fires and removes the repetition from the persisted dialogue event.
#[test]
fn verbosity_guard_strips_tell_me_repeat_through_real_loop() {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    // Find Nora Duffy — a Rundale NPC we can co-locate with the player.
    let nora_id = h
        .app
        .npc_manager
        .all_npcs()
        .find(|n| n.name == "Nora Duffy")
        .map(|n| n.id)
        .expect("Rundale mod must contain Nora Duffy");

    {
        let nora = h.app.npc_manager.get_mut(nora_id).expect("Nora exists");
        nora.set_location_and_state(player_loc, NpcState::Present);
    }
    h.app.npc_manager.mark_introduced(nora_id);

    // Script the mock with the #1505 pattern: a run-on with "tell me, tell me".
    let injected = "Aye, the journey is a long one from the south. \
        Does the journey yet weigh heavy on yer heart, I wonder indeed, \
        aye or nay, tell me, tell me?";
    h.mock().push_for("Nora", injected);

    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("talk to Nora about the journey");

    let events = drain_events(&mut rx);
    let dialogue_events: Vec<Option<String>> = events
        .iter()
        .filter_map(|e| {
            if let GameEvent::DialogueOccurred {
                npc_id, npc_said, ..
            } = e
                && *npc_id == nora_id
            {
                return Some(npc_said.clone());
            }
            None
        })
        .collect();

    assert!(
        !dialogue_events.is_empty(),
        "expected a DialogueOccurred event for Nora; got events: {events:?}"
    );

    for npc_said_opt in &dialogue_events {
        let npc_said = npc_said_opt.as_deref().unwrap_or("");
        let tell_me_count = npc_said.to_lowercase().matches("tell me").count();
        assert!(
            tell_me_count <= 1,
            "#1505: guard must strip \"tell me, tell me\" repetition from the \
             persisted dialogue; got {tell_me_count} occurrences in: {npc_said:?}"
        );
    }
}

/// AC-2 (#1505): a clean NPC reply (no repeated phrase) must pass through the
/// verbosity guard unchanged — the guard must not alter legitimate dialogue.
#[test]
fn verbosity_guard_passes_clean_reply_through_real_loop() {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    let nora_id = h
        .app
        .npc_manager
        .all_npcs()
        .find(|n| n.name == "Nora Duffy")
        .map(|n| n.id)
        .expect("Rundale mod must contain Nora Duffy");

    {
        let nora = h.app.npc_manager.get_mut(nora_id).expect("Nora exists");
        nora.set_location_and_state(player_loc, NpcState::Present);
    }
    h.app.npc_manager.mark_introduced(nora_id);

    let clean_reply = "Aye, the journey from the south is never easy. Are ye well?";
    h.mock().push_for("Nora", clean_reply);

    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("talk to Nora about the journey");

    let events = drain_events(&mut rx);
    let dialogue_texts: Vec<Option<String>> = events
        .iter()
        .filter_map(|e| {
            if let GameEvent::DialogueOccurred {
                npc_id, npc_said, ..
            } = e
                && *npc_id == nora_id
            {
                return Some(npc_said.clone());
            }
            None
        })
        .collect();

    assert!(
        !dialogue_texts.is_empty(),
        "expected a DialogueOccurred event for Nora; got: {events:?}"
    );

    // The clean reply must pass through (allowing for length/question-cap
    // which won't fire on this short reply).
    let found = dialogue_texts.iter().any(|d| {
        d.as_deref()
            .unwrap_or("")
            .contains("journey from the south")
    });
    assert!(
        found,
        "clean legitimate dialogue must pass through unchanged; got: {dialogue_texts:?}"
    );
}

// ── #1492 — cross-NPC stock opener across separate turns ─────────────────────

/// AC-3 (#1492): when two NPCs are addressed sequentially (separate turns) at
/// the same location, and both would open with a near-identical stock phrase,
/// the second NPC's opener is stripped by the session-level opener dedup.
///
/// This test drives two separate `execute_via_real_loop` calls and checks that
/// the second NPC's `DialogueOccurred` event does NOT contain the stock opener
/// phrase that the first NPC already used.
#[test]
fn cross_npc_opener_dedup_fires_across_separate_turns() {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    // Find two NPCs to co-locate with the player.
    let nora_id = h
        .app
        .npc_manager
        .all_npcs()
        .find(|n| n.name == "Nora Duffy")
        .map(|n| n.id)
        .expect("Rundale mod must contain Nora Duffy");

    let brendan_id = h
        .app
        .npc_manager
        .all_npcs()
        .find(|n| n.name == "Brendan Duffy")
        .map(|n| n.id)
        .expect("Rundale mod must contain Brendan Duffy");

    {
        let nora = h.app.npc_manager.get_mut(nora_id).expect("Nora exists");
        nora.set_location_and_state(player_loc, NpcState::Present);
    }
    {
        let brendan = h
            .app
            .npc_manager
            .get_mut(brendan_id)
            .expect("Brendan exists");
        brendan.set_location_and_state(player_loc, NpcState::Present);
    }
    h.app.npc_manager.mark_introduced(nora_id);
    h.app.npc_manager.mark_introduced(brendan_id);

    // Both NPCs use the same stock opener — near-identical (Jaccard >= 0.75).
    // "Ye've come to the right place for news" (Nora, turn 1)
    // "Ye've come to the right place for a chat" (Brendan, turn 2)
    // Shared words: ye've, come, to, the, right, place, for = 7
    // Union = 7 + 1(news) + 2(a, chat) = 10... let's compute:
    // A: {ye've, come, to, the, right, place, for, news} = 8
    // B: {ye've, come, to, the, right, place, for, a, chat} = 9
    // Intersection: {ye've, come, to, the, right, place, for} = 7
    // Union = 8+9-7 = 10; Jaccard = 7/10 = 0.7 — just below the 0.75 threshold.
    //
    // Use phrases with Jaccard >= 0.75 to reliably trigger:
    // "Ye've come to the right place for a good chat" (8 content words)
    // "Ye've come to the right place for good chat" (7 content words)
    // A: {ye've, come, to, the, right, place, for, a, good, chat} = 10
    // B: {ye've, come, to, the, right, place, for, good, chat} = 9
    // Intersection: {ye've, come, to, the, right, place, for, good, chat} = 9
    // Union = 10+9-9 = 10; Jaccard = 9/10 = 0.9 — well above 0.75.
    let stock_opener_nora =
        "Ye've come to the right place for a good chat. I can tell ye all the news.";
    let stock_opener_brendan =
        "Ye've come to the right place for good chat. The harvest is looking fine.";

    // Turn 1: Nora speaks first.
    h.mock().push_for("Nora", stock_opener_nora);
    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("talk to Nora");

    let events_1 = drain_events(&mut rx);
    let nora_said: Vec<Option<String>> = events_1
        .iter()
        .filter_map(|e| {
            if let GameEvent::DialogueOccurred {
                npc_id, npc_said, ..
            } = e
                && *npc_id == nora_id
            {
                return Some(npc_said.clone());
            }
            None
        })
        .collect();

    assert!(
        !nora_said.is_empty(),
        "expected DialogueOccurred for Nora on turn 1; got: {events_1:?}"
    );
    // Nora's opener must pass through (no prior openers yet).
    let nora_text = nora_said[0].as_deref().unwrap_or("");
    assert!(
        nora_text
            .to_lowercase()
            .contains("ye've come to the right place"),
        "Nora's opener should NOT be stripped (no prior openers): {nora_text:?}"
    );

    // Turn 2: Brendan speaks with the near-identical opener.
    h.mock().push_for("Brendan", stock_opener_brendan);
    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("talk to Brendan");

    let events_2 = drain_events(&mut rx);
    let brendan_said: Vec<Option<String>> = events_2
        .iter()
        .filter_map(|e| {
            if let GameEvent::DialogueOccurred {
                npc_id, npc_said, ..
            } = e
                && *npc_id == brendan_id
            {
                return Some(npc_said.clone());
            }
            None
        })
        .collect();

    assert!(
        !brendan_said.is_empty(),
        "expected DialogueOccurred for Brendan on turn 2; got: {events_2:?}"
    );

    // Brendan's opener must be stripped — Nora already used the near-identical phrase.
    let brendan_text = brendan_said[0].as_deref().unwrap_or("");
    assert!(
        !brendan_text
            .to_lowercase()
            .starts_with("ye've come to the right place"),
        "#1492: Brendan's duplicate opener must be stripped across turns; \
         got: {brendan_text:?}"
    );
    // The substantive remainder must survive.
    assert!(
        brendan_text.contains("harvest is looking fine"),
        "#1492: Brendan's substantive content must survive after opener strip; \
         got: {brendan_text:?}"
    );
}
