//! End-to-end check that the demo prompt-builder, when fed the same payloads
//! the frontend consumes, never leaks identity information the GUI would
//! hide. Backs the issue-998 acceptance criteria — see
//! `docs/proofs/issue-998/`.

use std::path::{Path, PathBuf};

use parish_core::game_loop::save::load_fresh_world_and_npcs;
use parish_core::game_mod::GameMod;
use parish_core::ipc::demo::{build_demo_context, render_user_prompt};
use parish_core::ipc::handlers::{build_map_data, build_npcs_here, snapshot_from_world};
use parish_core::world::transport::TransportMode;

fn rundale_mod_dir() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.join("../../../mods/rundale")
}

fn walking_mode() -> TransportMode {
    TransportMode {
        id: "walking".to_string(),
        label: "on foot".to_string(),
        speed_m_per_s: 1.3,
    }
}

/// Fresh-save snapshot: every NPC is un-introduced, so the demo prompt must
/// not contain occupation or real-name tokens for any co-located NPC.
#[test]
fn demo_prompt_at_fresh_save_does_not_leak_widow_or_peig() {
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).expect("load rundale mod");
    let (world, npc_manager) =
        load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).expect("load fresh world");

    let snapshot = snapshot_from_world(&world);
    let npcs = build_npcs_here(&world, &npc_manager);
    let map = build_map_data(&world, &walking_mode(), false);

    let ctx = build_demo_context(
        &snapshot,
        &npcs,
        &map,
        "Wednesday, 14 May 1820, morning".to_string(),
        "spring".to_string(),
        None,
    );
    let prompt = render_user_prompt(&ctx);

    // None of these tokens should appear in the prompt before any NPC has
    // been introduced. The bug at the root of issue #998 produced "Widow"
    // and "Peig Hannigan" on turn 1; the post-fix prompt must show only
    // brief descriptions.
    let leaked: Vec<&str> = ["Widow", "Peig", "Hannigan", "Publican", "Gallagher"]
        .into_iter()
        .filter(|t| prompt.contains(t))
        .collect();
    assert!(
        leaked.is_empty(),
        "demo prompt leaked identity tokens at fresh save: {leaked:?}\n\nPrompt was:\n{prompt}"
    );

    // Every co-located NPC must show as the brief description (always
    // contains a comma or the word "a/an" — they're rendered as English
    // noun phrases like "a small, sharp-eyed old woman wrapped in a shawl").
    for npc in &ctx.npcs_here {
        assert!(
            npc.occupation.is_none(),
            "NPC {:?} should hide occupation pre-intro",
            npc.name
        );
    }
}

/// The map-derived adjacent list must agree with the GUI's fog-of-war
/// (`build_map_data`) — anything beyond the frontier is absent. Unvisited
/// frontier entries now carry a travel-time estimate so the auto-player can
/// judge distance before moving (#1207 #33/#36).
#[test]
fn demo_prompt_adjacent_block_mirrors_map_fog_of_war() {
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();
    let (world, npc_manager) = load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).unwrap();

    let snapshot = snapshot_from_world(&world);
    let npcs = build_npcs_here(&world, &npc_manager);
    let map = build_map_data(&world, &walking_mode(), false);

    let ctx = build_demo_context(
        &snapshot,
        &npcs,
        &map,
        "Wednesday, 14 May 1820, morning".to_string(),
        "spring".to_string(),
        None,
    );

    // Map adjacency is the source of truth — the demo list is a strict
    // subset (entries adjacent to the player, excluding the player's own
    // location).
    let map_adjacent_names: Vec<&str> = map
        .locations
        .iter()
        .filter(|l| l.adjacent && l.id != map.player_location)
        .map(|l| l.name.as_str())
        .collect();
    let demo_names: Vec<&str> = ctx.adjacent.iter().map(|a| a.name.as_str()).collect();

    for name in &demo_names {
        assert!(
            map_adjacent_names.contains(name),
            "demo adjacent list contained {name} which the GUI map hides"
        );
    }

    // Frontier entries (unvisited but reachable) must carry a travel-time
    // estimate — the auto-player needs the distance cue to choose a move.
    for adj in &ctx.adjacent {
        if !adj.visited {
            assert!(
                adj.travel_minutes.is_some(),
                "frontier entry {} should carry a travel estimate",
                adj.name
            );
        }
    }
}

/// Prints the full rendered demo prompt at fresh-save to stdout. Run with
/// `cargo test ... -- --nocapture` to capture the live prompt body for the
/// proof bundle (issue-998 acceptance criteria).
#[test]
fn print_fresh_save_prompt_for_proof_bundle() {
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();
    let (world, npc_manager) = load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).unwrap();

    let snapshot = snapshot_from_world(&world);
    let npcs = build_npcs_here(&world, &npc_manager);
    let map = build_map_data(&world, &walking_mode(), false);

    let ctx = build_demo_context(
        &snapshot,
        &npcs,
        &map,
        "Wednesday, 14 May 1820, morning".to_string(),
        "spring".to_string(),
        None,
    );
    let prompt = render_user_prompt(&ctx);
    println!("=== BEGIN demo user_prompt (issue-998 proof) ===");
    println!("{prompt}");
    println!("=== END demo user_prompt ===");
}
