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
use limerick_core::game_loop::{AdvanceOptions, GossipMode, WeatherMode, advance_world};
use limerick_engine::npc::NpcId;
use limerick_engine::npc::types::NpcState;
use limerick_engine::testing::{GameTestHarness, run_script_to};
use limerick_engine::world::{LocationId, Weather};
use limerick_types::{PlayerProgress, TaskStatus};
use rand::SeedableRng;
use rand::rngs::StdRng;

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
    village: String,
}

/// The GOSSIP block: the one initial fact and the one path it should take.
#[derive(Debug, Default)]
struct SheetGossip {
    fact: String,
    source: String,
    path: Vec<String>,
    never_told: BTreeSet<String>,
}

/// The TASK block: the one task and the states it passes through.
#[derive(Debug, Default)]
struct SheetTask {
    giver: String,
    site: String,
    task: String,
    assigned: String,
    in_progress: String,
    completed: String,
    afterwards: String,
}

/// The WEATHER block: one NPC's one weather-dependent behaviour.
#[derive(Debug, Default)]
struct SheetWeather {
    npc: String,
    weather: String,
    time: Minute,
    in_that_weather: String,
    otherwise: String,
}

/// One scheduled walk: leaves `from` for `to` at `at`.
#[derive(Debug, Default, PartialEq, Eq)]
struct SheetWalk {
    at: Minute,
    from: String,
    to: String,
}

/// The MOVEMENT block: one NPC's scheduled walks.
#[derive(Debug, Default)]
struct SheetMovement {
    npc: String,
    out: SheetWalk,
    back: SheetWalk,
}

#[derive(Debug, Default)]
struct Sheet {
    start_time: Minute,
    start_location: String,
    gossip: SheetGossip,
    task: SheetTask,
    weather: SheetWeather,
    movement: SheetMovement,
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

/// Parses `07:00 Letter Office to Kilteevan Village`.
fn parse_walk(text: &str) -> SheetWalk {
    let (time, route) = text
        .split_once(' ')
        .unwrap_or_else(|| panic!("walk {text:?} is not `HH:MM from to to`"));
    let (from, to) = route
        .split_once(" to ")
        .unwrap_or_else(|| panic!("walk {text:?} is not `HH:MM from to to`"));
    SheetWalk {
        at: parse_minute(time),
        from: from.trim().to_string(),
        to: to.trim().to_string(),
    }
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
        if matches!(heading.as_str(), "GOSSIP" | "TASK" | "WEATHER" | "MOVEMENT") {
            parse_proof_block(&mut sheet, &heading, fields);
        } else if heading == "START" {
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
                    "village" => npc.village = value,
                    other => panic!("unknown NPC field {other:?} under {heading}"),
                }
            }
        }
    }
    assert!(!sheet.locations.is_empty(), "sheet lists no locations");
    assert!(!sheet.npcs.is_empty(), "sheet lists no NPCs");
    sheet
}

fn parse_proof_block(sheet: &mut Sheet, heading: &str, fields: Vec<(String, String)>) {
    for (key, value) in fields {
        match (heading, key.as_str()) {
            ("GOSSIP", "fact") => sheet.gossip.fact = value,
            ("GOSSIP", "source") => sheet.gossip.source = value,
            ("GOSSIP", "path") => {
                sheet.gossip.path = value
                    .split(',')
                    .map(|name| name.trim().to_string())
                    .collect()
            }
            ("GOSSIP", "never told") => sheet.gossip.never_told = parse_names(&value),
            ("TASK", "giver") => sheet.task.giver = value,
            ("TASK", "site") => sheet.task.site = value,
            ("TASK", "task") => sheet.task.task = value,
            ("TASK", "assigned") => sheet.task.assigned = value,
            ("TASK", "in progress") => sheet.task.in_progress = value,
            ("TASK", "completed") => sheet.task.completed = value,
            ("TASK", "afterwards") => sheet.task.afterwards = value,
            ("WEATHER", "npc") => sheet.weather.npc = value,
            ("WEATHER", "weather") => sheet.weather.weather = value,
            ("WEATHER", other) if other.starts_with("in that weather at ") => {
                sheet.weather.time = parse_minute(&other["in that weather at ".len()..]);
                sheet.weather.in_that_weather = value;
            }
            ("WEATHER", other) if other.starts_with("otherwise at ") => {
                assert_eq!(
                    parse_minute(&other["otherwise at ".len()..]),
                    sheet.weather.time,
                    "WEATHER lines name different times"
                );
                sheet.weather.otherwise = value;
            }
            ("MOVEMENT", "npc") => sheet.movement.npc = value,
            ("MOVEMENT", "out") => sheet.movement.out = parse_walk(&value),
            ("MOVEMENT", "back") => sheet.movement.back = parse_walk(&value),
            (heading, other) => panic!("unknown {heading} field {other:?}"),
        }
    }
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

/// Each NPC's dialogue prompt describes the village the sheet names: the
/// mod's dialogue content puts them in it, the village lists them, and it
/// gives them a backstory and words for every relationship they have
/// (issue 2299).
#[test]
fn dialogue_villages_match_the_sheet() {
    let sheet = parse_sheet();
    let h = harness();
    let game_mod = limerick_core::game_mod::GameMod::load(&mod_dir()).expect("load the mod");
    let dialogue = &game_mod.endpoints.dialogue_context;
    for npc in h.app.npc_manager.all_npcs() {
        let expected = &sheet.npcs[&first_name(&npc.name).to_uppercase()];
        let character = dialogue
            .character(npc.id.0)
            .unwrap_or_else(|| panic!("{} has no dialogue content", npc.name));
        assert_eq!(character.village, expected.village, "{} village", npc.name);
        let village = dialogue
            .village(&character.village)
            .unwrap_or_else(|| panic!("{} is not a village", character.village));
        assert!(
            village.people.iter().any(|person| person.npc == npc.id.0),
            "{} does not list {}",
            village.name,
            npc.name
        );
        assert!(!character.backstory.is_empty(), "{} backstory", npc.name);
        let worded: BTreeSet<u32> = character
            .relationships
            .iter()
            .map(|words| words.npc)
            .collect();
        let related: BTreeSet<u32> = npc.relationships.keys().map(|id| id.0).collect();
        assert_eq!(worded, related, "{} relationship words", npc.name);
    }
    for village in &dialogue.villages {
        let places: BTreeSet<u32> = village.places.iter().map(|place| place.location).collect();
        let world: BTreeSet<u32> = h
            .app
            .world
            .graph
            .location_ids()
            .iter()
            .map(|id| id.0)
            .collect();
        assert!(
            places.is_subset(&world),
            "{} names unknown places",
            village.name
        );
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

// ---------------------------------------------------------------------------
// Living-world proof facts (issue 2195, product spec §17 Milestone 6)
// ---------------------------------------------------------------------------

fn living_world() -> limerick_core::game_mod::LivingWorld {
    limerick_core::game_mod::GameMod::load(&mod_dir())
        .expect("load the mod")
        .living_world
}

fn npc_id(h: &GameTestHarness, first: &str) -> NpcId {
    h.app
        .npc_manager
        .all_npcs()
        .find(|npc| first_name(&npc.name) == first)
        .unwrap_or_else(|| panic!("sheet NPC {first:?} is not in the mod"))
        .id
}

fn npc_first_name(h: &GameTestHarness, id: u32) -> String {
    let npc = h
        .app
        .npc_manager
        .get(NpcId(id))
        .unwrap_or_else(|| panic!("NPC {id} is not in the mod"));
    first_name(&npc.name).to_string()
}

/// Advances the clock and runs the shared world pump once, with the weather
/// held where the test put it so the sheet's condition is the only weather.
fn pump(h: &mut GameTestHarness, rng: &mut StdRng, advance_minutes: i64) {
    h.app.world.clock.advance(advance_minutes);
    advance_world(
        &mut h.app.world,
        &mut h.app.npc_manager,
        rng,
        AdvanceOptions {
            weather: WeatherMode::Skip,
            run_banshee: false,
            gossip: GossipMode::All,
            run_tier4: false,
        },
    );
}

/// A new game with the given weather, pumped to `until` (minutes since
/// midnight) one minute at a time. It pumps once before the first minute
/// passes so the schedule is read at the start, as the first turn of a real
/// game would.
fn play_until(weather: Weather, until: Minute) -> GameTestHarness {
    let mut h = harness();
    h.app.world.weather = weather;
    let mut rng = StdRng::seed_from_u64(2195);
    pump(&mut h, &mut rng, 0);
    while minute_of_day(&h) < until {
        pump(&mut h, &mut rng, 1);
    }
    h
}

#[test]
fn living_world_facts_name_real_npcs_and_locations() {
    let h = harness();
    let npcs: BTreeSet<u32> = h.app.npc_manager.all_npcs().map(|npc| npc.id.0).collect();
    let locations: BTreeSet<u32> = h
        .app
        .world
        .graph
        .location_ids()
        .iter()
        .map(|id| id.0)
        .collect();
    let facts = living_world();
    assert!(facts.gossip.is_some(), "the mod authors its gossip fact");
    assert!(facts.task.is_some(), "the mod authors its task");
    facts.validate(&npcs, &locations).expect("valid facts");
}

#[test]
fn the_gossip_fact_matches_the_sheet_and_starts_with_its_source_alone() {
    let sheet = parse_sheet();
    let h = harness();
    let gossip = living_world().gossip.expect("gossip fact");

    assert_eq!(gossip.content, sheet.gossip.fact);
    assert_eq!(npc_first_name(&h, gossip.source), sheet.gossip.source);
    let path: Vec<String> = gossip
        .path
        .iter()
        .map(|id| npc_first_name(&h, *id))
        .collect();
    assert_eq!(path, sheet.gossip.path);
    let off_path: BTreeSet<String> = h
        .app
        .npc_manager
        .all_npcs()
        .map(|npc| first_name(&npc.name).to_string())
        .filter(|name| !sheet.gossip.path.contains(name))
        .collect();
    assert_eq!(off_path, sheet.gossip.never_told, "who must never know");

    // Exactly one fact exists, and only its source knows it.
    let network = &h.app.world.gossip_network;
    assert_eq!(network.len(), 1, "exactly one initial gossip fact");
    for npc in h.app.npc_manager.all_npcs() {
        let knows = !network.known_by(npc.id).is_empty();
        assert_eq!(
            knows,
            first_name(&npc.name) == sheet.gossip.source,
            "{} at the start",
            npc.name
        );
    }
    let item = network.known_by(npc_id(&h, &sheet.gossip.source))[0];
    assert_eq!(item.content, sheet.gossip.fact);
    assert_eq!(item.distortion_level, 0);

    // Each hop is between NPCs with an authored relationship.
    for pair in gossip.path.windows(2) {
        let speaker = h.app.npc_manager.get(NpcId(pair[0])).expect("speaker");
        assert!(
            speaker.relationships.contains_key(&NpcId(pair[1])),
            "{} has no relationship with the next NPC on the path",
            speaker.name
        );
    }
}

/// Played through the shared world pump with the player in the village, the
/// fact reaches the next NPC on the path and nobody else: the NPC the sheet
/// says is never told is never in a group with anyone who knows it.
#[test]
fn the_gossip_fact_travels_only_along_the_sheets_path() {
    let sheet = parse_sheet();
    let h = play_until(Weather::Clear, 24 * 60 - 1);
    let network = &h.app.world.gossip_network;
    assert_eq!(network.len(), 1, "no other gossip appears");
    let knowers: BTreeSet<String> = h
        .app
        .npc_manager
        .all_npcs()
        .filter(|npc| !network.known_by(npc.id).is_empty())
        .map(|npc| first_name(&npc.name).to_string())
        .collect();
    assert_eq!(
        knowers,
        sheet.gossip.path.iter().cloned().collect::<BTreeSet<_>>(),
        "who knows the fact after a day"
    );
    for name in &sheet.gossip.never_told {
        assert!(
            network.known_by(npc_id(&h, name)).is_empty(),
            "{name} must never be told"
        );
    }
}

#[test]
fn the_task_matches_the_sheet_and_fits_the_task_ledger() {
    let sheet = parse_sheet();
    let h = harness();
    let task = living_world().task.expect("task");

    assert_eq!(npc_first_name(&h, task.assigned_by), sheet.task.giver);
    assert_eq!(
        location_name(&h, LocationId(task.location)),
        sheet.task.site
    );
    assert_eq!(task.description, sheet.task.task);
    assert_eq!(task.assigned, sheet.task.assigned);
    assert_eq!(task.in_progress, sheet.task.in_progress);
    assert_eq!(task.completed, sheet.task.completed);
    assert_eq!(task.afterwards, sheet.task.afterwards);

    // Someone other than the giver is at the site to receive the message.
    // The receiver is the one sheet NPC, other than the giver, that the task
    // names; it is derived so the test needs no edit if the task changes hands.
    let description = task.description.to_uppercase();
    let receivers: Vec<&String> = sheet
        .npcs
        .keys()
        .filter(|name| {
            description.contains(name.as_str()) && **name != sheet.task.giver.to_uppercase()
        })
        .collect();
    assert_eq!(
        receivers.len(),
        1,
        "the task names exactly one receiver besides its giver: {receivers:?}"
    );
    let receiver = receivers[0];
    let site = &sheet.locations[&sheet.task.site.to_uppercase()];
    assert!(
        site.present
            .values()
            .any(|present| present.iter().any(|name| name.to_uppercase() == *receiver)),
        "{receiver} is at the {} on the sheet",
        sheet.task.site
    );

    // The task ledger records the authored task exactly, as assigned.
    let mut ledger = PlayerProgress::default();
    let id = ledger
        .assign_task(
            &task.description,
            NpcId(task.assigned_by),
            LocationId(task.location),
            h.app.world.clock.now(),
        )
        .expect("the task is assignable");
    let recorded = ledger.task(id).expect("recorded");
    assert_eq!(recorded.description, task.description, "not truncated");
    assert_eq!(recorded.status, TaskStatus::Assigned);
    assert_eq!(recorded.assigned_by, NpcId(task.assigned_by));
    assert_eq!(recorded.location, LocationId(task.location));
    assert!(
        h.app.world.player_progress.is_empty(),
        "a new game has no task assigned"
    );
}

/// Peig's one weather-dependent behaviour: in the sheet's weather she is
/// indoors at the Letter Office at 08:00 rather than on the village road.
#[test]
fn the_weather_behaviour_matches_the_sheet() {
    let sheet = parse_sheet();
    let weather: Weather = sheet.weather.weather.parse().expect("sheet weather");
    let where_is_the_npc = |weather: Weather| {
        let h = play_until(weather, sheet.weather.time);
        let npc = h
            .app
            .npc_manager
            .get(npc_id(&h, &sheet.weather.npc))
            .expect("the NPC");
        assert!(
            matches!(npc.state(), NpcState::Present),
            "{} is not mid-walk at the checked time under {weather}",
            npc.name
        );
        // `/people` at that place agrees with where the NPC is.
        assert!(present_at(&h, npc.location()).contains(&sheet.weather.npc));
        location_name(&h, npc.location())
    };
    assert_eq!(where_is_the_npc(weather), sheet.weather.in_that_weather);
    assert_eq!(where_is_the_npc(Weather::Clear), sheet.weather.otherwise);
}

/// The scheduled movement: the NPC leaves for the place and time the sheet
/// states, is at neither end while on the way, and arrives.
#[test]
fn the_scheduled_movement_matches_the_sheet() {
    let sheet = parse_sheet();
    let mut h = harness();
    h.app.world.weather = Weather::Clear;
    let mut rng = StdRng::seed_from_u64(2195);
    let npc = npc_id(&h, &sheet.movement.npc);

    let mut walks: Vec<SheetWalk> = Vec::new();
    let mut on_the_way = false;
    pump(&mut h, &mut rng, 0);
    while minute_of_day(&h) <= sheet.movement.back.at + 30 {
        match h.app.npc_manager.get(npc).expect("the NPC").state() {
            NpcState::InTransit { from, to, .. } => {
                if !on_the_way {
                    walks.push(SheetWalk {
                        at: minute_of_day(&h),
                        from: location_name(&h, *from),
                        to: location_name(&h, *to),
                    });
                    on_the_way = true;
                }
                for end in [*from, *to] {
                    assert!(
                        !present_at(&h, end).contains(&sheet.movement.npc),
                        "{} is listed at an end of her own walk",
                        sheet.movement.npc
                    );
                }
            }
            NpcState::Present => on_the_way = false,
        }
        pump(&mut h, &mut rng, 1);
    }
    assert_eq!(
        walks,
        [sheet.movement.out, sheet.movement.back],
        "the scheduled walks, in order"
    );
    let end = h.app.npc_manager.get(npc).expect("the NPC");
    assert!(matches!(end.state(), NpcState::Present));
    assert_eq!(location_name(&h, end.location()), walks[1].to);
}
