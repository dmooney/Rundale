//! Sidebar panels for Irish word hints and NPC information.
//!
//! Provides two collapsible sections:
//! 1. **Focail (Words)** — Irish pronunciation hints from NPC dialogue
//! 2. **NPCs Here** — NPCs present at the player's current location

use eframe::egui;

use crate::npc::IrishWordHint;
use crate::npc::manager::NpcManager;
use crate::world::LocationId;

use super::theme::GuiPalette;

/// Draws the right sidebar with Irish words and NPC information.
///
/// The sidebar contains two collapsible sections. Irish word hints
/// show the word, its pronunciation, and meaning. The NPC section
/// lists characters at the player's current location with their
/// occupation and mood.
pub fn draw_sidebar(
    ui: &mut egui::Ui,
    pronunciation_hints: &[IrishWordHint],
    npc_manager: &NpcManager,
    player_location: LocationId,
    palette: &GuiPalette,
) {
    let frame = egui::Frame::new()
        .fill(palette.panel_bg)
        .inner_margin(egui::Margin::same(8))
        .corner_radius(4.0);

    frame.show(ui, |ui| {
        // Irish Words section
        egui::CollapsingHeader::new(
            egui::RichText::new("Focail — Words")
                .color(palette.accent)
                .strong()
                .size(14.0),
        )
        .default_open(true)
        .show(ui, |ui| {
            if pronunciation_hints.is_empty() {
                ui.label(
                    egui::RichText::new("Irish words from conversation will appear here.")
                        .color(palette.muted)
                        .italics()
                        .size(12.0),
                );
            } else {
                for hint in pronunciation_hints.iter().rev().take(15) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(&hint.word)
                                .color(palette.accent)
                                .strong()
                                .size(13.0),
                        );
                        ui.label(
                            egui::RichText::new(format!("[{}]", hint.pronunciation))
                                .color(palette.fg)
                                .size(12.0),
                        );
                        if let Some(meaning) = &hint.meaning {
                            ui.label(
                                egui::RichText::new(format!("— {meaning}"))
                                    .color(palette.muted)
                                    .size(12.0),
                            );
                        }
                    });
                    ui.add_space(2.0);
                }
            }
        });

        ui.add_space(8.0);

        // NPCs Here section
        egui::CollapsingHeader::new(
            egui::RichText::new("NPCs Here")
                .color(palette.accent)
                .strong()
                .size(14.0),
        )
        .default_open(true)
        .show(ui, |ui| {
            let npcs_here = npc_manager.npcs_at(player_location);
            if npcs_here.is_empty() {
                ui.label(
                    egui::RichText::new("No one else is here.")
                        .color(palette.muted)
                        .italics()
                        .size(12.0),
                );
            } else {
                for npc in &npcs_here {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(&npc.name)
                                .color(palette.fg)
                                .strong()
                                .size(13.0),
                        );
                        ui.label(
                            egui::RichText::new(format!("{} · {}", npc.occupation, npc.mood))
                                .color(palette.muted)
                                .size(12.0),
                        );
                    });
                    ui.add_space(4.0);
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_hints_and_no_npcs() {
        let hints: Vec<IrishWordHint> = vec![];
        let mgr = NpcManager::new();
        let loc = LocationId(1);
        // Verify data structures are compatible
        assert!(hints.is_empty());
        assert_eq!(mgr.npcs_at(loc).len(), 0);
    }

    #[test]
    fn test_hint_display_fields() {
        let hint = IrishWordHint {
            word: "sláinte".to_string(),
            pronunciation: "SLAWN-cha".to_string(),
            meaning: Some("Health/cheers".to_string()),
        };
        assert_eq!(hint.word, "sláinte");
        assert_eq!(hint.pronunciation, "SLAWN-cha");
        assert!(hint.meaning.is_some());
    }

    #[test]
    fn test_hint_without_meaning() {
        let hint = IrishWordHint {
            word: "craic".to_string(),
            pronunciation: "crack".to_string(),
            meaning: None,
        };
        assert!(hint.meaning.is_none());
    }
}
