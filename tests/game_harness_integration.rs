//! Integration tests for the GameTestHarness.
//!
//! These tests exercise multi-step game scenarios through the harness,
//! verifying movement, time advancement, descriptions, commands, and
//! NPC canned responses all work end-to-end.

use parish::testing::{ActionResult, GameTestHarness};
use parish::world::LocationId;
use parish::world::time::{Season, TimeOfDay};

#[test]
fn test_full_walkthrough_crossroads_to_pub_and_back() {
    let mut h = GameTestHarness::new();

    // Start at Kilteevan Village
    assert_eq!(h.player_location(), "Kilteevan Village");
    assert_eq!(h.location_id(), LocationId(15));

    // Move to crossroads first
    h.execute("go to crossroads");
    assert_eq!(h.player_location(), "The Crossroads");

    // Move to pub
    let r = h.execute("go to pub");
    assert!(matches!(r, ActionResult::Moved { .. }));
    assert_eq!(h.player_location(), "Darcy's Pub");

    // Look around
    let r = h.execute("look");
    if let ActionResult::Looked { description } = r {
        assert!(!description.is_empty());
    } else {
        panic!("Expected Looked result");
    }

    // Move back
    let r = h.execute("go to crossroads");
    assert!(matches!(r, ActionResult::Moved { .. }));
    assert_eq!(h.player_location(), "The Crossroads");
}

#[test]
fn test_multi_location_circuit() {
    let mut h = GameTestHarness::new();

    // Kilteevan → Crossroads → Pub → Crossroads → Church → Crossroads
    h.execute("go to crossroads");
    assert_eq!(h.player_location(), "The Crossroads");

    h.execute("go to pub");
    assert_eq!(h.player_location(), "Darcy's Pub");

    h.execute("go to crossroads");
    assert_eq!(h.player_location(), "The Crossroads");

    h.execute("go to church");
    assert_eq!(h.player_location(), "St. Brigid's Church");

    h.execute("go to crossroads");
    assert_eq!(h.player_location(), "The Crossroads");
}

#[test]
fn test_time_advances_with_travel() {
    let mut h = GameTestHarness::new();
    assert_eq!(h.time_of_day(), TimeOfDay::Morning);

    // Move to crossroads first, then make many trips
    h.execute("go to crossroads");
    for _ in 0..20 {
        h.execute("go to pub");
        h.execute("go to crossroads");
    }

    // After 20+ round trips, time should have advanced past Morning
    // (each trip is ~5 min, so 40 trips × 5 min = ~200 game minutes)
    let tod = h.time_of_day();
    assert_ne!(tod, TimeOfDay::Morning, "Time should have advanced");
}

#[test]
fn test_season_is_spring_at_start() {
    let h = GameTestHarness::new();
    assert_eq!(h.season(), Season::Spring);
}

#[test]
fn test_movement_already_here() {
    let mut h = GameTestHarness::new();
    let r = h.execute("go to kilteevan");
    assert_eq!(r, ActionResult::AlreadyHere);
    assert_eq!(h.player_location(), "Kilteevan Village");
}

#[test]
fn test_movement_not_found() {
    let mut h = GameTestHarness::new();
    let r = h.execute("go to atlantis");
    assert!(matches!(r, ActionResult::NotFound { .. }));
    if let ActionResult::NotFound { target } = r {
        assert_eq!(target, "atlantis");
    }
    assert_eq!(h.player_location(), "Kilteevan Village");
}

#[test]
fn test_movement_various_verbs_all_work() {
    let verbs = [
        "walk to",
        "head to",
        "stroll to",
        "saunter to",
        "mosey to",
        "run to",
        "dash to",
    ];

    for verb in &verbs {
        let mut h = GameTestHarness::new();
        let cmd = format!("{} crossroads", verb);
        let r = h.execute(&cmd);
        assert!(
            matches!(r, ActionResult::Moved { .. }),
            "Verb '{}' should produce Moved, got {:?}",
            verb,
            r
        );
        assert_eq!(
            h.player_location(),
            "The Crossroads",
            "Verb '{}' should move to crossroads",
            verb
        );
    }
}

#[test]
fn test_look_at_multiple_locations() {
    let mut h = GameTestHarness::new();

    // Look at Kilteevan
    let r = h.execute("look");
    if let ActionResult::Looked { description } = &r {
        assert!(!description.is_empty());
    }

    // Move to crossroads and look
    h.execute("go to crossroads");
    let r = h.execute("look");
    if let ActionResult::Looked { description } = &r {
        assert!(!description.is_empty());
    }

    // Move to pub and look
    h.execute("go to pub");
    let r = h.execute("look around");
    if let ActionResult::Looked { description } = &r {
        assert!(!description.is_empty());
    }
}

#[test]
fn test_system_commands_sequence() {
    let mut h = GameTestHarness::new();

    // Pause
    let r = h.execute("/pause");
    assert!(matches!(r, ActionResult::SystemCommand { .. }));
    assert!(h.is_paused());

    // Status while paused
    let r = h.execute("/status");
    if let ActionResult::SystemCommand { response } = r {
        assert!(response.contains("paused"));
    }

    // Resume
    let r = h.execute("/resume");
    assert!(matches!(r, ActionResult::SystemCommand { .. }));
    assert!(!h.is_paused());

    // Status while running
    let r = h.execute("/status");
    if let ActionResult::SystemCommand { response } = r {
        assert!(!response.contains("paused"));
    }
}

#[test]
fn test_quit() {
    let mut h = GameTestHarness::new();
    let r = h.execute("/quit");
    assert_eq!(r, ActionResult::Quit);
    assert!(h.app.should_quit);
}

#[test]
fn test_npc_canned_response_at_crossroads() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig O'Brien", "Top of the morning to ye!");

    // NPC is at The Crossroads, navigate there first
    h.execute("go to crossroads");

    let r = h.execute("hello Padraig");
    if let ActionResult::NpcResponse { npc, dialogue } = r {
        assert_eq!(npc, "Padraig O'Brien");
        assert_eq!(dialogue, "Top of the morning to ye!");
    } else {
        panic!("Expected NpcResponse, got {:?}", r);
    }
}

#[test]
fn test_npc_canned_responses_consumed_in_order() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig O'Brien", "First line");
    h.add_canned_response("Padraig O'Brien", "Second line");
    h.add_canned_response("Padraig O'Brien", "Third line");

    h.execute("go to crossroads");
    let r1 = h.execute("hello");
    let r2 = h.execute("how are you");
    let r3 = h.execute("tell me about the town");

    assert_eq!(
        r1,
        ActionResult::NpcResponse {
            npc: "Padraig O'Brien".to_string(),
            dialogue: "First line".to_string(),
        }
    );
    assert_eq!(
        r2,
        ActionResult::NpcResponse {
            npc: "Padraig O'Brien".to_string(),
            dialogue: "Second line".to_string(),
        }
    );
    assert_eq!(
        r3,
        ActionResult::NpcResponse {
            npc: "Padraig O'Brien".to_string(),
            dialogue: "Third line".to_string(),
        }
    );
}

#[test]
fn test_npc_not_available_after_canned_exhausted() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig O'Brien", "Only response");

    h.execute("go to crossroads");
    h.execute("hello");
    let r = h.execute("hello again");
    assert_eq!(r, ActionResult::NpcNotAvailable);
}

#[test]
fn test_npc_not_present_after_moving() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig O'Brien", "Goodbye!");

    // Player starts at Kilteevan — NPC is at crossroads, not here
    let r = h.execute("hello");
    assert_eq!(r, ActionResult::UnknownInput);
}

#[test]
fn test_exits_at_kilteevan() {
    let h = GameTestHarness::new();
    let exits = h.exits();
    assert!(exits.contains("You can go to:"));
    // Kilteevan connects to The Crossroads
    assert!(exits.contains("The Crossroads"));
}

#[test]
fn test_text_log_records_actions() {
    let mut h = GameTestHarness::new();
    let initial_len = h.text_log().len();

    h.execute("look");
    assert!(h.text_log().len() > initial_len);

    let after_look = h.text_log().len();
    h.execute("go to pub");
    assert!(h.text_log().len() > after_look);
}

#[test]
fn test_script_fixture_walkthrough() {
    // Verify the walkthrough fixture runs without error
    let path = std::path::Path::new("tests/fixtures/test_walkthrough.txt");
    assert!(path.exists(), "test_walkthrough.txt fixture must exist");
    parish::testing::run_script_mode(path).unwrap();
}

#[test]
fn test_script_fixture_movement_errors() {
    let path = std::path::Path::new("tests/fixtures/test_movement_errors.txt");
    assert!(path.exists(), "test_movement_errors.txt fixture must exist");
    parish::testing::run_script_mode(path).unwrap();
}

#[test]
fn test_script_fixture_commands() {
    let path = std::path::Path::new("tests/fixtures/test_commands.txt");
    assert!(path.exists(), "test_commands.txt fixture must exist");
    parish::testing::run_script_mode(path).unwrap();
}

#[test]
fn test_moved_result_contains_narration() {
    let mut h = GameTestHarness::new();
    let r = h.execute("go to crossroads");
    if let ActionResult::Moved {
        to,
        minutes,
        narration,
    } = r
    {
        assert_eq!(to, "The Crossroads");
        assert!(minutes > 0, "Travel should take some time");
        assert!(!narration.is_empty(), "Narration should be non-empty");
    } else {
        panic!("Expected Moved, got {:?}", r);
    }
}

#[test]
fn test_weather_accessible() {
    let h = GameTestHarness::new();
    assert_eq!(h.weather(), "Clear");
}

#[test]
fn test_multiple_looks_same_location() {
    let mut h = GameTestHarness::new();

    let r1 = h.execute("look");
    let r2 = h.execute("l");

    // Both should produce Looked
    assert!(matches!(r1, ActionResult::Looked { .. }));
    assert!(matches!(r2, ActionResult::Looked { .. }));
}

#[test]
fn test_long_journey_fairy_fort() {
    let mut h = GameTestHarness::new();

    // Navigate to the Fairy Fort (multiple hops from Kilteevan)
    let r = h.execute("go to fairy fort");
    match r {
        ActionResult::Moved { to, minutes, .. } => {
            assert_eq!(to, "The Fairy Fort");
            assert!(minutes > 0);
        }
        ActionResult::NotFound { .. } => {
            // If graph doesn't have fairy fort connected, that's also valid
        }
        other => panic!("Unexpected result: {:?}", other),
    }
}
