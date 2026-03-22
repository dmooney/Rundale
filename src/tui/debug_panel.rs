//! Debug sidebar panel for the TUI.
//!
//! Renders a live view of NPC tiers, locations, moods, and clock state
//! in a sidebar panel. Toggled via `/debug panel` or F12.

use chrono::Timelike;
use ratatui::style::Style;
use ratatui::text::Line;

use crate::npc::types::{CogTier, NpcState};
use crate::tui::App;
use crate::world::LocationId;
use crate::world::graph::WorldGraph;

/// Builds the lines for the debug sidebar panel.
///
/// Groups NPCs by cognitive tier and shows their mood, location,
/// and transit state. Also shows the game clock.
pub fn build_debug_lines<'a>(app: &App, accent_style: Style, base_style: Style) -> Vec<Line<'a>> {
    let mut lines: Vec<Line> = Vec::new();

    // Clock line
    let now = app.world.clock.now();
    let tod = app.world.clock.time_of_day();
    let season = app.world.clock.season();
    let paused = if app.world.clock.is_paused() {
        " PAUSED"
    } else {
        ""
    };
    lines.push(
        Line::from(format!(
            "{:02}:{:02} {} {}{}",
            now.hour(),
            now.minute(),
            tod,
            season,
            paused
        ))
        .style(accent_style),
    );

    // Weather
    lines.push(Line::from(app.world.weather.to_string()).style(base_style));
    lines.push(Line::from(""));

    // NPCs grouped by tier
    let mut tier1_npcs = Vec::new();
    let mut tier2_npcs = Vec::new();
    let mut tier3_npcs = Vec::new();

    for npc in app.npc_manager.all_npcs() {
        let tier = app.npc_manager.tier_of(npc.id).unwrap_or(CogTier::Tier3);
        match tier {
            CogTier::Tier1 => tier1_npcs.push(npc),
            CogTier::Tier2 => tier2_npcs.push(npc),
            CogTier::Tier3 | CogTier::Tier4 => tier3_npcs.push(npc),
        }
    }

    // Tier 1: HERE
    lines.push(Line::from("HERE:").style(accent_style));
    if tier1_npcs.is_empty() {
        lines.push(Line::from(" (nobody)").style(base_style));
    } else {
        for npc in &tier1_npcs {
            lines.push(Line::from(format!(" {} [{}]", npc.name, npc.mood)).style(base_style));
        }
    }
    lines.push(Line::from(""));

    // Tier 2: NEARBY
    lines.push(Line::from("NEARBY:").style(accent_style));
    if tier2_npcs.is_empty() {
        lines.push(Line::from(" (nobody)").style(base_style));
    } else {
        for npc in &tier2_npcs {
            let state_str = match &npc.state {
                NpcState::Present => {
                    let loc = loc_name(npc.location, &app.world.graph);
                    format!(" {} [{}] @{}", npc.name, npc.mood, loc)
                }
                NpcState::InTransit { to, arrives_at, .. } => {
                    let dest = loc_name(*to, &app.world.graph);
                    format!(
                        " {} ->{}({}:{:02})",
                        npc.name,
                        dest,
                        arrives_at.hour(),
                        arrives_at.minute()
                    )
                }
            };
            lines.push(Line::from(state_str).style(base_style));
        }
    }
    lines.push(Line::from(""));

    // Tier 3: FAR
    lines.push(Line::from("FAR:").style(accent_style));
    if tier3_npcs.is_empty() {
        lines.push(Line::from(" (nobody)").style(base_style));
    } else {
        // Compact: just names on one or two lines
        let names: Vec<&str> = tier3_npcs.iter().map(|n| n.name.as_str()).collect();
        for chunk in names.chunks(2) {
            lines.push(Line::from(format!(" {}", chunk.join(", "))).style(base_style));
        }
    }

    // Activity log
    if !app.debug_log.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("LOG:").style(accent_style));
        // Show most recent entries (last N that fit)
        for entry in app.debug_log.iter().rev().take(15) {
            lines.push(Line::from(format!(" {}", entry)).style(base_style));
        }
    }

    lines
}

/// Short location name from the world graph.
fn loc_name(id: LocationId, graph: &WorldGraph) -> String {
    graph
        .get(id)
        .map(|d| {
            // Shorten common prefixes for sidebar space
            d.name.strip_prefix("The ").unwrap_or(&d.name).to_string()
        })
        .unwrap_or_else(|| format!("#{}", id.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_build_debug_lines_empty_app() {
        let app = App::new();
        let accent = Style::default().fg(Color::Yellow);
        let base = Style::default().fg(Color::White);
        let lines = build_debug_lines(&app, accent, base);
        // Should at least have clock, weather, and tier headers
        assert!(lines.len() >= 6);
    }

    #[test]
    fn test_loc_name_strips_the() {
        // Can't easily test with a real graph, but test the fallback
        let graph = crate::world::graph::WorldGraph::new();
        assert_eq!(loc_name(LocationId(99), &graph), "#99");
    }
}
