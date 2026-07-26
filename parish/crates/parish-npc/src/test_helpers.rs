//! Shared NPC test fixtures used across unit-test suites.

use std::collections::HashMap;
use std::path::Path;

use chrono::{TimeZone, Utc};

use crate::Npc;
use crate::memory::{LongTermMemory, ShortTermMemory};
use crate::reactions::ReactionLog;
use crate::types::{Intelligence, NpcState, ScheduleEntry, ScheduleVariant, SeasonalSchedule};
use parish_types::{LocationId, NpcId};
use parish_world::WorldState;
use parish_world::graph::WorldGraph;
use parish_world::time::GameClock;

/// Minimal NPC with all required fields populated.
pub fn make_test_npc(id: u32, location: u32) -> Npc {
    Npc {
        id: NpcId(id),
        name: format!("NPC {id}"),
        brief_description: "a person".to_string(),
        age: 30,
        occupation: "Test".to_string(),
        personality: "Test personality".to_string(),
        pronouns: "they/them".to_string(),
        intelligence: Intelligence::default(),
        location: LocationId(location),
        mood: "calm".to_string(),
        home: Some(LocationId(location)),
        workplace: None,
        schedule: None,
        relationships: HashMap::new(),
        memory: ShortTermMemory::new(),
        long_term_memory: LongTermMemory::new(),
        knowledge: Vec::new(),
        state: NpcState::Present,
        grounding_revision: Npc::fresh_grounding_revision(),
        observed_activity_fingerprint: None,
        deflated_summary: None,
        reaction_log: ReactionLog::default(),
        last_activity: None,
        is_ill: false,
        doom: None,
        banshee_heralded: false,
    }
}

/// NPC with a name and location set — shared by prompt / tier2 / tier3 test modules.
///
/// Mirrors the repeated `named_npc` / local `make_test_npc` wrappers that
/// previously existed in each of those modules (TD-002).
pub fn make_named_npc(id: u32, name: &str, location: u32) -> Npc {
    let mut npc = make_test_npc(id, location);
    npc.name = name.to_string();
    npc.brief_description = format!("a test NPC named {}", name);
    npc.age = 40;
    npc.personality = "Friendly".to_string();
    npc
}

/// NPC with name, occupation, and optional workplace — shared by
/// `reactions/emoji_reactions` and `reactions/arrival_reactions/tests` (TD-002).
///
/// Intelligence is set to an all-3 vector so tests that assert on reaction
/// thresholds get consistent results.
pub fn make_named_occupation_npc(
    id: u32,
    name: &str,
    occupation: &str,
    workplace: Option<LocationId>,
) -> Npc {
    let mut npc = make_test_npc(id, workplace.map(|l| l.0).unwrap_or(1));
    npc.name = name.to_string();
    npc.brief_description = format!("a {}", occupation.to_lowercase());
    npc.age = 40;
    npc.occupation = occupation.to_string();
    npc.personality = "A friendly person.".to_string();
    npc.intelligence = Intelligence {
        verbal: 3,
        analytical: 3,
        emotional: 3,
        practical: 3,
        wisdom: 3,
        creative: 3,
    };
    npc.workplace = workplace;
    npc
}

/// NPC with a specific age and occupation — shared by `tier4` tests (TD-002).
///
/// Location is fixed to 1 and workplace to 2, matching the previous hand-rolled
/// `make_npc` in `tier4.rs::tests`.
pub fn make_aged_occupation_npc(id: u32, age: u8, occupation: &str) -> Npc {
    let mut npc = make_test_npc(id, 1);
    npc.age = age;
    npc.occupation = occupation.to_string();
    npc.personality = "friendly".to_string();
    npc.mood = "content".to_string();
    npc.workplace = Some(LocationId(2));
    npc
}

/// NPC with a three-slot daily schedule: sleep at home, work, evening at home.
pub fn make_scheduled_npc(id: u32, home: u32, work: u32) -> Npc {
    let mut npc = make_test_npc(id, home);
    npc.set_schedule(Some(SeasonalSchedule {
        variants: vec![ScheduleVariant {
            season: None,
            day_type: None,
            entries: vec![
                ScheduleEntry {
                    start_hour: 0,
                    end_hour: 7,
                    location: LocationId(home),
                    activity: "sleeping".to_string(),
                    cuaird: false,
                },
                ScheduleEntry {
                    start_hour: 8,
                    end_hour: 17,
                    location: LocationId(work),
                    activity: "working".to_string(),
                    cuaird: false,
                },
                ScheduleEntry {
                    start_hour: 18,
                    end_hour: 23,
                    location: LocationId(home),
                    activity: "evening rest".to_string(),
                    cuaird: false,
                },
            ],
        }],
    }));
    npc
}

/// Loads the real parish graph; skips the calling test if the file is absent.
pub fn load_test_graph() -> Option<WorldGraph> {
    let path = Path::new("data/parish.json");
    if path.exists() {
        WorldGraph::load_from_file(path).ok()
    } else {
        None
    }
}

/// Builds a linear chain graph 0 — 1 — 2 — … — n.
pub fn make_chain_graph(n: u32) -> WorldGraph {
    let locations: Vec<serde_json::Value> = (0..=n)
        .map(|i| {
            let mut conns = Vec::new();
            if i > 0 {
                conns.push(serde_json::json!({"target": i - 1, "path_description": "a path"}));
            }
            if i < n {
                conns.push(serde_json::json!({"target": i + 1, "path_description": "a path"}));
            }
            serde_json::json!({
                "id": i,
                "name": format!("Loc {i}"),
                "description_template": "Test",
                "indoor": false,
                "public": true,
                "connections": conns
            })
        })
        .collect();
    let json = serde_json::json!({"locations": locations}).to_string();
    WorldGraph::load_from_str(&json).unwrap()
}

/// WorldState with the given graph and player location.
pub fn make_test_world(graph: WorldGraph, player_location: u32) -> WorldState {
    let mut world = WorldState::new();
    world.graph = graph;
    world.player_location = LocationId(player_location);
    world
}

/// WorldState seeded at 22:00 (night, inside the banshee herald window).
pub fn make_mourning_world() -> WorldState {
    let mut world = WorldState::new();
    world.graph = make_chain_graph(4);
    world.player_location = LocationId(0);
    world.clock = GameClock::new(Utc.with_ymd_and_hms(1820, 6, 15, 22, 0, 0).unwrap());
    world
}
