//! Mode-parity golden test (#1172).
//!
//! Mode-parity drift — three per-turn dialogue paths quietly diverging — is the
//! dominant recurring defect class (#1028, #1035, #1077/#1079). After #1173 all
//! paths funnel one NPC reply through the single shared chokepoint
//! [`parish_core::game_session::apply_dialogue_turn`], so the `GameEvent` stream
//! each path publishes must be identical.
//!
//! This test drives the **script-harness** path through its real public entry
//! (`GameTestHarness::execute`) and the **shared seam** that the live loop
//! (`game_loop::npc_turn::run_npc_turn`) and the **headless** CLI
//! (`headless::apply_npc_response`) now both call, then asserts the emitted
//! `DialogueOccurred` streams match field-for-field (modulo the per-event
//! `timestamp`, which the live LLM path stamps from the same in-game clock).
//!
//! If any path stops publishing `DialogueOccurred`, or publishes a different
//! `summary` / `player_said` / `npc_said` / location, the equality assertion
//! below fails with a diff — which is exactly what catches a harness-only
//! behaviour regression before it ships.

use parish_core::game_session::{DialogueTurn, apply_dialogue_turn};
use parish_core::npc::{NpcMetadata, NpcStreamResponse};
use parish_engine::testing::{ActionResult, GameTestHarness};
use parish_types::events::GameEvent;
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

const DIALOGUE: &str = "Ah, good morning to ye, and isn't it a grand soft day.";
/// Free text with no `say`/`tell` verb, so `strip_dialogue_verb` is a no-op and
/// `player_said` is identical across paths.
const PLAYER_INPUT: &str = "hello there";

/// A `DialogueOccurred` reduced to the fields that must match across paths
/// (everything except the wall-/clock-stamped `timestamp`).
#[derive(Debug, PartialEq)]
struct DialogueShape {
    npc_id: parish_core::npc::NpcId,
    location: parish_core::world::LocationId,
    summary: String,
    player_said: Option<String>,
    npc_said: Option<String>,
    request_id: Option<u64>,
}

/// Fresh Rundale harness with the player standing in the pub at ~10am, where
/// Padraig Darcy is reliably co-located (mirrors `test_canned_npc_response`).
fn harness_at_pub() -> GameTestHarness {
    let mut h = GameTestHarness::new();
    h.advance_time(120); // 10am — Padraig is scheduled at the pub (9-22).
    h.execute("go to crossroads");
    h.execute("go to pub");
    assert!(
        h.npcs_here().iter().any(|n| n == &"Padraig Darcy"),
        "expected Padraig Darcy at the pub, found {:?}",
        h.npcs_here()
    );
    h
}

/// Padraig's `(id, name, mood)` at the player's current location.
fn padraig(h: &GameTestHarness) -> (parish_core::npc::NpcId, String, String) {
    let loc = h.app.world.player_location;
    let npc = h
        .app
        .npc_manager
        .npcs_at(loc)
        .into_iter()
        .find(|n| n.name == "Padraig Darcy")
        .expect("Padraig Darcy co-located");
    (npc.id, npc.name.clone(), npc.mood.clone())
}

/// Drains every pending `DialogueOccurred` from a bus receiver, reduced to its
/// comparable shape. Non-dialogue events (schedule ticks, etc.) are ignored.
fn drain_dialogue(rx: &mut Receiver<GameEvent>) -> Vec<DialogueShape> {
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(GameEvent::DialogueOccurred {
                npc_id,
                location,
                summary,
                player_said,
                npc_said,
                request_id,
                ..
            }) => out.push(DialogueShape {
                npc_id,
                location,
                summary,
                player_said,
                npc_said,
                request_id,
            }),
            Ok(_) => continue,
            Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }
    out
}

#[test]
fn script_harness_and_shared_seam_publish_identical_dialogue_events() {
    // ── Path A: the real script-harness entry point ──────────────────────────
    let mut ha = harness_at_pub();
    ha.add_canned_response("Padraig Darcy", DIALOGUE);
    let mut rx_a = ha.app.world.event_bus.subscribe();
    let result = ha.execute(PLAYER_INPUT);
    assert!(
        matches!(result, ActionResult::NpcResponse { .. }),
        "harness should route {PLAYER_INPUT:?} to an NPC dialogue turn, got {result:?}"
    );
    let path_a = drain_dialogue(&mut rx_a);

    // ── Path B: the shared seam the live + headless paths call ────────────────
    let mut hb = harness_at_pub();
    let (npc_id, npc_name, npc_mood) = padraig(&hb);
    let parsed = NpcStreamResponse {
        dialogue: DIALOGUE.to_string(),
        metadata: Some(NpcMetadata {
            action: "responds".to_string(),
            mood: npc_mood,
            internal_thought: None,
            language_hints: Vec::new(),
            mentioned_people: Vec::new(),
        }),
    };
    let mut rx_b = hb.app.world.event_bus.subscribe();
    apply_dialogue_turn(
        &mut hb.app.world,
        &mut hb.app.npc_manager,
        DialogueTurn {
            npc_id,
            speaker_name: &npc_name,
            player_input: PLAYER_INPUT,
            player_said: PLAYER_INPUT,
            parsed: &parsed,
            request_id: None,
        },
    );
    let path_b = drain_dialogue(&mut rx_b);

    // ── Parity assertions ─────────────────────────────────────────────────────
    assert_eq!(
        path_a.len(),
        1,
        "harness path must publish exactly one DialogueOccurred, got {path_a:?}"
    );
    assert_eq!(
        path_b.len(),
        1,
        "shared seam must publish exactly one DialogueOccurred — a regression that \
         drops the publish would empty this stream; got {path_b:?}"
    );
    assert_eq!(
        path_a, path_b,
        "script-harness and shared-seam DialogueOccurred streams diverged — \
         mode parity broken (#1172/#1173)"
    );

    // Pin the exact shape so a silent field change (e.g. dropping player_said,
    // or summarising instead of carrying the full reply) also fails loudly.
    let only = &path_a[0];
    assert_eq!(only.npc_id, npc_id);
    assert_eq!(only.summary, DIALOGUE);
    assert_eq!(only.npc_said.as_deref(), Some(DIALOGUE));
    assert_eq!(only.player_said.as_deref(), Some(PLAYER_INPUT));
    assert_eq!(only.request_id, None);
}
