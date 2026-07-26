//! Real-loop proof that the dialogue prompt grounds each known NPC's pronouns
//! and age (#1506), so the model never has to guess gender from a name.
//!
//! Strategy: a known NPC is given a sentinel pronoun ("xe/xem") that appears
//! nowhere else. The mock inference client is told to reply ONLY when that
//! sentinel is present in the prompt it receives. Driving a real dialogue turn
//! through `execute_via_real_loop` (the same `handle_game_input -> run_npc_turn`
//! path the Tauri app and server use) therefore proves the roster descriptor
//! carrying the pronoun reached the assembled prompt.

use parish_core::npc::types::NpcState;
use parish_core::world::events::GameEvent;
use parish_engine::testing::GameTestHarness;

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

#[test]
fn real_loop_prompt_grounds_known_npc_pronouns() {
    let mut h = GameTestHarness::new();
    let player_loc = h.app.world.player_location;

    // Speaker: first NPC, present + introduced + homed at the player location.
    let speaker_id = h
        .app
        .npc_manager
        .all_npcs()
        .map(|n| n.id)
        .next()
        .expect("harness loads at least one NPC");
    let speaker_name = {
        let n = h
            .app
            .npc_manager
            .get_mut(speaker_id)
            .expect("speaker exists");
        n.set_location_and_state(player_loc, NpcState::Present);
        n.home = Some(player_loc);
        n.name.clone()
    };
    h.app.npc_manager.mark_introduced(speaker_id);

    // A known NPC (same home → appears in the speaker's known_roster) with a
    // sentinel pronoun that appears nowhere else in the prompt.
    let other_id = h
        .app
        .npc_manager
        .all_npcs()
        .map(|n| n.id)
        .find(|id| *id != speaker_id)
        .expect("a second NPC");
    {
        let o = h.app.npc_manager.get_mut(other_id).expect("other exists");
        o.home = Some(player_loc);
        o.pronouns = "xe/xem".to_string();
    }

    // The mock only matches when the sentinel pronoun is in the prompt/system
    // text, so a canned reply proves the grounding shipped.
    h.mock().push_for("xe/xem", "Aye, a grand day to ye.");
    let mut rx = h.app.world.event_bus.subscribe();
    h.execute_via_real_loop(&format!("talk to {speaker_name}"));

    let said: Vec<String> = drain(&mut rx)
        .iter()
        .filter_map(|ev| match ev {
            GameEvent::DialogueOccurred { npc_said, .. } => npc_said.as_ref().cloned(),
            _ => None,
        })
        .collect();
    assert!(
        said.iter().any(|t| t.contains("grand day")),
        "the system prompt must ground the known NPC's pronoun (xe/xem) — the mock \
         only fires when that token is present; got: {said:?}"
    );
}
