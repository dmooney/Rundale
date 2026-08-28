//! Integration tests for the GameTestHarness.
//!
//! These tests exercise multi-step game scenarios through the harness,
//! verifying movement, time advancement, descriptions, commands, and
//! NPC canned responses all work end-to-end.

use parish_engine::testing::{ActionResult, GameTestHarness};
use parish_engine::world::DEFAULT_START_LOCATION;
use parish_engine::world::time::{Season, TimeOfDay};

#[test]
fn test_full_walkthrough_crossroads_to_pub_and_back() {
    let mut h = GameTestHarness::new();

    // Start at Kilteevan Village
    assert_eq!(h.player_location(), "Kilteevan Village");
    assert_eq!(h.location_id(), DEFAULT_START_LOCATION);

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
    let start = h.app.world.clock.now();
    assert_eq!(h.time_of_day(), TimeOfDay::Morning);

    // Move through the world until we've spent at least two in-game hours
    // traveling. This keeps the test stable if map coordinates change.
    let mut elapsed_minutes = match h.execute("go to crossroads") {
        ActionResult::Moved { minutes, .. } => i64::from(minutes),
        other => panic!("expected initial move to crossroads, got {other:?}"),
    };

    while elapsed_minutes < 120 {
        let to_pub = h.execute("go to pub");
        let ActionResult::Moved { minutes, .. } = to_pub else {
            panic!("expected move to pub, got {to_pub:?}");
        };
        elapsed_minutes += i64::from(minutes);

        let to_crossroads = h.execute("go to crossroads");
        let ActionResult::Moved { minutes, .. } = to_crossroads else {
            panic!("expected move to crossroads, got {to_crossroads:?}");
        };
        elapsed_minutes += i64::from(minutes);
    }

    // Upstream world-data changes can keep short trips within the same coarse
    // time-of-day bucket, but the game clock itself should still move forward.
    let end = h.app.world.clock.now();
    assert!(end > start, "Time should advance after repeated travel");
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
fn test_npc_canned_response_at_pub() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig Darcy", "Top of the morning to ye!");

    // Advance to 10am when Padraig is scheduled at the pub (9-22)
    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");

    let r = h.execute("hello Padraig");
    if let ActionResult::NpcResponse { npc, dialogue, .. } = r {
        assert_eq!(npc, "Padraig Darcy");
        assert_eq!(dialogue, "Top of the morning to ye!");
    } else {
        panic!("Expected NpcResponse, got {:?}", r);
    }
}

#[test]
fn test_npc_canned_responses_consumed_in_order() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig Darcy", "First line");
    h.add_canned_response("Padraig Darcy", "Second line");
    h.add_canned_response("Padraig Darcy", "Third line");

    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");
    let r1 = h.execute("hello");
    let r2 = h.execute("how are you");
    let r3 = h.execute("tell me about the town");

    assert_eq!(
        r1,
        ActionResult::NpcResponse {
            npc: "Padraig Darcy".to_string(),
            dialogue: "First line".to_string(),
            anachronisms: vec![],
        }
    );
    assert_eq!(
        r2,
        ActionResult::NpcResponse {
            npc: "Padraig Darcy".to_string(),
            dialogue: "Second line".to_string(),
            anachronisms: vec![],
        }
    );
    assert_eq!(
        r3,
        ActionResult::NpcResponse {
            npc: "Padraig Darcy".to_string(),
            dialogue: "Third line".to_string(),
            anachronisms: vec![],
        }
    );
}

/// #1228 (live, harness end-to-end): a degenerate NPC reply that repeats the
/// same clause many times must be collapsed to a single clause before it
/// reaches the player. Drives the real `GameTestHarness` dialogue path
/// (`parish_core::game_session::apply_npc_dialogue_turn`), so this is an
/// integration-level proof, not a unit test of the helper.
#[test]
fn test_npc_intra_line_repetition_collapsed_at_pub() {
    let mut h = GameTestHarness::new();
    let clause = "Speak yer mind, and we'll see what be in it, m'friend.";
    // 20 verbatim repeats — the exact #1228 degenerate-loop shape.
    h.add_canned_response("Padraig Darcy", &clause.repeat(20));

    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");

    let r = h.execute("what be the talk of the town");
    if let ActionResult::NpcResponse { dialogue, .. } = r {
        let occurrences = dialogue.matches("Speak yer mind").count();
        assert_eq!(
            occurrences, 1,
            "#1228: runaway clause must collapse to one instance, got {occurrences}:\n{dialogue}"
        );
    } else {
        panic!("Expected NpcResponse, got {r:?}");
    }
}

/// #1228 (live, harness end-to-end): when an NPC's fresh line is near-identical
/// to its own previous line at this location, the guard substitutes a varied
/// fallback so the player does not see the same line twice in a row.
#[test]
fn test_npc_cross_turn_repetition_varied_at_pub() {
    let mut h = GameTestHarness::new();
    let line = "Sure, the talk 'round here be always of the weather and the rents, m'friend.";
    h.add_canned_response("Padraig Darcy", line);
    // Second turn echoes the first near-verbatim (only trailing punctuation).
    h.add_canned_response(
        "Padraig Darcy",
        "Sure, the talk 'round here be always of the weather and the rents, m'friend!",
    );

    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");

    let first = h.execute("what be the talk of the town");
    let ActionResult::NpcResponse {
        dialogue: first_line,
        ..
    } = first
    else {
        panic!("Expected NpcResponse on first turn, got {first:?}");
    };

    let second = h.execute("tell me again, what be the talk");
    let ActionResult::NpcResponse {
        dialogue: second_line,
        ..
    } = second
    else {
        panic!("Expected NpcResponse on second turn, got {second:?}");
    };

    assert!(!second_line.trim().is_empty(), "#1228: fallback non-empty");
    assert_ne!(
        first_line.to_lowercase().trim_end_matches(['.', '!', '?']),
        second_line.to_lowercase().trim_end_matches(['.', '!', '?']),
        "#1228: second line must be varied, not a verbatim repeat:\n1: {first_line}\n2: {second_line}"
    );
}

/// Regression: when the player's input contains anachronistic terms, the
/// harness's `NpcResponse` result must surface those terms in its
/// `anachronisms` field so UI layers can highlight them.
#[test]
fn test_npc_response_surfaces_anachronism_terms() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig Darcy", "What's that now?");

    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");

    // "telephone" and "computer" are in the rundale anachronism list.
    let r = h.execute("can I borrow your telephone to call the computer shop?");
    match r {
        ActionResult::NpcResponse {
            anachronisms,
            dialogue,
            ..
        } => {
            assert_eq!(dialogue, "What's that now?");
            assert!(
                anachronisms.iter().any(|t| t == "telephone"),
                "expected 'telephone' in anachronisms, got {:?}",
                anachronisms
            );
            assert!(
                anachronisms.iter().any(|t| t == "computer"),
                "expected 'computer' in anachronisms, got {:?}",
                anachronisms
            );
        }
        other => panic!("expected NpcResponse, got {:?}", other),
    }
}

/// Regression: period-appropriate input must NOT surface any anachronism
/// terms — the field must be empty, not merely absent.
#[test]
fn test_npc_response_anachronism_field_empty_for_period_input() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig Darcy", "Ah, grand.");

    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");

    let r = h.execute("good day to you, Padraig");
    match r {
        ActionResult::NpcResponse { anachronisms, .. } => {
            assert!(
                anachronisms.is_empty(),
                "period-appropriate input should produce no anachronisms, got {:?}",
                anachronisms
            );
        }
        other => panic!("expected NpcResponse, got {:?}", other),
    }
}

#[test]
fn test_npc_not_available_after_canned_exhausted() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig Darcy", "Only response");

    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");
    h.execute("hello");
    let r = h.execute("hello again");
    assert_eq!(r, ActionResult::NpcNotAvailable);
}

#[test]
fn test_npc_not_present_at_empty_location() {
    let mut h = GameTestHarness::new();

    // Navigate to hurling green — no NPCs start there
    h.execute("go to crossroads");
    h.execute("go to hurling green");
    // Freeze the accelerated wall clock so this assertion measures only
    // command-driven time changes, not scheduler delay during the command.
    h.app.world.clock.pause();
    let before = h.app.world.clock.now();
    let r = h.execute("hello");
    let after = h.app.world.clock.now();
    let ActionResult::SystemCommand { response } = r else {
        panic!("expected empty-location dialogue feedback, got {r:?}");
    };
    assert!(
        response.to_lowercase().contains("no one answers")
            || response.to_lowercase().contains("empty"),
        "empty-location response should explain that no one answers: {response:?}"
    );
    assert_eq!(
        after, before,
        "empty-location dialogue must not advance time"
    );
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
    // Verify the walkthrough fixture runs without error. Inject a
    // vanilla harness (character logs off) so this test doesn't write
    // into the live `~/Library/Application Support/<app>/logs/` dir
    // owned by `parish --script`.
    let path = std::path::Path::new("../../testing/fixtures/test_walkthrough.txt");
    assert!(path.exists(), "test_walkthrough.txt fixture must exist");
    parish_engine::testing::run_script_mode_with(path, GameTestHarness::new()).unwrap();
}

#[test]
fn test_script_fixture_movement_errors() {
    let path = std::path::Path::new("../../testing/fixtures/test_movement_errors.txt");
    assert!(path.exists(), "test_movement_errors.txt fixture must exist");
    parish_engine::testing::run_script_mode_with(path, GameTestHarness::new()).unwrap();
}

#[test]
fn test_script_fixture_commands() {
    let path = std::path::Path::new("../../testing/fixtures/test_commands.txt");
    assert!(path.exists(), "test_commands.txt fixture must exist");
    parish_engine::testing::run_script_mode_with(path, GameTestHarness::new()).unwrap();
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
    assert_eq!(*h.weather(), parish_engine::world::Weather::Clear);
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

    // Navigate to the Fairy Fort (multiple hops from Kilteevan).
    // world.json confirms The Fairy Fort (id=11) is connected via The Bog Road
    // (id=12), which is reachable from Kilteevan Village. NotFound is a bug.
    let r = h.execute("go to fairy fort");
    match r {
        ActionResult::Moved { to, minutes, .. } => {
            assert_eq!(to, "The Fairy Fort");
            assert!(minutes > 0);
        }
        other => panic!(
            "Expected Moved to 'The Fairy Fort' but got: {other:?}\n\
             The Fairy Fort exists in world.json (id=11) and is reachable \
             from Kilteevan Village via The Bog Road — NotFound is a bug."
        ),
    }
}

#[test]
fn test_fog_of_war_frontier_at_pub() {
    use parish_engine::ipc::handlers::build_map_data;

    let mut h = GameTestHarness::new();
    // Start at Kilteevan Village
    assert_eq!(h.player_location(), "Kilteevan Village");

    // Move to crossroads then pub
    h.execute("go to crossroads");
    h.execute("go to pub");
    assert_eq!(h.player_location(), "Darcy's Pub");

    let map = build_map_data(
        &h.app.world,
        &parish_core::world::transport::TransportMode::walking(),
        false,
    );

    // The player is at the pub
    assert_eq!(
        map.player_location,
        h.app.world.player_location.0.to_string()
    );

    // We should have visited locations (Kilteevan, Crossroads, Pub) + frontier
    let visited: Vec<_> = map.locations.iter().filter(|l| l.visited).collect();
    let frontier: Vec<_> = map.locations.iter().filter(|l| !l.visited).collect();

    assert_eq!(visited.len(), 3, "should have 3 visited locations");
    assert!(
        !frontier.is_empty(),
        "frontier should include unvisited neighbors of visited locations"
    );

    // Crossroads should be visited and visible
    assert!(
        visited.iter().any(|l| l.name == "The Crossroads"),
        "crossroads should be in visited set"
    );

    // The pub's other neighbors (besides crossroads) should be in frontier
    let pub_id = h.app.world.player_location;
    let pub_neighbors: Vec<_> = h
        .app
        .world
        .graph
        .neighbors(pub_id)
        .iter()
        .map(|(id, _)| *id)
        .collect();
    for neighbor_id in &pub_neighbors {
        if !h.app.world.visited_locations.contains(neighbor_id) {
            let name = h
                .app
                .world
                .graph
                .get(*neighbor_id)
                .map(|d| d.name.as_str())
                .unwrap_or("?");
            assert!(
                frontier.iter().any(|l| l.id == neighbor_id.0.to_string()),
                "unvisited neighbor '{}' should appear in frontier",
                name
            );
        }
    }

    // Edges should connect visited to frontier
    assert!(
        map.edges.len() >= visited.len() - 1,
        "should have edges between visited locations and to frontier"
    );

    eprintln!("=== Map at Pub ===");
    eprintln!(
        "Visited: {:?}",
        visited.iter().map(|l| &l.name).collect::<Vec<_>>()
    );
    eprintln!(
        "Frontier: {:?}",
        frontier.iter().map(|l| &l.name).collect::<Vec<_>>()
    );
    eprintln!("Edges: {}", map.edges.len());
}

/// Regression: the world graph's travel-time calculation is driven by the
/// transport speed, so faster transport modes must produce proportionally
/// shorter travel times for an identical path in the real loaded world.
#[test]
fn test_transport_mode_scales_travel_time() {
    use parish_core::world::transport::TransportMode;

    let h = GameTestHarness::new();
    let graph = &h.app.world.graph;

    // Find Kilteevan Village → The Crossroads → Darcy's Pub
    let start = h
        .app
        .world
        .graph
        .find_by_name("Kilteevan Village")
        .expect("Kilteevan Village should exist in default mod");
    let dest = h
        .app
        .world
        .graph
        .find_by_name("Darcy's Pub")
        .expect("Darcy's Pub should exist in default mod");
    let path = graph
        .shortest_path(start, dest)
        .expect("Kilteevan → Pub must be reachable");

    let walk = TransportMode::walking();
    let cart = TransportMode {
        id: "jaunting_car".to_string(),
        label: "in a jaunting car".to_string(),
        speed_m_per_s: walk.speed_m_per_s * 4.0,
    };

    let walk_minutes = graph.path_travel_time(&path, walk.speed_m_per_s);
    let cart_minutes = graph.path_travel_time(&path, cart.speed_m_per_s);

    assert!(
        walk_minutes > 0,
        "walking travel time must be > 0 for a real path"
    );
    assert!(
        cart_minutes <= walk_minutes,
        "jaunting car ({cart_minutes} min) should not be slower than walking ({walk_minutes} min) on the same path"
    );
    // Cart is 4× faster → at least cut roughly in half even after rounding to u16 minutes.
    // (Travel times are quantised to u16 minutes so we don't assert ≥ 4× exactly.)
    assert!(
        cart_minutes * 2 <= walk_minutes || walk_minutes <= 4,
        "cart time ({cart_minutes}) should be substantially faster than walk time ({walk_minutes})"
    );
}

/// Regression: `format_exits` must reflect the transport label given to it, so
/// narration actually changes when a mod swaps transport modes.
#[test]
fn test_transport_label_surfaces_in_exit_listing() {
    use parish_core::world::description::format_exits;

    let h = GameTestHarness::new();

    let here = h.app.world.player_location;
    let walk = format_exits(here, &h.app.world.graph, 1.25, "on foot");
    let boat = format_exits(here, &h.app.world.graph, 0.5, "by currach");

    assert!(
        walk.contains("on foot"),
        "walk exits should mention 'on foot': {walk}"
    );
    assert!(
        boat.contains("by currach"),
        "boat exits should mention 'by currach': {boat}"
    );
    assert!(
        !walk.contains("by currach"),
        "walk exits should not leak the boat label"
    );
}

/// Regression: slower transport produces longer quoted times in the exit
/// listing for the same neighbor.
#[test]
fn test_slower_transport_lengthens_exit_durations() {
    use parish_core::world::description::format_exits;

    let h = GameTestHarness::new();
    let here = h.app.world.player_location;

    let fast = format_exits(here, &h.app.world.graph, 5.0, "on horseback");
    let slow = format_exits(here, &h.app.world.graph, 0.5, "crawling");

    // Each exit line is `NAME (N min LABEL)`; extract the numeric minute count.
    fn total_minutes(exits: &str) -> u32 {
        exits
            .split(',')
            .filter_map(|segment| {
                let open = segment.find('(')?;
                let rest = &segment[open + 1..];
                let space = rest.find(' ')?;
                rest[..space].parse::<u32>().ok()
            })
            .sum()
    }

    let fast_total = total_minutes(&fast);
    let slow_total = total_minutes(&slow);
    assert!(
        slow_total > fast_total,
        "crawling total ({slow_total}) should exceed horseback total ({fast_total}); fast='{fast}' slow='{slow}'"
    );
}
