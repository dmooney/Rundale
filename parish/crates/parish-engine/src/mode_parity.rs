//! Mode-parity golden test (#1172).
//!
//! Drives one deterministic, offline canned dialogue exchange through two of
//! the real per-turn entry points — the script harness
//! (`consume_canned_npc_response` / addressed handler in [`crate::testing`])
//! and the headless CLI (`crate::headless::apply_npc_response`) — captures the
//! `GameEvent` stream each publishes on the world event bus, and asserts the
//! per-turn `DialogueOccurred` events are identical (modulo the documented
//! `request_id` / `timestamp` fields).
//!
//! Since #1173 routes all three paths (harness, headless, live `run_npc_turn`)
//! through the single `parish_core::game_session::apply_npc_turn` chokepoint,
//! this test fails loudly if any path stops publishing `DialogueOccurred` or
//! drifts on the speaker / location / reply — the exact regression class that
//! produced #1028, #1035, and #1077/#1079.
//!
//! The live `run_npc_turn` path is async and needs an `InferenceQueue`; it is
//! covered by the shared seam (every path is a thin wrapper over
//! `apply_npc_turn`) plus the live fixture
//! `parish/testing/fixtures/play_1172-mode-parity-golden.txt`.

#![cfg(test)]

use parish_core::world::events::GameEvent;
use tokio::sync::broadcast::Receiver;

use crate::npc::NpcId;
use crate::testing::GameTestHarness;
use crate::world::LocationId;

const CANNED: &str = "A fair morning to ye, and welcome to the pub.";
// Plain greeting: no `say`/`tell` verb and no `@mention`, so the harness's
// `strip_dialogue_verb` leaves it untouched and `player_said` matches the
// value the headless path records verbatim.
const PLAYER_LINE: &str = "Good morning to you, neighbour.";

/// Deterministically pick the lowest-id NPC and co-locate the player with it,
/// so every path targets the same speaker at the same location.
fn pick_and_colocate(h: &mut GameTestHarness) -> (NpcId, String, LocationId) {
    let app = h.app_mut();
    let mut npcs: Vec<(NpcId, String, LocationId)> = app
        .npc_manager
        .npcs()
        .values()
        .map(|n| (n.id, n.name.clone(), n.location))
        .collect();
    npcs.sort_by_key(|(id, _, _)| *id);
    let (id, name, loc) = npcs.first().cloned().expect("mod loads at least one NPC");
    app.world.player_location = loc;
    (id, name, loc)
}

/// Drain every event currently queued on a broadcast receiver.
fn drain(rx: &mut Receiver<GameEvent>) -> Vec<GameEvent> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        out.push(ev);
    }
    out
}

/// The single `DialogueOccurred` a per-turn chokepoint must publish. Panics
/// (with a path label) unless exactly one was emitted — this is the assertion
/// that catches a path silently dropping the publish.
fn sole_dialogue(path: &str, events: &[GameEvent]) -> GameEvent {
    let dialogues: Vec<&GameEvent> = events
        .iter()
        .filter(|e| matches!(e, GameEvent::DialogueOccurred { .. }))
        .collect();
    assert_eq!(
        dialogues.len(),
        1,
        "path '{path}' must publish exactly one DialogueOccurred, got {}: {events:#?}",
        dialogues.len(),
    );
    dialogues[0].clone()
}

/// Run the canned exchange through the script-harness addressed entry point.
fn run_harness() -> GameEvent {
    let mut h = GameTestHarness::new();
    let (_id, name, _loc) = pick_and_colocate(&mut h);
    h.add_canned_response(&name, CANNED);
    let mut rx = h.event_bus().subscribe();
    let _ = h.execute(PLAYER_LINE);
    sole_dialogue("harness", &drain(&mut rx))
}

/// Run the same exchange through the headless `apply_npc_response` path.
fn run_headless() -> GameEvent {
    let mut h = GameTestHarness::new();
    let (id, name, loc) = pick_and_colocate(&mut h);
    let app = h.app_mut();
    let game_time = app.world.clock.now();
    let mut rx = app.world.event_bus.subscribe();
    crate::headless::apply_npc_response(
        app,
        id,
        CANNED,
        PLAYER_LINE,
        game_time,
        loc,
        &name,
        name.clone(),
    );
    sole_dialogue("headless", &drain(&mut rx))
}

/// The fields of a `DialogueOccurred` that must match across paths. The
/// `request_id` (`Some` only on the live LLM path) and `timestamp` are the two
/// documented, intentionally-varying fields and are excluded.
fn normalized(ev: &GameEvent) -> (NpcId, LocationId, String, Option<String>, Option<String>) {
    match ev {
        GameEvent::DialogueOccurred {
            npc_id,
            location,
            summary,
            player_said,
            npc_said,
            ..
        } => (
            *npc_id,
            *location,
            summary.clone(),
            player_said.clone(),
            npc_said.clone(),
        ),
        other => panic!("expected DialogueOccurred, got {other:?}"),
    }
}

#[test]
fn harness_and_headless_publish_identical_dialogue_event() {
    let harness = run_harness();
    let headless = run_headless();
    assert_eq!(
        normalized(&harness),
        normalized(&headless),
        "harness and headless DialogueOccurred diverged:\n  harness:  {harness:#?}\n  headless: {headless:#?}",
    );
}

#[test]
fn both_paths_carry_the_npc_reply_and_player_line() {
    // Guards C2: the published event actually carries the verbatim reply and
    // the player's line, not an empty summary.
    for (path, ev) in [("harness", run_harness()), ("headless", run_headless())] {
        let (_npc, _loc, summary, player_said, npc_said) = normalized(&ev);
        assert_eq!(summary, CANNED, "path '{path}' summary");
        assert_eq!(npc_said.as_deref(), Some(CANNED), "path '{path}' npc_said");
        assert_eq!(
            player_said.as_deref(),
            Some(PLAYER_LINE),
            "path '{path}' player_said",
        );
    }
}
