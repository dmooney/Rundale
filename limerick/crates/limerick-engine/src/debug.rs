//! `/debug` for the engine's [`App`]: the shared queries in
//! [`limerick_core::debug_view`], run against the app's world and NPCs.

use crate::app::App;
use limerick_core::debug_view::{self, DebugView};

/// Handles a `/debug` command and returns lines to display.
///
/// The `sub` argument is the text after `/debug `, or `None` for bare `/debug`.
pub fn handle_debug(sub: Option<&str>, app: &App) -> Vec<String> {
    let language = app.language_settings();
    debug_view::handle_debug(
        sub,
        &DebugView {
            world: &app.world,
            npc_manager: &app.npc_manager,
            language: &language,
        },
    )
}
