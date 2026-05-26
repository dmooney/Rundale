//! Session command — music vignette at the pub.

use crate::world::WorldState;
use crate::world::session::{is_session_hour, session_seed, vignette_from_seed};

use super::CommandResult;
use crate::ipc::config::GameConfig;

/// Handles the `/session` command.
///
/// Three guards, in order:
///
/// 1. **Feature flag** — the `session` flag is default-on; explicitly
///    disabling it (`/flag disable session`) silences the command with
///    a short acknowledgement.
/// 2. **Location** — only fires at a pub. The check walks the current
///    location's `name` and `aliases` case-insensitively so a mod that
///    renames Darcy's Pub (or adds another pub) keeps working. A miss
///    prints a hint pointing the player at the right place.
/// 3. **Time** — only fires at dusk, night, or midnight. At other hours
///    the pub is effectively empty of musicians and the command prints
///    a hint rather than a vignette.
pub(super) fn handle_session_command(world: &WorldState, config: &GameConfig) -> CommandResult {
    if config.flags.is_disabled("session") {
        return CommandResult::text("The session feature is currently disabled.");
    }

    let loc = world.current_location();
    let loc_name = loc.name.clone();
    let is_pub = location_is_pub(world);
    if !is_pub {
        return CommandResult::text(format!(
            "{loc_name} is quiet — no music here. Try Darcy's Pub after dusk."
        ));
    }

    let tod = world.clock.time_of_day();
    if !is_session_hour(tod) {
        return CommandResult::text(
            "The pub is quiet — too early for music. Come back after dusk.",
        );
    }

    let date = world.clock.now().date_naive();
    let seed = session_seed(date, loc.id.0);
    let v = vignette_from_seed(seed, world.weather, world.clock.season());

    // Compose the vignette into a single response. Paragraph-style so
    // the frontend's prose renderer handles line wrapping; the pub name
    // anchors the reader before the scene begins.
    let mut out = String::new();
    out.push_str(&format!("A session is in flow at {loc_name}.\n\n"));
    out.push_str(&v.ambient);
    out.push_str("\n\n");
    out.push_str(&v.musician);
    out.push(' ');
    out.push_str(&v.tune);
    if let Some(verse) = v.verse {
        out.push_str("\n\n");
        out.push_str(&verse);
    }
    CommandResult::text(out)
}

/// Returns `true` when the player's current location looks like a pub.
///
/// Checks the location's name and aliases case-insensitively for the
/// substring "pub". Mods that rename Darcy's (or add a second pub) keep
/// working as long as the word is retained somewhere in name or aliases.
fn location_is_pub(world: &WorldState) -> bool {
    let name_lc = world.current_location().name.to_lowercase();
    if name_lc.contains("pub") {
        return true;
    }
    world
        .current_location_data()
        .map(|data| {
            data.aliases
                .iter()
                .any(|a| a.to_lowercase().contains("pub"))
        })
        .unwrap_or(false)
}
