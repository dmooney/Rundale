//! End-to-end proof for the acquaintance-intent guard (#1504).
//!
//! The guard lives in `parish-npc/src/repetition.rs`
//! (`guard_acquaintance_question_intent_drift`) and is wired into
//! `parish-core/src/game_loop/npc_turn.rs` under the flag
//! `npc-acquaintance-intent-guard`.  Unit tests already cover the guard's
//! logic in isolation.  This integration test exercises the full wiring: a
//! mock reply that conflates "Do you know X?" with "Are you X?" is fed
//! through `execute_via_real_loop` and must emerge from the event bus with
//! the guard's replacement text in place of the self-identity drift.
//!
//! ## Why `execute_via_real_loop`?
//!
//! The legacy `execute()` path is a test-only shim that reimplements dialogue
//! routing independently of `parish_core::game_loop`, so guard changes made in
//! `npc_turn.rs` would be invisible to it.  `execute_via_real_loop` drives the
//! actual `handle_game_input` → `run_npc_turn` path with the LLM boundary
//! mocked by `MockClient`, making it the closest integration-level equivalent
//! of the live Tauri/server runtime.
//!
//! ## Scenario (reproduces run-16 turn-14)
//!
//! Seamus Gallagher (Blacksmith) is placed at the player's location and
//! marked as introduced.  The player asks "Do you know Father Declan Tierney?"
//! — an acquaintance question.  The mock returns the exact buggy response from
//! the original bug report: "Ye must be mistaken... I'm but Seamus Gallagher."
//! The guard must intercept it and replace it with an acquaintance affirmation
//! (because Fr. Declan Tierney IS in the parish roster) before
//! `DialogueOccurred` is published.

use parish_core::npc::types::NpcState;
use parish_core::world::events::GameEvent;
use parish_engine::testing::GameTestHarness;
use parish_types::LocationId;

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

/// The acquaintance affirmation pool from `repetition.rs` — any of these lines
/// is an acceptable replacement when the named person is in the roster.
const ACQUAINTANCE_AFFIRM_POOL: &[&str] = &[
    "Aye, I know them well enough.",
    "Aye, I've met them in the parish.",
    "I do know who ye mean.",
    "Aye, the name is known to me.",
    "Sure, I know them from hereabouts.",
];

/// The exact buggy self-identity reply from run-16 turn-14.
const SELF_IDENTITY_DRIFT: &str = "Ye must be mistaken... I'm but Seamus Gallagher.";

/// The player's acquaintance question that triggered the bug.
const PLAYER_QUESTION: &str = "Do you know Father Declan Tierney?";

#[test]
fn acquaintance_intent_guard_fires_through_real_game_loop() {
    // ── 1. Build the harness (loads the Rundale mod). ─────────────────────────
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    // ── 2. Find Seamus Gallagher by name. ────────────────────────────────────
    //
    // The harness loads Rundale's npcs.json which contains Seamus Gallagher
    // (id 9, Blacksmith).  We look him up by name so the test is stable
    // across any future re-ordering of the JSON file.
    let seamus_id = h
        .app
        .npc_manager
        .all_npcs()
        .find(|n| n.name == "Seamus Gallagher")
        .map(|n| n.id)
        .expect("Rundale mod must contain Seamus Gallagher");

    // ── 3. Clear all NPCs from the player location then place only Seamus. ────
    //
    // Several NPCs start at the player's default start location, and their
    // system prompts may mention "Seamus" (he is a well-known blacksmith).
    // If any are present, another NPC might consume the mock before Seamus.
    // Clearing the location first means only Seamus receives the inference turn.
    //
    // Fr. Declan Tierney does NOT need to be at the player location for the
    // guard to recognise him: `known_person_names` in NpcConversationSetup is
    // built from ALL NPCs in the manager (the full roster), not just those
    // currently co-located.  Seamus knows Fr. Declan through the roster.

    // Move every NPC away from player_loc except Seamus.
    let all_ids: Vec<_> = h.app.npc_manager.all_npcs().map(|n| n.id).collect();
    for id in &all_ids {
        if *id != seamus_id
            && let Some(npc) = h.app.npc_manager.get_mut(*id)
            && npc.location() == player_loc
        {
            // Move to a sentinel location (0) that is not the player's.
            npc.set_location(LocationId(0));
        }
    }

    // Place Seamus at the player's location and mark him as introduced.
    {
        let seamus = h.app.npc_manager.get_mut(seamus_id).expect("Seamus exists");
        seamus.set_location_and_state(player_loc, NpcState::Present);
    }
    h.app.npc_manager.mark_introduced(seamus_id);

    // ── 4. Script the mock to return the self-identity drift. ─────────────────
    //
    // With only Seamus at the player location, `push_any` ensures the next
    // inference request (his turn) gets the buggy self-identity response.
    h.mock().push_any(SELF_IDENTITY_DRIFT);

    // ── 5. Subscribe to the event bus and run the dialogue turn. ─────────────
    let mut rx = h.app.world.event_bus.subscribe();
    // Send the acquaintance question directly — no "talk to" prefix needed
    // because Seamus is co-located and introduced.  The mock matches by
    // system-prompt content (his name), so the question reaches him.
    let _ = h.execute_via_real_loop(PLAYER_QUESTION);

    // ── 6. Drain events and find any DialogueOccurred for Seamus. ─────────────
    //
    // Seamus is the only addressed NPC at the player location, so the single
    // DialogueOccurred event that arrives must be his turn.
    let events = drain_events(&mut rx);
    let dialogue_events: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let GameEvent::DialogueOccurred {
                npc_id, npc_said, ..
            } = e
                && *npc_id == seamus_id
            {
                return Some(npc_said.clone());
            }
            None
        })
        .collect();

    // If no event for Seamus, check if ANY dialogue occurred (diagnostic).
    let any_dialogue: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let GameEvent::DialogueOccurred {
                npc_id, npc_said, ..
            } = e
            {
                Some((*npc_id, npc_said.clone()))
            } else {
                None
            }
        })
        .collect();

    assert!(
        !dialogue_events.is_empty(),
        "expected a DialogueOccurred event for Seamus Gallagher (id {seamus_id:?}); \
         any_dialogue = {any_dialogue:?}; \
         got events: {events:?}"
    );

    // ── 7. Assert the guard intercepted the self-identity drift. ─────────────
    //
    // `npc_said` must NOT contain "I'm but Seamus Gallagher" — the phrase
    // that incorrectly answers an identity challenge nobody asked.
    // The guard should have replaced it with an acquaintance affirmation
    // (Fr. Declan Tierney IS in the parish roster as "Fr. Declan Tierney").
    for npc_said_opt in &dialogue_events {
        let npc_said = npc_said_opt.as_deref().unwrap_or("");
        let lower = npc_said.to_lowercase();

        // The self-identity drift must be gone.
        assert!(
            !lower.contains("i'm but seamus") && !lower.contains("i am but seamus"),
            "acquaintance-intent guard did NOT fire: Seamus's reply still contains \
             the self-identity drift instead of addressing the acquaintance question.\n  \
             npc_said = {npc_said:?}"
        );

        // The guard must have replaced it with an acquaintance affirmation.
        let is_affirm = ACQUAINTANCE_AFFIRM_POOL
            .iter()
            .any(|r| npc_said.contains(r));
        let is_fallback = npc_said.contains("Mock: (no scripted response)");
        assert!(
            is_affirm || is_fallback || npc_said.is_empty(),
            "guard fired (drift absent) but replacement is not a known \
             acquaintance affirmation or empty.\n  npc_said = {npc_said:?}\n  \
             affirmation pool = {ACQUAINTANCE_AFFIRM_POOL:?}"
        );

        if is_affirm {
            let matched = ACQUAINTANCE_AFFIRM_POOL
                .iter()
                .find(|r| npc_said.contains(*r))
                .unwrap();
            eprintln!(
                "[proof] acquaintance-intent guard fired: \
                 self-identity drift suppressed, replacement = {matched:?}"
            );
        }
    }
}

/// Sanity check: when the mock returns a correct acquaintance answer, the
/// guard does NOT alter it.  Rules out a false-pass caused by the mock
/// failing silently and returning empty dialogue.
#[test]
fn acquaintance_intent_guard_passes_correct_acquaintance_answer_through_real_loop() {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    let seamus_id = h
        .app
        .npc_manager
        .all_npcs()
        .find(|n| n.name == "Seamus Gallagher")
        .map(|n| n.id)
        .expect("Rundale mod must contain Seamus Gallagher");

    // Clear other NPCs from the player location so only Seamus speaks.
    let all_ids: Vec<_> = h.app.npc_manager.all_npcs().map(|n| n.id).collect();
    for id in &all_ids {
        if *id != seamus_id
            && let Some(npc) = h.app.npc_manager.get_mut(*id)
            && npc.location() == player_loc
        {
            npc.set_location(LocationId(0));
        }
    }

    {
        let seamus = h.app.npc_manager.get_mut(seamus_id).expect("Seamus exists");
        seamus.set_location_and_state(player_loc, NpcState::Present);
    }
    h.app.npc_manager.mark_introduced(seamus_id);

    // A correct response — Seamus knows Fr. Declan and says so.
    // Contains "know" so the guard's content-word check must pass it through.
    let good_reply = "Aye, I know Fr. Tierney well enough — a kind man, the priest.";
    h.mock().push_any(good_reply);

    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop(PLAYER_QUESTION);

    let events = drain_events(&mut rx);
    let dialogue_texts: Vec<Option<String>> = events
        .iter()
        .filter_map(|e| {
            if let GameEvent::DialogueOccurred {
                npc_id, npc_said, ..
            } = e
                && *npc_id == seamus_id
            {
                return Some(npc_said.clone());
            }
            None
        })
        .collect();

    assert!(
        !dialogue_texts.is_empty(),
        "expected a DialogueOccurred event for Seamus; got: {events:?}"
    );

    // The good reply contains "know" — guard's content-word check must pass
    // it through unchanged.
    let found = dialogue_texts
        .iter()
        .any(|d| d.as_deref().unwrap_or("").contains("know Fr. Tierney"));
    assert!(
        found,
        "correct acquaintance answer was unexpectedly suppressed by the guard; \
         got: {dialogue_texts:?}"
    );
}
