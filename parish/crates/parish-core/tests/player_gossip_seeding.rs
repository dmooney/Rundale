//! Integration tests for player-arrival gossip seeding (#1425).
//!
//! Closes the coverage gap where NPC-NPC Tier 2 events seed the gossip network
//! but player movement never did.
//!
//! Two tests:
//! - Flag ON (default): `apply_movement` to a new location must produce a
//!   gossip entry attributed to NpcId(0) (the synthetic player source).
//! - Flag OFF: explicitly disabling `player-action-gossip` must produce NO new
//!   entry.

use parish_core::config::FeatureFlags;
use parish_core::game_session::apply_movement;
use parish_core::npc::reactions::ReactionTemplates;
use parish_core::npc::{NpcId, manager::NpcManager};
use parish_core::world::{LocationId, WorldState, transport::TransportMode};

fn make_transport() -> TransportMode {
    TransportMode {
        label: "on foot".to_string(),
        id: "walking".to_string(),
        speed_m_per_s: 1.2,
    }
}

/// Build a minimal two-location world so the test runs without the Rundale mod.
fn two_location_world() -> WorldState {
    let file = tempfile::NamedTempFile::new().expect("temp world file");
    std::fs::write(
        file.path(),
        r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "Crossroads",
                    "description_template": "Crossroads in {weather}.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.0,
                    "lon": -8.0,
                    "connections": [
                        {"target": 2, "path_description": "a lane to the chapel"}
                    ]
                },
                {
                    "id": 2,
                    "name": "Chapel",
                    "description_template": "The chapel is quiet in {weather}.",
                    "indoor": true,
                    "public": true,
                    "lat": 53.001,
                    "lon": -8.0,
                    "connections": [
                        {"target": 1, "path_description": "a lane to the crossroads"}
                    ]
                }
            ]
        }"#,
    )
    .expect("write tiny world");
    WorldState::from_parish_file(file.path(), LocationId(1)).expect("load tiny world")
}

/// When `player-action-gossip` is NOT explicitly disabled (the default-on
/// case), a successful player move must seed exactly one gossip entry
/// attributed to NpcId(0) (the player's synthetic source id) and the
/// content must name the destination location.
///
/// This test FAILS against the pre-fix `apply_movement` because the old code
/// never called `gossip_network.create` for player movement — it only seeded
/// gossip from Tier 2 NPC events.
#[test]
fn player_arrival_seeds_gossip_when_flag_on() {
    let mut world = two_location_world();
    let mut npc_manager = NpcManager::new();
    let templates = ReactionTemplates::default();
    let transport = make_transport();

    // FeatureFlags::default() — `player-action-gossip` is unknown → not
    // disabled → gossip seeding is active.
    let flags = FeatureFlags::default();

    assert_eq!(
        world.gossip_network.len(),
        0,
        "gossip network must be empty before movement"
    );

    let effects = apply_movement(
        &mut world,
        &mut npc_manager,
        &templates,
        "Chapel",
        &transport,
        &flags,
    );

    assert!(effects.world_changed, "movement to Chapel must succeed");
    assert_eq!(world.player_location, LocationId(2));

    assert_eq!(
        world.gossip_network.len(),
        1,
        "exactly one gossip item must be seeded on player arrival (flag ON)"
    );

    let items = world.gossip_network.known_by(NpcId(0));
    assert_eq!(
        items.len(),
        1,
        "the seeded item must be attributed to NpcId(0) (the player)"
    );
    assert_eq!(
        items[0].source,
        NpcId(0),
        "gossip source must be the synthetic player NpcId"
    );
    assert!(
        items[0].content.contains("Chapel"),
        "gossip content must name the destination; got: {:?}",
        items[0].content
    );
}

/// When `player-action-gossip` is explicitly disabled, player movement must
/// NOT produce any new gossip entries.
///
/// This is the negative-path / kill-switch test.
#[test]
fn player_arrival_does_not_seed_gossip_when_flag_off() {
    let mut world = two_location_world();
    let mut npc_manager = NpcManager::new();
    let templates = ReactionTemplates::default();
    let transport = make_transport();

    let mut flags = FeatureFlags::default();
    flags.disable("player-action-gossip");

    let effects = apply_movement(
        &mut world,
        &mut npc_manager,
        &templates,
        "Chapel",
        &transport,
        &flags,
    );

    assert!(effects.world_changed, "movement to Chapel must succeed");
    assert_eq!(
        world.gossip_network.len(),
        0,
        "no gossip must be seeded when player-action-gossip flag is disabled"
    );
}
