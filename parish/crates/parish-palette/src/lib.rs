//! Smooth color palette interpolation engine.
//!
//! Provides backend-agnostic RGB palette computation that smoothly
//! interpolates between caller-supplied time-of-day keyframes and enforces
//! a minimum foreground/background contrast floor. UI renderers consume
//! [`RawPalette`] values from this module.
//!
//! The engine ships no built-in keyframes — mods supply them via
//! `ui.toml`'s `[[theme.keyframes]]` array. When no mod is loaded, callers
//! should use [`neutral_grey_palette`] so the boot UI still renders.

use parish_config::PaletteConfig;
use parish_types::ThemePalette;

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

/// Parses a `#RRGGBB` (or `RRGGBB`) hex string into a [`RawColor`].
///
/// Returns `None` for malformed input. Whitespace and a leading `#` are
/// tolerated.
pub fn parse_hex_color(s: &str) -> Option<RawColor> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(RawColor::new(r, g, b))
}

/// A backend-agnostic color palette with 7 semantic color slots.
///
/// Backend-agnostic palette mirroring `parish_core::ipc::types::ThemePalette`,
/// using [`RawColor`] instead of egui types so it can be shared between renderers.
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
///
/// Mods provide a sorted list of keyframes via their `ui.toml`'s
/// `[[theme.keyframes]]` array. Hours are floating-point so anchors can sit
/// at midpoints of time-of-day periods (e.g. `8.5` for morning).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    /// Anchor hour in [0.0, 24.0).
    pub hour: f32,
    /// Palette at this anchor hour.
    pub palette: RawPalette,
}

/// Returns a flat neutral-grey palette used when no mod is loaded.
///
/// Sufficient contrast for legible UI text so the "select a base mod"
/// prompt renders before any mod-supplied keyframes exist.
pub fn neutral_grey_palette() -> RawPalette {
    RawPalette {
        bg: RawColor::new(24, 24, 26),
        fg: RawColor::new(220, 220, 224),
        accent: RawColor::new(140, 140, 150),
        panel_bg: RawColor::new(32, 32, 36),
        input_bg: RawColor::new(40, 40, 44),
        border: RawColor::new(72, 72, 80),
        muted: RawColor::new(150, 150, 158),
    }
}

/// Converts an f32 to a u8, clamping to [0, 255] and rounding to nearest.
fn f32_to_u8_clamped(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Linearly interpolates a single byte channel.
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let v = a as f32 + (b as f32 - a as f32) * t;
    f32_to_u8_clamped(v)
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

/// Computes the smoothly interpolated time-of-day palette from a
/// caller-supplied keyframe set.
///
/// Uses linear interpolation between the two nearest keyframes. Handles the
/// circular midnight wrap-around. Returns [`neutral_grey_palette`] when the
/// slice is empty.
fn interpolated_palette_with(hour: u32, minute: u32, keyframes: &[Keyframe]) -> RawPalette {
    if keyframes.is_empty() {
        return neutral_grey_palette();
    }
    if keyframes.len() == 1 {
        return keyframes[0].palette;
    }

    let frac = hour as f32 + minute as f32 / 60.0;
    let n = keyframes.len();
    let first = &keyframes[0];
    let last = &keyframes[n - 1];

    // Wrap-around span: from last keyframe → 24.0 → first keyframe
    let wrap_span = (24.0 - last.hour) + first.hour;

    if frac >= last.hour {
        let t = (frac - last.hour) / wrap_span;
        return lerp_palette(&last.palette, &first.palette, t);
    }
    if frac < first.hour {
        let elapsed = (24.0 - last.hour) + frac;
        let t = elapsed / wrap_span;
        return lerp_palette(&last.palette, &first.palette, t);
    }

    for i in 0..n - 1 {
        let from = &keyframes[i];
        let to = &keyframes[i + 1];
        if frac >= from.hour && frac < to.hour {
            let t = (frac - from.hour) / (to.hour - from.hour);
            return lerp_palette(&from.palette, &to.palette, t);
        }
    }

    unreachable!("frac={frac} should always be covered by a keyframe range")
}

/// Computes the luminance of an RGB color (ITU-R BT.601).
fn luminance(c: RawColor) -> f32 {
    0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32
}

/// Pushes a foreground color away from a background color to meet a minimum
/// luminance contrast. Preserves the hue by scaling all channels proportionally.
fn ensure_color_contrast(fg: RawColor, bg: RawColor, min_contrast: f32) -> RawColor {
    let fg_lum = luminance(fg);
    let bg_lum = luminance(bg);
    let contrast = (fg_lum - bg_lum).abs();

    if contrast >= min_contrast {
        return fg;
    }

    let bg_is_dark = bg_lum < 128.0;
    let target_lum = if bg_is_dark {
        bg_lum + min_contrast
    } else {
        bg_lum - min_contrast
    };

    if fg_lum < 1.0 {
        let v = f32_to_u8_clamped(target_lum);
        return RawColor::new(v, v, v);
    }

    let scale = target_lum / fg_lum;
    let r = f32_to_u8_clamped(fg.r as f32 * scale);
    let g = f32_to_u8_clamped(fg.g as f32 * scale);
    let b = f32_to_u8_clamped(fg.b as f32 * scale);
    RawColor::new(r, g, b)
}

/// Ensures fg/muted/accent meet contrast thresholds against bg.
fn ensure_contrast_with_config(palette: &mut RawPalette, config: &PaletteConfig) {
    palette.fg = ensure_color_contrast(palette.fg, palette.bg, config.min_fg_bg_contrast);
    palette.muted = ensure_color_contrast(palette.muted, palette.bg, config.min_muted_bg_contrast);
    palette.accent =
        ensure_color_contrast(palette.accent, palette.bg, config.min_muted_bg_contrast);
}

/// Computes the interpolated palette using caller-supplied keyframes plus
/// the contrast thresholds from `config`.
///
/// This is the main entry point for UI renderers.
pub fn compute_palette_with_keyframes(
    hour: u32,
    minute: u32,
    keyframes: &[Keyframe],
    config: &PaletteConfig,
) -> RawPalette {
    let total_minutes = hour as u64 * 60 + minute as u64;
    let normalized_hour = ((total_minutes / 60) % 24) as u32;
    let normalized_minute = (total_minutes % 60) as u32;
    let mut palette = interpolated_palette_with(normalized_hour, normalized_minute, keyframes);
    ensure_contrast_with_config(&mut palette, config);
    palette
}

/// Converts an interpolated byte-RGB [`RawPalette`] into the CSS hex-string
/// [`ThemePalette`] consumed by the frontend.
///
/// The `ThemePalette` type itself lives in `parish-types` (the zero-dep leaf)
/// so the IPC layer and the mod loader can name it without depending on this
/// crate; the conversion lives here because `RawPalette` is local, satisfying
/// the orphan rule.
impl From<RawPalette> for ThemePalette {
    fn from(raw: RawPalette) -> Self {
        let hex = |c: RawColor| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
        ThemePalette {
            bg: hex(raw.bg),
            fg: hex(raw.fg),
            accent: hex(raw.accent),
            panel_bg: hex(raw.panel_bg),
            input_bg: hex(raw.input_bg),
            border: hex(raw.border),
            muted: hex(raw.muted),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_palette_from_raw_palette() {
        let raw = RawPalette {
            bg: RawColor::new(10, 20, 30),
            fg: RawColor::new(200, 210, 220),
            accent: RawColor::new(255, 128, 0),
            panel_bg: RawColor::new(15, 25, 35),
            input_bg: RawColor::new(20, 30, 40),
            border: RawColor::new(50, 60, 70),
            muted: RawColor::new(100, 110, 120),
        };
        let palette = ThemePalette::from(raw);
        assert_eq!(palette.bg, "#0a141e");
        assert_eq!(palette.fg, "#c8d2dc");
        assert_eq!(palette.accent, "#ff8000");
    }

    #[test]
    fn theme_palette_serde_shape_is_seven_hex_keys() {
        // Serde shape contract for the TS interface `types.ts:88`: the JSON
        // object must carry exactly these seven keys.
        let palette = ThemePalette {
            bg: "#000000".to_string(),
            fg: "#ffffff".to_string(),
            accent: "#ff8000".to_string(),
            panel_bg: "#111111".to_string(),
            input_bg: "#222222".to_string(),
            border: "#333333".to_string(),
            muted: "#444444".to_string(),
        };
        let value =
            serde_json::to_value(&palette).expect("ThemePalette should serialize to a value");
        let obj = value
            .as_object()
            .expect("serialized ThemePalette should be a JSON object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "accent", "bg", "border", "fg", "input_bg", "muted", "panel_bg"
            ]
        );
    }

    /// Test fixture: parchment time-of-day keyframes (the former engine
    /// default; Rundale now ships an equivalent set in `mods/rundale/ui.toml`).
    /// These exist only to keep the interpolation / contrast algorithms under
    /// regression test in this crate.
    const PARCHMENT_KEYFRAMES: [Keyframe; 7] = [
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

    const MIN_FG_BG_CONTRAST: f32 = 80.0;

    fn interpolated_palette(hour: u32, minute: u32) -> RawPalette {
        interpolated_palette_with(hour, minute, &PARCHMENT_KEYFRAMES)
    }

    fn compute_palette(hour: u32, minute: u32) -> RawPalette {
        compute_palette_with_keyframes(
            hour,
            minute,
            &PARCHMENT_KEYFRAMES,
            &PaletteConfig::default(),
        )
    }

    fn ensure_contrast(palette: &mut RawPalette) {
        ensure_contrast_with_config(palette, &PaletteConfig::default());
    }

    fn assert_color(actual: RawColor, expected: (u8, u8, u8)) {
        assert_eq!(actual, RawColor::new(expected.0, expected.1, expected.2));
    }

    fn assert_documented_keyframe(
        hour: u32,
        minute: u32,
        bg: (u8, u8, u8),
        fg: (u8, u8, u8),
        accent: (u8, u8, u8),
    ) {
        let p = interpolated_palette(hour, minute);
        assert_color(p.bg, bg);
        assert_color(p.fg, fg);
        assert_color(p.accent, accent);
    }

    #[test]
    fn test_parse_hex_color_basic() {
        assert_eq!(
            parse_hex_color("#ffffff"),
            Some(RawColor::new(255, 255, 255))
        );
        assert_eq!(parse_hex_color("000000"), Some(RawColor::new(0, 0, 0)));
        assert_eq!(parse_hex_color("#0a1929"), Some(RawColor::new(10, 25, 41)));
        assert_eq!(parse_hex_color("#00D4FF"), Some(RawColor::new(0, 212, 255)));
    }

    #[test]
    fn test_parse_hex_color_rejects_bad_input() {
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("#fff"), None);
        assert_eq!(parse_hex_color("#fffffff"), None);
        assert_eq!(parse_hex_color("#gg0000"), None);
    }

    #[test]
    fn test_neutral_grey_palette_is_legible() {
        let p = neutral_grey_palette();
        let contrast = (luminance(p.fg) - luminance(p.bg)).abs();
        assert!(
            contrast >= 80.0,
            "neutral fg/bg contrast too low: {contrast}"
        );
    }

    #[test]
    fn test_interpolated_palette_empty_keyframes_falls_back_to_grey() {
        let p = interpolated_palette_with(8, 30, &[]);
        assert_eq!(p, neutral_grey_palette());
    }

    #[test]
    fn test_interpolated_palette_single_keyframe_returns_it() {
        let kf = Keyframe {
            hour: 12.0,
            palette: RawPalette {
                bg: RawColor::new(10, 20, 30),
                fg: RawColor::new(200, 210, 220),
                accent: RawColor::new(100, 110, 120),
                panel_bg: RawColor::new(15, 25, 35),
                input_bg: RawColor::new(20, 30, 40),
                border: RawColor::new(50, 60, 70),
                muted: RawColor::new(150, 160, 170),
            },
        };
        let p = interpolated_palette_with(8, 0, std::slice::from_ref(&kf));
        assert_eq!(p, kf.palette);
    }

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
        let p = interpolated_palette(5, 30);
        assert_eq!(p.bg, PARCHMENT_KEYFRAMES[1].palette.bg);
        assert_eq!(p.fg, PARCHMENT_KEYFRAMES[1].palette.fg);
        assert_eq!(p.accent, PARCHMENT_KEYFRAMES[1].palette.accent);
    }

    #[test]
    fn test_keyframe_midday_exact() {
        let p = interpolated_palette(12, 0);
        assert_eq!(p.bg, PARCHMENT_KEYFRAMES[3].palette.bg);
    }

    #[test]
    fn test_keyframe_midnight_exact() {
        let p = interpolated_palette(1, 30);
        assert_eq!(p.bg, PARCHMENT_KEYFRAMES[0].palette.bg);
    }

    #[test]
    fn test_keyframe_night_exact() {
        let p = interpolated_palette(21, 0);
        assert_eq!(p.bg, PARCHMENT_KEYFRAMES[6].palette.bg);
    }

    #[test]
    fn test_keyframe_morning_exact() {
        let p = interpolated_palette(8, 30);
        assert_eq!(p.bg, PARCHMENT_KEYFRAMES[2].palette.bg);
    }

    #[test]
    fn test_keyframe_afternoon_exact() {
        let p = interpolated_palette(15, 30);
        assert_eq!(p.bg, PARCHMENT_KEYFRAMES[4].palette.bg);
    }

    #[test]
    fn test_keyframe_dusk_exact() {
        let p = interpolated_palette(18, 0);
        assert_eq!(p.bg, PARCHMENT_KEYFRAMES[5].palette.bg);
    }

    #[test]
    fn test_keyframe_values_match_design_golden_table() {
        assert_documented_keyframe(5, 30, (255, 220, 180), (60, 40, 20), (200, 140, 60));
        assert_documented_keyframe(8, 30, (255, 245, 220), (50, 35, 15), (180, 130, 50));
        assert_documented_keyframe(12, 0, (255, 255, 240), (40, 30, 10), (160, 120, 40));
        assert_documented_keyframe(15, 30, (240, 220, 170), (50, 35, 15), (180, 130, 50));
        assert_documented_keyframe(18, 0, (60, 70, 110), (220, 210, 190), (200, 160, 80));
        assert_documented_keyframe(21, 0, (20, 25, 40), (180, 180, 190), (100, 110, 140));
        assert_documented_keyframe(1, 30, (10, 12, 20), (150, 150, 165), (70, 75, 100));
    }

    #[test]
    fn test_interpolation_midpoint_dawn_morning() {
        let p = interpolated_palette(7, 0);
        let a = PARCHMENT_KEYFRAMES[1].palette.bg;
        let b = PARCHMENT_KEYFRAMES[2].palette.bg;
        let expected_r = ((a.r as f32 + b.r as f32) / 2.0).round() as u8;
        let expected_g = ((a.g as f32 + b.g as f32) / 2.0).round() as u8;
        assert!((p.bg.r as i16 - expected_r as i16).unsigned_abs() <= 1);
        assert!((p.bg.g as i16 - expected_g as i16).unsigned_abs() <= 1);
    }

    #[test]
    fn test_midnight_wraparound_hour_23() {
        let p = interpolated_palette(23, 0);
        let night_bg = PARCHMENT_KEYFRAMES[6].palette.bg;
        let midnight_bg = PARCHMENT_KEYFRAMES[0].palette.bg;
        assert!(p.bg.r <= night_bg.r && p.bg.r >= midnight_bg.r);
        assert!(p.bg.g <= night_bg.g && p.bg.g >= midnight_bg.g);
    }

    #[test]
    fn test_midnight_wraparound_hour_0() {
        let p = interpolated_palette(0, 0);
        let night_bg = PARCHMENT_KEYFRAMES[6].palette.bg;
        let midnight_bg = PARCHMENT_KEYFRAMES[0].palette.bg;
        assert!(p.bg.r <= night_bg.r && p.bg.r >= midnight_bg.r);
    }

    #[test]
    fn test_every_hour_produces_valid_palette() {
        for hour in 0..24 {
            for minute in [0, 15, 30, 45] {
                let p = interpolated_palette(hour, minute);
                assert_ne!(p.bg, RawColor::new(0, 0, 0));
                assert_ne!(p.fg, RawColor::new(0, 0, 0));
                assert_ne!(p.accent, RawColor::new(0, 0, 0));
                assert_ne!(p.fg, p.bg);
            }
        }
    }

    #[test]
    fn test_compute_palette_all_hours_valid() {
        for hour in [0, 6, 12, 18, 23] {
            for minute in [0, 15, 30, 45] {
                let p = compute_palette(hour, minute);
                assert_ne!(p.bg, RawColor::new(0, 0, 0));
                let contrast = (luminance(p.fg) - luminance(p.bg)).abs();
                assert!(
                    contrast >= MIN_FG_BG_CONTRAST - 1.0,
                    "Contrast too low at {hour}:{minute:02}: {contrast:.1}"
                );
            }
        }
    }

    #[test]
    fn test_smooth_transition_no_jumps() {
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
        for minute_offset in 0..150 {
            let total_minutes = 15 * 60 + 30 + minute_offset;
            let hour = total_minutes / 60;
            let minute = total_minutes % 60;
            let p = interpolated_palette(hour, minute);
            let mut adjusted = p;
            ensure_contrast(&mut adjusted);
            let contrast = (luminance(adjusted.fg) - luminance(adjusted.bg)).abs();
            assert!(
                contrast >= MIN_FG_BG_CONTRAST - 1.0,
                "Contrast too low at {hour}:{minute:02}: {contrast:.1}"
            );
        }
    }

    #[test]
    fn test_contrast_floor_all_hours() {
        for hour in 0..24 {
            for minute in [0, 15, 30, 45] {
                let p = compute_palette(hour, minute);
                let contrast = (luminance(p.fg) - luminance(p.bg)).abs();
                assert!(
                    contrast >= MIN_FG_BG_CONTRAST - 1.0,
                    "Contrast too low at {hour}:{minute:02}: {contrast:.1}"
                );
            }
        }
    }

    #[test]
    fn test_muted_and_accent_contrast_floor_all_hours() {
        let config = PaletteConfig::default();
        for hour in 0..24 {
            for minute in [0, 15, 30, 45] {
                let p = compute_palette(hour, minute);
                let muted_contrast = (luminance(p.muted) - luminance(p.bg)).abs();
                let accent_contrast = (luminance(p.accent) - luminance(p.bg)).abs();
                assert!(
                    muted_contrast >= config.min_muted_bg_contrast - 1.0,
                    "Muted contrast too low at {hour}:{minute:02}: {muted_contrast:.1}"
                );
                assert!(
                    accent_contrast >= config.min_muted_bg_contrast - 1.0,
                    "Accent contrast too low at {hour}:{minute:02}: {accent_contrast:.1}"
                );
            }
        }
    }

    #[test]
    fn test_compute_palette_normalizes_out_of_range_time() {
        assert_eq!(compute_palette(24, 0), compute_palette(0, 0));
        assert_eq!(compute_palette(48, 0), compute_palette(0, 0));
        assert_eq!(compute_palette(23, 75), compute_palette(0, 15));
        assert_eq!(compute_palette(50, 125), compute_palette(4, 5));
    }
}
