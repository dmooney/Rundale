//! Integration tests for Phase 3 — NPC system.

use std::path::Path;

use parish::npc::manager::{CogTier, NpcManager};
use parish::npc::memory::{MemoryEntry, ShortTermMemory};
use parish::npc::overhear::check_overhear;
use parish::npc::relationship::RelationshipKind;
use parish::npc::schedule::NpcState;
use parish::npc::tier::Tier2Event;
use parish::npc::{Npc, NpcId};
use parish::world::LocationId;
use parish::world::graph::WorldGraph;
use parish::world::time::GameClock;

/// Helper to load the parish graph.
fn load_parish_graph() -> WorldGraph {
    let path = Path::new("data/parish.json");
    WorldGraph::load_from_file(path).unwrap()
}

/// Helper to load NPCs from the data file.
fn load_npcs() -> NpcManager {
    let path = Path::new("data/npcs.json");
    NpcManager::load_from_file(path).unwrap()
}

// --- NPC Data Loading ---

#[test]
fn test_npcs_json_loads() {
    let mgr = load_npcs();
    assert_eq!(mgr.count(), 8, "Should load exactly 8 NPCs");
}

#[test]
fn test_npcs_all_have_schedules() {
    let mgr = load_npcs();
    for id in mgr.ids() {
        let npc = mgr.get(id).unwrap();
        assert!(
            !npc.schedule.weekday.is_empty(),
            "NPC {} should have weekday schedule",
            npc.name
        );
        assert!(
            !npc.schedule.weekend.is_empty(),
            "NPC {} should have weekend schedule",
            npc.name
        );
    }
}

#[test]
fn test_npcs_all_have_relationships() {
    let mgr = load_npcs();
    for id in mgr.ids() {
        let npc = mgr.get(id).unwrap();
        assert!(
            npc.relationships.len() >= 3,
            "NPC {} should have at least 3 relationships, has {}",
            npc.name,
            npc.relationships.len()
        );
    }
}

#[test]
fn test_npcs_all_have_knowledge() {
    let mgr = load_npcs();
    for id in mgr.ids() {
        let npc = mgr.get(id).unwrap();
        assert!(
            !npc.knowledge.is_empty(),
            "NPC {} should have knowledge items",
            npc.name
        );
    }
}

#[test]
fn test_npc_padraig_darcy() {
    let mgr = load_npcs();
    let padraig = mgr.get(NpcId(1)).unwrap();
    assert_eq!(padraig.name, "Padraig Darcy");
    assert_eq!(padraig.age, 58);
    assert_eq!(padraig.occupation, "Publican");
    assert_eq!(padraig.home, LocationId(2)); // Darcy's Pub
    assert_eq!(padraig.workplace, Some(LocationId(2)));
}

#[test]
fn test_npc_niamh_is_padraigs_daughter() {
    let mgr = load_npcs();
    let niamh = mgr.get(NpcId(8)).unwrap();
    assert_eq!(niamh.name, "Niamh Darcy");
    // Niamh has a family relationship with Padraig (id 1)
    let rel = niamh.relationships.get(&NpcId(1)).unwrap();
    assert_eq!(rel.kind, RelationshipKind::Family);
    assert!(rel.strength > 0.5);
}

// --- Tier Assignment ---

#[test]
fn test_tier_assignment_at_pub() {
    let graph = load_parish_graph();
    let mut mgr = load_npcs();

    // Place player at Darcy's Pub (id 2)
    mgr.assign_tiers(LocationId(2), &graph);

    // Padraig (home=pub) should be Tier1 if he starts there
    let padraig = mgr.get(NpcId(1)).unwrap();
    if padraig.state.is_at(LocationId(2)) {
        assert_eq!(mgr.tier(NpcId(1)), Some(CogTier::Tier1));
    }
}

#[test]
fn test_tier_assignment_nearby_farmer() {
    let graph = load_parish_graph();
    let mut mgr = NpcManager::new();

    // Create NPCs: one at crossroads, one at Murphy's Farm (8 minutes away)
    let mut npc1 = Npc::new_test_npc();
    npc1.id = NpcId(1);
    npc1.state = NpcState::Present(LocationId(1)); // Crossroads

    let mut npc2 = Npc::new_test_npc();
    npc2.id = NpcId(2);
    npc2.name = "Siobhan".to_string();
    npc2.state = NpcState::Present(LocationId(9)); // Murphy's Farm (1 edge from crossroads)

    mgr.add(npc1);
    mgr.add(npc2);

    // Player at crossroads
    mgr.assign_tiers(LocationId(1), &graph);

    assert_eq!(mgr.tier(NpcId(1)), Some(CogTier::Tier1));
    assert_eq!(mgr.tier(NpcId(2)), Some(CogTier::Tier2)); // 1 edge away
}

// --- Schedule Movement ---

#[test]
fn test_schedule_publican_morning() {
    let mgr = load_npcs();
    let padraig = mgr.get(NpcId(1)).unwrap();

    // Friday at 11:00 — Padraig should be at the pub (location 2)
    use chrono::{TimeZone, Utc};
    let time = Utc.with_ymd_and_hms(2026, 3, 20, 11, 0, 0).unwrap();
    let mut clock = GameClock::new(time);
    clock.pause();

    let desired = padraig.schedule.desired_location(&clock);
    assert_eq!(
        desired,
        Some(LocationId(2)),
        "Padraig should want to be at the pub at 11:00"
    );
}

#[test]
fn test_schedule_publican_afternoon() {
    let mgr = load_npcs();
    let padraig = mgr.get(NpcId(1)).unwrap();

    // Friday at 16:00 — Padraig should be at Connolly's Shop (location 13)
    use chrono::{TimeZone, Utc};
    let time = Utc.with_ymd_and_hms(2026, 3, 20, 16, 0, 0).unwrap();
    let mut clock = GameClock::new(time);
    clock.pause();

    let desired = padraig.schedule.desired_location(&clock);
    assert_eq!(
        desired,
        Some(LocationId(13)),
        "Padraig should want to be at Connolly's at 16:00"
    );
}

#[test]
fn test_schedule_farmer_evening() {
    let mgr = load_npcs();
    let siobhan = mgr.get(NpcId(2)).unwrap();

    // Friday at 20:00 — Siobhan should be at the pub
    use chrono::{TimeZone, Utc};
    let time = Utc.with_ymd_and_hms(2026, 3, 20, 20, 0, 0).unwrap();
    let mut clock = GameClock::new(time);
    clock.pause();

    let desired = siobhan.schedule.desired_location(&clock);
    assert_eq!(
        desired,
        Some(LocationId(2)),
        "Siobhan should want to be at the pub at 20:00"
    );
}

#[test]
fn test_schedule_weekend_mass() {
    let mgr = load_npcs();
    let padraig = mgr.get(NpcId(1)).unwrap();

    // Saturday at 11:00 — Padraig should be at church (location 3) for Mass
    use chrono::{TimeZone, Utc};
    let time = Utc.with_ymd_and_hms(2026, 3, 21, 11, 0, 0).unwrap();
    let mut clock = GameClock::new(time);
    clock.pause();

    let desired = padraig.schedule.desired_location(&clock);
    assert_eq!(
        desired,
        Some(LocationId(3)),
        "Padraig should want to be at church on Saturday for Mass"
    );
}

// --- Short-Term Memory ---

#[test]
fn test_memory_ring_buffer_overflow() {
    let mut memory = ShortTermMemory::new();
    for i in 0..25 {
        memory.add(MemoryEntry {
            timestamp: chrono::Utc::now(),
            content: format!("Event {}", i),
            participants: vec![],
            location: LocationId(1),
        });
    }
    assert_eq!(memory.len(), 20, "Memory should cap at 20 entries");
    let recent = memory.recent(1);
    assert_eq!(
        recent[0].content, "Event 24",
        "Most recent should be last added"
    );
}

// --- Relationship Graph ---

#[test]
fn test_relationship_queries() {
    let mgr = load_npcs();

    // Padraig should have a relationship with Niamh (family)
    let padraig = mgr.get(NpcId(1)).unwrap();
    let rel = padraig.relationships.get(&NpcId(8)).unwrap();
    assert_eq!(rel.kind, RelationshipKind::Family);
    assert!(rel.strength > 0.5);

    // Tommy and Roisin should be rivals
    let tommy = mgr.get(NpcId(5)).unwrap();
    let rel = tommy.relationships.get(&NpcId(4)).unwrap();
    assert_eq!(rel.kind, RelationshipKind::Rival);
    assert!(rel.strength < 0.0);
}

// --- Overhear Mechanic ---

#[test]
fn test_overhear_at_pub_from_crossroads() {
    let graph = load_parish_graph();

    let events = vec![Tier2Event {
        location: LocationId(2), // Darcy's Pub
        participants: vec![NpcId(1), NpcId(5)],
        summary: "Padraig and Tommy argue about the GAA match.".to_string(),
        relationship_changes: vec![],
    }];

    // Player at crossroads (id 1), pub is 1 edge away
    let overheard = check_overhear(&events, LocationId(1), &graph);
    assert_eq!(overheard.len(), 1);
    assert!(overheard[0].contains("Darcy's Pub"));
    assert!(overheard[0].contains("argue about the GAA"));
}

#[test]
fn test_overhear_far_location_not_heard() {
    let graph = load_parish_graph();

    let events = vec![Tier2Event {
        location: LocationId(11), // Fairy Fort — far from crossroads
        participants: vec![NpcId(5)],
        summary: "Tommy mutters to himself at the fairy fort.".to_string(),
        relationship_changes: vec![],
    }];

    // Player at crossroads — fairy fort is 3+ edges away
    let overheard = check_overhear(&events, LocationId(1), &graph);
    assert!(
        overheard.is_empty(),
        "Should not overhear events at distant locations"
    );
}

// --- NPC Manager Loading ---

#[test]
fn test_npc_manager_npcs_at_initial() {
    let mgr = load_npcs();

    // NPCs without explicit state in JSON default to Present(LocationId(1)) — the crossroads
    let at_crossroads = mgr.npcs_at(LocationId(1));
    assert_eq!(
        at_crossroads.len(),
        8,
        "All 8 NPCs should default to crossroads"
    );
}

#[test]
fn test_npc_locations_are_valid_parish_locations() {
    let mgr = load_npcs();
    let graph = load_parish_graph();

    for id in mgr.ids() {
        let npc = mgr.get(id).unwrap();
        assert!(
            graph.get(npc.home).is_some(),
            "NPC {} home location {} should exist in parish",
            npc.name,
            npc.home.0
        );
        if let Some(workplace) = npc.workplace {
            assert!(
                graph.get(workplace).is_some(),
                "NPC {} workplace {} should exist in parish",
                npc.name,
                workplace.0
            );
        }
        // Check all schedule locations exist
        for entry in &npc.schedule.weekday {
            assert!(
                graph.get(entry.location).is_some(),
                "NPC {} weekday schedule location {} should exist",
                npc.name,
                entry.location.0
            );
        }
        for entry in &npc.schedule.weekend {
            assert!(
                graph.get(entry.location).is_some(),
                "NPC {} weekend schedule location {} should exist",
                npc.name,
                entry.location.0
            );
        }
    }
}
