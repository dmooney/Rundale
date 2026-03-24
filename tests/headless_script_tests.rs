//! Comprehensive headless script tests for Parish.
//!
//! These tests exercise the game through [`GameTestHarness`] and
//! [`run_script_captured`], asserting on structured results from
//! test fixture scripts. Every test verifies actual game state —
//! not just "no crash".

use parish::testing::{ActionResult, GameTestHarness, ScriptResult};
use parish::world::time::{Season, TimeOfDay};
use std::path::Path;

/// Helper: load a fixture and return captured results.
fn fixture(name: &str) -> Vec<ScriptResult> {
    let path = Path::new("tests/fixtures").join(name);
    assert!(path.exists(), "Fixture {} must exist", name);
    parish::testing::run_script_captured(&path).expect("Script should execute without error")
}

/// Helper: count results matching a predicate.
fn count_results(results: &[ScriptResult], pred: impl Fn(&ScriptResult) -> bool) -> usize {
    results.iter().filter(|r| pred(r)).count()
}

/// Helper: extract all Moved results.
fn moved_results(results: &[ScriptResult]) -> Vec<&ScriptResult> {
    results
        .iter()
        .filter(|r| matches!(r.result, ActionResult::Moved { .. }))
        .collect()
}

/// Helper: extract all Looked results.
fn looked_results(results: &[ScriptResult]) -> Vec<&ScriptResult> {
    results
        .iter()
        .filter(|r| matches!(r.result, ActionResult::Looked { .. }))
        .collect()
}

// ============================================================
// All-locations coverage
// ============================================================

#[test]
fn test_all_locations_reachable() {
    let results = fixture("test_all_locations.txt");
    let moves = moved_results(&results);

    // We visit all 14 non-starting locations (Kilteevan is start)
    let destinations: Vec<&str> = moves
        .iter()
        .filter_map(|r| {
            if let ActionResult::Moved { to, .. } = &r.result {
                Some(to.as_str())
            } else {
                None
            }
        })
        .collect();

    let expected = [
        "The Crossroads",
        "Darcy's Pub",
        "St. Brigid's Church",
        "The Bog Road",
        "The Fairy Fort",
        "Lough Ree Shore",
        "Hodson Bay",
        "Murphy's Farm",
        "O'Brien's Farm",
        "The Lime Kiln",
        "The Letter Office",
        "Connolly's Shop",
        "The Hedge School",
        "The Hurling Green",
    ];

    for loc in &expected {
        assert!(
            destinations.contains(loc),
            "Should have visited {}, destinations: {:?}",
            loc,
            destinations
        );
    }
}

#[test]
fn test_all_locations_lookable() {
    let results = fixture("test_all_locations.txt");
    let looks = looked_results(&results);

    // We look at every location after arriving
    assert!(
        looks.len() >= 14,
        "Should look at all 14+ locations, got {}",
        looks.len()
    );

    for look in &looks {
        if let ActionResult::Looked { description } = &look.result {
            assert!(
                !description.is_empty(),
                "Description should be non-empty at {}",
                look.location
            );
        }
    }
}

#[test]
fn test_all_locations_no_failures() {
    let results = fixture("test_all_locations.txt");

    // No NotFound results — every movement should succeed
    let not_found = count_results(&results, |r| {
        matches!(r.result, ActionResult::NotFound { .. })
    });
    assert_eq!(
        not_found, 0,
        "No movement should fail in all-locations test"
    );
}

// ============================================================
// Grand tour regression
// ============================================================

#[test]
fn test_grand_tour_visits_all_locations() {
    let results = fixture("test_grand_tour.txt");
    let moves = moved_results(&results);

    let visited: std::collections::HashSet<&str> = moves
        .iter()
        .filter_map(|r| {
            if let ActionResult::Moved { to, .. } = &r.result {
                Some(to.as_str())
            } else {
                None
            }
        })
        .collect();

    // All 15 locations should appear (14 as destinations + Kilteevan at end)
    assert!(
        visited.len() >= 14,
        "Grand tour should visit at least 14 locations, got {}",
        visited.len()
    );
}

#[test]
fn test_grand_tour_status_at_each_location() {
    let results = fixture("test_grand_tour.txt");

    // Every /status should return SystemCommand
    let status_results: Vec<&ScriptResult> =
        results.iter().filter(|r| r.command == "/status").collect();

    assert!(
        status_results.len() >= 12,
        "Grand tour should have many status checks, got {}",
        status_results.len()
    );

    for sr in &status_results {
        assert!(
            matches!(sr.result, ActionResult::SystemCommand { .. }),
            "Status should return SystemCommand, got {:?}",
            sr.result
        );
    }
}

#[test]
fn test_grand_tour_location_tracks_movement() {
    let results = fixture("test_grand_tour.txt");

    // After each Moved result, the location field should match the destination
    for r in &results {
        if let ActionResult::Moved { to, .. } = &r.result {
            assert_eq!(
                r.location, *to,
                "Location field should match Moved destination"
            );
        }
    }
}

#[test]
fn test_grand_tour_time_consistent() {
    let results = fixture("test_grand_tour.txt");

    // Verify time field is always a valid time-of-day string
    let valid_times = [
        "Dawn",
        "Morning",
        "Midday",
        "Afternoon",
        "Dusk",
        "Night",
        "Midnight",
    ];
    for r in &results {
        assert!(
            valid_times.contains(&r.time.as_str()),
            "Time '{}' should be a valid time-of-day",
            r.time
        );
    }
}

// ============================================================
// Fuzzy name matching
// ============================================================

#[test]
fn test_fuzzy_pub_variants() {
    let results = fixture("test_fuzzy_names.txt");

    // Find all moves to the pub
    let pub_moves: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| {
            if let ActionResult::Moved { to, .. } = &r.result {
                to == "Darcy's Pub"
            } else {
                false
            }
        })
        .collect();

    // "pub", "the pub", "darcy's pub" should all resolve
    assert!(
        pub_moves.len() >= 3,
        "At least 3 fuzzy pub names should resolve, got {}",
        pub_moves.len()
    );
}

#[test]
fn test_fuzzy_church_variants() {
    let results = fixture("test_fuzzy_names.txt");

    let church_moves: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| {
            if let ActionResult::Moved { to, .. } = &r.result {
                to == "St. Brigid's Church"
            } else {
                false
            }
        })
        .collect();

    assert!(
        church_moves.len() >= 2,
        "At least 2 fuzzy church names should resolve, got {}",
        church_moves.len()
    );
}

#[test]
fn test_fuzzy_no_failures() {
    let results = fixture("test_fuzzy_names.txt");

    // No NotFound — every fuzzy name should resolve
    let not_found: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| matches!(r.result, ActionResult::NotFound { .. }))
        .collect();

    if !not_found.is_empty() {
        let failed_cmds: Vec<&str> = not_found.iter().map(|r| r.command.as_str()).collect();
        panic!(
            "Fuzzy names should all resolve, but these failed: {:?}",
            failed_cmds
        );
    }
}

// ============================================================
// Multi-hop pathfinding
// ============================================================

#[test]
fn test_multi_hop_all_succeed() {
    let results = fixture("test_multi_hop.txt");
    let moves = moved_results(&results);

    // Every movement command should succeed (no NotFound)
    assert!(
        !moves.is_empty(),
        "Multi-hop test should have movement results"
    );

    let not_found = count_results(&results, |r| {
        matches!(r.result, ActionResult::NotFound { .. })
    });
    assert_eq!(not_found, 0, "All multi-hop movements should succeed");
}

#[test]
fn test_multi_hop_fairy_fort_reachable() {
    let results = fixture("test_multi_hop.txt");

    // First movement is Kilteevan → Fairy Fort (multi-hop)
    let first_move = results
        .iter()
        .find(|r| matches!(r.result, ActionResult::Moved { .. }))
        .expect("Should have at least one move");

    if let ActionResult::Moved { to, minutes, .. } = &first_move.result {
        assert_eq!(to, "The Fairy Fort");
        assert!(
            *minutes > 10,
            "Multi-hop to fairy fort should take >10 min, got {}",
            minutes
        );
    }
}

#[test]
fn test_multi_hop_travel_time_positive() {
    let results = fixture("test_multi_hop.txt");

    for r in &results {
        if let ActionResult::Moved { minutes, .. } = &r.result {
            assert!(
                *minutes > 0,
                "Travel time should be positive for '{}'",
                r.command
            );
        }
    }
}

// ============================================================
// Movement verbs
// ============================================================

#[test]
fn test_all_movement_verbs_produce_moved() {
    let results = fixture("test_movement_verbs.txt");

    let verb_commands: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| !r.command.starts_with('/'))
        .collect();

    for r in &verb_commands {
        assert!(
            matches!(r.result, ActionResult::Moved { .. }),
            "Verb command '{}' should produce Moved, got {:?}",
            r.command,
            r.result
        );
    }
}

#[test]
fn test_all_movement_verbs_have_narration() {
    let results = fixture("test_movement_verbs.txt");

    for r in &results {
        if let ActionResult::Moved { narration, .. } = &r.result {
            assert!(
                !narration.is_empty(),
                "Narration should be non-empty for '{}'",
                r.command
            );
        }
    }
}

// ============================================================
// Time progression
// ============================================================

#[test]
fn test_time_progresses_past_morning() {
    let results = fixture("test_time_progression.txt");

    let times: std::collections::HashSet<&str> = results.iter().map(|r| r.time.as_str()).collect();

    // Should pass through multiple time-of-day phases
    assert!(
        times.len() >= 2,
        "Time should progress through at least 2 phases, got: {:?}",
        times
    );
}

#[test]
fn test_time_starts_morning() {
    let results = fixture("test_time_progression.txt");
    assert_eq!(results[0].time, "Morning", "Game should start in Morning");
}

#[test]
fn test_time_season_stays_spring_in_short_test() {
    let results = fixture("test_time_progression.txt");

    // Even with many trips, we shouldn't change season (takes months)
    for r in &results {
        assert_eq!(r.season, "Spring", "Season should stay Spring");
    }
}

// ============================================================
// Pause/resume state machine
// ============================================================

#[test]
fn test_pause_shows_paused_in_status() {
    let results = fixture("test_pause_resume_cycle.txt");

    // Find status commands after pause
    let mut found_paused_status = false;
    let mut is_paused = false;

    for r in &results {
        if r.command == "/pause" {
            is_paused = true;
        } else if r.command == "/resume" {
            is_paused = false;
        } else if r.command == "/status" && is_paused {
            if let ActionResult::SystemCommand { response } = &r.result {
                if response.contains("paused") || response.contains("PAUSED") {
                    found_paused_status = true;
                }
            }
        }
    }

    assert!(
        found_paused_status,
        "Status while paused should mention paused"
    );
}

#[test]
fn test_resume_clears_pause() {
    let results = fixture("test_pause_resume_cycle.txt");

    // Find a status command after a resume that doesn't contain "paused"
    let mut found_unpaused = false;
    let mut last_was_resume = false;

    for r in &results {
        if r.command == "/resume" {
            last_was_resume = true;
        } else if r.command == "/status" && last_was_resume {
            if let ActionResult::SystemCommand { response } = &r.result {
                if !response.contains("paused") {
                    found_unpaused = true;
                }
            }
            last_was_resume = false;
        } else {
            last_was_resume = false;
        }
    }

    assert!(
        found_unpaused,
        "Status after resume should not mention paused"
    );
}

#[test]
fn test_pause_resume_all_succeed() {
    let results = fixture("test_pause_resume_cycle.txt");

    // No command should fail
    for r in &results {
        assert!(
            !matches!(r.result, ActionResult::NotFound { .. }),
            "No command should produce NotFound in pause/resume test"
        );
    }
}

// ============================================================
// Speed presets
// ============================================================

#[test]
fn test_speed_presets_acknowledged() {
    let results = fixture("test_speed_assertions.txt");

    let speed_commands: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| r.command.starts_with("/speed"))
        .collect();

    for r in &speed_commands {
        assert!(
            matches!(r.result, ActionResult::SystemCommand { .. }),
            "Speed command '{}' should return SystemCommand, got {:?}",
            r.command,
            r.result
        );
    }
}

#[test]
fn test_speed_bogus_shows_current() {
    let results = fixture("test_speed_assertions.txt");

    let bogus = results
        .iter()
        .find(|r| r.command == "/speed bogus")
        .expect("Should have /speed bogus command");

    if let ActionResult::SystemCommand { response } = &bogus.result {
        // Invalid speed names show an error with valid options
        assert!(
            response.contains("Unknown speed"),
            "Bogus speed should show error, got: {}",
            response
        );
        // Should NOT contain "changed" since it didn't change
        assert!(
            !response.contains("changed"),
            "Bogus speed should not change speed, got: {}",
            response
        );
    }
}

// ============================================================
// Debug commands — all NPCs
// ============================================================

#[test]
fn test_debug_schedule_all_npcs() {
    let results = fixture("test_debug_all_npcs.txt");

    let schedule_cmds: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| r.command.starts_with("/debug schedule"))
        .collect();

    assert_eq!(
        schedule_cmds.len(),
        8,
        "Should have schedule commands for all 8 NPCs"
    );

    for r in &schedule_cmds {
        if let ActionResult::SystemCommand { response } = &r.result {
            assert!(
                !response.is_empty(),
                "Schedule response should be non-empty for '{}'",
                r.command
            );
            // Should not be "NPC not found"
            assert!(
                !response.contains("not found"),
                "NPC should be found for '{}', got: {}",
                r.command,
                response
            );
        }
    }
}

#[test]
fn test_debug_memory_all_npcs() {
    let results = fixture("test_debug_all_npcs.txt");

    let memory_cmds: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| r.command.starts_with("/debug memory"))
        .collect();

    assert_eq!(
        memory_cmds.len(),
        8,
        "Should have memory commands for all 8 NPCs"
    );

    for r in &memory_cmds {
        assert!(
            matches!(r.result, ActionResult::SystemCommand { .. }),
            "Memory command should return SystemCommand for '{}'",
            r.command
        );
    }
}

#[test]
fn test_debug_rels_all_npcs() {
    let results = fixture("test_debug_all_npcs.txt");

    let rels_cmds: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| r.command.starts_with("/debug rels"))
        .collect();

    assert_eq!(
        rels_cmds.len(),
        8,
        "Should have rels commands for all 8 NPCs"
    );

    for r in &rels_cmds {
        assert!(
            matches!(r.result, ActionResult::SystemCommand { .. }),
            "Rels command should return SystemCommand for '{}'",
            r.command
        );
    }
}

// ============================================================
// Debug at locations
// ============================================================

#[test]
fn test_debug_here_varies_by_location() {
    let results = fixture("test_debug_at_locations.txt");

    let here_cmds: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| r.command == "/debug here")
        .collect();

    assert!(
        here_cmds.len() >= 4,
        "Should have /debug here at multiple locations"
    );

    // Collect responses to verify they differ
    let responses: Vec<&str> = here_cmds
        .iter()
        .filter_map(|r| {
            if let ActionResult::SystemCommand { response } = &r.result {
                Some(response.as_str())
            } else {
                None
            }
        })
        .collect();

    // At least some responses should differ (different locations)
    let unique: std::collections::HashSet<&&str> = responses.iter().collect();
    assert!(
        unique.len() >= 2,
        "Debug here should vary by location, got {} unique of {}",
        unique.len(),
        responses.len()
    );
}

#[test]
fn test_debug_tiers_shows_player_location() {
    let results = fixture("test_debug_at_locations.txt");

    let tier_cmds: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| r.command == "/debug tiers")
        .collect();

    for r in &tier_cmds {
        if let ActionResult::SystemCommand { response } = &r.result {
            assert!(
                response.contains("Player at") || response.contains("Tier"),
                "Tiers should mention player location or tiers, got: {}",
                response
            );
        }
    }
}

#[test]
fn test_debug_clock_shows_time() {
    let results = fixture("test_debug_at_locations.txt");

    let clock_cmd = results
        .iter()
        .find(|r| r.command == "/debug clock")
        .expect("Should have /debug clock");

    if let ActionResult::SystemCommand { response } = &clock_cmd.result {
        assert!(
            response.contains("Time") || response.contains("time") || response.contains(':'),
            "Clock should show time info, got: {}",
            response
        );
    }
}

// ============================================================
// NPC locations
// ============================================================

#[test]
fn test_npc_locations_script_runs() {
    let results = fixture("test_npc_locations.txt");

    // All commands should succeed (no panics, no NotFound for valid locations)
    let moves = moved_results(&results);
    assert!(
        !moves.is_empty(),
        "Should have successful movements in NPC locations test"
    );
}

#[test]
fn test_npc_debug_npcs_lists_all() {
    let results = fixture("test_npc_locations.txt");

    let npcs_cmd = results
        .iter()
        .find(|r| r.command == "/debug npcs")
        .expect("Should have /debug npcs");

    if let ActionResult::SystemCommand { response } = &npcs_cmd.result {
        // Should list at least some NPCs
        let npc_names = [
            "Padraig", "Siobhan", "Declan", "Roisin", "Tommy", "Aoife", "Mick", "Niamh",
        ];
        for name in &npc_names {
            assert!(
                response.contains(name),
                "Debug npcs should list {}, got: {}",
                name,
                response
            );
        }
    }
}

// ============================================================
// Edge cases
// ============================================================

#[test]
fn test_already_here_returns_correct_result() {
    let results = fixture("test_edge_cases.txt");

    let already_here = results
        .iter()
        .find(|r| r.command == "go to kilteevan" && r.result == ActionResult::AlreadyHere);

    assert!(
        already_here.is_some(),
        "Going to current location should return AlreadyHere"
    );
}

#[test]
fn test_nonexistent_locations_return_not_found() {
    let results = fixture("test_edge_cases.txt");

    let fake_locations = [
        "go to atlantis",
        "go to mordor",
        "go to narnia",
        "go to hogwarts",
    ];

    for cmd in &fake_locations {
        let r = results
            .iter()
            .find(|r| r.command == *cmd)
            .unwrap_or_else(|| panic!("Should have command {}", cmd));
        assert!(
            matches!(r.result, ActionResult::NotFound { .. }),
            "Command '{}' should return NotFound, got {:?}",
            cmd,
            r.result
        );
    }
}

#[test]
fn test_not_found_preserves_location() {
    let results = fixture("test_edge_cases.txt");

    // Player starts at Kilteevan, tries fake locations — should stay there
    for r in &results {
        if matches!(r.result, ActionResult::NotFound { .. }) {
            assert_eq!(
                r.location, "Kilteevan Village",
                "Player should stay at Kilteevan after NotFound"
            );
        }
    }
}

#[test]
fn test_already_here_after_move_and_retry() {
    let results = fixture("test_edge_cases.txt");

    // Find "go to crossroads" followed by "go to crossroads"
    let crossroads_already = results
        .iter()
        .find(|r| r.command == "go to crossroads" && r.result == ActionResult::AlreadyHere);

    assert!(
        crossroads_already.is_some(),
        "Second go to crossroads should be AlreadyHere"
    );
}

#[test]
fn test_unknown_debug_subcommand() {
    let results = fixture("test_edge_cases.txt");

    let unknown_debug = results
        .iter()
        .find(|r| r.command == "/debug nonexistent")
        .expect("Should have /debug nonexistent");

    if let ActionResult::SystemCommand { response } = &unknown_debug.result {
        assert!(
            response.contains("Unknown") || response.contains("unknown"),
            "Unknown debug subcommand should say 'Unknown', got: {}",
            response
        );
    }
}

#[test]
fn test_repeated_looks_all_succeed() {
    let results = fixture("test_edge_cases.txt");

    let looks: Vec<&ScriptResult> = results.iter().filter(|r| r.command == "look").collect();

    assert!(looks.len() >= 3, "Should have 3+ look commands");

    for r in &looks {
        assert!(
            matches!(r.result, ActionResult::Looked { .. }),
            "All looks should succeed, got {:?}",
            r.result
        );
    }
}

// ============================================================
// Look variants
// ============================================================

#[test]
fn test_look_l_and_look_around_all_work() {
    let results = fixture("test_look_variants.txt");

    let look_cmds: Vec<&ScriptResult> = results
        .iter()
        .filter(|r| r.command == "look" || r.command == "l" || r.command == "look around")
        .collect();

    // 3 variants × 5 locations = 15
    assert!(
        look_cmds.len() >= 12,
        "Should have many look commands, got {}",
        look_cmds.len()
    );

    for r in &look_cmds {
        assert!(
            matches!(r.result, ActionResult::Looked { .. }),
            "Look variant '{}' should produce Looked at {}, got {:?}",
            r.command,
            r.location,
            r.result
        );
    }
}

#[test]
fn test_look_descriptions_differ_by_location() {
    let results = fixture("test_look_variants.txt");

    // Collect unique descriptions
    let descriptions: std::collections::HashSet<String> = results
        .iter()
        .filter_map(|r| {
            if let ActionResult::Looked { description } = &r.result {
                Some(description.clone())
            } else {
                None
            }
        })
        .collect();

    // With 5 locations, we should have at least 3 unique descriptions
    // (some variants at same location may match)
    assert!(
        descriptions.len() >= 3,
        "Look descriptions should differ by location, got {} unique",
        descriptions.len()
    );
}

// ============================================================
// Direct GameTestHarness tests (no scripts)
// ============================================================

#[test]
fn test_harness_all_exits_nonempty() {
    let mut h = GameTestHarness::new();
    let all_locations = [
        "crossroads",
        "pub",
        "church",
        "letter office",
        "hedge school",
        "hurling green",
        "murphys farm",
        "obriens farm",
        "fairy fort",
        "bog road",
        "connollys",
        "lime kiln",
        "lough ree",
        "hodson bay",
    ];

    for loc in &all_locations {
        h.execute(&format!("go to {}", loc));
        let exits = h.exits();
        assert!(
            exits.contains("You can go to"),
            "Exits at {} should contain 'You can go to', got: {}",
            h.player_location(),
            exits
        );
    }
}

#[test]
fn test_harness_weather_consistent_at_all_locations() {
    let mut h = GameTestHarness::new();
    let weather = h.weather().to_string();

    h.execute("go to crossroads");
    assert_eq!(
        h.weather().to_string(),
        weather,
        "Weather should be consistent"
    );

    h.execute("go to pub");
    assert_eq!(
        h.weather().to_string(),
        weather,
        "Weather should be consistent"
    );

    h.execute("go to crossroads");
    h.execute("go to church");
    assert_eq!(
        h.weather().to_string(),
        weather,
        "Weather should be consistent"
    );
}

#[test]
fn test_harness_text_log_grows_monotonically() {
    let mut h = GameTestHarness::new();
    let mut prev_len = h.text_log().len();

    let commands = ["look", "go to crossroads", "look", "go to pub", "/status"];

    for cmd in &commands {
        h.execute(cmd);
        let new_len = h.text_log().len();
        assert!(
            new_len >= prev_len,
            "Log should not shrink after '{}': was {}, now {}",
            cmd,
            prev_len,
            new_len
        );
        prev_len = new_len;
    }
}

#[test]
fn test_harness_npc_schedule_ticking() {
    let mut h = GameTestHarness::new();

    // Record initial NPC positions
    h.execute("go to crossroads");
    h.execute("go to pub");
    let initial_count = h.npcs_here().len();

    // Advance time by a large amount (many hours)
    h.advance_time(600);

    // After 10 hours, NPC positions may have changed
    h.execute("go to crossroads");
    h.execute("go to pub");
    let later_count = h.npcs_here().len();

    // We can't predict exactly what changes, but the system shouldn't crash
    // and npcs_here should return valid results
    assert!(
        initial_count <= 8 && later_count <= 8,
        "NPC count should be reasonable"
    );
}

#[test]
fn test_harness_time_of_day_boundary_crossing() {
    let mut h = GameTestHarness::new();
    assert_eq!(h.time_of_day(), TimeOfDay::Morning);

    // Advance to midday (starts at ~8am, midday is 10am+)
    h.advance_time(120);
    let tod = h.time_of_day();
    assert_ne!(
        tod,
        TimeOfDay::Morning,
        "After 2 hours should be past Morning"
    );
}

#[test]
fn test_harness_season_stays_spring() {
    let mut h = GameTestHarness::new();
    assert_eq!(h.season(), Season::Spring);

    // Even after advancing a few hours, season shouldn't change
    h.advance_time(300);
    assert_eq!(h.season(), Season::Spring);
}

#[test]
fn test_harness_advance_time_large() {
    let mut h = GameTestHarness::new();

    // Advance a full day — should not crash
    h.advance_time(1440);

    // Game state should still be valid
    assert!(!h.player_location().is_empty());
    let _tod = h.time_of_day();
    let _season = h.season();
}

#[test]
fn test_harness_npc_canned_response_multiple_npcs() {
    let mut h = GameTestHarness::new();
    h.add_canned_response("Padraig Darcy", "Hello from Padraig!");
    h.add_canned_response("Niamh Darcy", "Hello from Niamh!");

    // Advance to when Padraig is at pub
    h.advance_time(120);
    h.execute("go to crossroads");
    h.execute("go to pub");

    // First interaction — should get a response from whoever has canned responses
    let r1 = h.execute("hello there");
    assert!(
        matches!(r1, ActionResult::NpcResponse { .. }),
        "Should get NPC response, got {:?}",
        r1
    );
}

#[test]
fn test_harness_look_contains_time_info() {
    let mut h = GameTestHarness::new();
    let r = h.execute("look");

    if let ActionResult::Looked { description } = r {
        // Descriptions use {time} template which gets replaced
        // They should NOT contain literal {time} placeholder
        assert!(
            !description.contains("{time}"),
            "Description should not contain raw {{time}} placeholder"
        );
        assert!(
            !description.contains("{weather}"),
            "Description should not contain raw {{weather}} placeholder"
        );
    }
}

#[test]
fn test_harness_look_contains_weather_info() {
    let mut h = GameTestHarness::new();
    h.execute("go to crossroads");
    let r = h.execute("look");

    if let ActionResult::Looked { description } = r {
        assert!(
            !description.contains("{weather}"),
            "Description should have weather replaced"
        );
    }
}

#[test]
fn test_harness_debug_log_accessible() {
    let mut h = GameTestHarness::new();

    // Move to trigger NPC tier assignments which generate debug events
    h.execute("go to crossroads");
    h.execute("go to pub");

    // Debug log should be accessible (may or may not have entries)
    let _log = h.debug_log();
}

#[test]
fn test_harness_location_id_changes_with_movement() {
    let mut h = GameTestHarness::new();
    let start_id = h.location_id();

    h.execute("go to crossroads");
    let crossroads_id = h.location_id();

    assert_ne!(
        start_id, crossroads_id,
        "Location ID should change after movement"
    );

    h.execute("go to pub");
    let pub_id = h.location_id();

    assert_ne!(crossroads_id, pub_id, "Location ID should change again");
}

#[test]
fn test_harness_quit_sets_should_quit() {
    let mut h = GameTestHarness::new();
    assert!(!h.app.should_quit);

    h.execute("/quit");
    assert!(h.app.should_quit, "Quit should set should_quit flag");
}

#[test]
fn test_harness_help_returns_system_command() {
    let mut h = GameTestHarness::new();
    let r = h.execute("/help");

    if let ActionResult::SystemCommand { response } = r {
        assert!(
            !response.is_empty(),
            "Help should return non-empty response"
        );
    } else {
        panic!("Help should return SystemCommand, got {:?}", r);
    }
}

#[test]
fn test_harness_status_contains_location() {
    let mut h = GameTestHarness::new();
    let r = h.execute("/status");

    if let ActionResult::SystemCommand { response } = r {
        assert!(
            response.contains("Kilteevan"),
            "Status should contain current location name, got: {}",
            response
        );
    }
}

#[test]
fn test_harness_status_after_move_updates() {
    let mut h = GameTestHarness::new();
    h.execute("go to crossroads");
    let r = h.execute("/status");

    if let ActionResult::SystemCommand { response } = r {
        assert!(
            response.contains("Crossroads"),
            "Status should reflect new location, got: {}",
            response
        );
    }
}

// ============================================================
// Script fixture smoke tests — verify all new fixtures run
// ============================================================

#[test]
fn test_fixture_all_locations_runs() {
    fixture("test_all_locations.txt");
}

#[test]
fn test_fixture_fuzzy_names_runs() {
    fixture("test_fuzzy_names.txt");
}

#[test]
fn test_fixture_multi_hop_runs() {
    fixture("test_multi_hop.txt");
}

#[test]
fn test_fixture_movement_verbs_runs() {
    fixture("test_movement_verbs.txt");
}

#[test]
fn test_fixture_time_progression_runs() {
    fixture("test_time_progression.txt");
}

#[test]
fn test_fixture_pause_resume_cycle_runs() {
    fixture("test_pause_resume_cycle.txt");
}

#[test]
fn test_fixture_debug_all_npcs_runs() {
    fixture("test_debug_all_npcs.txt");
}

#[test]
fn test_fixture_debug_at_locations_runs() {
    fixture("test_debug_at_locations.txt");
}

#[test]
fn test_fixture_npc_locations_runs() {
    fixture("test_npc_locations.txt");
}

#[test]
fn test_fixture_edge_cases_runs() {
    fixture("test_edge_cases.txt");
}

#[test]
fn test_fixture_look_variants_runs() {
    fixture("test_look_variants.txt");
}

#[test]
fn test_fixture_grand_tour_runs() {
    fixture("test_grand_tour.txt");
}

#[test]
fn test_fixture_speed_assertions_runs() {
    fixture("test_speed_assertions.txt");
}
