//! Bridge from [`super::GameMod`] to [`limerick_world::WorldState`].

use super::GameMod;

/// Creates a [`limerick_world::WorldState`] from a loaded [`GameMod`].
///
/// Bridges [`GameMod`] (which lives in `limerick-mod`) and
/// [`limerick_world::WorldState::from_mod_params`] (which lives in `limerick-world`
/// and cannot depend on the mod loader).
pub fn world_state_from_mod(
    game_mod: &GameMod,
) -> Result<limerick_world::WorldState, limerick_types::LimerickError> {
    let mut world = limerick_world::WorldState::from_mod_params(
        &game_mod.world_path(),
        limerick_types::LocationId(game_mod.start_location()),
        game_mod.start_date(),
    )?;
    world.content = Some(game_mod.content_identity());
    validate_living_world(game_mod, &world)?;
    // A new game starts with the mod's one gossip fact, known to its source
    // alone. A resumed save restores its own network over this one.
    game_mod.living_world.seed_gossip(&mut world.gossip_network);
    Ok(world)
}

/// Checks the mod's living-world facts against its own NPC and location ids.
///
/// `GameMod::load` cannot do this (it loads neither NPCs nor the world graph),
/// so it runs here, where both catalogues are available. A bad reference
/// fails the mod load with `LimerickError::Config`, like every other mod file.
fn validate_living_world(
    game_mod: &GameMod,
    world: &limerick_world::WorldState,
) -> Result<(), limerick_types::LimerickError> {
    use limerick_types::LimerickError;
    if game_mod.living_world == crate::LivingWorld::default() {
        return Ok(());
    }
    let npcs_path = game_mod.npcs_path();
    let text = std::fs::read_to_string(&npcs_path).map_err(|e| {
        LimerickError::Config(format!(
            "living world: cannot read NPC file {}: {e}",
            npcs_path.display()
        ))
    })?;
    let file: limerick_npc::NpcFile = serde_json::from_str(&text).map_err(|e| {
        LimerickError::Config(format!(
            "living world: cannot parse NPC file {}: {e}",
            npcs_path.display()
        ))
    })?;
    let npcs = file.npcs.iter().map(|npc| npc.id).collect();
    let locations = world.graph.location_ids().iter().map(|id| id.0).collect();
    game_mod
        .living_world
        .validate(&npcs, &locations)
        .map_err(|e| {
            LimerickError::Config(format!(
                "invalid living-world facts in mod '{}': {e}",
                game_mod.manifest.meta.id
            ))
        })
}

#[cfg(test)]
mod tests {
    /// Copies the shipped Rundale mod into a temp dir with `living_world`
    /// replaced by `body`.
    fn rundale_with_living_world(body: &str) -> tempfile::TempDir {
        fn copy(from: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), &target).unwrap();
                }
            }
        }
        let tmp = tempfile::TempDir::new().unwrap();
        copy(&shipped_rundale(), tmp.path());
        std::fs::write(tmp.path().join("living-world.json"), body).unwrap();
        tmp
    }

    fn shipped_rundale() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale")
    }

    #[test]
    fn the_shipped_rundale_mod_loads_with_its_living_world() {
        let game_mod = GameMod::load(&shipped_rundale()).expect("load shipped Rundale mod");
        assert!(game_mod.living_world.gossip.is_some());
        let world = world_state_from_mod(&game_mod).expect("shipped facts validate at load");
        assert_eq!(world.gossip_network.len(), 1);
    }

    #[test]
    fn a_bad_npc_id_in_a_gossip_path_fails_the_mod_load() {
        let mut bad = GameMod::load(&shipped_rundale()).unwrap().living_world;
        bad.gossip.as_mut().unwrap().path.push(99_999);
        let tmp = rundale_with_living_world(&serde_json::to_string(&bad).unwrap());
        let game_mod = GameMod::load(tmp.path()).expect("the file itself parses");
        let error = world_state_from_mod(&game_mod)
            .err()
            .expect("bad reference fails the load");
        let message = error.to_string();
        assert!(
            message.contains("99999") && message.contains("living-world"),
            "clear error naming the id: {message}"
        );
    }

    #[test]
    fn a_bad_task_location_fails_the_mod_load() {
        let mut bad = GameMod::load(&shipped_rundale()).unwrap().living_world;
        bad.task.as_mut().unwrap().location = 88_888;
        let tmp = rundale_with_living_world(&serde_json::to_string(&bad).unwrap());
        let game_mod = GameMod::load(tmp.path()).unwrap();
        let error = world_state_from_mod(&game_mod)
            .err()
            .expect("bad location fails the load");
        assert!(error.to_string().contains("88888"), "{error}");
    }

    use super::*;

    #[test]
    fn world_from_mod_carries_content_identity() {
        let mod_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testing/fixtures/mods/rundale-legacy");
        let game_mod = GameMod::load(&mod_path).expect("load Rundale mod");
        let world = world_state_from_mod(&game_mod).expect("build Rundale world");
        assert_eq!(
            world.content,
            Some(limerick_types::ContentIdentity {
                id: "rundale".to_string(),
                version: game_mod.manifest.meta.version.clone(),
            }),
            "the world carries the content identity saves record"
        );
    }
}
