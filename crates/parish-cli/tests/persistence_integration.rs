//! Integration tests for the Phase 4 persistence system.
//!
//! Tests save/load round-trips, branching, and journal replay
//! through the GameTestHarness.

use parish::testing::{ActionResult, GameTestHarness};

#[test]
fn test_save_and_load_roundtrip() {
    let mut h = GameTestHarness::new();

    // Move somewhere
    h.execute("go to crossroads");
    assert_eq!(h.player_location(), "The Crossroads");

    // Save
    let result = h.execute("/save");
    assert!(matches!(result, ActionResult::SystemCommand { .. }));

    // Move again
    h.execute("go to pub");
    assert_eq!(h.player_location(), "Darcy's Pub");

    // Load main — should go back to crossroads (last save)
    let result = h.execute("/load main");
    if let ActionResult::SystemCommand { response } = result {
        assert!(
            response.contains("Loaded branch"),
            "expected load confirmation, got: {}",
            response
        );
    }
    assert_eq!(h.player_location(), "The Crossroads");
}

#[test]
fn test_fork_creates_independent_branch() {
    let mut h = GameTestHarness::new();

    // Start at village, move to crossroads, save
    h.execute("go to crossroads");
    h.execute("/save");

    // Fork to test branch
    let result = h.execute("/fork test");
    if let ActionResult::SystemCommand { response } = result {
        assert!(
            response.contains("Forked to branch"),
            "expected fork confirmation, got: {}",
            response
        );
    }

    // Move on the forked branch
    h.execute("go to pub");
    assert_eq!(h.player_location(), "Darcy's Pub");

    // Load main — should be at crossroads (not pub)
    h.execute("/load main");
    assert_eq!(h.player_location(), "The Crossroads");

    // Load test — should be at pub
    h.execute("/load test");
    assert_eq!(h.player_location(), "Darcy's Pub");
}

#[test]
fn test_branches_lists_all() {
    let mut h = GameTestHarness::new();

    h.execute("/fork alpha");
    h.execute("/fork beta");

    let result = h.execute("/branches");
    if let ActionResult::SystemCommand { response } = result {
        assert!(response.contains("main"), "should list main branch");
        assert!(response.contains("alpha"), "should list alpha branch");
        assert!(response.contains("beta"), "should list beta branch");
    } else {
        panic!("expected SystemCommand result");
    }
}

#[test]
fn test_log_shows_snapshots() {
    let mut h = GameTestHarness::new();

    // Save a few times
    h.execute("/save");
    h.execute("/save");

    let result = h.execute("/log");
    if let ActionResult::SystemCommand { response } = result {
        assert!(
            response.contains("Snapshot history"),
            "should show snapshot history, got: {}",
            response
        );
    } else {
        panic!("expected SystemCommand result");
    }
}

#[test]
fn test_load_nonexistent_branch() {
    let mut h = GameTestHarness::new();
    let result = h.execute("/load nonexistent");
    if let ActionResult::SystemCommand { response } = result {
        assert!(
            response.contains("No branch named"),
            "should report missing branch, got: {}",
            response
        );
    }
}

#[test]
fn test_quit_returns_quit_result() {
    let mut h = GameTestHarness::new();
    let result = h.execute("/quit");
    assert!(matches!(result, ActionResult::Quit));
}

#[test]
fn test_save_preserves_weather() {
    let mut h = GameTestHarness::new();

    // Change weather (directly, since we don't have a weather command)
    h.app.world.weather = parish::world::Weather::Storm;
    h.execute("/save");

    // Change it again
    h.app.world.weather = parish::world::Weather::Clear;

    // Load — should restore Storm
    h.execute("/load main");
    assert_eq!(*h.weather(), parish::world::Weather::Storm);
}

#[test]
fn test_save_preserves_text_log() {
    let mut h = GameTestHarness::new();

    let initial_log_len = h.text_log().len();
    h.execute("look");
    assert!(h.text_log().len() > initial_log_len);

    h.execute("/save");
    let saved_log_len = h.text_log().len();

    // Clear the log (simulate change)
    h.app.world.text_log.clear();
    assert_eq!(h.text_log().len(), 0);

    // Load should restore log
    h.execute("/load main");
    // The load adds its own log entry, so len may be >= saved
    assert!(h.text_log().len() >= saved_log_len);
}

/// Regression: every dynamic `WorldState` field captured by `GameSnapshot`
/// must round-trip through save/load with no loss.
///
/// This is the "if you add a new field to `GameSnapshot`, don't forget to
/// serialize AND restore it" test — it mutates every field, saves, mutates
/// them again, loads, and asserts the reloaded values match the saved ones.
#[test]
fn test_full_world_state_roundtrip() {
    use parish::world::time::GameSpeed;
    use parish_types::{ConversationExchange, LocationId, NpcId};

    let mut h = GameTestHarness::new();

    // ----- 1. Mutate every snapshot field -----

    // (a) player_location + visited_locations + edge_traversals:
    // moving through apply_movement records the traversal and visited set.
    h.execute("go to crossroads");
    h.execute("go to pub");
    let expected_location = h.app.world.player_location;
    let expected_visited = h.app.world.visited_locations.clone();
    let expected_edges = h.app.world.edge_traversals.clone();
    assert!(
        !expected_edges.is_empty(),
        "player movement should record at least one edge traversal"
    );
    assert!(expected_visited.len() >= 3);

    // (b) weather
    h.app.world.weather = parish::world::Weather::Storm;

    // (c) text_log: execute a look so the log contains something meaningful.
    h.execute("look");
    let expected_log = h.app.world.text_log.clone();
    assert!(!expected_log.is_empty());

    // (d) clock: advance, set speed, pause.
    h.advance_time(45);
    h.app.world.clock.set_speed(GameSpeed::Fast);
    h.app.world.clock.pause();
    let expected_game_time = h.app.world.clock.now();
    let expected_speed = h.app.world.clock.speed_factor();
    let expected_paused = h.app.world.clock.is_paused();

    // (e) gossip_network
    let gossip_id = h.app.world.gossip_network.create(
        "The high king's cousin was seen at the crossroads".to_string(),
        NpcId(1),
        expected_game_time,
    );
    let expected_gossip = h.app.world.gossip_network.clone();
    assert!(!expected_gossip.known_by(NpcId(1)).is_empty());

    // (f) conversation_log
    h.app.world.conversation_log.add(ConversationExchange {
        timestamp: expected_game_time,
        speaker_id: NpcId(1),
        speaker_name: "Padraig Darcy".to_string(),
        player_input: "a word, friend?".to_string(),
        npc_dialogue: "Aye, what is it?".to_string(),
        location: LocationId(1),
    });
    let expected_conversation = h.app.world.conversation_log.clone();

    // (g) npc state: move an NPC so manager state differs from load baseline
    let npc_ids: Vec<NpcId> = h.app.npc_manager.all_npcs().map(|n| n.id).collect();
    let moved_npc_id = npc_ids
        .first()
        .copied()
        .expect("harness mod must load at least one NPC");
    let new_loc = h.app.world.player_location;
    let expected_npc_loc = new_loc;
    if let Some(npc) = h.app.npc_manager.get_mut(moved_npc_id) {
        npc.location = new_loc;
    }

    // ----- 2. Save -----
    let save_result = h.execute("/save");
    assert!(matches!(save_result, ActionResult::SystemCommand { .. }));

    // ----- 3. Mutate every field again so a failed restore is visible -----
    h.app.world.weather = parish::world::Weather::Clear;
    h.app.world.text_log.clear();
    h.app.world.player_location = LocationId(1);
    h.app.world.visited_locations.clear();
    h.app.world.edge_traversals.clear();
    h.app.world.gossip_network = parish_types::GossipNetwork::new();
    h.app.world.conversation_log = parish_types::ConversationLog::new();
    h.app.world.clock.resume();
    h.app.world.clock.set_speed(GameSpeed::Normal);
    if let Some(npc) = h.app.npc_manager.get_mut(moved_npc_id) {
        npc.location = LocationId(999);
    }

    // ----- 4. Load -----
    h.execute("/load main");

    // ----- 5. Verify each field round-tripped -----

    // player_location
    assert_eq!(h.app.world.player_location, expected_location);

    // weather
    assert_eq!(h.app.world.weather, parish::world::Weather::Storm);

    // text_log (load appends its own confirmation line, so we assert the
    // saved prefix survives — not strict equality)
    let restored_log = &h.app.world.text_log;
    assert!(
        restored_log.len() >= expected_log.len(),
        "text_log should be at least as long as the saved version"
    );
    for (i, line) in expected_log.iter().enumerate() {
        assert_eq!(&restored_log[i], line, "text_log line {i} mismatch");
    }

    // clock
    assert_eq!(h.app.world.clock.now(), expected_game_time);
    assert!(
        (h.app.world.clock.speed_factor() - expected_speed).abs() < f64::EPSILON,
        "speed_factor should round-trip: {} vs {}",
        h.app.world.clock.speed_factor(),
        expected_speed
    );
    assert_eq!(h.app.world.clock.is_paused(), expected_paused);

    // visited_locations
    assert_eq!(
        h.app.world.visited_locations, expected_visited,
        "visited_locations must round-trip"
    );

    // edge_traversals
    assert_eq!(
        h.app.world.edge_traversals, expected_edges,
        "edge_traversals must round-trip"
    );

    // gossip_network
    assert_eq!(
        h.app.world.gossip_network, expected_gossip,
        "gossip_network must round-trip"
    );
    // The specific gossip item we created should still be present
    let restored_items = h.app.world.gossip_network.known_by(NpcId(1));
    assert!(
        restored_items.iter().any(|g| g.id == gossip_id),
        "gossip item {gossip_id} should survive round-trip"
    );

    // conversation_log
    assert_eq!(
        h.app.world.conversation_log, expected_conversation,
        "conversation_log must round-trip"
    );

    // NPC state was restored
    let moved_npc = h
        .app
        .npc_manager
        .get(moved_npc_id)
        .expect("NPC should still exist after load");
    assert_eq!(
        moved_npc.location, expected_npc_loc,
        "moved NPC's location must round-trip"
    );
}

#[test]
fn test_fork_preserves_npc_state() {
    let mut h = GameTestHarness::new();

    // Advance time to move NPCs
    h.advance_time(120);
    h.execute("/save");

    let npcs_before: Vec<String> = h.npcs_here().iter().map(|s| s.to_string()).collect();

    h.execute("/fork npc-test");

    // The fork should have the same NPCs at the same locations
    let npcs_after: Vec<String> = h.npcs_here().iter().map(|s| s.to_string()).collect();
    assert_eq!(npcs_before, npcs_after);
}
