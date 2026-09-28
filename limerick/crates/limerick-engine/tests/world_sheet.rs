//! The canonical world sheet as a test oracle (product spec §14, #2040).
//!
//! `mods/rundale/world-sheet.txt` describes the whole tiny world. These
//! tests load the mod through the normal mod pipeline, play it from a new
//! game, and fail on any contradiction between the sheet and authoritative
//! game state. A failure means either a bug or an intentional change that
//! must also update the sheet.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::Timelike;
use limerick_engine::testing::{GameTestHarness, run_script_to};
use limerick_engine::world::LocationId;

fn mod_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/fixtures/rundale/tiny_world.txt")
}

fn harness() -> GameTestHarness {
    GameTestHarness::new_with_mod_dir(&mod_dir())
}

/// Minutes since midnight for an `HH:MM` sheet time.
type Minute = u32;

#[derive(Debug, Default)]
struct SheetLocation {
    exits: BTreeSet<String>,
    present: BTreeMap<Minute, BTreeSet<String>>,
}

#[derive(Debug, Default)]
struct SheetNpc {
    home: String,
    knows: BTreeSet<String>,
}

#[derive(Debug, Default)]
struct Sheet {
    start_time: Minute,
    start_location: String,
    /// Keyed by the sheet heading (upper-case location name).
    locations: BTreeMap<String, SheetLocation>,
    /// Keyed by the sheet heading (upper-case first name).
    npcs: BTreeMap<String, SheetNpc>,
}

fn parse_minute(text: &str) -> Minute {
    let (hour, minute) = text
        .trim()
        .split_once(':')
        .unwrap_or_else(|| panic!("sheet time {text:?} is not HH:MM"));
    hour.parse::<u32>().expect("sheet hour") * 60 + minute.parse::<u32>().expect("sheet minute")
}

fn parse_names(text: &str) -> BTreeSet<String> {
    text.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_sheet() -> Sheet {
    let path = mod_dir().join("world-sheet.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("world sheet at {}: {e}", path.display()));
    let mut sheet = Sheet::default();
    let blocks = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for block in blocks.split("\n\n") {
        let mut lines = block.lines().filter(|line| !line.trim().is_empty());
        let Some(heading) = lines.next() else {
            continue;
        };
        let heading = heading.trim().to_string();
        let fields: Vec<(String, String)> = lines
            .map(|line| {
                let (key, value) = line
                    .split_once(": ")
                    .unwrap_or_else(|| panic!("sheet line {line:?} under {heading} has no key"));
                (key.trim().to_string(), value.trim().to_string())
            })
            .collect();
        let is_location = fields
            .iter()
            .any(|(key, _)| key == "exits" || key.starts_with("present at "));
        if heading == "START" {
            for (key, value) in fields {
                match key.as_str() {
                    "time" => sheet.start_time = parse_minute(&value),
                    "location" => sheet.start_location = value,
                    other => panic!("unknown START field {other:?}"),
                }
            }
        } else if is_location {
            let location = sheet.locations.entry(heading.clone()).or_default();
            for (key, value) in fields {
                if key == "exits" {
                    location.exits = parse_names(&value);
                } else if let Some(time) = key.strip_prefix("present at ") {
                    location
                        .present
                        .insert(parse_minute(time), parse_names(&value));
                } else {
                    panic!("unknown location field {key:?} under {heading}");
                }
            }
        } else {
            let npc = sheet.npcs.entry(heading.clone()).or_default();
            for (key, value) in fields {
                match key.as_str() {
                    "home" => npc.home = value,
                    "knows" => npc.knows = parse_names(&value),
                    other => panic!("unknown NPC field {other:?} under {heading}"),
                }
            }
        }
    }
    assert!(!sheet.locations.is_empty(), "sheet lists no locations");
    assert!(!sheet.npcs.is_empty(), "sheet lists no NPCs");
    sheet
}

fn first_name(full_name: &str) -> &str {
    full_name.split_whitespace().next().unwrap_or(full_name)
}

fn location_name(h: &GameTestHarness, id: LocationId) -> String {
    h.app
        .world
        .graph
        .get(id)
        .map(|location| location.name.clone())
        .unwrap_or_else(|| panic!("location id {} is not in the graph", id.0))
}

fn location_id(h: &GameTestHarness, name: &str) -> LocationId {
    let graph = &h.app.world.graph;
    graph
        .location_ids()
        .into_iter()
        .find(|id| graph.get(*id).is_some_and(|location| location.name == name))
        .unwrap_or_else(|| panic!("sheet location {name:?} is not in the graph"))
}

/// First names of the NPCs present at `location`, as the sheet writes them.
fn present_at(h: &GameTestHarness, location: LocationId) -> BTreeSet<String> {
    h.app
        .npc_manager
        .npcs_at(location)
        .into_iter()
        .map(|npc| first_name(&npc.name).to_string())
        .collect()
}

fn minute_of_day(h: &GameTestHarness) -> Minute {
    let now = h.app.world.clock.now();
    now.hour() * 60 + now.minute()
}

#[test]
fn sheet_lists_exactly_the_mod_locations_and_npcs() {
    let sheet = parse_sheet();
    let h = harness();

    let graph = &h.app.world.graph;
    let world_locations: BTreeSet<String> = graph
        .location_ids()
        .into_iter()
        .map(|id| location_name(&h, id).to_uppercase())
        .collect();
    let sheet_locations: BTreeSet<String> = sheet.locations.keys().cloned().collect();
    assert_eq!(world_locations, sheet_locations, "locations differ");

    let world_npcs: BTreeSet<String> = h
        .app
        .npc_manager
        .all_npcs()
        .map(|npc| first_name(&npc.name).to_uppercase())
        .collect();
    let sheet_npcs: BTreeSet<String> = sheet.npcs.keys().cloned().collect();
    assert_eq!(world_npcs, sheet_npcs, "NPCs differ");
}

#[test]
fn new_game_starts_where_the_sheet_says() {
    let sheet = parse_sheet();
    let h = harness();
    assert_eq!(h.player_location(), sheet.start_location);
    assert_eq!(minute_of_day(&h), sheet.start_time);
}

#[test]
fn exits_match_the_sheet() {
    let sheet = parse_sheet();
    let h = harness();
    let graph = &h.app.world.graph;
    for id in graph.location_ids() {
        let name = location_name(&h, id);
        let exits: BTreeSet<String> = graph
            .neighbors(id)
            .into_iter()
            .map(|(target, _)| location_name(&h, target))
            .collect();
        assert_eq!(
            exits,
            sheet.locations[&name.to_uppercase()].exits,
            "exits from {name}"
        );
    }
}

#[test]
fn homes_and_relationships_match_the_sheet() {
    let sheet = parse_sheet();
    let h = harness();
    let manager = &h.app.npc_manager;
    for npc in manager.all_npcs() {
        let expected = &sheet.npcs[&first_name(&npc.name).to_uppercase()];
        let home = npc.home.expect("every tiny-world NPC has a home");
        assert_eq!(location_name(&h, home), expected.home, "{} home", npc.name);
        assert_eq!(npc.location(), home, "{} starts at home", npc.name);
        let knows: BTreeSet<String> = npc
            .relationships
            .keys()
            .map(|id| first_name(&manager.get(*id).expect("relationship target").name).to_string())
            .collect();
        assert_eq!(knows, expected.knows, "{} relationships", npc.name);
    }
}

/// Plays the first day minute by minute through the shared world pump and
/// checks every location's occupants at each time the sheet names.
#[test]
fn presence_over_the_first_day_matches_the_sheet() {
    let sheet = parse_sheet();
    let mut h = harness();
    let times: BTreeSet<Minute> = sheet
        .locations
        .values()
        .flat_map(|location| location.present.keys().copied())
        .collect();
    assert!(!times.is_empty(), "sheet names no presence times");

    for time in times {
        assert!(
            time > minute_of_day(&h),
            "sheet time {time} precedes the clock"
        );
        while minute_of_day(&h) < time {
            h.advance_time(1);
        }
        for (heading, location) in &sheet.locations {
            let name = h
                .app
                .world
                .graph
                .location_ids()
                .into_iter()
                .map(|id| location_name(&h, id))
                .find(|name| name.to_uppercase() == *heading)
                .expect("sheet location in graph");
            let expected = location.present.get(&time).cloned().unwrap_or_default();
            assert_eq!(
                present_at(&h, location_id(&h, &name)),
                expected,
                "who is at {name} at {:02}:{:02}",
                time / 60,
                time % 60
            );
        }
    }
}

/// The desktop engine plays the tiny world from a headless script, and the
/// script replays byte-identically.
#[test]
fn headless_script_plays_the_tiny_world() {
    let run = || {
        let mut out = Vec::new();
        run_script_to(&fixture(), harness(), &mut out).expect("script runs");
        out
    };
    let first = run();
    assert_eq!(first, run(), "tiny-world script did not replay");

    let lines: Vec<serde_json::Value> = std::str::from_utf8(&first)
        .expect("utf-8 output")
        .lines()
        .map(|line| serde_json::from_str(line).expect("json line"))
        .collect();

    let visited: Vec<&str> = lines
        .iter()
        .filter(|line| line["result"] == "moved")
        .map(|line| line["location"].as_str().expect("location"))
        .collect();
    assert_eq!(
        visited,
        [
            "Connolly Cottage",
            "Kilteevan Village",
            "Letter Office",
            "Kilteevan Village"
        ]
    );

    let people: Vec<(String, String)> = lines
        .iter()
        .filter(|line| line["command"] == "/npcs")
        .map(|line| {
            (
                line["location"].as_str().expect("location").to_string(),
                line["response"].as_str().expect("response").to_string(),
            )
        })
        .collect();
    let expect = |index: usize, location: &str, descriptions: &[&str]| {
        let (at, response) = &people[index];
        assert_eq!(at, location, "/npcs #{index} location");
        if descriptions.is_empty() {
            assert_eq!(response, "No one else is here.", "/npcs #{index} at {at}");
        }
        for description in descriptions {
            assert!(
                response.contains(description),
                "/npcs #{index} at {at} should list {description:?}: {response}"
            );
        }
    };
    const PEIG: &str = "a sharp-eyed woman with a satchel of letters";
    const MICHEAL: &str = "a weathered man in a mud-spattered frieze coat";
    const ROISIN: &str = "a young woman with yarn wound about her wrist";
    assert_eq!(people.len(), 5, "/npcs checks in the fixture");
    expect(0, "Kilteevan Village", &[]);
    expect(1, "Kilteevan Village", &[PEIG]);
    expect(2, "Connolly Cottage", &[MICHEAL, ROISIN]);
    expect(3, "Letter Office", &[PEIG]);
    expect(4, "Kilteevan Village", &[MICHEAL, ROISIN]);
}
