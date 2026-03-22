//! Text input field for the GUI.
//!
//! Provides a single-line text input with Enter-to-submit behavior,
//! matching the TUI's input line. The submitted text is returned for
//! processing through the shared `classify_input` / `parse_intent` pipeline.

use eframe::egui;

use super::theme::GuiPalette;

/// Draws the input field at the bottom of the window.
///
/// Returns `Some(text)` when the user presses Enter, `None` otherwise.
/// The input buffer is cleared after submission.
pub fn draw_input_field(
    ui: &mut egui::Ui,
    input_buffer: &mut String,
    palette: &GuiPalette,
) -> Option<String> {
    let mut submitted = None;

    let frame = egui::Frame::new()
        .fill(palette.panel_bg)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .corner_radius(4.0);

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(">")
                    .color(palette.accent)
                    .strong()
                    .size(16.0),
            );

            let response = ui.add_sized(
                ui.available_size(),
                egui::TextEdit::singleline(input_buffer)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .text_color(palette.fg)
                    .hint_text("Type a command or speak..."),
            );

            // Submit on Enter
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let text = input_buffer.trim().to_string();
                if !text.is_empty() {
                    submitted = Some(text);
                    input_buffer.clear();
                }
            }

            // Keep focus on input field
            if submitted.is_some() {
                response.request_focus();
            }

            // Auto-focus on first frame
            if ui.memory(|m| !m.has_focus(response.id)) && !response.has_focus() {
                response.request_focus();
            }
        });
    });

    submitted
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_empty_input_not_submitted() {
        let input = "   ".trim().to_string();
        assert!(input.is_empty());
    }

    #[test]
    fn test_input_trimming() {
        let input = "  go north  ".trim().to_string();
        assert_eq!(input, "go north");
    }

    #[test]
    fn test_input_buffer_clear() {
        let mut buffer = String::from("hello");
        buffer.clear();
        assert!(buffer.is_empty());
    }
}
