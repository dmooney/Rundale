use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use parish_npc::Npc;
use parish_npc::data::{load_npcs_from_file, load_npcs_from_str};
use parish_types::{DayType, LocationId, NpcId, Season};
use parish_world::graph::WorldGraph;

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("../../..")
        .canonicalize()
        .expect("CARGO_MANIFEST_DIR should resolve to the repository root via ../../..")
}

fn rundale_file(name: &str) -> PathBuf {
    let path = repo_root().join("mods/rundale").join(name);
    assert!(
        path.is_file(),
        "expected real Rundale fixture at {}; this test must fail loudly instead of skipping",
        path.display()
    );
    path
}

fn validate_npcs_against_world(npcs: &[Npc], world: &WorldGraph) -> Vec<String> {
    let mut errors = Vec::new();
    let npc_ids: HashSet<NpcId> = npcs.iter().map(|npc| npc.id).collect();
    let npc_by_id: HashMap<NpcId, &Npc> = npcs.iter().map(|npc| (npc.id, npc)).collect();

    for npc in npcs {
        if !world_has_location(world, npc.location()) {
            errors.push(format!(
                "{} starts at missing location {}",
                npc.name,
                npc.location().0
            ));
        }

        match npc.home {
            Some(home) if !world_has_location(world, home) => {
                errors.push(format!("{} has missing home location {}", npc.name, home.0))
            }
            None => errors.push(format!("{} has no home location", npc.name)),
            _ => {}
        }

        if let Some(workplace) = npc.workplace
            && !world_has_location(world, workplace)
        {
            errors.push(format!(
                "{} has missing workplace location {}",
                npc.name, workplace.0
            ));
        }

        let Some(schedule) = npc.schedule() else {
            errors.push(format!("{} has no schedule", npc.name));
            continue;
        };

        if schedule.variants.is_empty() {
            errors.push(format!("{} has an empty schedule", npc.name));
        }

        for variant in &schedule.variants {
            if variant.entries.is_empty() {
                errors.push(format!("{} has an empty schedule variant", npc.name));
            }

            for entry in &variant.entries {
                if entry.start_hour > entry.end_hour || entry.end_hour > 23 {
                    errors.push(format!(
                        "{} has invalid schedule hours {}-{}",
                        npc.name, entry.start_hour, entry.end_hour
                    ));
                }

                if entry.activity.trim().is_empty() {
                    errors.push(format!("{} has a blank schedule activity", npc.name));
                }

                if variant.season.is_none()
                    && variant.day_type.is_none()
                    && let Some(season) = unscoped_activity_season_word(&entry.activity)
                {
                    errors.push(format!(
                        "{} has season-specific word {season:?} in unscoped fallback activity {:?}",
                        npc.name, entry.activity
                    ));
                }

                if !world_has_location(world, entry.location) {
                    errors.push(format!(
                        "{} has missing schedule location {}",
                        npc.name, entry.location.0
                    ));
                }
            }
        }

        for target_id in npc.relationships.keys() {
            let Some(target) = npc_by_id.get(target_id) else {
                errors.push(format!(
                    "{} has relationship with missing NPC {}",
                    npc.name, target_id.0
                ));
                continue;
            };

            if !target.relationships.contains_key(&npc.id) {
                errors.push(format!(
                    "{} relates to {} without a reciprocal relationship",
                    npc.name, target.name
                ));
            }
        }
    }

    for location_id in world.location_ids() {
        let location = world
            .get(location_id)
            .expect("location_ids should only return locations present in the graph");
        for npc_id in &location.associated_npcs {
            if !npc_ids.contains(npc_id) {
                errors.push(format!(
                    "{} associates missing NPC {}",
                    location.name, npc_id.0
                ));
            }
        }
    }

    errors
}

/// Returns the first explicit season word in authored activity prose.
///
/// An unscoped `(None, None)` schedule variant can resolve in every season and
/// day type not covered by a more specific variant. Its prose therefore must
/// not claim that any one season is currently active.
fn unscoped_activity_season_word(activity: &str) -> Option<&'static str> {
    const SEASONS: [&str; 5] = ["spring", "summer", "autumn", "fall", "winter"];
    activity
        .split(|character: char| !character.is_alphabetic())
        .find_map(|word| {
            SEASONS
                .into_iter()
                .find(|season| word.eq_ignore_ascii_case(season))
        })
}

fn world_has_location(world: &WorldGraph, id: LocationId) -> bool {
    world.get(id).is_some()
}

#[test]
fn real_rundale_npcs_and_world_are_consistent() {
    let npc_path = rundale_file("npcs.json");
    let world_path = rundale_file("world.json");

    let npcs = load_npcs_from_file(&npc_path).expect("real Rundale NPC data should load");
    let world = WorldGraph::load_from_file(&world_path).expect("real Rundale world should load");

    assert_eq!(npcs.len(), 23, "Rundale should currently define 23 NPCs");
    assert_eq!(
        world.location_count(),
        22,
        "Rundale should currently define 22 locations"
    );
    assert!(
        npcs.iter().any(|npc| npc.name == "Padraig Darcy"),
        "real NPC data should include Padraig Darcy"
    );
    assert!(
        world
            .location_ids()
            .iter()
            .any(|id| world.get(*id).is_some_and(|loc| loc.name == "Darcy's Pub")),
        "real world data should include Darcy's Pub"
    );

    let errors = validate_npcs_against_world(&npcs, &world);
    assert!(
        errors.is_empty(),
        "Rundale NPC/world consistency errors:\n{}",
        errors.join("\n")
    );
}

#[test]
fn spring_weekday_schedule_activities_are_seasonally_grounded() {
    let npcs =
        load_npcs_from_file(&rundale_file("npcs.json")).expect("real Rundale NPC data should load");

    let expected = [
        (
            "Siobhan Murphy",
            13,
            "afternoon farm work — tending the fields, livestock and outbuildings",
        ),
        (
            "Fr. Declan Tierney",
            14,
            "walking the parish roads, checking on outlying families",
        ),
        (
            "Roisin Connolly",
            9,
            "minding the shop — serving local trade, tinkers and travellers",
        ),
        (
            "Aoife Brennan",
            9,
            "teaching at the hedge school — lessons in reading, writing, arithmetic and Irish",
        ),
        (
            "Niamh Darcy",
            18,
            "serving at the pub through the busy evening",
        ),
    ];

    for (name, hour, expected_activity) in expected {
        let npc = npcs
            .iter()
            .find(|npc| npc.name == name)
            .unwrap_or_else(|| panic!("real Rundale data should include {name}"));
        let activity = npc
            .schedule()
            .and_then(|schedule| schedule.entry_at(hour, Season::Spring, DayType::Weekday))
            .map(|entry| entry.activity.as_str())
            .unwrap_or_else(|| {
                panic!("{name} should have a Spring weekday activity at {hour:02}:00")
            });

        assert_eq!(
            activity, expected_activity,
            "wrong Spring activity for {name}"
        );
        assert_eq!(
            unscoped_activity_season_word(activity),
            None,
            "Spring activity for {name} must not assert another season"
        );
    }
}

/// TD-002: cross-file content validation for the supporting mod data files
/// (festivals / encounters / transport). These do not reference world
/// locations or NPCs by id in the current Rundale content, so the validation
/// confirms they parse and their well-formedness invariants hold — failing
/// loudly if a future edit introduces a malformed calendar date, an empty
/// encounter table, or a transport mode set whose `default` does not name a
/// defined mode.
#[test]
fn real_rundale_supporting_files_are_well_formed() {
    use serde_json::Value;

    // festivals.json — array of {name, month 1-12, day 1-31, description}.
    let festivals: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(rundale_file("festivals.json")).expect("read festivals.json"),
    )
    .expect("festivals.json should be a JSON array");
    assert!(!festivals.is_empty(), "festivals.json must not be empty");
    for f in &festivals {
        let month = f
            .get("month")
            .and_then(Value::as_u64)
            .expect("festival month");
        let day = f.get("day").and_then(Value::as_u64).expect("festival day");
        assert!(
            (1..=12).contains(&month),
            "festival month out of range: {month}"
        );
        assert!((1..=31).contains(&day), "festival day out of range: {day}");
        assert!(
            f.get("name")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty()),
            "festival missing name: {f}"
        );
    }

    // encounters.json — map of time-of-day -> non-empty flavour text.
    let encounters: std::collections::BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(rundale_file("encounters.json")).expect("read encounters.json"),
    )
    .expect("encounters.json should be a string map");
    assert!(!encounters.is_empty(), "encounters.json must not be empty");
    for (slot, text) in &encounters {
        assert!(
            !text.trim().is_empty(),
            "encounter slot {slot:?} has empty flavour text"
        );
    }

    // transport.toml — `default` must name a defined `[[modes]]` id.
    let transport: toml::Value = toml::from_str(
        &std::fs::read_to_string(rundale_file("transport.toml")).expect("read transport.toml"),
    )
    .expect("transport.toml should parse");
    let default_mode = transport
        .get("default")
        .and_then(toml::Value::as_str)
        .expect("transport default mode");
    let mode_ids: Vec<&str> = transport
        .get("modes")
        .and_then(toml::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(toml::Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        mode_ids.contains(&default_mode),
        "transport default {default_mode:?} is not among defined modes {mode_ids:?}"
    );
}

#[test]
fn bad_fixture_reports_missing_npc_location_references() {
    let world = WorldGraph::load_from_str(minimal_world_json(&[]).as_str())
        .expect("control world fixture should load");
    let npcs = load_npcs_from_str(
        r#"{
            "npcs": [{
                "id": 1,
                "name": "Alice",
                "age": 30,
                "occupation": "Farmer",
                "personality": "Quiet",
                "home": 1,
                "workplace": 99,
                "mood": "calm",
                "schedule": [
                    {"start_hour": 0, "end_hour": 23, "location": 42, "activity": "resting"}
                ],
                "relationships": []
            }]
        }"#,
    )
    .expect("NPC loader does not validate world references on its own");

    let errors = validate_npcs_against_world(&npcs, &world);

    assert_contains_error(&errors, "Alice has missing workplace location 99");
    assert_contains_error(&errors, "Alice has missing schedule location 42");
}

#[test]
fn bad_fixture_reports_world_associated_npc_without_matching_npc() {
    let world = WorldGraph::load_from_str(minimal_world_json(&[99]).as_str())
        .expect("control world fixture should load");
    let npcs = load_npcs_from_str(
        r#"{
            "npcs": [{
                "id": 1,
                "name": "Alice",
                "age": 30,
                "occupation": "Farmer",
                "personality": "Quiet",
                "home": 1,
                "workplace": 2,
                "mood": "calm",
                "schedule": [
                    {"start_hour": 0, "end_hour": 23, "location": 2, "activity": "working"}
                ],
                "relationships": []
            }]
        }"#,
    )
    .expect("control NPC fixture should load");

    let errors = validate_npcs_against_world(&npcs, &world);

    assert_contains_error(&errors, "Home associates missing NPC 99");
}

fn assert_contains_error(errors: &[String], expected: &str) {
    assert!(
        errors.iter().any(|error| error.contains(expected)),
        "expected error containing {expected:?}; got:\n{}",
        errors.join("\n")
    );
}

fn minimal_world_json(associated_npcs: &[u32]) -> String {
    format!(
        r#"{{
            "locations": [
                {{
                    "id": 1,
                    "name": "Home",
                    "description_template": "Home. It is {{time}} and {{weather}}.",
                    "indoor": true,
                    "public": false,
                    "connections": [
                        {{"target": 2, "path_description": "the lane to work"}}
                    ],
                    "associated_npcs": {associated_npcs}
                }},
                {{
                    "id": 2,
                    "name": "Work",
                    "description_template": "Work. It is {{time}} and {{weather}}.",
                    "indoor": false,
                    "public": true,
                    "connections": [
                        {{"target": 1, "path_description": "the lane home"}}
                    ]
                }}
            ]
        }}"#,
        associated_npcs =
            serde_json::to_string(associated_npcs).expect("associated NPC list should serialize")
    )
}
