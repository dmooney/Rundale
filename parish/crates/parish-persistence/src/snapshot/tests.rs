//! Tests for snapshot serialization, conversion, and restore.

use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use parish_npc::manager::NpcManager;
use parish_npc::memory::{LongTermMemory, ShortTermMemory};
use parish_npc::types::Intelligence;
use parish_npc::{Npc, NpcPersistedFields};
use parish_types::{LocationId, NpcId, TaskStatus};
use parish_world::WorldState;

use super::types::{ClockSnapshot, GameSnapshot, NpcSnapshot};

fn make_test_npc(id: u32, location: u32) -> Npc {
    let mut npc = Npc::new_test_npc();
    npc.id = NpcId(id);
    npc.name = format!("NPC {}", id);
    npc.brief_description = "a person".to_string();
    npc.age = 30;
    npc.occupation = "Test".to_string();
    npc.personality = "Test personality".to_string();
    npc.pronouns = "they/them".to_string();
    npc.intelligence = Intelligence::default();
    npc.set_location(LocationId(location));
    npc.mood = "calm".to_string();
    npc.home = Some(LocationId(location));
    npc
}

#[test]
fn test_npc_snapshot_roundtrip() {
    let npc = make_test_npc(1, 2);
    let original_revision = npc.grounding_revision();
    let snap = NpcSnapshot::from_npc(&npc);
    let json = serde_json::to_string(&snap).unwrap();
    assert!(
        !json.contains("grounding_revision"),
        "process-local grounding lineage must never enter save data"
    );
    assert!(
        !json.contains("observed_activity_fingerprint"),
        "schedule-tick activity observations must never enter save data"
    );
    let restored = snap.into_npc();
    assert_eq!(restored.id, NpcId(1));
    assert_eq!(restored.name, "NPC 1");
    assert_eq!(restored.location(), LocationId(2));
    assert_eq!(restored.mood, "calm");
    assert_ne!(
        restored.grounding_revision(),
        original_revision,
        "deserialization must allocate a fresh live grounding lineage"
    );
    assert!(
        restored.observed_activity_fingerprint().is_none(),
        "restored NPCs must wait for the production schedule tick to synchronize activity"
    );
}

/// Regression guard for #749 — every persisted field must survive a
/// `from_npc` → JSON → `into_npc` round-trip with non-default values.
/// If a new field is added to `Npc` but not wired through `NpcSnapshot`,
/// the exhaustive destructuring in `from_npc`/`into_npc` will already
/// produce a compile error; this test provides runtime correctness coverage.
#[test]
fn test_npc_snapshot_roundtrip_all_persisted_fields() {
    use parish_npc::memory::MemoryEntry;
    use parish_npc::transitions::NpcSummary;
    use parish_npc::types::{Intelligence, NpcState, Relationship, RelationshipKind};

    let summary = NpcSummary {
        npc_id: NpcId(42),
        location: LocationId(7),
        mood: "melancholy".to_string(),
        recent_activity: vec!["tended the field".to_string()],
        key_relationship_changes: vec![],
    };

    let npc = Npc::from_persisted_fields(NpcPersistedFields {
        id: NpcId(42),
        name: "Brigid Ní Fhaoláin".to_string(),
        brief_description: "a tall woman in a grey shawl".to_string(),
        age: 47,
        occupation: "Midwife".to_string(),
        personality: "Quiet and deliberate, moves with purpose.".to_string(),
        pronouns: "she/her".to_string(),
        intelligence: Intelligence::new(4, 3, 5, 2, 4, 3),
        location: LocationId(7),
        mood: "melancholy".to_string(),
        home: Some(LocationId(3)),
        workplace: Some(LocationId(7)),
        schedule: None,
        relationships: {
            let mut m = HashMap::new();
            m.insert(NpcId(5), Relationship::new(RelationshipKind::Friend, 0.6));
            m
        },
        memory: {
            let mut mem = ShortTermMemory::new();
            mem.add(MemoryEntry {
                timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap(),
                content: "Saw a crow on the gate post.".to_string(),
                participants: vec![NpcId(42)],
                location: LocationId(7),
                kind: None,
            });
            mem
        },
        long_term_memory: LongTermMemory::new(),
        knowledge: vec!["The well on Kilmore road is dry.".to_string()],
        state: NpcState::Present,
        deflated_summary: Some(summary.clone()),
        last_activity: Some("Delivered a baby at the Burke farm.".to_string()),
        is_ill: true,
        doom: Some(Utc.with_ymd_and_hms(1820, 6, 1, 0, 0, 0).unwrap()),
        banshee_heralded: true,
    });

    // Round-trip through JSON to exercise the full serialize/deserialize path.
    let snap = NpcSnapshot::from_npc(&npc);
    let json = serde_json::to_string(&snap).unwrap();
    let parsed: NpcSnapshot = serde_json::from_str(&json).unwrap();
    let restored = parsed.into_npc();

    assert_eq!(restored.id, NpcId(42));
    assert_eq!(restored.name, "Brigid Ní Fhaoláin");
    assert_eq!(restored.brief_description, "a tall woman in a grey shawl");
    assert_eq!(restored.age, 47);
    assert_eq!(restored.occupation, "Midwife");
    assert_eq!(
        restored.personality,
        "Quiet and deliberate, moves with purpose."
    );
    assert_eq!(restored.pronouns, "she/her", "pronouns lost on round-trip");
    assert_eq!(restored.location(), LocationId(7));
    assert_eq!(restored.mood, "melancholy");
    assert_eq!(restored.home, Some(LocationId(3)));
    assert_eq!(restored.workplace, Some(LocationId(7)));
    assert_eq!(restored.relationships.len(), 1);
    let rel = restored.relationships.get(&NpcId(5)).unwrap();
    assert!((rel.strength - 0.6).abs() < f64::EPSILON);
    assert_eq!(restored.memory.len(), 1, "short-term memory entry lost");
    assert_eq!(restored.knowledge, vec!["The well on Kilmore road is dry."]);
    assert!(matches!(restored.state(), NpcState::Present));
    assert_eq!(
        restored.deflated_summary,
        Some(summary),
        "deflated_summary must survive save/load (#338)"
    );
    assert_eq!(
        restored.last_activity.as_deref(),
        Some("Delivered a baby at the Burke farm.")
    );
    assert!(restored.is_ill, "is_ill lost on round-trip");
    assert_eq!(
        restored.doom,
        Some(Utc.with_ymd_and_hms(1820, 6, 1, 0, 0, 0).unwrap()),
        "doom timestamp lost on round-trip"
    );
    assert!(
        restored.banshee_heralded,
        "banshee_heralded lost on round-trip"
    );
    // reaction_log is intentionally not persisted — verify it is reset.
    assert!(
        restored.reaction_log.is_empty(),
        "reaction_log should be reset on load (not persisted)"
    );
}

/// #338: deflated_summary used to be hard-coded to None on
/// into_npc, so every autosave + reload erased the cognitive-LOD
/// compression history that NpcManager::assign_tiers wrote on
/// demotion. Round-trip it through the snapshot now.
#[test]
fn test_npc_snapshot_preserves_deflated_summary() {
    use parish_npc::transitions::NpcSummary;

    let mut npc = make_test_npc(7, 4);
    let summary = NpcSummary {
        npc_id: NpcId(7),
        location: LocationId(4),
        mood: "wistful".to_string(),
        recent_activity: vec!["minded the bar".to_string()],
        key_relationship_changes: vec![],
    };
    npc.deflated_summary = Some(summary.clone());

    let snap = NpcSnapshot::from_npc(&npc);
    // Serialize through JSON to also exercise the (de)serialize
    // glue, since this is what hits disk in autosaves.
    let json = serde_json::to_string(&snap).unwrap();
    let parsed: NpcSnapshot = serde_json::from_str(&json).unwrap();
    let restored = parsed.into_npc();

    assert_eq!(
        restored.deflated_summary,
        Some(summary),
        "deflated_summary must survive a save→load cycle"
    );
}

/// Backwards compatibility: a snapshot serialized by the
/// pre-#338 schema (no `deflated_summary` field) must still
/// deserialize cleanly, defaulting to None.
#[test]
fn test_npc_snapshot_legacy_blob_without_deflated_summary() {
    let legacy_json = r#"{
        "id": 1, "name": "Legacy", "age": 30, "occupation": "Farmer",
        "personality": "stoic", "location": 1, "mood": "neutral",
        "home": null, "workplace": null, "schedule": null,
        "relationships": {}, "memory": {"entries": []},
        "knowledge": [], "state": "Present"
    }"#;
    let parsed: NpcSnapshot =
        serde_json::from_str(legacy_json).expect("legacy NpcSnapshot must parse");
    assert!(parsed.deflated_summary.is_none());
}

#[test]
fn test_clock_snapshot_serialize_roundtrip() {
    let clock = ClockSnapshot {
        game_time: Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap(),
        speed_factor: 36.0,
        paused: false,
    };
    let json = serde_json::to_string(&clock).unwrap();
    let restored: ClockSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(clock, restored);
}

#[test]
fn test_game_snapshot_capture_and_serialize() {
    let world = WorldState::new();
    let mut npc_manager = NpcManager::new();
    npc_manager.add_npc(make_test_npc(1, 1));
    npc_manager.add_npc(make_test_npc(2, 2));

    let snapshot = GameSnapshot::capture(&world, &npc_manager);

    // Serialize and deserialize
    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snapshot.player_location, restored.player_location);
    assert_eq!(snapshot.weather, restored.weather);
    assert_eq!(snapshot.npcs.len(), restored.npcs.len());
}

#[test]
fn test_game_snapshot_restore() {
    let mut world = WorldState::new();
    world.weather = parish_types::Weather::LightRain;
    world.log("Test entry".to_string());
    let mut npc_manager = NpcManager::new();
    npc_manager.add_npc(make_test_npc(1, 1));

    let snapshot = GameSnapshot::capture(&world, &npc_manager);

    // Create a fresh world and restore
    let mut new_world = WorldState::new();
    let mut new_npcs = NpcManager::new();
    snapshot.restore(&mut new_world, &mut new_npcs);

    assert_eq!(new_world.weather, parish_types::Weather::LightRain);
    assert_eq!(new_world.text_log.len(), 1);
    assert_eq!(new_world.text_log[0], "Test entry");
    assert_eq!(new_npcs.npc_count(), 1);
    assert!(new_npcs.get(NpcId(1)).is_some());
}

#[test]
fn test_player_progress_capture_and_restore() {
    let mut world = WorldState::new();
    let assigned_at = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
    let started_at = Utc.with_ymd_and_hms(1820, 3, 20, 11, 0, 0).unwrap();
    let task_id = world
        .player_progress
        .assign_task(
            "Break the clods and plant seed in the potato patch.",
            NpcId(7),
            LocationId(1),
            assigned_at,
        )
        .unwrap();
    assert_eq!(
        world.player_progress.advance_assigned_task(
            "I take up a spade, break the clods in the potato patch, and plant the seed as Siobhan instructed.",
            LocationId(1),
            started_at,
        ),
        Some(task_id)
    );
    let npc_manager = NpcManager::new();

    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    let json = serde_json::to_string(&snapshot).unwrap();
    let restored_snapshot: GameSnapshot = serde_json::from_str(&json).unwrap();
    let mut restored_world = WorldState::new();
    let mut restored_npcs = NpcManager::new();
    restored_snapshot.restore(&mut restored_world, &mut restored_npcs);

    let restored = restored_world.player_progress.task(task_id).unwrap();
    assert_eq!(
        restored.description,
        "Break the clods and plant seed in the potato patch."
    );
    assert_eq!(restored.assigned_by, NpcId(7));
    assert_eq!(restored.location, LocationId(1));
    assert_eq!(restored.assigned_at, assigned_at);
    assert_eq!(restored.status, TaskStatus::InProgress);
    assert_eq!(restored.started_at, Some(started_at));
    assert_eq!(
        restored.last_matching_action.as_deref(),
        Some(
            "I take up a spade, break the clods in the potato patch, and plant the seed as Siobhan instructed."
        )
    );
    assert_eq!(
        restored_world
            .player_progress
            .assign_task(
                "A later task.",
                NpcId(8),
                LocationId(1),
                Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap(),
            )
            .unwrap()
            .0,
        task_id.0 + 1,
        "save/load must preserve the monotonic task id counter"
    );
}

#[test]
fn test_old_snapshot_without_player_progress_restores_empty_ledger() {
    let json = r#"{
        "player_location": 1,
        "weather": "Clear",
        "text_log": [],
        "clock": {"game_time": "1820-03-20T08:00:00Z", "speed_factor": 36.0, "paused": false},
        "npcs": [],
        "last_tier2_game_time": null
    }"#;
    let snapshot: GameSnapshot =
        serde_json::from_str(json).expect("legacy snapshot without progress must parse");
    assert!(snapshot.player_progress.is_empty());

    let mut world = WorldState::new();
    world
        .player_progress
        .assign_task(
            "This task must be replaced by the loaded save.",
            NpcId(1),
            LocationId(1),
            Utc.with_ymd_and_hms(1820, 3, 20, 7, 0, 0).unwrap(),
        )
        .unwrap();
    let mut npcs = NpcManager::new();
    snapshot.restore(&mut world, &mut npcs);

    assert!(world.player_progress.is_empty());
}

#[test]
fn test_game_snapshot_paused_clock() {
    let mut world = WorldState::new();
    world.clock.pause();
    world.clock.advance(120); // advance 2 hours while paused
    let npc_manager = NpcManager::new();

    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    assert!(snapshot.clock.paused);

    let mut new_world = WorldState::new();
    let mut new_npcs = NpcManager::new();
    snapshot.restore(&mut new_world, &mut new_npcs);

    assert!(new_world.clock.is_paused());
}

#[test]
fn test_game_snapshot_ludicrous_speed_roundtrip() {
    use parish_types::GameSpeed;

    let mut world = WorldState::new();
    world.clock.set_speed(GameSpeed::Ludicrous);
    let npc_manager = NpcManager::new();

    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    assert!(
        (snapshot.clock.speed_factor - GameSpeed::Ludicrous.factor()).abs() < 0.01,
        "captured factor should match Ludicrous"
    );

    let mut new_world = WorldState::new();
    let mut new_npcs = NpcManager::new();
    snapshot.restore(&mut new_world, &mut new_npcs);

    assert!(
        (new_world.clock.speed_factor() - GameSpeed::Ludicrous.factor()).abs() < 0.01,
        "restored speed factor should match Ludicrous"
    );
}

#[test]
fn test_game_snapshot_preserves_tier2_time() {
    let world = WorldState::new();
    let mut npc_manager = NpcManager::new();
    let t = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
    npc_manager.record_tier2_tick(t);

    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    assert_eq!(snapshot.last_tier2_game_time, Some(t));

    let mut new_world = WorldState::new();
    let mut new_npcs = NpcManager::new();
    snapshot.restore(&mut new_world, &mut new_npcs);
    assert!(!new_npcs.needs_tier2_tick(t));
}

#[test]
fn test_visited_locations_roundtrip() {
    let mut world = WorldState::new();
    world.mark_visited(LocationId(2));
    world.mark_visited(LocationId(3));
    let npc_manager = NpcManager::new();

    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    assert_eq!(snapshot.visited_locations.len(), 3); // 1 (start) + 2

    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.visited_locations.len(), 3);
    assert!(restored.visited_locations.contains(&LocationId(1)));
    assert!(restored.visited_locations.contains(&LocationId(2)));
    assert!(restored.visited_locations.contains(&LocationId(3)));
}

#[test]
fn test_old_save_backward_compat_visited() {
    // Simulate an old save JSON without visited_locations field
    let json = r#"{
        "player_location": 5,
        "weather": "Clear",
        "text_log": [],
        "clock": {"game_time": "1820-03-20T08:00:00Z", "speed_factor": 36.0, "paused": false},
        "npcs": [],
        "last_tier2_game_time": null
    }"#;
    let snapshot: GameSnapshot = serde_json::from_str(json).unwrap();
    // serde(default) gives empty set
    assert!(snapshot.visited_locations.is_empty());

    // But restore inserts player location
    let mut world = WorldState::new();
    let mut npcs = NpcManager::new();
    snapshot.restore(&mut world, &mut npcs);
    assert!(world.visited_locations.contains(&LocationId(5)));
}

#[test]
fn test_npc_snapshot_with_relationships() {
    let mut npc = make_test_npc(1, 1);
    use parish_npc::types::{Relationship, RelationshipKind};
    npc.relationships
        .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.7));

    let snap = NpcSnapshot::from_npc(&npc);
    let json = serde_json::to_string(&snap).unwrap();
    let restored: NpcSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.relationships.len(), 1);
    let rel = restored.relationships.get(&NpcId(2)).unwrap();
    assert!((rel.strength - 0.7).abs() < f64::EPSILON);
}

/// Regression for #277 — the set of introduced NPCs must survive a
/// capture/restore cycle; otherwise players lose all name-recognition
/// state across save/load.
#[test]
fn test_introduced_npcs_roundtrip() {
    let world = WorldState::new();
    let mut npc_manager = NpcManager::new();
    npc_manager.add_npc(make_test_npc(1, 1));
    npc_manager.add_npc(make_test_npc(2, 1));
    npc_manager.add_npc(make_test_npc(3, 1));
    npc_manager.mark_introduced(NpcId(1));
    npc_manager.mark_introduced(NpcId(3));
    assert!(npc_manager.is_introduced(NpcId(1)));
    assert!(!npc_manager.is_introduced(NpcId(2)));
    assert!(npc_manager.is_introduced(NpcId(3)));

    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    assert_eq!(snapshot.introduced_npcs.len(), 2);
    assert!(snapshot.introduced_npcs.contains(&NpcId(1)));
    assert!(snapshot.introduced_npcs.contains(&NpcId(3)));

    // Round-trip through JSON to simulate a real save/load.
    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();

    let mut new_world = WorldState::new();
    let mut new_npcs = NpcManager::new();
    restored.restore(&mut new_world, &mut new_npcs);

    assert!(new_npcs.is_introduced(NpcId(1)));
    assert!(!new_npcs.is_introduced(NpcId(2)));
    assert!(new_npcs.is_introduced(NpcId(3)));
}

/// Old saves predating the `introduced_npcs` field must deserialize
/// cleanly with an empty set (serde `default`), so loading a legacy
/// save doesn't produce a deserialization error.
#[test]
fn test_old_save_backward_compat_introduced_npcs() {
    let json = r#"{
        "player_location": 1,
        "weather": "Clear",
        "text_log": [],
        "clock": {"game_time": "1820-03-20T08:00:00Z", "speed_factor": 36.0, "paused": false},
        "npcs": [],
        "last_tier2_game_time": null
    }"#;
    let snapshot: GameSnapshot = serde_json::from_str(json).unwrap();
    assert!(snapshot.introduced_npcs.is_empty());

    let mut world = WorldState::new();
    let mut npcs = NpcManager::new();
    snapshot.restore(&mut world, &mut npcs);
    assert_eq!(npcs.introduced_count(), 0);
}

/// Regression for #286 — `current_location()` must not panic after
/// restoring a snapshot into a `WorldState::new()` (empty graph, only
/// Crossroads in the legacy map) when the player's saved location
/// differs from any entry already present. The fallback placeholder
/// guarantees `current_location()` is total.
#[test]
fn test_restore_current_location_does_not_panic_with_empty_graph() {
    let mut world = WorldState::new();
    // `new()` only registers LocationId(1); a saved location of 99 is
    // unknown to both the legacy `locations` map and the empty graph.
    let mut npc_manager = NpcManager::new();
    let mut snapshot = GameSnapshot::capture(&world, &npc_manager);
    snapshot.player_location = LocationId(99);

    snapshot.restore(&mut world, &mut npc_manager);

    // No panic — the fallback placeholder is inserted for id 99.
    let loc = world.current_location();
    assert_eq!(loc.id, LocationId(99));
    assert_eq!(loc.name, "Unknown location");
}

/// Regression for #286 — when the graph does contain the player's
/// saved location but the legacy `locations` map does not (e.g. a
/// snapshot restored into a fresh `WorldState::new()` before any
/// movement), `restore()` must back-fill the legacy map so
/// `current_location()` returns the real location data.
/// The fallback path in `restore` for a speed_factor that doesn't match
/// any `GameSpeed::ALL` preset must produce a clock with the custom factor.
#[test]
fn test_restore_custom_speed_factor_fallback() {
    let custom_factor = 100.0;
    let world = WorldState::new();
    let npc_manager = NpcManager::new();
    let mut snapshot = GameSnapshot::capture(&world, &npc_manager);
    snapshot.clock.speed_factor = custom_factor;

    let mut new_world = WorldState::new();
    let mut new_npcs = NpcManager::new();
    snapshot.restore(&mut new_world, &mut new_npcs);

    let restored_factor = new_world.clock.speed_factor();
    assert!(
        (restored_factor - custom_factor).abs() < 0.01,
        "expected factor {custom_factor}, got {restored_factor}"
    );
}

#[test]
fn test_restore_populates_legacy_locations_from_graph() {
    use parish_world::graph::WorldGraph;

    // Minimal graph with id 2 ("Darcy's Pub") that's NOT in
    // `WorldState::new()`'s legacy `locations` map. The graph loader
    // rejects orphan locations, so we include a second connected node.
    let graph_json = r#"{
        "locations": [
            {
                "id": 2,
                "name": "Darcy's Pub",
                "description_template": "Warm pub interior.",
                "indoor": true,
                "public": true,
                "lat": 53.6195,
                "lon": -8.0925,
                "connections": [
                    {"target": 7, "path_description": "a short lane"}
                ],
                "associated_npcs": [],
                "mythological_significance": null
            },
            {
                "id": 7,
                "name": "The Crossroads",
                "description_template": "A quiet junction.",
                "indoor": false,
                "public": true,
                "lat": 53.618,
                "lon": -8.095,
                "connections": [
                    {"target": 2, "path_description": "a short lane back"}
                ],
                "associated_npcs": [],
                "mythological_significance": null
            }
        ]
    }"#;
    let graph = WorldGraph::load_from_str(graph_json).unwrap();

    let mut world = WorldState::new();
    world.graph = graph;
    // Deliberately leave `world.locations` as only the Crossroads.
    assert!(!world.locations.contains_key(&LocationId(2)));

    let mut npc_manager = NpcManager::new();
    let mut snapshot = GameSnapshot::capture(&world, &npc_manager);
    snapshot.player_location = LocationId(2);

    snapshot.restore(&mut world, &mut npc_manager);

    // Legacy map is back-filled from the graph, so the real data lands.
    let loc = world.current_location();
    assert_eq!(loc.id, LocationId(2));
    assert_eq!(loc.name, "Darcy's Pub");
    assert!(loc.indoor);
}

// ── snapshot-tier-preserve ──────────────────────────────────────────
//
// Regression tests for the "every session resume re-fires
// `NpcArrived` for everyone at Tier1" bug. `restore_npcs` wipes
// `tier_assignments` by rebuilding `NpcManager` from scratch; the
// fix is `seed_tier_state(world)` in `restore` so the next live
// `assign_tiers` call doesn't see all NPCs as `Tier4` defaults.

use parish_npc::types::CogTier;
use parish_types::events::GameEvent;

fn snapshot_tier_preserve_world_with_npcs() -> (WorldState, NpcManager) {
    let mut world = WorldState::new();
    world.player_location = LocationId(1);
    let mut mgr = NpcManager::new();
    // NPC at the player's location → ends up at Tier1.
    mgr.add_npc(make_test_npc(1, 1));
    // NPC at the same location too — second Tier1.
    mgr.add_npc(make_test_npc(2, 1));
    (world, mgr)
}

#[test]
fn tier_state_preserved_across_restore() {
    let (mut world, mut npcs) = snapshot_tier_preserve_world_with_npcs();

    // Run a real `assign_tiers` so the tier map is populated for the
    // capture below.
    let _ = npcs.assign_tiers(&world, &[]);
    assert_eq!(npcs.tier_of(NpcId(1)), Some(CogTier::Tier1));
    assert_eq!(npcs.tier_of(NpcId(2)), Some(CogTier::Tier1));

    let snapshot = GameSnapshot::capture(&world, &npcs);

    // C1 — fresh manager, restore, then check tier state was seeded
    // even though `tier_assignments` isn't serialised.
    let mut new_world = WorldState::new();
    let mut new_npcs = NpcManager::new();
    snapshot.restore(&mut new_world, &mut new_npcs);
    assert_eq!(
        new_npcs.tier_of(NpcId(1)),
        Some(CogTier::Tier1),
        "C1: tier_assignments missing for NpcId(1) after restore",
    );
    assert_eq!(
        new_npcs.tier_of(NpcId(2)),
        Some(CogTier::Tier1),
        "C1: tier_assignments missing for NpcId(2) after restore",
    );

    // Subsequent `assign_tiers` on the restored state should be a
    // no-op for tier transitions.
    let transitions = new_npcs.assign_tiers(&new_world, &[]);
    assert!(
        transitions.is_empty(),
        "C3: assign_tiers after restore must produce no transitions, \
         got {} (tiers were re-promoted from Tier4 default)",
        transitions.len(),
    );

    // Replicate the C1/C3 chain on the same world reused (not a
    // fresh `WorldState::new()`) — guards against accidental
    // dependency on the new-world default state.
    let _ = world.event_bus.subscribe(); // detach from prior tests
    let _ = npcs.assign_tiers(&world, &[]); // re-prime
    let snapshot2 = GameSnapshot::capture(&world, &npcs);
    let mut npcs2 = NpcManager::new();
    snapshot2.restore(&mut world, &mut npcs2);
    let transitions2 = npcs2.assign_tiers(&world, &[]);
    assert!(transitions2.is_empty(), "C3 (same-world reuse)");
}

#[test]
fn restore_publishes_no_npc_arrived_events() {
    let (world, mut npcs) = snapshot_tier_preserve_world_with_npcs();
    let _ = npcs.assign_tiers(&world, &[]);
    let snapshot = GameSnapshot::capture(&world, &npcs);

    let mut new_world = WorldState::new();
    let mut new_npcs = NpcManager::new();
    let mut rx = new_world.event_bus.subscribe();

    snapshot.restore(&mut new_world, &mut new_npcs);

    // C2 — drain the bus and assert no `NpcArrived` event fired.
    let mut npc_arrived_count = 0;
    while let Ok(evt) = rx.try_recv() {
        if matches!(evt, GameEvent::NpcArrived { .. }) {
            npc_arrived_count += 1;
        }
    }
    assert_eq!(
        npc_arrived_count, 0,
        "C2: restore must not publish NpcArrived; saw {} event(s)",
        npc_arrived_count,
    );

    // C3 — same drain after the first post-restore `assign_tiers`.
    let _ = new_npcs.assign_tiers(&new_world, &[]);
    let mut post_assign_count = 0;
    while let Ok(evt) = rx.try_recv() {
        if matches!(evt, GameEvent::NpcArrived { .. }) {
            post_assign_count += 1;
        }
    }
    assert_eq!(
        post_assign_count, 0,
        "C3: first assign_tiers after restore must not refire NpcArrived; saw {} event(s)",
        post_assign_count,
    );
}

#[test]
fn tier_promotion_does_not_fire_npc_arrived() {
    // Cognitive tier transitions describe player-relative attention,
    // not physical movement. An NPC that was already at LocationId(2)
    // when the player walks toward them did not "arrive" anywhere —
    // they were always there. NpcArrived only fires from real moves
    // (schedule transit completing, tier-3 LLM-driven relocations).
    let mut world = WorldState::new();
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        r#"{
            "locations": [
                {"id":1,"name":"Home","description_template":"","indoor":true,
                 "public":true,"lat":53.0,"lon":-8.0,
                 "connections":[{"target":2,"path_description":"path"}]},
                {"id":2,"name":"Away","description_template":"","indoor":false,
                 "public":true,"lat":53.001,"lon":-8.0,
                 "connections":[{"target":1,"path_description":"path"}]}
            ]
        }"#,
    )
    .unwrap();
    world.graph = parish_world::graph::WorldGraph::load_from_file(file.path()).unwrap();
    world.player_location = LocationId(1);

    let mut npcs = NpcManager::new();
    npcs.add_npc(make_test_npc(7, 2));

    let _ = npcs.assign_tiers(&world, &[]);
    assert_eq!(npcs.tier_of(NpcId(7)), Some(CogTier::Tier2));

    let snapshot = GameSnapshot::capture(&world, &npcs);
    let mut new_world = WorldState::new();
    new_world.graph = world.graph.clone();
    let mut new_npcs = NpcManager::new();
    let mut rx = new_world.event_bus.subscribe();
    snapshot.restore(&mut new_world, &mut new_npcs);

    assert!(matches!(
        rx.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));

    // Player walks to the NPC's location → NPC promotes Tier2 → Tier1.
    // No NpcArrived event must fire; the NPC didn't move.
    new_world.player_location = LocationId(2);
    let transitions = new_npcs.assign_tiers(&new_world, &[]);
    assert!(
        transitions.iter().any(|t| t.npc_id == NpcId(7)),
        "NPC should have transitioned tiers"
    );
    let mut arrived_for_seven = 0;
    while let Ok(evt) = rx.try_recv() {
        if let GameEvent::NpcArrived { npc_id, .. } = evt
            && npc_id == NpcId(7)
        {
            arrived_for_seven += 1;
        }
    }
    assert_eq!(
        arrived_for_seven, 0,
        "tier promotion alone must not publish NpcArrived; saw {}",
        arrived_for_seven,
    );
}
