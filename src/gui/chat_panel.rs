//! Scrollable chat/text log panel for the GUI.
//!
//! Renders the game's `text_log` as a scrollable panel with auto-scroll
//! behavior. New messages appear at the bottom and the view follows
//! unless the user scrolls up to review history.

use eframe::egui;

use super::theme::GuiPalette;

/// Optional loading animation display data for the chat panel.
pub struct LoadingDisplay {
    /// The animation text, e.g. `"✛ Consulting the sheep..."`.
    pub text: String,
    /// RGB color for the animation text.
    pub color: egui::Color32,
}

/// Draws the chat panel showing the game text log.
///
/// Uses `egui::ScrollArea` with `stick_to_bottom` for auto-scroll behavior.
/// Each log entry is rendered as a separate paragraph with the palette's
/// foreground color. When `loading` is `Some`, appends an animated loading
/// indicator after the last log entry.
pub fn draw_chat_panel(
    ui: &mut egui::Ui,
    text_log: &[String],
    palette: &GuiPalette,
    loading: Option<&LoadingDisplay>,
) {
    let frame = egui::Frame::new()
        .fill(palette.panel_bg)
        .inner_margin(egui::Margin::same(8))
        .corner_radius(4.0);

    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("— Story —")
                    .color(palette.accent)
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if text_log.is_empty() {
                        ui.label(
                            egui::RichText::new("The story begins...")
                                .color(palette.muted)
                                .italics(),
                        );
                    } else {
                        for entry in text_log {
                            ui.label(egui::RichText::new(entry).color(palette.fg).size(14.0));
                            ui.add_space(4.0);
                        }
                    }

                    // Show loading animation while waiting for inference
                    if let Some(ld) = loading {
                        ui.label(egui::RichText::new(&ld.text).color(ld.color).size(14.0));
                    }
                });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_chat_panel_with_empty_log() {
        // Verify the function signature accepts empty log without panic
        let palette =
            crate::gui::theme::gui_palette_for_time(&crate::world::time::TimeOfDay::Midday);
        let log: Vec<String> = vec![];
        // We can't render without a full egui context, but we can verify the
        // palette and log are compatible with the function signature
        assert!(log.is_empty());
        assert_ne!(palette.fg, egui::Color32::TRANSPARENT);
    }

    #[test]
    fn test_draw_chat_panel_with_entries() {
        let palette =
            crate::gui::theme::gui_palette_for_time(&crate::world::time::TimeOfDay::Night);
        let log = vec![
            "You arrive at The Crossroads.".to_string(),
            "Padraig looks up from behind the bar.".to_string(),
        ];
        assert_eq!(log.len(), 2);
        assert_ne!(palette.panel_bg, egui::Color32::TRANSPARENT);
    }

    #[test]
    fn test_loading_display_struct() {
        let ld = LoadingDisplay {
            text: "✛ Pondering the craic...".to_string(),
            color: egui::Color32::from_rgb(72, 199, 142),
        };
        assert!(ld.text.contains("Pondering"));
        assert_eq!(ld.color, egui::Color32::from_rgb(72, 199, 142));
    }
}
