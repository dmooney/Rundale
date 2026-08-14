//! Help text — the canonical list of user-facing commands and its renderer.

/// Canonical list of user-facing system commands shown by `/help`.
///
/// Kept in alphabetical order by command name so the rendered output is
/// stable and easy to scan. Descriptions are short so the list fits
/// comfortably in the chat panel.
const HELP_ENTRIES: &[(&str, &str)] = &[
    ("/about", "About this game"),
    ("/branches", "List save branches"),
    ("/designer", "Open the Parish Designer"),
    ("/flag disable <name>", "Disable a feature flag"),
    ("/flag enable <name>", "Enable a feature flag"),
    ("/flag list", "List all feature flags"),
    ("/folklore", "Recall the old account of this place"),
    ("/fork <name>", "Fork a new branch from here"),
    ("/help", "Show this help"),
    ("/hints", "Toggle language-hints sidebar"),
    ("/improv", "Toggle improv craft mode"),
    (
        "/inference-log [on|off|status|path]",
        "Toggle or inspect the on-disk LLM log",
    ),
    ("/irish", "Toggle Irish pronunciation sidebar"),
    ("/listen", "Attend to the ordinary sounds around you"),
    ("/load <name>", "Load a named branch"),
    ("/log", "Show branch history"),
    ("/map [id]", "List or switch map tile sources"),
    ("/new-game", "Start a fresh game"),
    ("/npcs", "Who is nearby?"),
    ("/omen", "Watch for a cautious sign in this place"),
    ("/pause", "Hold time still"),
    ("/resume", "Let time flow again"),
    ("/save", "Save the game"),
    ("/session", "Listen to the music session at the pub"),
    (
        "/speed [slow|normal|fast|fastest|ludicrous]",
        "Show or change game speed",
    ),
    ("/status", "Where am I?"),
    ("/time", "Time, weather, and season details"),
    (
        "/unexplored [reveal|hide]",
        "Reveal or hide all unexplored locations",
    ),
    ("/wait [minutes]", "Wait in place (default: 15 min)"),
];

/// Renders the `/help` body as a monospace-aligned two-column list.
///
/// Command names are left-padded to the widest entry so the em-dash
/// separator lines up in a fixed-width font. Frontends tag this response
/// with [`TextPresentation::Tabular`](super::TextPresentation) so the chat
/// UI picks a monospace font.
pub(super) fn render_help_text() -> String {
    let max_cmd_width = HELP_ENTRIES
        .iter()
        .map(|(cmd, _)| cmd.chars().count())
        .max()
        .unwrap_or(0);

    let mut out = String::from("Available commands:");
    for (cmd, desc) in HELP_ENTRIES {
        out.push('\n');
        out.push_str(&format!("  {cmd:<max_cmd_width$} — {desc}"));
    }
    out
}
