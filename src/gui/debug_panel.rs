//! Debug inspection panel for the egui GUI.
//!
//! Provides a floating window with tabbed views for inspecting all game
//! state: world overview, locations, NPCs, event log, and relationships.
//! All views are read-only. Toggled via F12 or `/debug panel`.

use std::collections::VecDeque;
use std::fmt;

use chrono::Timelike;
use eframe::egui;

use crate::npc::manager::NpcManager;
use crate::npc::types::{CogTier, NpcState};
use crate::npc::{Npc, NpcId};
use crate::world::graph::WorldGraph;
use crate::world::{LocationId, WorldState};

use super::theme::GuiPalette;

/// Maximum number of debug events to retain.
pub const DEBUG_EVENT_CAPACITY: usize = 500;

/// Category of a debug event for color-coding and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugEventCategory {
    /// Player or NPC movement.
    Movement,
    /// NPC schedule transitions.
    Schedule,
    /// Player-NPC dialogue started.
    Conversation,
    /// NPC tier reassignment.
    TierChange,
    /// NPC mood update.
    MoodChange,
    /// Relationship strength change.
    Relationship,
    /// Atmospheric overheard event.
    Overheard,
    /// System event (clock, weather, etc.).
    System,
}

impl fmt::Display for DebugEventCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DebugEventCategory::Movement => write!(f, "MOVE"),
            DebugEventCategory::Schedule => write!(f, "SCHED"),
            DebugEventCategory::Conversation => write!(f, "CHAT"),
            DebugEventCategory::TierChange => write!(f, "TIER"),
            DebugEventCategory::MoodChange => write!(f, "MOOD"),
            DebugEventCategory::Relationship => write!(f, "REL"),
            DebugEventCategory::Overheard => write!(f, "HEAR"),
            DebugEventCategory::System => write!(f, "SYS"),
        }
    }
}

impl DebugEventCategory {
    /// Returns a color for this category.
    fn color(&self) -> egui::Color32 {
        match self {
            DebugEventCategory::Movement => egui::Color32::from_rgb(100, 180, 255),
            DebugEventCategory::Schedule => egui::Color32::from_rgb(180, 180, 100),
            DebugEventCategory::Conversation => egui::Color32::from_rgb(100, 220, 100),
            DebugEventCategory::TierChange => egui::Color32::from_rgb(200, 140, 255),
            DebugEventCategory::MoodChange => egui::Color32::from_rgb(255, 180, 100),
            DebugEventCategory::Relationship => egui::Color32::from_rgb(255, 130, 130),
            DebugEventCategory::Overheard => egui::Color32::from_rgb(150, 200, 200),
            DebugEventCategory::System => egui::Color32::from_rgb(180, 180, 180),
        }
    }
}

/// A structured debug event with timestamp and category.
#[derive(Debug, Clone)]
pub struct DebugEvent {
    /// Game time formatted as HH:MM.
    pub timestamp: String,
    /// Event category for color-coding.
    pub category: DebugEventCategory,
    /// Human-readable event description.
    pub message: String,
}

/// Which tab is active in the debug panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugTab {
    /// World overview: clock, weather, tiers, player location.
    World,
    /// Location browser with detail pane.
    Locations,
    /// NPC browser with detail pane.
    Npcs,
    /// Scrollable event log.
    Events,
    /// All NPC-to-NPC relationships.
    Relationships,
}

/// Persistent UI state for the debug panel.
#[derive(Debug)]
pub struct DebugUiState {
    /// Which tab is currently active.
    pub active_tab: DebugTab,
    /// Selected location for detail view (Locations tab).
    pub selected_location: Option<LocationId>,
    /// Selected NPC for detail view (NPCs tab).
    pub selected_npc: Option<NpcId>,
    /// Text filter for the NPC list.
    pub npc_filter: String,
    /// Text filter for the location list.
    pub location_filter: String,
    /// Whether the event log auto-scrolls to bottom.
    pub event_log_auto_scroll: bool,
    /// Text filter for relationships tab.
    pub relationship_filter: String,
}

impl Default for DebugUiState {
    fn default() -> Self {
        Self {
            active_tab: DebugTab::World,
            selected_location: None,
            selected_npc: None,
            npc_filter: String::new(),
            location_filter: String::new(),
            event_log_auto_scroll: true,
            relationship_filter: String::new(),
        }
    }
}

/// Draws the debug inspector window.
///
/// This is a floating `egui::Window` with tabs for each inspection domain.
/// Returns `true` if the window's close button was clicked (toggle off).
pub fn draw_debug_window(
    ctx: &egui::Context,
    state: &mut DebugUiState,
    world: &WorldState,
    npc_manager: &NpcManager,
    debug_events: &VecDeque<DebugEvent>,
    palette: &GuiPalette,
) -> bool {
    let mut open = true;
    egui::Window::new("Debug Inspector")
        .open(&mut open)
        .default_size([700.0, 500.0])
        .min_width(500.0)
        .min_height(300.0)
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // Tab bar
            ui.horizontal(|ui| {
                let tabs = [
                    (DebugTab::World, "World"),
                    (DebugTab::Locations, "Locations"),
                    (DebugTab::Npcs, "NPCs"),
                    (DebugTab::Events, "Events"),
                    (DebugTab::Relationships, "Relationships"),
                ];
                for (tab, label) in &tabs {
                    let selected = state.active_tab == *tab;
                    if ui.selectable_label(selected, *label).clicked() {
                        state.active_tab = *tab;
                    }
                }
            });
            ui.separator();

            // Tab content
            match state.active_tab {
                DebugTab::World => draw_world_tab(ui, world, npc_manager, palette),
                DebugTab::Locations => {
                    draw_locations_tab(ui, state, world, npc_manager, palette);
                }
                DebugTab::Npcs => draw_npcs_tab(ui, state, world, npc_manager, palette),
                DebugTab::Events => draw_events_tab(ui, state, debug_events, palette),
                DebugTab::Relationships => {
                    draw_relationships_tab(ui, state, npc_manager, &world.graph, palette);
                }
            }
        });

    !open
}

/// Renders the World overview tab.
fn draw_world_tab(
    ui: &mut egui::Ui,
    world: &WorldState,
    npc_manager: &NpcManager,
    palette: &GuiPalette,
) {
    egui::Grid::new("world_grid")
        .num_columns(2)
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            let now = world.clock.now();
            let tod = world.clock.time_of_day();
            let season = world.clock.season();

            label_value(
                ui,
                "Time",
                &format!("{:02}:{:02} {}", now.hour(), now.minute(), tod),
                palette,
            );
            label_value(ui, "Date", &format!("{}", now.format("%Y-%m-%d")), palette);
            label_value(ui, "Season", &season.to_string(), palette);

            let festival = world.clock.check_festival();
            label_value(
                ui,
                "Festival",
                &festival
                    .map(|f| f.to_string())
                    .unwrap_or_else(|| "None".to_string()),
                palette,
            );

            label_value(ui, "Weather", &world.weather, palette);

            let speed_str = world
                .clock
                .current_speed()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Custom ({}x)", world.clock.speed_factor()));
            label_value(ui, "Speed", &speed_str, palette);

            let paused = if world.clock.is_paused() { "Yes" } else { "No" };
            label_value(ui, "Paused", paused, palette);

            let loc_name = world
                .graph
                .get(world.player_location)
                .map(|l| l.name.as_str())
                .unwrap_or("?");
            label_value(
                ui,
                "Player Location",
                &format!("{} (id:{})", loc_name, world.player_location.0),
                palette,
            );
        });

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("NPC Tier Distribution")
            .color(palette.accent)
            .strong(),
    );
    ui.separator();

    let mut t1 = 0u32;
    let mut t2 = 0u32;
    let mut t3 = 0u32;
    for npc in npc_manager.all_npcs() {
        match npc_manager.tier_of(npc.id).unwrap_or(CogTier::Tier3) {
            CogTier::Tier1 => t1 += 1,
            CogTier::Tier2 => t2 += 1,
            CogTier::Tier3 | CogTier::Tier4 => t3 += 1,
        }
    }
    let total = npc_manager.npc_count();
    ui.label(format!(
        "Total: {} | Tier1 (here): {} | Tier2 (nearby): {} | Tier3+ (far): {}",
        total, t1, t2, t3
    ));
}

/// Renders the Locations tab with list + detail pane.
fn draw_locations_tab(
    ui: &mut egui::Ui,
    state: &mut DebugUiState,
    world: &WorldState,
    npc_manager: &NpcManager,
    palette: &GuiPalette,
) {
    let available = ui.available_size();

    // Horizontal layout: list on left, detail on right
    ui.horizontal(|ui| {
        // Left: location list
        ui.allocate_ui(egui::vec2(200.0, available.y), |ui| {
            ui.label(egui::RichText::new("Filter:").color(palette.muted).small());
            ui.text_edit_singleline(&mut state.location_filter);
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("loc_list")
                .show(ui, |ui| {
                    let filter_lower = state.location_filter.to_lowercase();
                    let mut loc_ids = world.graph.location_ids();
                    loc_ids.sort_by_key(|id| id.0);

                    for loc_id in &loc_ids {
                        let Some(loc) = world.graph.get(*loc_id) else {
                            continue;
                        };
                        if !filter_lower.is_empty()
                            && !loc.name.to_lowercase().contains(&filter_lower)
                        {
                            continue;
                        }
                        let selected = state.selected_location == Some(*loc_id);
                        let is_player_loc = *loc_id == world.player_location;
                        let label = if is_player_loc {
                            format!("* {} ({})", loc.name, loc_id.0)
                        } else {
                            format!("  {} ({})", loc.name, loc_id.0)
                        };
                        if ui.selectable_label(selected, &label).clicked() {
                            state.selected_location = Some(*loc_id);
                        }
                    }
                });
        });

        ui.separator();

        // Right: detail pane
        ui.vertical(|ui| {
            if let Some(loc_id) = state.selected_location {
                if let Some(loc) = world.graph.get(loc_id) {
                    ui.label(
                        egui::RichText::new(&loc.name)
                            .color(palette.accent)
                            .heading(),
                    );
                    ui.label(format!("ID: {}", loc.id.0));

                    let flags = format!(
                        "Indoor: {} | Public: {}",
                        if loc.indoor { "Yes" } else { "No" },
                        if loc.public { "Yes" } else { "No" }
                    );
                    ui.label(&flags);

                    if let Some(ref myth) = loc.mythological_significance {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Mythological Significance")
                                .color(palette.accent)
                                .strong(),
                        );
                        ui.label(myth);
                    }

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Description Template")
                            .color(palette.accent)
                            .strong(),
                    );
                    ui.label(&loc.description_template);

                    // Connections
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Connections")
                            .color(palette.accent)
                            .strong(),
                    );
                    for conn in &loc.connections {
                        let target_name = world
                            .graph
                            .get(conn.target)
                            .map(|t| t.name.as_str())
                            .unwrap_or("?");
                        ui.label(format!(
                            "  -> {} ({} min) — {}",
                            target_name, conn.traversal_minutes, conn.path_description
                        ));
                    }

                    // Associated NPCs
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Associated NPCs")
                            .color(palette.accent)
                            .strong(),
                    );
                    if loc.associated_npcs.is_empty() {
                        ui.label("  (none)");
                    } else {
                        for npc_id in &loc.associated_npcs {
                            let name = npc_manager
                                .get(*npc_id)
                                .map(|n| n.name.as_str())
                                .unwrap_or("?");
                            ui.label(format!("  {} (id:{})", name, npc_id.0));
                        }
                    }

                    // NPCs currently here
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("NPCs Currently Here")
                            .color(palette.accent)
                            .strong(),
                    );
                    let npcs_here = npc_manager.npcs_at(loc_id);
                    if npcs_here.is_empty() {
                        ui.label("  (nobody)");
                    } else {
                        for npc in &npcs_here {
                            ui.label(format!(
                                "  {} — {} [{}]",
                                npc.name, npc.occupation, npc.mood
                            ));
                        }
                    }
                } else {
                    ui.label("Location not found.");
                }
            } else {
                ui.label(
                    egui::RichText::new("Select a location from the list.").color(palette.muted),
                );
            }
        });
    });
}

/// Renders the NPCs tab with list + detail pane.
fn draw_npcs_tab(
    ui: &mut egui::Ui,
    state: &mut DebugUiState,
    world: &WorldState,
    npc_manager: &NpcManager,
    palette: &GuiPalette,
) {
    let available = ui.available_size();

    ui.horizontal(|ui| {
        // Left: NPC list
        ui.allocate_ui(egui::vec2(220.0, available.y), |ui| {
            ui.label(egui::RichText::new("Filter:").color(palette.muted).small());
            ui.text_edit_singleline(&mut state.npc_filter);
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("npc_list")
                .show(ui, |ui| {
                    let filter_lower = state.npc_filter.to_lowercase();
                    let mut npcs: Vec<&Npc> = npc_manager.all_npcs().collect();
                    npcs.sort_by_key(|n| n.id.0);

                    for npc in &npcs {
                        if !filter_lower.is_empty()
                            && !npc.name.to_lowercase().contains(&filter_lower)
                            && !npc.occupation.to_lowercase().contains(&filter_lower)
                        {
                            continue;
                        }
                        let tier = npc_manager
                            .tier_of(npc.id)
                            .map(|t| tier_label(t).to_string())
                            .unwrap_or_else(|| "?".to_string());
                        let loc_name = world
                            .graph
                            .get(npc.location)
                            .map(|l| l.name.clone())
                            .unwrap_or_else(|| "?".to_string());
                        let state_indicator = match &npc.state {
                            NpcState::Present => "",
                            NpcState::InTransit { .. } => " ->",
                        };

                        let label = format!(
                            "{}{} [{}] @{} ({})",
                            npc.name, state_indicator, npc.mood, loc_name, tier
                        );
                        let selected = state.selected_npc == Some(npc.id);
                        if ui.selectable_label(selected, &label).clicked() {
                            state.selected_npc = Some(npc.id);
                        }
                    }
                });
        });

        ui.separator();

        // Right: NPC detail
        egui::ScrollArea::vertical()
            .id_salt("npc_detail")
            .show(ui, |ui| {
                if let Some(npc_id) = state.selected_npc {
                    if let Some(npc) = npc_manager.get(npc_id) {
                        draw_npc_detail(ui, npc, world, npc_manager, palette);
                    } else {
                        ui.label("NPC not found.");
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Select an NPC from the list.").color(palette.muted),
                    );
                }
            });
    });
}

/// Renders full detail for a single NPC.
fn draw_npc_detail(
    ui: &mut egui::Ui,
    npc: &Npc,
    world: &WorldState,
    npc_manager: &NpcManager,
    palette: &GuiPalette,
) {
    ui.label(
        egui::RichText::new(&npc.name)
            .color(palette.accent)
            .heading(),
    );

    // Identity
    egui::Grid::new("npc_identity")
        .num_columns(2)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            label_value(ui, "ID", &npc.id.0.to_string(), palette);
            label_value(ui, "Age", &npc.age.to_string(), palette);
            label_value(ui, "Occupation", &npc.occupation, palette);
            label_value(ui, "Mood", &npc.mood, palette);

            let loc_name = world
                .graph
                .get(npc.location)
                .map(|l| l.name.as_str())
                .unwrap_or("?");
            label_value(
                ui,
                "Location",
                &format!("{} (id:{})", loc_name, npc.location.0),
                palette,
            );

            let tier = npc_manager
                .tier_of(npc.id)
                .map(|t| tier_label(t).to_string())
                .unwrap_or_else(|| "?".to_string());
            label_value(ui, "Tier", &tier, palette);

            match &npc.state {
                NpcState::Present => {
                    label_value(ui, "State", "Present", palette);
                }
                NpcState::InTransit {
                    from,
                    to,
                    arrives_at,
                } => {
                    let from_name = world
                        .graph
                        .get(*from)
                        .map(|l| l.name.as_str())
                        .unwrap_or("?");
                    let to_name = world.graph.get(*to).map(|l| l.name.as_str()).unwrap_or("?");
                    label_value(
                        ui,
                        "State",
                        &format!(
                            "In Transit: {} -> {} (arrives {:02}:{:02})",
                            from_name,
                            to_name,
                            arrives_at.hour(),
                            arrives_at.minute()
                        ),
                        palette,
                    );
                }
            }

            if let Some(home) = npc.home {
                let name = world
                    .graph
                    .get(home)
                    .map(|l| l.name.as_str())
                    .unwrap_or("?");
                label_value(ui, "Home", &format!("{} (id:{})", name, home.0), palette);
            }
            if let Some(work) = npc.workplace {
                let name = world
                    .graph
                    .get(work)
                    .map(|l| l.name.as_str())
                    .unwrap_or("?");
                label_value(
                    ui,
                    "Workplace",
                    &format!("{} (id:{})", name, work.0),
                    palette,
                );
            }
        });

    // Personality
    ui.add_space(6.0);
    ui.collapsing(
        egui::RichText::new("Personality")
            .color(palette.accent)
            .strong(),
        |ui| {
            ui.label(&npc.personality);
        },
    );

    // Schedule
    ui.add_space(4.0);
    ui.collapsing(
        egui::RichText::new("Schedule")
            .color(palette.accent)
            .strong(),
        |ui| {
            if let Some(ref schedule) = npc.schedule {
                egui::Grid::new("npc_schedule")
                    .num_columns(3)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Time").strong());
                        ui.label(egui::RichText::new("Activity").strong());
                        ui.label(egui::RichText::new("Location").strong());
                        ui.end_row();

                        for entry in &schedule.entries {
                            ui.label(format!(
                                "{:02}:00 - {:02}:00",
                                entry.start_hour, entry.end_hour
                            ));
                            ui.label(&entry.activity);
                            let loc_name = world
                                .graph
                                .get(entry.location)
                                .map(|l| l.name.as_str())
                                .unwrap_or("?");
                            ui.label(loc_name);
                            ui.end_row();
                        }
                    });
            } else {
                ui.label("No schedule.");
            }
        },
    );

    // Memory
    ui.add_space(4.0);
    ui.collapsing(
        egui::RichText::new("Memory").color(palette.accent).strong(),
        |ui| {
            let memories = npc.memory.recent(20);
            if memories.is_empty() {
                ui.label("No memories.");
            } else {
                for mem in &memories {
                    let time = format!("{:02}:{:02}", mem.timestamp.hour(), mem.timestamp.minute());
                    let loc_name = world
                        .graph
                        .get(mem.location)
                        .map(|l| l.name.as_str())
                        .unwrap_or("?");
                    ui.label(format!("[{}] @{}: {}", time, loc_name, mem.content));
                }
            }
        },
    );

    // Relationships
    ui.add_space(4.0);
    ui.collapsing(
        egui::RichText::new("Relationships")
            .color(palette.accent)
            .strong(),
        |ui| {
            if npc.relationships.is_empty() {
                ui.label("No relationships.");
            } else {
                egui::Grid::new("npc_rels")
                    .num_columns(4)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("NPC").strong());
                        ui.label(egui::RichText::new("Kind").strong());
                        ui.label(egui::RichText::new("Strength").strong());
                        ui.label(egui::RichText::new("Bar").strong());
                        ui.end_row();

                        let mut rels: Vec<_> = npc.relationships.iter().collect();
                        rels.sort_by_key(|(id, _)| id.0);

                        for (other_id, rel) in rels {
                            let other_name = npc_manager
                                .get(*other_id)
                                .map(|n| n.name.as_str())
                                .unwrap_or("?");
                            ui.label(other_name);
                            ui.label(format!("{}", rel.kind));
                            ui.label(format!("{:+.2}", rel.strength));
                            draw_strength_bar(ui, rel.strength);
                            ui.end_row();
                        }
                    });
            }
        },
    );

    // Knowledge
    ui.add_space(4.0);
    ui.collapsing(
        egui::RichText::new("Knowledge")
            .color(palette.accent)
            .strong(),
        |ui| {
            if npc.knowledge.is_empty() {
                ui.label("No knowledge entries.");
            } else {
                for item in &npc.knowledge {
                    ui.label(format!("- {}", item));
                }
            }
        },
    );
}

/// Renders the Events tab with scrollable log.
fn draw_events_tab(
    ui: &mut egui::Ui,
    state: &mut DebugUiState,
    events: &VecDeque<DebugEvent>,
    _palette: &GuiPalette,
) {
    ui.horizontal(|ui| {
        ui.label(format!("{} events", events.len()));
        ui.checkbox(&mut state.event_log_auto_scroll, "Auto-scroll");
    });
    ui.separator();

    let scroll = egui::ScrollArea::vertical()
        .id_salt("event_log")
        .stick_to_bottom(state.event_log_auto_scroll);

    scroll.show(ui, |ui| {
        if events.is_empty() {
            ui.label("No events yet.");
            return;
        }
        for event in events {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&event.timestamp)
                        .monospace()
                        .color(egui::Color32::GRAY),
                );
                ui.label(
                    egui::RichText::new(format!("[{}]", event.category))
                        .monospace()
                        .color(event.category.color()),
                );
                ui.label(&event.message);
            });
        }
    });
}

/// Renders the Relationships tab showing all NPC-to-NPC relationships.
fn draw_relationships_tab(
    ui: &mut egui::Ui,
    state: &mut DebugUiState,
    npc_manager: &NpcManager,
    graph: &WorldGraph,
    palette: &GuiPalette,
) {
    ui.label(egui::RichText::new("Filter:").color(palette.muted).small());
    ui.text_edit_singleline(&mut state.relationship_filter);
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("rel_scroll")
        .show(ui, |ui| {
            let filter_lower = state.relationship_filter.to_lowercase();

            egui::Grid::new("rel_grid")
                .num_columns(5)
                .spacing([12.0, 2.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("From").strong());
                    ui.label(egui::RichText::new("To").strong());
                    ui.label(egui::RichText::new("Kind").strong());
                    ui.label(egui::RichText::new("Strength").strong());
                    ui.label(egui::RichText::new("Bar").strong());
                    ui.end_row();

                    let mut npcs: Vec<&Npc> = npc_manager.all_npcs().collect();
                    npcs.sort_by_key(|n| n.id.0);

                    for npc in &npcs {
                        let mut rels: Vec<_> = npc.relationships.iter().collect();
                        rels.sort_by_key(|(id, _)| id.0);

                        for (other_id, rel) in rels {
                            let other_name = npc_manager
                                .get(*other_id)
                                .map(|n| n.name.as_str())
                                .unwrap_or("?");

                            if !filter_lower.is_empty()
                                && !npc.name.to_lowercase().contains(&filter_lower)
                                && !other_name.to_lowercase().contains(&filter_lower)
                            {
                                continue;
                            }

                            ui.label(&npc.name);
                            ui.label(other_name);
                            ui.label(format!("{}", rel.kind));
                            ui.label(format!("{:+.2}", rel.strength));
                            draw_strength_bar(ui, rel.strength);
                            ui.end_row();
                        }
                    }
                });
        });

    // Also show a legend
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let _ = graph; // used for location lookups if needed
        ui.label(
            egui::RichText::new("Strength: -1.0 (hostile) to +1.0 (close)")
                .color(palette.muted)
                .small(),
        );
    });
}

// --- Helpers ---

/// Draws a label-value pair in a grid row.
fn label_value(ui: &mut egui::Ui, label: &str, value: &str, palette: &GuiPalette) {
    ui.label(egui::RichText::new(label).color(palette.muted).strong());
    ui.label(value);
    ui.end_row();
}

/// Draws a strength bar from -1.0 to +1.0.
fn draw_strength_bar(ui: &mut egui::Ui, strength: f64) {
    let (rect, _response) = ui.allocate_exact_size(egui::vec2(80.0, 12.0), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Background
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(40));

        // Center line
        let center_x = rect.center().x;
        painter.line_segment(
            [
                egui::pos2(center_x, rect.top()),
                egui::pos2(center_x, rect.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
        );

        // Fill bar
        let clamped = strength.clamp(-1.0, 1.0) as f32;
        let color = if clamped >= 0.0 {
            egui::Color32::from_rgb(80, 180, 80)
        } else {
            egui::Color32::from_rgb(200, 80, 80)
        };

        let bar_rect = if clamped >= 0.0 {
            egui::Rect::from_min_max(
                egui::pos2(center_x, rect.top() + 1.0),
                egui::pos2(
                    center_x + clamped * (rect.width() / 2.0),
                    rect.bottom() - 1.0,
                ),
            )
        } else {
            egui::Rect::from_min_max(
                egui::pos2(center_x + clamped * (rect.width() / 2.0), rect.top() + 1.0),
                egui::pos2(center_x, rect.bottom() - 1.0),
            )
        };
        painter.rect_filled(bar_rect, 1.0, color);
    }
}

/// Returns a short label for a cognitive tier.
fn tier_label(tier: CogTier) -> &'static str {
    match tier {
        CogTier::Tier1 => "T1",
        CogTier::Tier2 => "T2",
        CogTier::Tier3 => "T3",
        CogTier::Tier4 => "T4",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_ui_state_default() {
        let state = DebugUiState::default();
        assert_eq!(state.active_tab, DebugTab::World);
        assert!(state.selected_location.is_none());
        assert!(state.selected_npc.is_none());
        assert!(state.npc_filter.is_empty());
        assert!(state.location_filter.is_empty());
        assert!(state.event_log_auto_scroll);
        assert!(state.relationship_filter.is_empty());
    }

    #[test]
    fn test_debug_event_category_display() {
        assert_eq!(format!("{}", DebugEventCategory::Movement), "MOVE");
        assert_eq!(format!("{}", DebugEventCategory::Schedule), "SCHED");
        assert_eq!(format!("{}", DebugEventCategory::Conversation), "CHAT");
        assert_eq!(format!("{}", DebugEventCategory::TierChange), "TIER");
        assert_eq!(format!("{}", DebugEventCategory::MoodChange), "MOOD");
        assert_eq!(format!("{}", DebugEventCategory::Relationship), "REL");
        assert_eq!(format!("{}", DebugEventCategory::Overheard), "HEAR");
        assert_eq!(format!("{}", DebugEventCategory::System), "SYS");
    }

    #[test]
    fn test_debug_event_category_color() {
        // Just verify each category returns a non-transparent color
        let categories = [
            DebugEventCategory::Movement,
            DebugEventCategory::Schedule,
            DebugEventCategory::Conversation,
            DebugEventCategory::TierChange,
            DebugEventCategory::MoodChange,
            DebugEventCategory::Relationship,
            DebugEventCategory::Overheard,
            DebugEventCategory::System,
        ];
        for cat in &categories {
            assert_ne!(cat.color(), egui::Color32::TRANSPARENT);
        }
    }

    #[test]
    fn test_debug_event_creation() {
        let event = DebugEvent {
            timestamp: "12:00".to_string(),
            category: DebugEventCategory::Movement,
            message: "Player moved to pub".to_string(),
        };
        assert_eq!(event.timestamp, "12:00");
        assert_eq!(event.category, DebugEventCategory::Movement);
        assert_eq!(event.message, "Player moved to pub");
    }

    #[test]
    fn test_tier_label() {
        assert_eq!(tier_label(CogTier::Tier1), "T1");
        assert_eq!(tier_label(CogTier::Tier2), "T2");
        assert_eq!(tier_label(CogTier::Tier3), "T3");
        assert_eq!(tier_label(CogTier::Tier4), "T4");
    }

    #[test]
    fn test_debug_event_capacity() {
        assert_eq!(DEBUG_EVENT_CAPACITY, 500);
    }
}
