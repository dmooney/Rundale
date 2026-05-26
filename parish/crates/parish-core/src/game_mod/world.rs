//! Bridge from [`super::GameMod`] to [`crate::world::WorldState`].

use super::GameMod;

/// Interpolates `{placeholder}` patterns in a template string.
///
/// Replaces each `{key}` with the corresponding value from the provided
/// key-value pairs. Unknown placeholders are left as-is.
///
/// Creates a [`crate::world::WorldState`] from a loaded [`GameMod`].
///
/// Bridges [`GameMod`] (which lives in `parish-core`) and
/// [`crate::world::WorldState::from_mod_params`] (which lives in `parish-world`
/// and cannot depend on `parish-core`).
pub fn world_state_from_mod(
    game_mod: &GameMod,
) -> Result<crate::world::WorldState, parish_types::ParishError> {
    crate::world::WorldState::from_mod_params(
        &game_mod.world_path(),
        parish_types::LocationId(game_mod.start_location()),
        game_mod.start_date(),
    )
}
