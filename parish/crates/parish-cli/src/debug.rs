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
                "gossip" => debug_gossip(app, arg),
                "language" | "lang" => debug_language(app),
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
            .map(|n| {
                format!(
                    "{} {} [{}]",
                    n.name,
                    crate::npc::mood::mood_emoji(&n.mood),
                    n.mood
                )
            })
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
            "    Loc: {} | {} | Mood: {} {} | {}",
            loc_name,
            tier,
            crate::npc::mood::mood_emoji(&npc.mood),
            npc.mood,
            state
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
            lines.push(format!(
                "    {} {} [{}] ({})",
                npc.name,
                crate::npc::mood::mood_emoji(&npc.mood),
                npc.mood,
                tier
            ));
        }
    }

    // Connections
    if let Some(loc_data) = app.world.current_location_data() {
        lines.push("  Exits:".to_string());
        for conn in &loc_data.connections {
            let dest = location_name(conn.target, &app.world.graph);
            let minutes =
                app.world
                    .graph
                    .edge_travel_minutes(app.world.player_location, conn.target, 1.25);
            lines.push(format!("    -> {} ({}min)", dest, minutes));
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
            let season = app.world.clock.season();
            let day_type = app.world.clock.day_type();
            if let Some(entries) = schedule.resolve(season, day_type) {
                lines.push(format!("  (resolved for {}, {})", season, day_type));
                for entry in entries {
                    let loc = location_name(entry.location, &app.world.graph);
                    lines.push(format!(
                        "  {:02}:00-{:02}:00  {}  ({})",
                        entry.start_hour, entry.end_hour, loc, entry.activity
                    ));
                }
            } else {
                lines.push("  (no matching schedule variant)".to_string());
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

    // Short-term memory
    lines.push(format!("  Short-term ({}/{}):", npc.memory.len(), 20));
    let recent = npc.memory.recent(10);
    if recent.is_empty() {
        lines.push("    (no short-term memories)".to_string());
    } else {
        for entry in recent {
            let time = entry.timestamp.format("%H:%M");
            let loc = location_name(entry.location, &app.world.graph);
            lines.push(format!("    [{}] {} (at {})", time, entry.content, loc));
        }
    }

    // Long-term memory
    lines.push(format!(
        "  Long-term ({} entries):",
        npc.long_term_memory.len()
    ));
    if npc.long_term_memory.is_empty() {
        lines.push("    (no long-term memories)".to_string());
    } else {
        let all = npc.long_term_memory.recall(&[""], 10);
        // Show all if keyword recall returns nothing (empty query)
        if all.is_empty() {
            lines.push(format!(
                "    {} stored (use keyword search to recall)",
                npc.long_term_memory.len()
            ));
        } else {
            for entry in all {
                lines.push(format!(
                    "    [imp={:.1}] {} (keywords: {})",
                    entry.importance,
                    entry.content,
                    entry.keywords.join(", ")
                ));
            }
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
        "  /debug gossip [name]    — Gossip network (or NPC's known gossip)".to_string(),
        "  /debug language         — Active language settings from the loaded mod".to_string(),
    ]
}

/// Active language settings derived from the loaded mod's `[setting]` block.
fn debug_language(app: &App) -> Vec<String> {
    let lang = app.language_settings();
    let directive = crate::npc::language_directive(&lang);
    let mut lines = vec![
        "[DEBUG LANGUAGE]".to_string(),
        format!("  player_language: {}", lang.player),
        format!(
            "  native_language: {}",
            lang.native.as_deref().unwrap_or("(none)")
        ),
        "".to_string(),
        "  Rendered LANGUAGE directive injected into every dialogue prompt:".to_string(),
    ];
    for line in directive.lines() {
        lines.push(format!("    {line}"));
    }
    lines
}

/// Gossip network overview, or a specific NPC's known gossip.
fn debug_gossip(app: &App, name: Option<&str>) -> Vec<String> {
    let network = &app.world.gossip_network;

    if let Some(name) = name {
        // Show gossip known by a specific NPC
        let Some(npc) = find_npc_by_name(&app.npc_manager, name) else {
            return vec![format!("NPC not found: {}", name)];
        };

        let items = network.known_by(npc.id);
        let mut lines = vec![format!(
            "[DEBUG GOSSIP: {} ({} items)]",
            npc.name,
            items.len()
        )];
        if items.is_empty() {
            lines.push("  (no gossip known)".to_string());
        } else {
            for item in &items {
                lines.push(format!(
                    "  [id={}] \"{}\" (from NPC#{}, distortion={})",
                    item.id, item.content, item.source.0, item.distortion_level
                ));
            }
        }
        lines
    } else {
        // Show network overview
        let mut lines = vec![format!("[DEBUG GOSSIP NETWORK: {} items]", network.len())];
        if network.is_empty() {
            lines.push("  (no gossip circulating)".to_string());
        } else {
            let all_items = network.all_items();
            for item in all_items.iter().take(15) {
                lines.push(format!(
                    "  [id={}] \"{}\" (source=NPC#{}, known_by={}, distortion={})",
                    item.id,
                    item.content,
                    item.source.0,
                    item.known_by.len(),
                    item.distortion_level
                ));
            }
            if all_items.len() > 15 {
                lines.push(format!("  ... and {} more", all_items.len() - 15));
            }
        }
        lines
    }
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

/// Renders a visual strength bar: `[##########]` for 1.0 to `[..........]` for -1.0.
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
    fn test_debug_tiers_empty() {
        let app = App::new();
        let lines = debug_tiers(&app);
        assert!(lines[0].contains("DEBUG TIERS"));
        assert!(lines[1].contains("Player at:"));
        // All tiers should show (none)
        assert!(lines[2..].iter().all(|l| l.contains("(none)")));
    }

    #[test]
    fn test_debug_here() {
        let app = App::new();
        let lines = debug_here(&app);
        assert!(lines[0].contains("DEBUG HERE"));
        // Should show indoor/public info
        assert!(lines.iter().any(|l| l.contains("Indoor:")));
        // Should show exits
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Exits:") || l.contains("NPCs:"))
        );
    }

    #[test]
    fn test_debug_relationships_no_name() {
        let app = App::new();
        let lines = debug_relationships(&app, None);
        assert!(lines[0].contains("Usage:"));
    }

    #[test]
    fn test_debug_relationships_not_found() {
        let app = App::new();
        let lines = debug_relationships(&app, Some("nobody"));
        assert!(lines[0].contains("NPC not found"));
    }

    #[test]
    fn test_debug_memory_no_name() {
        let app = App::new();
        let lines = debug_memory(&app, None);
        assert!(lines[0].contains("Usage:"));
    }

    #[test]
    fn test_debug_schedule_not_found() {
        let app = App::new();
        let lines = debug_schedule(&app, Some("nobody"));
        assert!(lines[0].contains("NPC not found"));
    }

    #[test]
    fn test_handle_debug_all_subcommands() {
        let app = App::new();
        // Each valid subcommand should return without panicking
        for sub in &["npcs", "tiers", "clock", "here", "help"] {
            let lines = handle_debug(Some(sub), &app);
            assert!(
                !lines.is_empty(),
                "Debug subcommand '{}' returned empty",
                sub
            );
        }
    }

    #[test]
    fn test_handle_debug_rels_alias() {
        let app = App::new();
        let lines = handle_debug(Some("rels"), &app);
        assert!(lines[0].contains("Usage:"));
    }

    #[test]
    fn test_strength_bar_midpoints() {
        assert_eq!(strength_bar(0.5), "[#######...]");
        assert_eq!(strength_bar(-0.5), "[##........]");
    }

    #[test]
    fn test_tier_counts_empty() {
        let mgr = NpcManager::new();
        assert_eq!(tier_counts(&mgr), (0, 0, 0));
    }

    #[test]
    fn test_location_name_unknown() {
        let graph = crate::world::graph::WorldGraph::new();
        assert_eq!(location_name(LocationId(999), &graph), "Location(999)");
    }
}
