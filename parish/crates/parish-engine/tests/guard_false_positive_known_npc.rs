//! Integration test: repetition guard does not fire when near-identical lines
//! are explained by a shared grounded (in-roster) person name (#1228 fix).
//!
//! ## Root cause (run-17, turn-20)
//!
//! The anti-repetition guard fires when `is_near_identical` (word-level Jaccard
//! ≥ 0.92) between a new NPC line and the NPC's previous line at this location.
//! When BOTH lines are correct, grounded replies about the same in-roster person
//! ("Una Malone, aye, she's a skilled weaver"), the guard was treating the
//! legitimate topic-consistency as degenerate repetition and substituting a
//! fallback ("I've said my piece on that, so.").
//!
//! ## Fix
//!
//! `guard_against_repetition` now accepts `grounded_person_names: &[String]`.
//! When the near-identical pair shares a full name (≥2 tokens) from the roster
//! in both lines, the guard skips the fallback substitution.
//! `apply_npc_dialogue_turn` passes `setup.known_person_names` through the live
//! game-loop path.
//!
//! ## Test strategy
//!
//! Both tests drive the full `handle_game_input → run_npc_turn →
//! apply_npc_dialogue_turn` path via `execute_via_real_loop` with a mock client.
//!
//! **Test 1 (false-positive prevention)**: two near-identical correct replies
//! about an in-roster person ("Roisin Connolly, aye, she's a fine weaver…")
//! must NOT be substituted by a fallback.
//!
//! **Test 2 (true-positive: real degenerate loop still fires)**: two
//! near-identical replies with no person-name grounding (about the weather)
//! MUST still be substituted by a fallback.

use parish_core::npc::types::NpcState;
use parish_core::world::events::GameEvent;
use parish_engine::testing::GameTestHarness;
use parish_types::LocationId;

/// The varied fallback pool from `repetition.rs`. If `npc_said` is any of
/// these, the guard fired.
const REPETITION_FALLBACK_POOL: &[&str] = &[
    "Aye, as I said.",
    "Sure, ye have the right of it.",
    "Mm. There's little more to add to that.",
    "Well now, I'll not say the same twice.",
    "Aye, that's the way of it.",
    "I've said my piece on that, so.",
];

/// Drain all broadcast events from the receiver.
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

/// Returns true when the given `npc_said` text is a repetition-guard fallback.
fn is_repetition_fallback(text: &str) -> bool {
    REPETITION_FALLBACK_POOL.iter().any(|f| text.contains(f))
}

/// Set up a harness with a single NPC at the player location.
///
/// Finds an NPC by name from the Rundale roster (Roisin Connolly if available,
/// otherwise the first available NPC), places them at the player location, moves
/// all others away, and marks the NPC as introduced.
///
/// Returns `(harness, npc_id, npc_name)`.
fn harness_with_single_npc() -> (GameTestHarness, parish_core::npc::NpcId, String) {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    // Prefer Roisin Connolly (used in the original run-17 bug report), fall
    // back to any NPC available. The guard is roster-aware: any in-roster name
    // that appears in both lines causes the guard to skip the fallback.
    let npc_id = h
        .app
        .npc_manager
        .all_npcs()
        .find(|n| n.name == "Roisin Connolly")
        .or_else(|| h.app.npc_manager.all_npcs().next())
        .map(|n| n.id)
        .expect("Rundale mod must contain at least one NPC");

    let npc_name = {
        let npc = h.app.npc_manager.get_mut(npc_id).expect("NPC exists");
        npc.set_location_and_state(player_loc, NpcState::Present);
        npc.name.clone()
    };
    h.app.npc_manager.mark_introduced(npc_id);

    // Move all other NPCs away from the player location so the mock response
    // goes to exactly this NPC.
    let other_loc: LocationId = h
        .app
        .world
        .graph
        .location_ids()
        .into_iter()
        .find(|&l| l != player_loc)
        .expect("graph has at least two locations");
    let others: Vec<_> = h
        .app
        .npc_manager
        .all_npcs()
        .filter(|n| n.location() == player_loc && n.id != npc_id)
        .map(|n| n.id)
        .collect();
    for id in others {
        if let Some(n) = h.app.npc_manager.get_mut(id) {
            n.set_location(other_loc);
        }
    }

    (h, npc_id, npc_name)
}

/// Find any in-roster full name (≥2 tokens) from the NPC manager.
///
/// Returns `None` if there are no multi-token names (should not happen in
/// Rundale, where all NPCs have First + Last names).
fn find_roster_full_name(
    h: &GameTestHarness,
    exclude_id: parish_core::npc::NpcId,
) -> Option<String> {
    h.app
        .npc_manager
        .all_npcs()
        .filter(|n| n.id != exclude_id)
        .find(|n| n.name.split_whitespace().count() >= 2)
        .map(|n| n.name.clone())
}

// ── Test 1: false-positive prevention ────────────────────────────────────────

/// The repetition guard must NOT fire when near-identical NPC lines are both
/// correct grounded replies discussing the same in-roster person (#1228 fix).
///
/// Scenario:
/// - Turn 1: NPC gives a reply that mentions a real roster person by full name.
/// - Turn 2: NPC gives a near-identical reply about the same person (≥0.92 Jaccard).
/// - Expected: the second reply reaches the player unchanged, NOT replaced by a
///   fallback, because the shared full name explains the high word similarity.
#[test]
fn guard_does_not_fire_on_grounded_person_name_topic_continuity() {
    let (mut h, npc_id, npc_name) = harness_with_single_npc();

    // Find a full name from the roster to use as the grounding anchor.
    let roster_name = match find_roster_full_name(&h, npc_id) {
        Some(n) => n,
        None => {
            // No multi-token roster name available — skip silently.
            // This should never happen in Rundale; if it does, the harness
            // doesn't have the mod loaded correctly.
            return;
        }
    };

    // Build two near-identical lines that both mention the roster name.
    // These are intentionally very similar (≥0.92 Jaccard) because they're
    // both factually correct replies about the same person.
    //
    // Jaccard math: both lines share a 25-token base (including the 2-token
    // roster name) and each appends exactly ONE unique word ("indeed" vs
    // "surely"). Union = 27, intersection = 25, J = 25/27 ≈ 0.926 ≥ 0.92.
    let base = format!(
        "{roster_name} aye a fine weaver and a good woman known to all by her hard diligent work through every season every passing year in the townland"
    );
    let first_line = format!("{base} indeed.");
    let second_line = format!("{base} surely.");

    // ── Turn 1: push first line and run. Stores it in the conversation log. ──
    h.mock().push_any(&first_line);
    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("Do you know anyone skilled at weaving?");

    // Drain first turn events (we don't assert on them).
    let _ = drain(&mut rx);

    // ── Turn 2: push the near-identical second line. ──────────────────────────
    h.mock().push_any(&second_line);
    let mut rx2 = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("Tell me more about the weaver.");

    let events = drain(&mut rx2);
    let shown: Vec<String> = events
        .iter()
        .filter_map(|ev| {
            if let GameEvent::DialogueOccurred {
                npc_id: ev_npc,
                npc_said,
                ..
            } = ev
                && *ev_npc == npc_id
            {
                npc_said.clone()
            } else {
                None
            }
        })
        .collect();

    assert!(
        !shown.is_empty(),
        "expected DialogueOccurred for NPC {npc_name} (id {npc_id:?}) on turn 2; \
         events: {events:?}"
    );

    let joined = shown.join(" ");
    assert!(
        !is_repetition_fallback(&joined),
        "repetition guard must NOT fire when near-identical lines are grounded \
         in a real roster person ({roster_name:?}); \
         got: {joined:?}"
    );
    // The actual second line should reach the player (possibly capped/processed).
    assert!(
        joined.to_lowercase().contains(&roster_name.to_lowercase()),
        "the grounded person name must appear in the player-visible output; \
         got: {joined:?}"
    );
}

// ── Test 2: true-positive (degenerate loop still fires) ───────────────────────

/// The repetition guard MUST still fire for genuine degenerate repetition —
/// near-identical lines with no grounded person name to explain the overlap.
///
/// Scenario:
/// - Turn 1: NPC gives a reply about the weather (no person name).
/// - Turn 2: NPC gives a near-identical reply about the weather (≥0.92 Jaccard).
/// - Expected: the second reply IS replaced by a fallback, because there is no
///   grounded person name to explain the high word similarity.
#[test]
fn guard_still_fires_on_degenerate_repetition_without_person_name() {
    let (mut h, npc_id, npc_name) = harness_with_single_npc();

    // Two near-identical lines with no person name — purely degenerate loop.
    //
    // Jaccard math: both lines share a ~24-token base (no person name) and each
    // appends exactly ONE unique word ("indeed" vs "truly"). Union = 26,
    // intersection = 24, J = 24/26 ≈ 0.923 ≥ 0.92 — guard must fire.
    let first_line = "Aye the weather has been fierce cold and bitter hard all through this week and every passing day brought fresh misery from the north and no comfort indeed.";
    let second_line = "Aye the weather has been fierce cold and bitter hard all through this week and every passing day brought fresh misery from the north and no comfort truly.";

    // ── Turn 1: push first line and run. Stores it in the conversation log. ──
    h.mock().push_any(first_line);
    let mut rx = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("What do you make of the weather?");
    let _ = drain(&mut rx);

    // ── Turn 2: push the near-identical second line. ──────────────────────────
    h.mock().push_any(second_line);
    let mut rx2 = h.app.world.event_bus.subscribe();
    let _ = h.execute_via_real_loop("And what of the weather today?");

    let events = drain(&mut rx2);
    let shown: Vec<String> = events
        .iter()
        .filter_map(|ev| {
            if let GameEvent::DialogueOccurred {
                npc_id: ev_npc,
                npc_said,
                ..
            } = ev
                && *ev_npc == npc_id
            {
                npc_said.clone()
            } else {
                None
            }
        })
        .collect();

    assert!(
        !shown.is_empty(),
        "expected DialogueOccurred for NPC {npc_name} (id {npc_id:?}) on turn 2; \
         events: {events:?}"
    );

    let joined = shown.join(" ");
    assert!(
        is_repetition_fallback(&joined),
        "repetition guard MUST fire on degenerate weather repetition (no grounded \
         person name to explain overlap); got: {joined:?}"
    );
}
