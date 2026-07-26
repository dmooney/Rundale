//! End-to-end proof for the wrong-speaker-identity guard (#1475).
//!
//! The guard lives in `parish-npc/src/repetition.rs` and is wired into
//! `parish-core/src/game_loop/npc_turn.rs` (the path shared by Tauri,
//! `parish-server`, and the headless CLI).  Unit tests already cover the
//! guard's logic in isolation.  This integration test exercises the full
//! wiring: a scripted wrong-identity reply fed through `execute_via_real_loop`
//! must emerge from the event bus with the guard's recovery text in place of
//! the injected impersonation.
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
//! ## Scenario
//!
//! Nora Duffy (NPC id 13, "Miller's Wife") is co-located with the player and
//! at least one roster member (Brendan Duffy, id 14, also in her known
//! relationships).  The mock returns "Sure, I'm Brendan, the Miller's Son" —
//! the exact wrong-identity line from the original bug report (#1475 AC-1).
//! The guard must intercept it and replace it with a recovery line before the
//! `DialogueOccurred` event is published.

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

/// The `wrong_speaker_recovery` pool in `repetition.rs` — any of these lines
/// is an acceptable guard replacement.
const RECOVERY_POOL: &[&str] = &[
    "Aye, what can I do for ye?",
    "Grand day to ye — what brings ye here?",
    "Sure, how can I help ye today?",
    "Aye, I'm right here. What do ye need?",
    "Well now, speak yer mind.",
];

/// The injected wrong-identity line — the exact dialogue from the bug report.
const WRONG_IDENTITY_LINE: &str =
    "Sure, I'm Brendan, the Miller's Son. Ye've caught me just 'tween chores.";

#[test]
fn wrong_speaker_guard_fires_through_real_game_loop() {
    // ── 1. Build the harness (loads the Rundale mod). ─────────────────────────
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    // ── 2. Find Nora Duffy and Brendan Duffy by name. ────────────────────────
    //
    // The harness loads Rundale's npcs.json which contains exactly the two
    // characters from the original bug.  We look them up by name so the test
    // is stable across any future re-ordering of the JSON file.
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

    // ── 3. Co-locate both NPCs with the player and mark Nora as introduced. ──
    //
    // Both must be Present at the player location so:
    //   (a) `handle_game_input` can resolve Nora as the target of "talk to Nora";
    //   (b) the roster passed to the guard includes at least two members
    //       (Nora herself + Brendan), which is the precondition for the guard
    //       to fire (roster.len() < 2 → no-op).
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
    // Mark Nora as introduced so the harness treats her as a named NPC
    // (not an anonymous figure) and routes the "talk to Nora" input to her.
    h.app.npc_manager.mark_introduced(nora_id);

    // ── 4. Script the mock to return the wrong-identity line for Nora. ────────
    //
    // `push_for("Nora", ...)` matches any inference request whose prompt or
    // system text contains "Nora" (case-insensitive).  The Nora Duffy system
    // prompt always includes her full name, so this reliably binds to her turn.
    h.mock().push_for("Nora", WRONG_IDENTITY_LINE);

    // ── 5. Subscribe to the event bus and run the dialogue turn. ─────────────
    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("talk to Nora about the harvest");

    // ── 6. Drain events and find the DialogueOccurred published for Nora. ─────
    let events = drain_events(&mut rx);
    let dialogue_events: Vec<_> = events
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
        "expected a DialogueOccurred event for Nora Duffy (id {nora_id:?}); \
         got events: {events:?}"
    );

    // ── 7. Assert the guard intercepted the wrong-identity line. ──────────────
    //
    // The `npc_said` field must NOT be the injected wrong-identity line.
    // More specifically: it must not contain the first-person identity claim
    // "I'm Brendan" that the guard targets.  A successful guard replacement
    // will be one of the RECOVERY_POOL strings.
    for npc_said_opt in &dialogue_events {
        let npc_said = npc_said_opt.as_deref().unwrap_or("");
        let lower = npc_said.to_lowercase();

        // The injected impersonation claim must be absent.
        assert!(
            !lower.contains("i'm brendan") && !lower.contains("i am brendan"),
            "wrong-speaker-identity guard did NOT fire: Nora's reply still contains \
             Brendan's first-person identity claim.\n  npc_said = {npc_said:?}"
        );

        // The guard must have replaced it with a recovery line.
        let is_recovery = RECOVERY_POOL.iter().any(|r| npc_said.contains(r));
        let is_fallback = npc_said.contains("Mock: (no scripted response)");
        assert!(
            is_recovery || is_fallback || npc_said.is_empty(),
            "guard fired (wrong text absent) but replacement is not a known \
             recovery line or empty dialogue.\n  npc_said = {npc_said:?}\n  \
             recovery pool = {RECOVERY_POOL:?}"
        );

        if is_recovery {
            // This is the happy path: guard replaced with a recovery line.
            let matched = RECOVERY_POOL
                .iter()
                .find(|r| npc_said.contains(*r))
                .unwrap();
            eprintln!(
                "[proof] wrong-speaker-identity guard fired: \
                 injected line suppressed, recovery = {matched:?}"
            );
        }
    }
}

/// Sanity check: when the mock returns a clean line, the guard does NOT fire.
/// This rules out the possibility that the main test passes because the mock
/// fails silently and the dialogue is just empty.
#[test]
fn wrong_speaker_guard_passes_legitimate_dialogue_through_real_loop() {
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

    let clean_reply = "Aye, the harvest is nearly in, God willing.";
    h.mock().push_for("Nora", clean_reply);

    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("talk to Nora about the harvest");

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

    // The clean reply must pass through unchanged (guard must not fire).
    let found = dialogue_texts
        .iter()
        .any(|d| d.as_deref().unwrap_or("").contains("harvest is nearly in"));
    assert!(
        found,
        "clean legitimate dialogue was unexpectedly suppressed by the guard; \
         got: {dialogue_texts:?}"
    );
}
