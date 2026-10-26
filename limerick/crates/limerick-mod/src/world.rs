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
    Ok(world)
}

#[cfg(test)]
mod tests {
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
