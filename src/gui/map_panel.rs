//! Visual location graph panel for the GUI.
//!
//! Renders the world graph as an interactive map using egui's `Painter` API.
//! Locations are drawn as circles with names, connected by lines representing
//! traversable paths. The player's current location is highlighted.

use eframe::egui;
use std::collections::HashMap;

use crate::npc::manager::NpcManager;
use crate::world::LocationId;
use crate::world::graph::WorldGraph;

use super::theme::GuiPalette;

/// Fixed 2D positions for map layout.
///
/// Assigns a normalized (0.0–1.0) position to each location id for
/// rendering. Uses a hand-tuned layout for the parish geography.
fn location_positions(graph: &WorldGraph) -> HashMap<LocationId, egui::Pos2> {
    let ids = graph.location_ids();
    let count = ids.len();
    if count == 0 {
        return HashMap::new();
    }

    let mut positions = HashMap::new();

    // Arrange locations in a circle with some jitter based on id
    for (i, id) in ids.iter().enumerate() {
        let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
        // Add slight variation based on location id to avoid perfect symmetry
        let radius = 0.35 + (id.0 as f32 * 0.01).sin() * 0.05;
        let x = 0.5 + angle.cos() * radius;
        let y = 0.5 + angle.sin() * radius;
        positions.insert(*id, egui::pos2(x, y));
    }

    positions
}

/// Draws the map panel showing the world graph.
///
/// Locations are circles with names. Connections are drawn as lines.
/// The player's current location is highlighted with the accent color.
/// NPCs at each location are shown as small dots.
///
/// Returns `Some(LocationId)` if the player clicked an adjacent location
/// to move there.
pub fn draw_map_panel(
    ui: &mut egui::Ui,
    graph: &WorldGraph,
    player_location: LocationId,
    npc_manager: &NpcManager,
    palette: &GuiPalette,
) -> Option<LocationId> {
    let mut clicked_location = None;

    let frame = egui::Frame::new()
        .fill(palette.panel_bg)
        .inner_margin(egui::Margin::same(8))
        .corner_radius(4.0);

    frame.show(ui, |ui| {
        ui.label(
            egui::RichText::new("— Map —")
                .color(palette.accent)
                .strong()
                .size(14.0),
        );
        ui.add_space(4.0);

        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(available, egui::Sense::hover());
        let rect = response.rect;

        let positions = location_positions(graph);
        let neighbors: Vec<LocationId> = graph
            .neighbors(player_location)
            .iter()
            .map(|(id, _)| *id)
            .collect();

        // Draw connections (edges)
        for id in graph.location_ids() {
            if let Some(&from_pos) = positions.get(&id) {
                let from_screen = egui::pos2(
                    rect.min.x + from_pos.x * rect.width(),
                    rect.min.y + from_pos.y * rect.height(),
                );
                for (target_id, _) in graph.neighbors(id) {
                    // Only draw each edge once (lower id draws)
                    if target_id.0 > id.0
                        && let Some(&to_pos) = positions.get(&target_id)
                    {
                        let to_screen = egui::pos2(
                            rect.min.x + to_pos.x * rect.width(),
                            rect.min.y + to_pos.y * rect.height(),
                        );
                        painter.line_segment(
                            [from_screen, to_screen],
                            egui::Stroke::new(1.5, palette.border),
                        );
                    }
                }
            }
        }

        // Draw location nodes
        let node_radius = 12.0_f32.min(rect.width() * 0.03);
        for id in graph.location_ids() {
            if let (Some(&norm_pos), Some(loc_data)) = (positions.get(&id), graph.get(id)) {
                let screen_pos = egui::pos2(
                    rect.min.x + norm_pos.x * rect.width(),
                    rect.min.y + norm_pos.y * rect.height(),
                );

                let is_player_here = id == player_location;
                let is_neighbor = neighbors.contains(&id);

                // Node color
                let fill = if is_player_here {
                    palette.accent
                } else if is_neighbor {
                    palette.accent.linear_multiply(0.5)
                } else {
                    palette.border
                };

                let stroke_color = if is_player_here {
                    palette.fg
                } else {
                    palette.border
                };

                // Draw node circle
                painter.circle(
                    screen_pos,
                    node_radius,
                    fill,
                    egui::Stroke::new(if is_player_here { 2.0 } else { 1.0 }, stroke_color),
                );

                // Draw location name
                let text_pos = egui::pos2(screen_pos.x, screen_pos.y + node_radius + 4.0);
                painter.text(
                    text_pos,
                    egui::Align2::CENTER_TOP,
                    &loc_data.name,
                    egui::FontId::proportional(11.0),
                    if is_player_here {
                        palette.accent
                    } else {
                        palette.muted
                    },
                );

                // Draw NPC dots at location
                let npcs_here = npc_manager.npcs_at(id);
                for (j, _npc) in npcs_here.iter().enumerate() {
                    let dot_offset = (j as f32 - npcs_here.len() as f32 / 2.0) * 6.0;
                    let dot_pos =
                        egui::pos2(screen_pos.x + dot_offset, screen_pos.y - node_radius - 6.0);
                    painter.circle_filled(dot_pos, 2.5, palette.fg);
                }

                // Click detection for adjacent locations
                if is_neighbor {
                    let click_rect = egui::Rect::from_center_size(
                        screen_pos,
                        egui::vec2(node_radius * 3.0, node_radius * 3.0),
                    );
                    let click_response = ui.allocate_rect(click_rect, egui::Sense::click());
                    if click_response.clicked() {
                        clicked_location = Some(id);
                    }
                    if click_response.hovered() {
                        // Redraw with hover effect
                        painter.circle(
                            screen_pos,
                            node_radius + 2.0,
                            egui::Color32::TRANSPARENT,
                            egui::Stroke::new(2.0, palette.accent),
                        );
                    }
                }
            }
        }
    });

    clicked_location
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::graph::WorldGraph;

    fn test_graph() -> WorldGraph {
        let json = r#"{
            "locations": [
                {
                    "id": 1, "name": "A", "description_template": "A",
                    "indoor": false, "public": true,
                    "connections": [{"target": 2, "traversal_minutes": 5, "path_description": "path"}]
                },
                {
                    "id": 2, "name": "B", "description_template": "B",
                    "indoor": false, "public": true,
                    "connections": [{"target": 1, "traversal_minutes": 5, "path_description": "path"}]
                }
            ]
        }"#;
        WorldGraph::load_from_str(json).unwrap()
    }

    #[test]
    fn test_location_positions_empty_graph() {
        let graph = WorldGraph::new();
        let positions = location_positions(&graph);
        assert!(positions.is_empty());
    }

    #[test]
    fn test_location_positions_all_locations_present() {
        let graph = test_graph();
        let positions = location_positions(&graph);
        assert_eq!(positions.len(), 2);
        assert!(positions.contains_key(&LocationId(1)));
        assert!(positions.contains_key(&LocationId(2)));
    }

    #[test]
    fn test_location_positions_within_bounds() {
        let graph = test_graph();
        let positions = location_positions(&graph);
        for (_id, pos) in &positions {
            assert!(pos.x >= 0.0 && pos.x <= 1.0, "x out of bounds: {}", pos.x);
            assert!(pos.y >= 0.0 && pos.y <= 1.0, "y out of bounds: {}", pos.y);
        }
    }

    #[test]
    fn test_location_positions_distinct() {
        let graph = test_graph();
        let positions = location_positions(&graph);
        let pos1 = positions[&LocationId(1)];
        let pos2 = positions[&LocationId(2)];
        let dist = ((pos1.x - pos2.x).powi(2) + (pos1.y - pos2.y).powi(2)).sqrt();
        assert!(dist > 0.01, "locations should have distinct positions");
    }
}
