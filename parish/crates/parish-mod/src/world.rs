//! Bridge from [`super::GameMod`] to [`parish_world::WorldState`].

use super::GameMod;

/// Creates a [`parish_world::WorldState`] from a loaded [`GameMod`].
///
/// Bridges [`GameMod`] (which lives in `parish-mod`) and
/// [`parish_world::WorldState::from_mod_params`] (which lives in `parish-world`
/// and cannot depend on the mod loader).
pub fn world_state_from_mod(
    game_mod: &GameMod,
) -> Result<parish_world::WorldState, parish_types::ParishError> {
    parish_world::WorldState::from_mod_params(
        &game_mod.world_path(),
        parish_types::LocationId(game_mod.start_location()),
        game_mod.start_date(),
    )
}
