//! Top status bar for the GUI.
//!
//! Displays the current location name, game time, weather, season,
//! and any active festival — matching the TUI's top bar information.

use eframe::egui;

use crate::world::WorldState;

use super::theme::GuiPalette;

/// Draws the top status bar showing game state at a glance.
///
/// Layout: `Location | HH:MM TimeOfDay | Weather | Season [| Festival] [| PAUSED]`
pub fn draw_status_bar(ui: &mut egui::Ui, world: &WorldState, palette: &GuiPalette) {
    let frame = egui::Frame::new()
        .fill(palette.accent.linear_multiply(0.3))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .corner_radius(4.0);

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            // Location name
            let loc_name = &world.current_location().name;
            ui.label(
                egui::RichText::new(loc_name)
                    .color(palette.accent)
                    .strong()
                    .size(15.0),
            );

            ui.separator();

            // Time
            let now = world.clock.now();
            let time_str = format!(
                "{:02}:{:02} {}",
                now.format("%H"),
                now.format("%M"),
                world.clock.time_of_day()
            );
            ui.label(egui::RichText::new(time_str).color(palette.fg).size(13.0));

            ui.separator();

            // Weather
            ui.label(
                egui::RichText::new(world.weather.to_string())
                    .color(palette.fg)
                    .size(13.0),
            );

            ui.separator();

            // Season
            ui.label(
                egui::RichText::new(world.clock.season().to_string())
                    .color(palette.fg)
                    .size(13.0),
            );

            // Festival (if active)
            if let Some(festival) = world.clock.check_festival() {
                ui.separator();
                ui.label(
                    egui::RichText::new(festival.to_string())
                        .color(palette.accent)
                        .strong()
                        .size(13.0),
                );
            }

            // Paused indicator
            if world.clock.is_paused() {
                ui.separator();
                ui.label(
                    egui::RichText::new("PAUSED")
                        .color(palette.accent)
                        .strong()
                        .size(13.0),
                );
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::time::Festival;

    #[test]
    fn test_festival_display_values() {
        assert_eq!(Festival::Imbolc.to_string(), "Imbolc");
        assert_eq!(Festival::Bealtaine.to_string(), "Bealtaine");
        assert_eq!(Festival::Lughnasa.to_string(), "Lughnasa");
        assert_eq!(Festival::Samhain.to_string(), "Samhain");
    }

    #[test]
    fn test_world_state_for_status_bar() {
        let world = WorldState::new();
        assert_eq!(world.current_location().name, "The Crossroads");
        assert_eq!(world.weather, crate::world::Weather::Clear);
        assert!(!world.clock.is_paused());
    }
}
