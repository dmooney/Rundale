//! Proves `build_tier1_context` enriches the NPC dialogue prompt with real
//! current-location knowledge when run against the *shipping* Rundale world
//! (`mods/rundale/world.json`) — not a synthetic test graph. This is the
//! deterministic, model-independent stand-in for the live behavioural
//! criterion: the prompt the model would answer from must name real graph
//! neighbours of the player's location, with their prose paths, on-foot
//! travel estimates, indoor/outdoor + public/private framing, the local
//! mythological note, and live weather hazards.

use std::path::PathBuf;

use parish_npc::build_tier1_context;
use parish_types::LocationId;
use parish_world::{Weather, WorldState};

fn rundale_world_file() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir
        .join("../../..")
        .canonicalize()
        .expect("CARGO_MANIFEST_DIR resolves to repo root via ../../..")
        .join("mods/rundale/world.json");
    assert!(
        path.is_file(),
        "expected real Rundale world at {}; this test must fail loudly, not skip",
        path.display()
    );
    path
}

/// Kilteevan Village (id 15) is the richest hub in the shipping world: ten
/// connections, an outdoor/public setting, a mythological note, and two
/// flood-hazard edges. It is also the player's live start location.
const KILTEEVAN_VILLAGE: LocationId = LocationId(15);

#[test]
fn tier1_context_from_real_world_names_real_neighbours_and_framing() {
    let world = WorldState::from_parish_file(&rundale_world_file(), KILTEEVAN_VILLAGE)
        .expect("real Rundale world should load");

    let context = build_tier1_context(&world);
    println!(
        "\n===== build_tier1_context @ Kilteevan Village (real world) =====\n{context}\n====="
    );

    // Paths block lists real graph neighbours by name (criterion 1).
    assert!(
        context.contains("Paths from here:"),
        "paths block missing: {context}"
    );
    for neighbour in [
        "The Crossroads",
        "St. Brigid's Church",
        "The Forge",
        "The Holy Well",
    ] {
        assert!(
            context.contains(neighbour),
            "real neighbour {neighbour:?} missing from context: {context}"
        );
    }

    // Real prose path description is carried verbatim (criterion 1).
    assert!(
        context.contains("the road north past low fields to the crossroads"),
        "real path description missing: {context}"
    );

    // On-foot travel estimate per connection (criterion 2).
    assert!(
        context.contains("min on foot"),
        "on-foot travel estimate missing: {context}"
    );

    // Indoor/outdoor + public/private framing matches the data (criterion 4).
    assert!(
        context.contains("an open outdoor place"),
        "outdoor framing missing: {context}"
    );
    assert!(
        context.contains("open to all"),
        "public framing missing: {context}"
    );

    // Mythological note from the real location (criterion 5).
    assert!(
        context.contains("Of local note:") && context.contains("Cill Taobháin"),
        "mythological note missing: {context}"
    );
}

#[test]
fn tier1_context_from_real_world_flags_weather_hazard_on_flood_edges() {
    // Kilteevan's edges to The Holy Well (17) and The Mill (18) carry a
    // "flood" hazard. Under clear weather they read normally; under a storm
    // the connection lines must carry a hazard note (criterion 3) — proven
    // against the real world data, not a synthetic edge.
    let world_file = rundale_world_file();

    let clear = WorldState::from_parish_file(&world_file, KILTEEVAN_VILLAGE).unwrap();
    let clear_ctx = build_tier1_context(&clear);
    assert!(
        !clear_ctx.contains("impassable in this weather"),
        "no edge should be impassable under clear weather: {clear_ctx}"
    );

    let mut stormy = WorldState::from_parish_file(&world_file, KILTEEVAN_VILLAGE).unwrap();
    stormy.weather = Weather::Storm;
    let storm_ctx = build_tier1_context(&stormy);
    println!("\n===== build_tier1_context @ Kilteevan Village (STORM) =====\n{storm_ctx}\n=====");
    assert!(
        storm_ctx.contains("impassable in this weather") || storm_ctx.contains("slow going today"),
        "flood-hazard edge should carry a weather note under storm: {storm_ctx}"
    );
}
