//! Deterministic place atmosphere for listening, omens, and folklore.

use chrono::Timelike;

use crate::input::AtmosphericTopic;
use crate::ipc::config::GameConfig;
use crate::world::WorldState;
use crate::world::folklore::{listen_at_location, listening_seed};
use crate::world::time::{Season, time_of_day_from_hour};

use super::CommandResult;

const LISTEN_INTRO: &str = "You stand still and listen.";
const OMEN_INTRO: &str = "You watch for an omen.";
const FOLKLORE_INTRO: &str = "You call to mind what is said of this place.";
const DISABLED_TEXT: &str = "Listening to places is currently disabled.";
const NO_OMEN_TEXT: &str = "Nothing in the place sets itself apart from the ordinary.";

/// Selects whether place atmosphere is the action itself or a brief addition
/// to another player turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtmospherePresentation {
    /// A complete response to `/listen`, `/omen`, or `/folklore`.
    Standalone,
    /// One concise paragraph that can accompany a natural-language turn.
    Supplemental,
}

/// Renders one grounded, deterministic strand of the current place.
///
/// Each topic owns one component of the place vignette:
///
/// - [`AtmosphericTopic::Listen`] renders only the ordinary soundscape.
/// - [`AtmosphericTopic::Omen`] renders only a cautious sensory echo, and
///   never asserts a prophecy.
/// - [`AtmosphericTopic::Folklore`] renders only the exact account authored
///   in `mythological_significance`.
///
/// A supplemental render returns `None` when `place-listening` is explicitly
/// disabled, allowing the underlying turn to continue unchanged. A standalone
/// render instead returns a concise explanation for the player. The function
/// reads one clock instant and never mutates world or configuration state.
pub fn render_place_atmosphere(
    world: &WorldState,
    config: &GameConfig,
    topic: AtmosphericTopic,
    presentation: AtmospherePresentation,
) -> Option<String> {
    if config.flags.is_disabled("place-listening") {
        return match presentation {
            AtmospherePresentation::Standalone => Some(DISABLED_TEXT.to_string()),
            AtmospherePresentation::Supplemental => None,
        };
    }

    let body = if let Some(location) = world.current_location_data() {
        // Derive every temporal input from one instant. At accelerated game
        // speeds, separate reads could otherwise straddle a phase boundary.
        let now = world.clock.now();
        let date = now.date_naive();
        let time = time_of_day_from_hour(now.hour());
        let seed = listening_seed(date, location.id, time);
        let vignette =
            listen_at_location(location, time, Season::from_date(date), world.weather, seed);

        match topic {
            AtmosphericTopic::Listen => vignette.ambient,
            AtmosphericTopic::Omen => vignette.echo.unwrap_or_else(|| NO_OMEN_TEXT.to_string()),
            AtmosphericTopic::Folklore => vignette.lore.unwrap_or_else(no_old_account),
        }
    } else {
        // Tiny test worlds and old saves may lack extended graph data. Keep
        // every response useful without inventing a sound, sign, or tradition.
        let location_name = world.current_location().name.as_str();
        match topic {
            AtmosphericTopic::Listen => {
                format!("The ordinary sounds of {location_name} settle around you.")
            }
            AtmosphericTopic::Omen => NO_OMEN_TEXT.to_string(),
            AtmosphericTopic::Folklore => no_old_account(),
        }
    };

    match presentation {
        AtmospherePresentation::Supplemental => Some(body),
        AtmospherePresentation::Standalone => {
            let intro = match topic {
                AtmosphericTopic::Listen => LISTEN_INTRO,
                AtmosphericTopic::Omen => OMEN_INTRO,
                AtmosphericTopic::Folklore => FOLKLORE_INTRO,
            };
            Some(format!("{intro}\n\n{body}"))
        }
    }
}

fn no_old_account() -> String {
    "No old account of this place comes readily to mind.".to_string()
}

/// Handles the three standalone atmosphere commands.
pub(super) fn handle_atmospheric_command(
    world: &WorldState,
    config: &GameConfig,
    topic: AtmosphericTopic,
) -> CommandResult {
    let response =
        render_place_atmosphere(world, config, topic, AtmospherePresentation::Standalone)
            .expect("standalone place-atmosphere rendering always returns text");
    CommandResult::text(response)
}
