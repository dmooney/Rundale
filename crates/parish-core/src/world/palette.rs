//! Smooth color palette interpolation engine.
//!
//! Provides backend-agnostic RGB palette computation that smoothly
//! interpolates between time-of-day keyframes and applies seasonal
//! and weather tinting. UI renderers consume [`RawPalette`] values from this module.

use super::Weather;
use super::time::Season;

/// A backend-agnostic RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawColor {
    /// Red channel (0–255).
    pub r: u8,
    /// Green channel (0–255).
    pub g: u8,
    /// Blue channel (0–255).
    pub b: u8,
}

impl RawColor {
    /// Creates a new `RawColor` from RGB values.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// A backend-agnostic color palette with 7 semantic color slots.
///
/// Mirrors [`crate::gui::theme::GuiPalette`] but uses [`RawColor`]
/// instead of egui types, so it can be shared between renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawPalette {
    /// Main background color.
    pub bg: RawColor,
    /// Foreground (text) color.
    pub fg: RawColor,
    /// Accent color for status bar and highlights.
    pub accent: RawColor,
    /// Slightly offset background for panels.
    pub panel_bg: RawColor,
    /// Background for the text input field.
    pub input_bg: RawColor,
    /// Border/separator color.
    pub border: RawColor,
    /// Muted text color for secondary information.
    pub muted: RawColor,
}

/// A keyframe: an anchor hour and its associated palette.
struct Keyframe {
    hour: f32,
    palette: RawPalette,
}

/// The 7 time-of-day keyframes, ordered by anchor hour.
///
/// Anchor hours are the midpoints of each [`TimeOfDay`] range.
/// RGB values are identical to the original discrete palettes.
const KEYFRAMES: [Keyframe; 7] = [
    // Midnight (23:00–4:59) → midpoint wraps; use 1.5
    Keyframe {
        hour: 1.5,
        palette: RawPalette {
            bg: RawColor::new(10, 12, 20),
            fg: RawColor::new(150, 150, 165),
            accent: RawColor::new(70, 75, 100),
            panel_bg: RawColor::new(15, 18, 28),
            input_bg: RawColor::new(20, 24, 36),
            border: RawColor::new(45, 48, 65),
            muted: RawColor::new(100, 100, 115),
        },
    },
    // Dawn (5:00–6:59) → midpoint 5.5
    Keyframe {
        hour: 5.5,
        palette: RawPalette {
            bg: RawColor::new(255, 220, 180),
            fg: RawColor::new(60, 40, 20),
            accent: RawColor::new(200, 140, 60),
            panel_bg: RawColor::new(250, 215, 175),
            input_bg: RawColor::new(245, 210, 170),
            border: RawColor::new(200, 170, 130),
            muted: RawColor::new(120, 100, 70),
        },
    },
    // Morning (7:00–9:59) → midpoint 8.0 (use 8.5 for center of 7-9)
    Keyframe {
        hour: 8.5,
        palette: RawPalette {
            bg: RawColor::new(255, 245, 220),
            fg: RawColor::new(50, 35, 15),
            accent: RawColor::new(180, 130, 50),
            panel_bg: RawColor::new(250, 240, 215),
            input_bg: RawColor::new(245, 235, 210),
            border: RawColor::new(210, 190, 150),
            muted: RawColor::new(120, 100, 60),
        },
    },
    // Midday (10:00–13:59) → midpoint 12.0
    Keyframe {
        hour: 12.0,
        palette: RawPalette {
            bg: RawColor::new(255, 255, 240),
            fg: RawColor::new(40, 30, 10),
            accent: RawColor::new(160, 120, 40),
            panel_bg: RawColor::new(250, 250, 235),
            input_bg: RawColor::new(245, 245, 230),
            border: RawColor::new(210, 200, 170),
            muted: RawColor::new(110, 100, 60),
        },
    },
    // Afternoon (14:00–16:59) → midpoint 15.5
    Keyframe {
        hour: 15.5,
        palette: RawPalette {
            bg: RawColor::new(240, 220, 170),
            fg: RawColor::new(50, 35, 15),
            accent: RawColor::new(180, 130, 50),
            panel_bg: RawColor::new(235, 215, 165),
            input_bg: RawColor::new(230, 210, 160),
            border: RawColor::new(200, 180, 130),
            muted: RawColor::new(120, 100, 60),
        },
    },
    // Dusk (17:00–18:59) → midpoint 18.0
    Keyframe {
        hour: 18.0,
        palette: RawPalette {
            bg: RawColor::new(60, 70, 110),
            fg: RawColor::new(220, 210, 190),
            accent: RawColor::new(200, 160, 80),
            panel_bg: RawColor::new(55, 65, 100),
            input_bg: RawColor::new(50, 60, 95),
            border: RawColor::new(90, 100, 140),
            muted: RawColor::new(160, 150, 140),
        },
    },
    // Night (19:00–22:59) → midpoint 21.0
    Keyframe {
        hour: 21.0,
        palette: RawPalette {
            bg: RawColor::new(20, 25, 40),
            fg: RawColor::new(180, 180, 190),
            accent: RawColor::new(100, 110, 140),
            panel_bg: RawColor::new(25, 30, 48),
            input_bg: RawColor::new(30, 35, 55),
            border: RawColor::new(60, 65, 90),
            muted: RawColor::new(120, 120, 135),
        },
    },
];

/// Linearly interpolates a single byte channel.
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let v = a as f32 + (b as f32 - a as f32) * t;
    v.round().clamp(0.0, 255.0) as u8
}

/// Linearly interpolates between two colors.
fn lerp_color(a: RawColor, b: RawColor, t: f32) -> RawColor {
    RawColor {
        r: lerp_u8(a.r, b.r, t),
        g: lerp_u8(a.g, b.g, t),
        b: lerp_u8(a.b, b.b, t),
    }
}

/// Linearly interpolates between two palettes (all 7 color fields).
fn lerp_palette(a: &RawPalette, b: &RawPalette, t: f32) -> RawPalette {
    RawPalette {
        bg: lerp_color(a.bg, b.bg, t),
        fg: lerp_color(a.fg, b.fg, t),
        accent: lerp_color(a.accent, b.accent, t),
        panel_bg: lerp_color(a.panel_bg, b.panel_bg, t),
        input_bg: lerp_color(a.input_bg, b.input_bg, t),
        border: lerp_color(a.border, b.border, t),
        muted: lerp_color(a.muted, b.muted, t),
    }
}

/// Computes the smoothly interpolated time-of-day palette.
///
/// Uses linear interpolation between the two nearest keyframe palettes
/// based on the exact fractional hour. Handles the circular midnight
/// wrap-around correctly.
fn interpolated_palette(hour: u32, minute: u32) -> RawPalette {
    let frac = hour as f32 + minute as f32 / 60.0;

    // Find the two bounding keyframes on the circular timeline.
    // KEYFRAMES is sorted by anchor hour: 1.5, 5.5, 8.5, 12.0, 15.5, 18.0, 21.0
    let n = KEYFRAMES.len(); // 7

    // Check if we're in the wrap-around segment: Night(21.0) → Midnight(1.5+24=25.5)
    let last = &KEYFRAMES[n - 1]; // Night at 21.0
    let first = &KEYFRAMES[0]; // Midnight at 1.5

    // Wrap-around span: from 21.0 to 25.5 (i.e., 21.0→24.0→1.5)
    let wrap_span = (24.0 - last.hour) + first.hour; // 3.0 + 1.5 = 4.5

    if frac >= last.hour {
        // Between Night(21.0) and Midnight(25.5), current hour is 21.0..24.0
        let t = (frac - last.hour) / wrap_span;
        return lerp_palette(&last.palette, &first.palette, t);
    }

    if frac < first.hour {
        // Between Night(21.0) and Midnight(1.5), current hour is 0.0..1.5
        let elapsed = (24.0 - last.hour) + frac;
        let t = elapsed / wrap_span;
        return lerp_palette(&last.palette, &first.palette, t);
    }

    // Normal case: find adjacent keyframes where KEYFRAMES[i].hour <= frac < KEYFRAMES[i+1].hour
    for i in 0..n - 1 {
        let from = &KEYFRAMES[i];
        let to = &KEYFRAMES[i + 1];
        if frac >= from.hour && frac < to.hour {
            let t = (frac - from.hour) / (to.hour - from.hour);
            return lerp_palette(&from.palette, &to.palette, t);
        }
    }

    // Fallback (shouldn't be reached)
    KEYFRAMES[0].palette
}

/// Season tint parameters: (r_mult, g_mult, b_mult, desaturation).
fn season_tint(season: Season) -> (f32, f32, f32, f32) {
    match season {
        Season::Spring => (0.98, 1.02, 0.98, 0.0),
        Season::Summer => (1.03, 1.01, 0.97, 0.0),
        Season::Autumn => (1.06, 1.00, 0.92, 0.0),
        Season::Winter => (0.94, 0.96, 1.04, 0.08),
    }
}

/// Weather tint parameters: (r_mult, g_mult, b_mult, desaturation, brightness, contrast_reduction).
fn weather_tint(weather: Weather) -> (f32, f32, f32, f32, f32, f32) {
    match weather {
        Weather::Clear => (1.0, 1.0, 1.0, 0.0, 1.0, 0.0),
        Weather::Overcast => (0.95, 0.95, 0.97, 0.15, 0.92, 0.0),
        Weather::Rain => (0.88, 0.90, 0.95, 0.20, 0.85, 0.0),
        Weather::Fog => (0.97, 0.97, 0.98, 0.35, 0.95, 0.15),
        Weather::Storm => (0.80, 0.82, 0.85, 0.30, 0.75, 0.0),
    }
}

/// Computes the luminance of an RGB color (ITU-R BT.601).
fn luminance(c: RawColor) -> f32 {
    0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32
}

/// Applies a multiplicative tint, desaturation, and brightness to a single color.
fn tint_color(
    c: RawColor,
    r_mult: f32,
    g_mult: f32,
    b_mult: f32,
    desat: f32,
    bright: f32,
) -> RawColor {
    let lum = luminance(c);

    // Desaturate: blend toward gray (luminance)
    let r = c.r as f32 + (lum - c.r as f32) * desat;
    let g = c.g as f32 + (lum - c.g as f32) * desat;
    let b = c.b as f32 + (lum - c.b as f32) * desat;

    // Apply color multiplier and brightness
    let r = (r * r_mult * bright).round().clamp(0.0, 255.0) as u8;
    let g = (g * g_mult * bright).round().clamp(0.0, 255.0) as u8;
    let b = (b * b_mult * bright).round().clamp(0.0, 255.0) as u8;

    RawColor::new(r, g, b)
}

/// Applies season tinting to all palette colors.
fn apply_season_tint(palette: &mut RawPalette, season: Season) {
    let (rm, gm, bm, desat) = season_tint(season);
    palette.bg = tint_color(palette.bg, rm, gm, bm, desat, 1.0);
    palette.fg = tint_color(palette.fg, rm, gm, bm, desat, 1.0);
    palette.accent = tint_color(palette.accent, rm, gm, bm, desat, 1.0);
    palette.panel_bg = tint_color(palette.panel_bg, rm, gm, bm, desat, 1.0);
    palette.input_bg = tint_color(palette.input_bg, rm, gm, bm, desat, 1.0);
    palette.border = tint_color(palette.border, rm, gm, bm, desat, 1.0);
    palette.muted = tint_color(palette.muted, rm, gm, bm, desat, 1.0);
}

/// Applies weather tinting to all palette colors.
///
/// For fog, also reduces contrast by blending fg toward bg.
fn apply_weather_tint(palette: &mut RawPalette, weather: Weather) {
    let (rm, gm, bm, desat, bright, contrast_reduction) = weather_tint(weather);
    palette.bg = tint_color(palette.bg, rm, gm, bm, desat, bright);
    palette.fg = tint_color(palette.fg, rm, gm, bm, desat, bright);
    palette.accent = tint_color(palette.accent, rm, gm, bm, desat, bright);
    palette.panel_bg = tint_color(palette.panel_bg, rm, gm, bm, desat, bright);
    palette.input_bg = tint_color(palette.input_bg, rm, gm, bm, desat, bright);
    palette.border = tint_color(palette.border, rm, gm, bm, desat, bright);
    palette.muted = tint_color(palette.muted, rm, gm, bm, desat, bright);

    // Fog contrast reduction: blend fg toward bg
    if contrast_reduction > 0.0 {
        palette.fg = lerp_color(palette.fg, palette.bg, contrast_reduction);
        palette.muted = lerp_color(palette.muted, palette.bg, contrast_reduction * 0.5);
    }
}

/// Minimum luminance difference between fg and bg to ensure readability.
///
/// During transitions between light-bg/dark-fg and dark-bg/light-fg palettes
/// (e.g. Afternoon→Dusk around 16:00–17:00), linear interpolation causes both
/// fg and bg to converge to similar medium tones. This floor prevents that
/// contrast collapse.
const MIN_FG_BG_CONTRAST: f32 = 80.0;

/// Minimum luminance difference between muted text and bg.
const MIN_MUTED_BG_CONTRAST: f32 = 45.0;

/// Pushes a foreground color away from a background color to meet a minimum
/// luminance contrast. Preserves the hue by scaling all channels proportionally.
fn ensure_color_contrast(fg: RawColor, bg: RawColor, min_contrast: f32) -> RawColor {
    let fg_lum = luminance(fg);
    let bg_lum = luminance(bg);
    let contrast = (fg_lum - bg_lum).abs();

    if contrast >= min_contrast {
        return fg;
    }

    // Determine direction: fg should go lighter if bg is dark, darker if bg is light.
    let bg_is_dark = bg_lum < 128.0;
    let target_lum = if bg_is_dark {
        bg_lum + min_contrast
    } else {
        bg_lum - min_contrast
    };

    // Scale fg channels to hit target luminance, preserving relative proportions.
    if fg_lum < 1.0 {
        // fg is near-black; just return a gray at target luminance
        let v = target_lum.round().clamp(0.0, 255.0) as u8;
        return RawColor::new(v, v, v);
    }

    let scale = target_lum / fg_lum;
    let r = (fg.r as f32 * scale).round().clamp(0.0, 255.0) as u8;
    let g = (fg.g as f32 * scale).round().clamp(0.0, 255.0) as u8;
    let b = (fg.b as f32 * scale).round().clamp(0.0, 255.0) as u8;
    RawColor::new(r, g, b)
}

/// Ensures all text colors in the palette have sufficient contrast against backgrounds.
fn ensure_contrast(palette: &mut RawPalette) {
    palette.fg = ensure_color_contrast(palette.fg, palette.bg, MIN_FG_BG_CONTRAST);
    palette.muted = ensure_color_contrast(palette.muted, palette.bg, MIN_MUTED_BG_CONTRAST);
    palette.accent = ensure_color_contrast(palette.accent, palette.bg, MIN_MUTED_BG_CONTRAST);
}

/// Computes the fully interpolated and tinted palette for the given time, season, and weather.
///
/// This is the main entry point for UI renderers.
/// 1. Smoothly interpolates between time-of-day keyframe palettes
/// 2. Applies seasonal color tinting
/// 3. Applies weather color tinting
/// 4. Enforces minimum contrast between text and background
pub fn compute_palette(hour: u32, minute: u32, season: Season, weather: Weather) -> RawPalette {
    let mut palette = interpolated_palette(hour, minute);
    apply_season_tint(&mut palette, season);
    apply_weather_tint(&mut palette, weather);
    ensure_contrast(&mut palette);
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lerp_u8_boundaries() {
        assert_eq!(lerp_u8(0, 255, 0.0), 0);
        assert_eq!(lerp_u8(0, 255, 1.0), 255);
        assert_eq!(lerp_u8(0, 255, 0.5), 128);
        assert_eq!(lerp_u8(100, 200, 0.5), 150);
    }

    #[test]
    fn test_lerp_color() {
        let a = RawColor::new(0, 0, 0);
        let b = RawColor::new(255, 255, 255);
        let mid = lerp_color(a, b, 0.5);
        assert_eq!(mid, RawColor::new(128, 128, 128));
    }

    #[test]
    fn test_keyframe_dawn_exact() {
        // At Dawn's anchor hour (5:30), should match Dawn palette exactly
        let p = interpolated_palette(5, 30);
        assert_eq!(p.bg, KEYFRAMES[1].palette.bg);
        assert_eq!(p.fg, KEYFRAMES[1].palette.fg);
        assert_eq!(p.accent, KEYFRAMES[1].palette.accent);
    }

    #[test]
    fn test_keyframe_midday_exact() {
        // At Midday's anchor hour (12:00), should match Midday palette exactly
        let p = interpolated_palette(12, 0);
        assert_eq!(p.bg, KEYFRAMES[3].palette.bg);
    }

    #[test]
    fn test_keyframe_midnight_exact() {
        // At Midnight's anchor hour (1:30), should match Midnight palette exactly
        let p = interpolated_palette(1, 30);
        assert_eq!(p.bg, KEYFRAMES[0].palette.bg);
    }

    #[test]
    fn test_keyframe_night_exact() {
        // At Night's anchor hour (21:00), should match Night palette exactly
        let p = interpolated_palette(21, 0);
        assert_eq!(p.bg, KEYFRAMES[6].palette.bg);
    }

    #[test]
    fn test_interpolation_midpoint_dawn_morning() {
        // Midpoint between Dawn(5.5) and Morning(8.5) is hour 7:00
        let p = interpolated_palette(7, 0);
        let dawn_bg = KEYFRAMES[1].palette.bg;
        let morning_bg = KEYFRAMES[2].palette.bg;
        // Should be roughly halfway between Dawn and Morning bg
        let expected_r = ((dawn_bg.r as f32 + morning_bg.r as f32) / 2.0).round() as u8;
        let expected_g = ((dawn_bg.g as f32 + morning_bg.g as f32) / 2.0).round() as u8;
        assert!((p.bg.r as i16 - expected_r as i16).unsigned_abs() <= 1);
        assert!((p.bg.g as i16 - expected_g as i16).unsigned_abs() <= 1);
    }

    #[test]
    fn test_midnight_wraparound_hour_23() {
        // Hour 23 should interpolate between Night(21.0) and Midnight(25.5)
        let p = interpolated_palette(23, 0);
        let night_bg = KEYFRAMES[6].palette.bg; // (20, 25, 40)
        let midnight_bg = KEYFRAMES[0].palette.bg; // (10, 12, 20)
        // Should be between Night and Midnight
        assert!(p.bg.r <= night_bg.r && p.bg.r >= midnight_bg.r);
        assert!(p.bg.g <= night_bg.g && p.bg.g >= midnight_bg.g);
    }

    #[test]
    fn test_midnight_wraparound_hour_0() {
        // Hour 0 should interpolate between Night and Midnight, closer to Midnight
        let p = interpolated_palette(0, 0);
        let night_bg = KEYFRAMES[6].palette.bg;
        let midnight_bg = KEYFRAMES[0].palette.bg;
        assert!(p.bg.r <= night_bg.r && p.bg.r >= midnight_bg.r);
    }

    #[test]
    fn test_every_hour_produces_valid_palette() {
        for hour in 0..24 {
            for minute in [0, 15, 30, 45] {
                let p = interpolated_palette(hour, minute);
                // Just verify no panics and colors are populated
                assert_ne!(p.bg, RawColor::new(0, 0, 0));
            }
        }
    }

    #[test]
    fn test_season_clear_compute() {
        // With Clear weather and Spring season, compute_palette should still produce valid colors
        let p = compute_palette(12, 0, Season::Spring, Weather::Clear);
        assert_ne!(p.bg, RawColor::new(0, 0, 0));
    }

    #[test]
    fn test_weather_clear_is_near_identity() {
        // Clear weather should barely change the palette (only season effect)
        let base = interpolated_palette(12, 0);
        let tinted = compute_palette(12, 0, Season::Summer, Weather::Clear);
        // Summer tint is subtle, bg should be close
        let diff_r = (base.bg.r as i16 - tinted.bg.r as i16).unsigned_abs();
        let diff_g = (base.bg.g as i16 - tinted.bg.g as i16).unsigned_abs();
        assert!(
            diff_r < 20,
            "Summer tint should be subtle, got diff_r={diff_r}"
        );
        assert!(
            diff_g < 20,
            "Summer tint should be subtle, got diff_g={diff_g}"
        );
    }

    #[test]
    fn test_winter_is_bluer() {
        let base = interpolated_palette(12, 0);
        let mut winter = base;
        apply_season_tint(&mut winter, Season::Winter);
        // Winter blue channel should be >= base (blue multiplier is 1.04)
        assert!(winter.bg.b >= base.bg.b, "Winter should tint bluer");
    }

    #[test]
    fn test_storm_is_darker() {
        let base = interpolated_palette(12, 0);
        let stormy = compute_palette(12, 0, Season::Summer, Weather::Storm);
        let base_lum = luminance(base.bg);
        let storm_lum = luminance(stormy.bg);
        assert!(
            storm_lum < base_lum,
            "Storm should darken: base_lum={base_lum}, storm_lum={storm_lum}"
        );
    }

    #[test]
    fn test_fog_reduces_contrast() {
        let clear = compute_palette(12, 0, Season::Summer, Weather::Clear);
        let foggy = compute_palette(12, 0, Season::Summer, Weather::Fog);
        let clear_contrast = (luminance(clear.fg) - luminance(clear.bg)).abs();
        let fog_contrast = (luminance(foggy.fg) - luminance(foggy.bg)).abs();
        assert!(
            fog_contrast < clear_contrast,
            "Fog should reduce contrast: clear={clear_contrast}, fog={fog_contrast}"
        );
    }

    #[test]
    fn test_all_combinations_valid() {
        let seasons = [
            Season::Spring,
            Season::Summer,
            Season::Autumn,
            Season::Winter,
        ];
        let weathers = [
            Weather::Clear,
            Weather::Overcast,
            Weather::Rain,
            Weather::Fog,
            Weather::Storm,
        ];
        for season in &seasons {
            for weather in &weathers {
                for hour in [0, 6, 12, 18, 23] {
                    let _p = compute_palette(hour, 0, *season, *weather);
                    // No panics, channels are within u8 range by construction
                }
            }
        }
    }

    #[test]
    fn test_smooth_transition_no_jumps() {
        // Walk through every 15-minute increment and verify adjacent palettes
        // don't have huge color jumps (max delta per channel < 30 per 15 min)
        let mut prev = interpolated_palette(0, 0);
        for hour in 0..24 {
            for minute in (0..60).step_by(15) {
                if hour == 0 && minute == 0 {
                    continue;
                }
                let curr = interpolated_palette(hour, minute);
                let dr = (curr.bg.r as i16 - prev.bg.r as i16).unsigned_abs();
                let dg = (curr.bg.g as i16 - prev.bg.g as i16).unsigned_abs();
                let db = (curr.bg.b as i16 - prev.bg.b as i16).unsigned_abs();
                assert!(
                    dr < 30 && dg < 30 && db < 30,
                    "Jump too large at {hour}:{minute:02}: dr={dr}, dg={dg}, db={db}"
                );
                prev = curr;
            }
        }
    }

    #[test]
    fn test_contrast_floor_afternoon_dusk_transition() {
        // The Afternoon→Dusk transition (15.5→18.0) crosses light-bg/dark-fg
        // to dark-bg/light-fg. Verify contrast never drops below the floor.
        for minute_offset in 0..150 {
            // Walk from 15:30 to 18:00 in 1-minute increments
            let total_minutes = 15 * 60 + 30 + minute_offset;
            let hour = total_minutes / 60;
            let minute = total_minutes % 60;
            let p = interpolated_palette(hour, minute);
            let mut adjusted = p;
            ensure_contrast(&mut adjusted);
            let contrast = (luminance(adjusted.fg) - luminance(adjusted.bg)).abs();
            assert!(
                contrast >= MIN_FG_BG_CONTRAST - 1.0,
                "Contrast too low at {hour}:{minute:02}: {contrast:.1} (bg={:?}, fg={:?})",
                adjusted.bg,
                adjusted.fg
            );
        }
    }

    #[test]
    fn test_contrast_floor_all_hours() {
        // Verify contrast floor holds for every 15-minute slot across the full day
        for hour in 0..24 {
            for minute in [0, 15, 30, 45] {
                let p = compute_palette(hour, minute, Season::Spring, Weather::Clear);
                let contrast = (luminance(p.fg) - luminance(p.bg)).abs();
                assert!(
                    contrast >= MIN_FG_BG_CONTRAST - 1.0,
                    "Contrast too low at {hour}:{minute:02}: {contrast:.1}"
                );
            }
        }
    }

    #[test]
    fn test_ensure_color_contrast_noop_when_sufficient() {
        let fg = RawColor::new(255, 255, 255);
        let bg = RawColor::new(0, 0, 0);
        let result = ensure_color_contrast(fg, bg, 80.0);
        assert_eq!(
            result, fg,
            "Should not modify fg when contrast is sufficient"
        );
    }

    #[test]
    fn test_ensure_color_contrast_adjusts_when_needed() {
        let fg = RawColor::new(130, 130, 130); // luminance ~130
        let bg = RawColor::new(140, 140, 140); // luminance ~140
        let result = ensure_color_contrast(fg, bg, 80.0);
        let contrast = (luminance(result) - luminance(bg)).abs();
        assert!(
            contrast >= 79.0,
            "Should push fg away from bg, got contrast {contrast:.1}"
        );
    }

    #[test]
    fn test_luminance() {
        assert!((luminance(RawColor::new(255, 255, 255)) - 255.0).abs() < 0.01);
        assert!((luminance(RawColor::new(0, 0, 0))).abs() < 0.01);
    }

    #[test]
    fn test_tint_color_identity() {
        let c = RawColor::new(100, 150, 200);
        let tinted = tint_color(c, 1.0, 1.0, 1.0, 0.0, 1.0);
        assert_eq!(tinted, c);
    }

    #[test]
    fn test_autumn_is_warmer() {
        let base = interpolated_palette(12, 0);
        let mut autumn = base;
        apply_season_tint(&mut autumn, Season::Autumn);
        // Autumn red multiplier is 1.06, should increase red
        assert!(autumn.bg.r >= base.bg.r, "Autumn should tint warmer/redder");
    }
}
