//! Debug interface — `/debug` command queries.
//!
//! Pure query functions that inspect game state and return formatted
//! lines for display. No mutation; shared by the headless engine and the
//! phone, which run `/debug` against the same world and NPC state.

use chrono::Timelike;

use crate::npc::LanguageSettings;
use crate::npc::NpcId;
use crate::npc::manager::NpcManager;
use crate::npc::types::{CogTier, NpcState};
use crate::world::LocationId;
use crate::world::graph::WorldGraph;

/// The state `/debug` reads.
pub struct DebugView<'a> {
    /// The world: clock, weather, graph, and the player's location.
    pub world: &'a crate::world::WorldState,
    /// Every NPC, with tiers, schedules, memory, and relationships.
    pub npc_manager: &'a NpcManager,
    /// The mod's language settings, for `/debug language`.
    pub language: &'a LanguageSettings,
}

/// A `/debug` view: what `/debug help` lists and a host offers for
/// completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugSubcommand {
    /// The word after `/debug`.
    pub name: &'static str,
    /// What the view shows.
    pub summary: &'static str,
    /// Whether an NPC's name follows (matched without case or diacritics).
    pub npc: NpcArgument,
}

/// Whether a `/debug` view takes an NPC's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpcArgument {
    /// No name.
    None,
    /// A name is needed.
    Required,
    /// A name narrows the view to that NPC.
    Optional,
}

impl DebugSubcommand {
    /// Whether a name may follow this view.
    pub fn takes_npc(&self) -> bool {
        self.npc != NpcArgument::None
    }
}

/// The views [`handle_debug`] answers, in `/debug help` order. Aliases
/// (`rels`, `lang`) are accepted but not listed.
pub const SUBCOMMANDS: &[DebugSubcommand] = &[
    DebugSubcommand {
        name: "memory",
        summary: "An NPC's recent memories",
        npc: NpcArgument::Required,
    },
    DebugSubcommand {
        name: "schedule",
        summary: "An NPC's daily schedule",
        npc: NpcArgument::Required,
    },
    DebugSubcommand {
        name: "relationships",
        summary: "An NPC's relationships",
        npc: NpcArgument::Required,
    },
    DebugSubcommand {
        name: "gossip",
        summary: "Gossip network, or an NPC's gossip",
        npc: NpcArgument::Optional,
    },
    DebugSubcommand {
        name: "clock",
        summary: "Game time details",
        npc: NpcArgument::None,
    },
    DebugSubcommand {
        name: "here",
        summary: "Current location details",
        npc: NpcArgument::None,
    },
    DebugSubcommand {
        name: "npcs",
        summary: "All NPCs with location, tier, mood",
        npc: NpcArgument::None,
    },
    DebugSubcommand {
        name: "tiers",
        summary: "Tier assignment summary",
        npc: NpcArgument::None,
    },
    DebugSubcommand {
        name: "language",
        summary: "Language settings from the loaded mod",
        npc: NpcArgument::None,
    },
    DebugSubcommand {
        name: "reactions",
        summary: "NPC reaction buffer and monoculture sensor",
        npc: NpcArgument::None,
    },
    DebugSubcommand {
        name: "help",
        summary: "These views",
        npc: NpcArgument::None,
    },
];

/// Handles a `/debug` command and returns lines to display.
///
/// The `sub` argument is the text after `/debug `, or `None` for bare `/debug`.
pub fn handle_debug(sub: Option<&str>, app: &DebugView<'_>) -> Vec<String> {
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
                "reactions" => debug_reactions(app),
                "help" => debug_help(),
                _ => vec![format!("Unknown debug command: {}. Try /debug help", cmd)],
            }
        }
    }
}

/// Compact overview: clock + tier counts + NPCs at current location.
fn debug_overview(app: &DebugView<'_>) -> Vec<String> {
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
    let (t1, t2, t3) = tier_counts(app.npc_manager);
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
fn debug_npcs(app: &DebugView<'_>) -> Vec<String> {
    let mut lines = vec!["[DEBUG NPCS]".to_string()];

    let mut npcs: Vec<_> = app.npc_manager.all_npcs().collect();
    npcs.sort_by_key(|n| n.id.0);

    for npc in npcs {
        let tier = app
            .npc_manager
            .tier_of(npc.id)
            .map(|t| format!("{:?}", t))
            .unwrap_or_else(|| "?".to_string());
        let loc_name = location_name(npc.location(), &app.world.graph);
        let state = match npc.state() {
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
fn debug_tiers(app: &DebugView<'_>) -> Vec<String> {
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

    // Tier 2 dispatch groups: only locations with >=2 co-located Tier 2
    // NPCs receive a Tier 2 tick (#1025). Solo Tier 2 NPCs are gated out.
    let groups = app.npc_manager.tier2_groups();
    if groups.is_empty() {
        lines.push("  Tier 2 dispatch groups (>=2 co-located): (none)".to_string());
    } else {
        lines.push("  Tier 2 dispatch groups (>=2 co-located):".to_string());
        let mut entries: Vec<(String, usize)> = groups
            .iter()
            .map(|(loc, ids)| (location_name(*loc, &app.world.graph), ids.len()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, count) in entries {
            lines.push(format!("    {}: {} NPCs", name, count));
        }
    }

    lines
}

/// Game clock details.
fn debug_clock(app: &DebugView<'_>) -> Vec<String> {
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
fn debug_here(app: &DebugView<'_>) -> Vec<String> {
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
fn debug_schedule(app: &DebugView<'_>, name: Option<&str>) -> Vec<String> {
    let Some(name) = name else {
        return vec!["Usage: /debug schedule <npc name>".to_string()];
    };

    let Some(npc) = find_npc_by_name(app.npc_manager, name) else {
        return vec![format!("NPC not found: {}", name)];
    };

    let mut lines = vec![format!("[DEBUG SCHEDULE: {}]", npc.name)];

    match npc.schedule() {
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
fn debug_memory(app: &DebugView<'_>, name: Option<&str>) -> Vec<String> {
    let Some(name) = name else {
        return vec!["Usage: /debug memory <npc name>".to_string()];
    };

    let Some(npc) = find_npc_by_name(app.npc_manager, name) else {
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
fn debug_relationships(app: &DebugView<'_>, name: Option<&str>) -> Vec<String> {
    let Some(name) = name else {
        return vec!["Usage: /debug relationships <npc name>".to_string()];
    };

    let Some(npc) = find_npc_by_name(app.npc_manager, name) else {
        return vec![format!("NPC not found: {}", name)];
    };

    let mut lines = vec![format!("[DEBUG RELATIONSHIPS: {}]", npc.name)];

    if npc.relationships.is_empty() {
        lines.push("  (no relationships)".to_string());
    } else {
        let mut rels: Vec<_> = npc.relationships.iter().collect();
        rels.sort_by(|a, b| b.1.strength.total_cmp(&a.1.strength).reverse());

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
    let mut lines = vec![
        "[DEBUG COMMANDS]".to_string(),
        "  /debug — Overview (clock, tiers, NPCs here)".to_string(),
    ];
    lines.extend(
        SUBCOMMANDS
            .iter()
            .filter(|sub| sub.name != "help")
            .map(|sub| {
                let name = match sub.npc {
                    NpcArgument::None => "",
                    NpcArgument::Required => " <name>",
                    NpcArgument::Optional => " [name]",
                };
                format!("  /debug {}{name} — {}", sub.name, sub.summary)
            }),
    );
    lines
}

/// Per-session NPC reaction emoji ring buffer + monoculture sensor state.
///
/// Surfaces the buffer that feeds [`limerick_npc::quality::detect_emoji_monoculture`]
/// so a play-test can observe the sensor without reading log files (issue #995).
fn debug_reactions(app: &DebugView<'_>) -> Vec<String> {
    let buffer = app.npc_manager.reaction_emoji_buffer();
    let refs: Vec<&str> = buffer.iter().map(String::as_str).collect();
    let detection = crate::npc::quality::detect_emoji_monoculture(&refs);

    let mut lines = vec!["[DEBUG REACTIONS]".to_string()];
    lines.push(format!(
        "  Buffer ({} / {}): {}",
        buffer.len(),
        crate::npc::manager::REACTION_EMOJI_BUFFER_CAPACITY,
        if buffer.is_empty() {
            "(empty)".to_string()
        } else {
            buffer.join(" ")
        }
    ));
    match detection {
        Some(issue) => lines.push(format!("  Monoculture: ACTIVE — {}", issue.detail)),
        None => lines.push("  Monoculture: clear".to_string()),
    }
    lines
}

/// Active language settings derived from the loaded mod's `[setting]` block.
fn debug_language(app: &DebugView<'_>) -> Vec<String> {
    let lang = app.language;
    let directive = crate::npc::language_directive(lang);
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
fn debug_gossip(app: &DebugView<'_>, name: Option<&str>) -> Vec<String> {
    let network = &app.world.gossip_network;

    if let Some(name) = name {
        // Show gossip known by a specific NPC
        let Some(npc) = find_npc_by_name(app.npc_manager, name) else {
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

/// Finds an NPC by fuzzy name match: a substring, ignoring case and
/// diacritics, so `micheal` finds Mícheál Connolly.
fn find_npc_by_name<'a>(mgr: &'a NpcManager, name: &str) -> Option<&'a crate::npc::Npc> {
    let wanted = fold(name);
    mgr.all_npcs().find(|n| fold(&n.name).contains(&wanted))
}

/// `text` lowercased with the accents taken off Latin letters (the Irish
/// fada and its neighbours), for matching typed names.
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'ē' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ō' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'ū' => 'u',
            'ý' | 'ÿ' => 'y',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
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
    use crate::world::WorldState;

    /// The state of a new default session, as the engine's `App::new()`.
    struct Session {
        world: WorldState,
        npc_manager: NpcManager,
        language: LanguageSettings,
    }

    impl Session {
        fn new() -> Self {
            Self {
                world: WorldState::new(),
                npc_manager: NpcManager::new(),
                language: LanguageSettings::english_only(),
            }
        }

        fn view(&self) -> DebugView<'_> {
            DebugView {
                world: &self.world,
                npc_manager: &self.npc_manager,
                language: &self.language,
            }
        }
    }

    #[test]
    fn test_strength_bar() {
        assert_eq!(strength_bar(1.0), "[##########]");
        assert_eq!(strength_bar(-1.0), "[..........]");
        assert_eq!(strength_bar(0.0), "[#####.....]");
    }

    #[test]
    fn test_debug_overview() {
        let session = Session::new();
        let app = session.view();
        let lines = debug_overview(&app);
        assert!(lines[0].contains("DEBUG OVERVIEW"));
        assert!(lines[1].contains("Clock:"));
    }

    #[test]
    fn test_debug_clock() {
        let session = Session::new();
        let app = session.view();
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
        let session = Session::new();
        let app = session.view();
        let lines = debug_npcs(&app);
        assert!(lines[0].contains("DEBUG NPCS"));
        // No NPCs in default App
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_debug_schedule_no_name() {
        let session = Session::new();
        let app = session.view();
        let lines = debug_schedule(&app, None);
        assert!(lines[0].contains("Usage:"));
    }

    #[test]
    fn test_debug_memory_not_found() {
        let session = Session::new();
        let app = session.view();
        let lines = debug_memory(&app, Some("nobody"));
        assert!(lines[0].contains("NPC not found"));
    }

    #[test]
    fn test_handle_debug_unknown_command() {
        let session = Session::new();
        let app = session.view();
        let lines = handle_debug(Some("bogus"), &app);
        assert!(lines[0].contains("Unknown debug command"));
    }

    #[test]
    fn test_handle_debug_none() {
        let session = Session::new();
        let app = session.view();
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
    fn find_npc_by_name_ignores_case_and_fadas() {
        use crate::npc::Npc;
        let mut mgr = NpcManager::new();
        for (id, name) in [(1, "Mícheál Connolly"), (2, "Róisín Connolly")] {
            let mut npc = Npc::new_test_npc();
            npc.id = NpcId(id);
            npc.name = name.to_string();
            mgr.add_npc(npc);
        }
        let found = |typed| find_npc_by_name(&mgr, typed).map(|npc| npc.name.as_str());
        assert_eq!(found("micheal"), Some("Mícheál Connolly"));
        assert_eq!(found("MICHEAL CONNOLLY"), Some("Mícheál Connolly"));
        assert_eq!(found("roisin"), Some("Róisín Connolly"));
        assert_eq!(found("Róisín"), Some("Róisín Connolly"));
        assert_eq!(found("roisin connolly"), Some("Róisín Connolly"));
        assert_eq!(found("nobody"), None);

        let session = Session {
            world: WorldState::new(),
            npc_manager: mgr,
            language: LanguageSettings::english_only(),
        };
        let app = session.view();
        for view in [
            "memory micheal",
            "schedule micheal",
            "relationships roisin",
            "gossip roisin",
        ] {
            let lines = handle_debug(Some(view), &app);
            assert!(!lines[0].contains("NPC not found"), "{view}: {lines:?}");
        }
        assert!(
            handle_debug(Some("memory micheal"), &app)[0]
                .contains("[DEBUG MEMORY: Mícheál Connolly]")
        );
    }

    #[test]
    fn every_listed_subcommand_is_answered_and_in_help() {
        let session = Session::new();
        let app = session.view();
        let help = debug_help().join("\n");
        for sub in SUBCOMMANDS {
            let lines = handle_debug(Some(sub.name), &app);
            assert!(!lines[0].contains("Unknown debug command"), "{}", sub.name);
            assert!(
                sub.name == "help" || help.contains(&format!("/debug {}", sub.name)),
                "{} in help",
                sub.name
            );
            let usage = lines[0].contains("Usage:");
            assert_eq!(usage, sub.npc == NpcArgument::Required, "{}", sub.name);
        }
    }

    #[test]
    fn test_debug_tiers_empty() {
        let session = Session::new();
        let app = session.view();
        let lines = debug_tiers(&app);
        assert!(lines[0].contains("DEBUG TIERS"));
        assert!(lines[1].contains("Player at:"));
        // All tiers should show (none)
        assert!(lines[2..].iter().all(|l| l.contains("(none)")));
    }

    #[test]
    fn test_debug_here() {
        let session = Session::new();
        let app = session.view();
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
        let session = Session::new();
        let app = session.view();
        let lines = debug_relationships(&app, None);
        assert!(lines[0].contains("Usage:"));
    }

    #[test]
    fn test_debug_relationships_not_found() {
        let session = Session::new();
        let app = session.view();
        let lines = debug_relationships(&app, Some("nobody"));
        assert!(lines[0].contains("NPC not found"));
    }

    #[test]
    fn test_debug_memory_no_name() {
        let session = Session::new();
        let app = session.view();
        let lines = debug_memory(&app, None);
        assert!(lines[0].contains("Usage:"));
    }

    #[test]
    fn test_debug_schedule_not_found() {
        let session = Session::new();
        let app = session.view();
        let lines = debug_schedule(&app, Some("nobody"));
        assert!(lines[0].contains("NPC not found"));
    }

    #[test]
    fn test_handle_debug_all_subcommands() {
        let session = Session::new();
        let app = session.view();
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
        let session = Session::new();
        let app = session.view();
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
