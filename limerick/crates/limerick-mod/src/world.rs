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
    world.dialogue_anachronisms = game_mod.anachronisms.terms.clone();
    world.dialogue_anachronism_alert_prefix = game_mod.anachronisms.context_alert_prefix.clone();
    world.dialogue_anachronism_alert_suffix = game_mod.anachronisms.context_alert_suffix.clone();
    Ok(world)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rundale_world_carries_authored_dialogue_anachronisms() {
        let mod_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        let game_mod = GameMod::load(&mod_path).expect("load Rundale mod");
        let world = world_state_from_mod(&game_mod).expect("build Rundale world");

        assert!(
            world
                .dialogue_anachronisms
                .iter()
                .any(|entry| entry.term == "planning board")
        );
        assert!(
            world
                .dialogue_anachronisms
                .iter()
                .any(|entry| entry.term == "agricultural show")
        );
    }
}
