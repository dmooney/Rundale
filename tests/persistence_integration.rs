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
