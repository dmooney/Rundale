//! Time-of-day color theming for the egui GUI.
//!
//! Converts the existing TUI `ColorPalette` RGB values into egui `Color32`
//! values and applies them as a cohesive visual style that shifts with the
//! in-game time of day.

use eframe::egui;

use crate::world::Weather;
use crate::world::palette::{RawPalette, compute_palette};
use crate::world::time::{Season, TimeOfDay};

/// A color palette for the GUI based on time of day.
///
/// Drives background, foreground, accent, and derived panel colors for all
/// GUI elements, creating a visual day/night cycle matching the TUI.
#[derive(Debug, Clone, Copy)]
pub struct GuiPalette {
    /// Main background color.
    pub bg: egui::Color32,
    /// Foreground (text) color.
    pub fg: egui::Color32,
    /// Accent color for status bar and highlights.
    pub accent: egui::Color32,
    /// Slightly offset background for panels (chat, sidebar).
    pub panel_bg: egui::Color32,
    /// Background for the text input field.
    pub input_bg: egui::Color32,
    /// Border/separator color.
    pub border: egui::Color32,
    /// Muted text color for secondary information.
    pub muted: egui::Color32,
}

impl From<RawPalette> for GuiPalette {
    fn from(raw: RawPalette) -> Self {
        GuiPalette {
            bg: egui::Color32::from_rgb(raw.bg.r, raw.bg.g, raw.bg.b),
            fg: egui::Color32::from_rgb(raw.fg.r, raw.fg.g, raw.fg.b),
            accent: egui::Color32::from_rgb(raw.accent.r, raw.accent.g, raw.accent.b),
            panel_bg: egui::Color32::from_rgb(raw.panel_bg.r, raw.panel_bg.g, raw.panel_bg.b),
            input_bg: egui::Color32::from_rgb(raw.input_bg.r, raw.input_bg.g, raw.input_bg.b),
            border: egui::Color32::from_rgb(raw.border.r, raw.border.g, raw.border.b),
            muted: egui::Color32::from_rgb(raw.muted.r, raw.muted.g, raw.muted.b),
        }
    }
}

/// Returns a smoothly interpolated GUI palette for the given time, season, and weather.
///
/// Uses linear interpolation between time-of-day keyframes and applies
/// seasonal and weather color tinting for gradual transitions.
pub fn gui_palette_smooth(hour: u32, minute: u32, season: Season, weather: Weather) -> GuiPalette {
    compute_palette(hour, minute, season, weather).into()
}

/// Returns the GUI palette for the given time of day.
///
/// RGB values match the TUI `palette_for_time()` spec. Panel and input
/// backgrounds are derived by blending toward white (day) or black (night).
pub fn gui_palette_for_time(tod: &TimeOfDay) -> GuiPalette {
    match tod {
        TimeOfDay::Dawn => GuiPalette {
            bg: egui::Color32::from_rgb(255, 220, 180),
            fg: egui::Color32::from_rgb(60, 40, 20),
            accent: egui::Color32::from_rgb(200, 140, 60),
            panel_bg: egui::Color32::from_rgb(250, 215, 175),
            input_bg: egui::Color32::from_rgb(245, 210, 170),
            border: egui::Color32::from_rgb(200, 170, 130),
            muted: egui::Color32::from_rgb(120, 100, 70),
        },
        TimeOfDay::Morning => GuiPalette {
            bg: egui::Color32::from_rgb(255, 245, 220),
            fg: egui::Color32::from_rgb(50, 35, 15),
            accent: egui::Color32::from_rgb(180, 130, 50),
            panel_bg: egui::Color32::from_rgb(250, 240, 215),
            input_bg: egui::Color32::from_rgb(245, 235, 210),
            border: egui::Color32::from_rgb(210, 190, 150),
            muted: egui::Color32::from_rgb(120, 100, 60),
        },
        TimeOfDay::Midday => GuiPalette {
            bg: egui::Color32::from_rgb(255, 255, 240),
            fg: egui::Color32::from_rgb(40, 30, 10),
            accent: egui::Color32::from_rgb(160, 120, 40),
            panel_bg: egui::Color32::from_rgb(250, 250, 235),
            input_bg: egui::Color32::from_rgb(245, 245, 230),
            border: egui::Color32::from_rgb(210, 200, 170),
            muted: egui::Color32::from_rgb(110, 100, 60),
        },
        TimeOfDay::Afternoon => GuiPalette {
            bg: egui::Color32::from_rgb(240, 220, 170),
            fg: egui::Color32::from_rgb(50, 35, 15),
            accent: egui::Color32::from_rgb(180, 130, 50),
            panel_bg: egui::Color32::from_rgb(235, 215, 165),
            input_bg: egui::Color32::from_rgb(230, 210, 160),
            border: egui::Color32::from_rgb(200, 180, 130),
            muted: egui::Color32::from_rgb(120, 100, 60),
        },
        TimeOfDay::Dusk => GuiPalette {
            bg: egui::Color32::from_rgb(60, 70, 110),
            fg: egui::Color32::from_rgb(220, 210, 190),
            accent: egui::Color32::from_rgb(200, 160, 80),
            panel_bg: egui::Color32::from_rgb(55, 65, 100),
            input_bg: egui::Color32::from_rgb(50, 60, 95),
            border: egui::Color32::from_rgb(90, 100, 140),
            muted: egui::Color32::from_rgb(160, 150, 140),
        },
        TimeOfDay::Night => GuiPalette {
            bg: egui::Color32::from_rgb(20, 25, 40),
            fg: egui::Color32::from_rgb(180, 180, 190),
            accent: egui::Color32::from_rgb(100, 110, 140),
            panel_bg: egui::Color32::from_rgb(25, 30, 48),
            input_bg: egui::Color32::from_rgb(30, 35, 55),
            border: egui::Color32::from_rgb(60, 65, 90),
            muted: egui::Color32::from_rgb(120, 120, 135),
        },
        TimeOfDay::Midnight => GuiPalette {
            bg: egui::Color32::from_rgb(10, 12, 20),
            fg: egui::Color32::from_rgb(150, 150, 165),
            accent: egui::Color32::from_rgb(70, 75, 100),
            panel_bg: egui::Color32::from_rgb(15, 18, 28),
            input_bg: egui::Color32::from_rgb(20, 24, 36),
            border: egui::Color32::from_rgb(45, 48, 65),
            muted: egui::Color32::from_rgb(100, 100, 115),
        },
    }
}

/// Applies the given palette to the egui context's global visuals.
///
/// Sets window backgrounds, text colors, widget colors, and stroke
/// colors to match the time-of-day palette.
pub fn apply_palette(ctx: &egui::Context, palette: &GuiPalette) {
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.panel_bg;
    visuals.faint_bg_color = palette.panel_bg;
    visuals.extreme_bg_color = palette.input_bg;

    // Widget visuals for inactive, hovered, and active states
    visuals.widgets.noninteractive.bg_fill = palette.panel_bg;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, palette.fg);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, palette.border);

    visuals.widgets.inactive.bg_fill = palette.input_bg;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, palette.fg);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, palette.border);

    visuals.widgets.hovered.bg_fill = palette.accent;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, palette.fg);

    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, palette.fg);

    visuals.override_text_color = Some(palette.fg);
    visuals.selection.bg_fill = palette.accent.linear_multiply(0.4);
    visuals.selection.stroke = egui::Stroke::new(1.0, palette.accent);

    ctx.set_visuals(visuals);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_for_each_time_of_day() {
        let times = [
            TimeOfDay::Dawn,
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
            TimeOfDay::Night,
            TimeOfDay::Midnight,
        ];
        for tod in &times {
            let palette = gui_palette_for_time(tod);
            // Verify all fields are populated (non-transparent)
            assert_ne!(palette.bg, egui::Color32::TRANSPARENT);
            assert_ne!(palette.fg, egui::Color32::TRANSPARENT);
            assert_ne!(palette.accent, egui::Color32::TRANSPARENT);
            assert_ne!(palette.panel_bg, egui::Color32::TRANSPARENT);
            assert_ne!(palette.input_bg, egui::Color32::TRANSPARENT);
            assert_ne!(palette.border, egui::Color32::TRANSPARENT);
            assert_ne!(palette.muted, egui::Color32::TRANSPARENT);
        }
    }

    #[test]
    fn test_dawn_matches_tui_spec() {
        let p = gui_palette_for_time(&TimeOfDay::Dawn);
        assert_eq!(p.bg, egui::Color32::from_rgb(255, 220, 180));
        assert_eq!(p.fg, egui::Color32::from_rgb(60, 40, 20));
        assert_eq!(p.accent, egui::Color32::from_rgb(200, 140, 60));
    }

    #[test]
    fn test_night_is_dark() {
        let p = gui_palette_for_time(&TimeOfDay::Night);
        // Background should be dark (low RGB values)
        let [r, g, b, _] = p.bg.to_array();
        assert!(r < 50 && g < 50 && b < 80);
    }

    #[test]
    fn test_midnight_is_darkest() {
        let night = gui_palette_for_time(&TimeOfDay::Night);
        let midnight = gui_palette_for_time(&TimeOfDay::Midnight);
        let [nr, ng, nb, _] = night.bg.to_array();
        let [mr, mg, mb, _] = midnight.bg.to_array();
        assert!(mr <= nr && mg <= ng && mb <= nb);
    }

    #[test]
    fn test_day_palettes_have_light_bg() {
        for tod in &[TimeOfDay::Morning, TimeOfDay::Midday, TimeOfDay::Afternoon] {
            let p = gui_palette_for_time(tod);
            let [r, g, _b, _] = p.bg.to_array();
            assert!(r > 200 && g > 200, "Day palette bg should be light: {tod}");
        }
    }

    #[test]
    fn test_gui_palette_smooth_returns_valid() {
        use crate::world::Weather;
        use crate::world::time::Season;

        let p = gui_palette_smooth(12, 0, Season::Summer, Weather::Clear);
        assert_ne!(p.bg, egui::Color32::TRANSPARENT);
        assert_ne!(p.fg, egui::Color32::TRANSPARENT);
    }

    #[test]
    fn test_gui_palette_smooth_storm_darkens() {
        use crate::world::Weather;
        use crate::world::time::Season;

        let clear = gui_palette_smooth(12, 0, Season::Summer, Weather::Clear);
        let storm = gui_palette_smooth(12, 0, Season::Summer, Weather::Storm);
        let [cr, cg, cb, _] = clear.bg.to_array();
        let [sr, sg, sb, _] = storm.bg.to_array();
        let clear_lum = cr as f32 * 0.299 + cg as f32 * 0.587 + cb as f32 * 0.114;
        let storm_lum = sr as f32 * 0.299 + sg as f32 * 0.587 + sb as f32 * 0.114;
        assert!(storm_lum < clear_lum, "Storm should darken the palette");
    }

    #[test]
    fn test_panel_bg_differs_from_main_bg() {
        for tod in &[TimeOfDay::Dawn, TimeOfDay::Night] {
            let p = gui_palette_for_time(tod);
            assert_ne!(p.bg, p.panel_bg, "panel_bg should differ from bg for {tod}");
        }
    }
}
