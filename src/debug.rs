//! Debug interface — `/debug` command handlers.
//!
//! Pure query functions that inspect game state and return formatted
//! lines for display. No mutation, works across headless and script modes.

use chrono::Timelike;

use crate::app::App;
use crate::npc::NpcId;
use crate::npc::manager::NpcManager;
use crate::npc::types::{CogTier, NpcState};
use crate::world::LocationId;
use crate::world::graph::WorldGraph;

/// Handles a `/debug` command and returns lines to display.
///
/// The `sub` argument is the text after `/debug `, or `None` for bare `/debug`.
pub fn handle_debug(sub: Option<&str>, app: &App) -> Vec<String> {
    match sub {
        None => debug_overview(app),
        Some(s) => {
            let parts: Vec<&str> = s.splitn(2, ' ').collect();
            let cmd = parts[0].to_lowercase();
            let arg = parts.get(1).map(|a| a.trim());

            match cmd.as_str() {
                "npcs" => debug_npcs(app),
                "tiers" => debug_tiers(app),
                "clock" => debug_clock(app),
                "here" => debug_here(app),
                "schedule" => debug_schedule(app, arg),
                "memory" => debug_memory(app, arg),
                "relationships" | "rels" => debug_relationships(app, arg),
                "help" => debug_help(),
                _ => vec![format!("Unknown debug command: {}. Try /debug help", cmd)],
            }
        }
    }
}

/// Compact overview: clock + tier counts + NPCs at current location.
fn debug_overview(app: &App) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("[DEBUG OVERVIEW]".to_string());

    // Clock
    let now = app.world.clock.now();
    let tod = app.world.clock.time_of_day();
    let season = app.world.clock.season();
    let paused = if app.world.clock.is_paused() {
        " (PAUSED)"
    } else {
        ""
    };
    lines.push(format!(
        "  Clock: {:02}:{:02} {} {} {}{}",
        now.hour(),
        now.minute(),
        now.format("%Y-%m-%d"),
        tod,
        season,
        paused
    ));

    // Tier counts
    let (t1, t2, t3) = tier_counts(&app.npc_manager);
    lines.push(format!(
        "  NPCs: {} total | Tier1: {} | Tier2: {} | Tier3+: {}",
        app.npc_manager.npc_count(),
        t1,
        t2,
        t3
    ));

    // NPCs here
    let here = app.npc_manager.npcs_at(app.world.player_location);
    if here.is_empty() {
        lines.push("  Here: (nobody)".to_string());
    } else {
        let names: Vec<String> = here
            .iter()
            .map(|n| format!("{} [{}]", n.name, n.mood))
            .collect();
        lines.push(format!("  Here: {}", names.join(", ")));
    }

    lines
}

/// All NPCs with location, tier, mood, state.
fn debug_npcs(app: &App) -> Vec<String> {
    let mut lines = vec!["[DEBUG NPCS]".to_string()];

    let mut npcs: Vec<_> = app.npc_manager.all_npcs().collect();
    npcs.sort_by_key(|n| n.id.0);

    for npc in npcs {
        let tier = app
            .npc_manager
            .tier_of(npc.id)
            .map(|t| format!("{:?}", t))
            .unwrap_or_else(|| "?".to_string());
        let loc_name = location_name(npc.location, &app.world.graph);
        let state = match &npc.state {
            NpcState::Present => "Present".to_string(),
            NpcState::InTransit { to, arrives_at, .. } => {
                let dest = location_name(*to, &app.world.graph);
                format!(
                    "-> {} ({}:{:02})",
                    dest,
                    arrives_at.hour(),
                    arrives_at.minute()
                )
            }
        };

        lines.push(format!("  {} ({}y, {})", npc.name, npc.age, npc.occupation));
        lines.push(format!(
            "    Loc: {} | {} | Mood: {} | {}",
            loc_name, tier, npc.mood, state
        ));
    }

    lines
}

/// Tier assignment summary with counts and names.
fn debug_tiers(app: &App) -> Vec<String> {
    let mut lines = vec!["[DEBUG TIERS]".to_string()];

    let player_loc = location_name(app.world.player_location, &app.world.graph);
    lines.push(format!("  Player at: {}", player_loc));

    for (tier_label, tier_val) in [
        ("Tier 1 (here)", CogTier::Tier1),
        ("Tier 2 (nearby)", CogTier::Tier2),
        ("Tier 3 (far)", CogTier::Tier3),
    ] {
        let ids: Vec<NpcId> = app
            .npc_manager
            .all_npcs()
            .filter(|n| app.npc_manager.tier_of(n.id) == Some(tier_val))
            .map(|n| n.id)
            .collect();

        if ids.is_empty() {
            lines.push(format!("  {}: (none)", tier_label));
        } else {
            let names: Vec<String> = ids
                .iter()
                .filter_map(|id| app.npc_manager.get(*id))
                .map(|n| n.name.clone())
                .collect();
            lines.push(format!("  {}: {}", tier_label, names.join(", ")));
        }
    }

    lines
}

/// Game clock details.
fn debug_clock(app: &App) -> Vec<String> {
    let now = app.world.clock.now();
    let tod = app.world.clock.time_of_day();
    let season = app.world.clock.season();
    let festival = app
        .world
        .clock
        .check_festival()
        .map(|f| format!("{}", f))
        .unwrap_or_else(|| "(none)".to_string());
    let paused = if app.world.clock.is_paused() {
        "yes"
    } else {
        "no"
    };

    vec![
        "[DEBUG CLOCK]".to_string(),
        format!(
            "  Game time: {:02}:{:02} {}",
            now.hour(),
            now.minute(),
            now.format("%Y-%m-%d")
        ),
        format!("  Time of day: {} | Season: {}", tod, season),
        format!("  Festival: {} | Paused: {}", festival, paused),
        format!("  Weather: {}", app.world.weather),
    ]
}

/// Current location details: NPCs, connections, properties.
fn debug_here(app: &App) -> Vec<String> {
    let mut lines = vec!["[DEBUG HERE]".to_string()];
    let loc = app.world.current_location();
    lines.push(format!(
        "  {} (id: {})",
        loc.name, app.world.player_location.0
    ));
    lines.push(format!("  Indoor: {} | Public: {}", loc.indoor, loc.public));

    // NPCs present
    let here = app.npc_manager.npcs_at(app.world.player_location);
    if here.is_empty() {
        lines.push("  NPCs: (none)".to_string());
    } else {
        lines.push("  NPCs:".to_string());
        for npc in &here {
            let tier = app
                .npc_manager
                .tier_of(npc.id)
                .map(|t| format!("{:?}", t))
                .unwrap_or_default();
            lines.push(format!("    {} [{}] ({})", npc.name, npc.mood, tier));
        }
    }

    // Connections
    if let Some(loc_data) = app.world.current_location_data() {
        lines.push("  Exits:".to_string());
        for conn in &loc_data.connections {
            let dest = location_name(conn.target, &app.world.graph);
            lines.push(format!("    -> {} ({}min)", dest, conn.traversal_minutes));
        }
    }

    lines
}

/// NPC's daily schedule.
fn debug_schedule(app: &App, name: Option<&str>) -> Vec<String> {
    let Some(name) = name else {
        return vec!["Usage: /debug schedule <npc name>".to_string()];
    };

    let Some(npc) = find_npc_by_name(&app.npc_manager, name) else {
        return vec![format!("NPC not found: {}", name)];
    };

    let mut lines = vec![format!("[DEBUG SCHEDULE: {}]", npc.name)];

    match &npc.schedule {
        Some(schedule) => {
            for entry in &schedule.entries {
                let loc = location_name(entry.location, &app.world.graph);
                lines.push(format!(
                    "  {:02}:00-{:02}:00  {}  ({})",
                    entry.start_hour, entry.end_hour, loc, entry.activity
                ));
            }
        }
        None => lines.push("  (no schedule)".to_string()),
    }

    lines
}

/// NPC's short-term memory (recent 10 entries).
fn debug_memory(app: &App, name: Option<&str>) -> Vec<String> {
    let Some(name) = name else {
        return vec!["Usage: /debug memory <npc name>".to_string()];
    };

    let Some(npc) = find_npc_by_name(&app.npc_manager, name) else {
        return vec![format!("NPC not found: {}", name)];
    };

    let mut lines = vec![format!("[DEBUG MEMORY: {}]", npc.name)];

    let recent = npc.memory.recent(10);
    if recent.is_empty() {
        lines.push("  (no memories)".to_string());
    } else {
        for entry in recent {
            let time = entry.timestamp.format("%H:%M");
            let loc = location_name(entry.location, &app.world.graph);
            lines.push(format!("  [{}] {} (at {})", time, entry.content, loc));
        }
    }

    lines
}

/// NPC's relationships.
fn debug_relationships(app: &App, name: Option<&str>) -> Vec<String> {
    let Some(name) = name else {
        return vec!["Usage: /debug relationships <npc name>".to_string()];
    };

    let Some(npc) = find_npc_by_name(&app.npc_manager, name) else {
        return vec![format!("NPC not found: {}", name)];
    };

    let mut lines = vec![format!("[DEBUG RELATIONSHIPS: {}]", npc.name)];

    if npc.relationships.is_empty() {
        lines.push("  (no relationships)".to_string());
    } else {
        let mut rels: Vec<_> = npc.relationships.iter().collect();
        rels.sort_by(|a, b| b.1.strength.partial_cmp(&a.1.strength).unwrap());

        for (target_id, rel) in rels {
            let target_name = app
                .npc_manager
                .get(*target_id)
                .map(|n| n.name.as_str())
                .unwrap_or("?");
            let bar = strength_bar(rel.strength);
            lines.push(format!(
                "  {} {} ({}, {:.1})",
                bar, target_name, rel.kind, rel.strength
            ));
        }
    }

    lines
}

/// Help for /debug subcommands.
fn debug_help() -> Vec<String> {
    vec![
        "[DEBUG COMMANDS]".to_string(),
        "  /debug          — Overview (clock, tiers, NPCs here)".to_string(),
        "  /debug npcs     — All NPCs with location, tier, mood".to_string(),
        "  /debug tiers    — Tier assignment summary".to_string(),
        "  /debug clock    — Game time details".to_string(),
        "  /debug here     — Current location details".to_string(),
        "  /debug schedule <name>  — NPC's daily schedule".to_string(),
        "  /debug memory <name>    — NPC's recent memories".to_string(),
        "  /debug rels <name>      — NPC's relationships".to_string(),
    ]
}

/// Counts NPCs by tier.
fn tier_counts(mgr: &NpcManager) -> (usize, usize, usize) {
    let mut t1 = 0;
    let mut t2 = 0;
    let mut t3 = 0;
    for npc in mgr.all_npcs() {
        match mgr.tier_of(npc.id) {
            Some(CogTier::Tier1) => t1 += 1,
            Some(CogTier::Tier2) => t2 += 1,
            _ => t3 += 1,
        }
    }
    (t1, t2, t3)
}

/// Looks up a location name from the world graph.
fn location_name(id: LocationId, graph: &WorldGraph) -> String {
    graph
        .get(id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| format!("Location({})", id.0))
}

/// Finds an NPC by fuzzy name match (case-insensitive substring).
fn find_npc_by_name<'a>(mgr: &'a NpcManager, name: &str) -> Option<&'a crate::npc::Npc> {
    let lower = name.to_lowercase();
    mgr.all_npcs()
        .find(|n| n.name.to_lowercase().contains(&lower))
}

/// Renders a visual strength bar: ████░░░░░░ for -1.0 to 1.0.
fn strength_bar(strength: f64) -> String {
    let normalized = ((strength + 1.0) / 2.0 * 10.0) as usize;
    let filled = normalized.min(10);
    let empty = 10 - filled;
    format!("[{}{}]", "#".repeat(filled), ".".repeat(empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strength_bar() {
        assert_eq!(strength_bar(1.0), "[##########]");
        assert_eq!(strength_bar(-1.0), "[..........]");
        assert_eq!(strength_bar(0.0), "[#####.....]");
    }

    #[test]
    fn test_debug_overview() {
        let app = App::new();
        let lines = debug_overview(&app);
        assert!(lines[0].contains("DEBUG OVERVIEW"));
        assert!(lines[1].contains("Clock:"));
    }

    #[test]
    fn test_debug_clock() {
        let app = App::new();
        let lines = debug_clock(&app);
        assert!(lines[0].contains("DEBUG CLOCK"));
        assert!(lines.iter().any(|l| l.contains("Game time:")));
        assert!(lines.iter().any(|l| l.contains("Season:")));
    }

    #[test]
    fn test_debug_help() {
        let lines = debug_help();
        assert!(lines.len() >= 8);
        assert!(lines[0].contains("DEBUG COMMANDS"));
    }

    #[test]
    fn test_debug_npcs_empty() {
        let app = App::new();
        let lines = debug_npcs(&app);
        assert!(lines[0].contains("DEBUG NPCS"));
        // No NPCs in default App
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_debug_schedule_no_name() {
        let app = App::new();
        let lines = debug_schedule(&app, None);
        assert!(lines[0].contains("Usage:"));
    }

    #[test]
    fn test_debug_memory_not_found() {
        let app = App::new();
        let lines = debug_memory(&app, Some("nobody"));
        assert!(lines[0].contains("NPC not found"));
    }

    #[test]
    fn test_handle_debug_unknown_command() {
        let app = App::new();
        let lines = handle_debug(Some("bogus"), &app);
        assert!(lines[0].contains("Unknown debug command"));
    }

    #[test]
    fn test_handle_debug_none() {
        let app = App::new();
        let lines = handle_debug(None, &app);
        assert!(lines[0].contains("DEBUG OVERVIEW"));
    }

    #[test]
    fn test_find_npc_by_name() {
        use crate::npc::Npc;
        let mut mgr = NpcManager::new();
        mgr.add_npc(Npc::new_test_npc());

        assert!(find_npc_by_name(&mgr, "padraig").is_some());
        assert!(find_npc_by_name(&mgr, "PADRAIG").is_some());
        assert!(find_npc_by_name(&mgr, "nobody").is_none());
    }

    #[test]
    fn test_location_name_unknown() {
        let graph = crate::world::graph::WorldGraph::new();
        assert_eq!(location_name(LocationId(999), &graph), "Location(999)");
    }
}
