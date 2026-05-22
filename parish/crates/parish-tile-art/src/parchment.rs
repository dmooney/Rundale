//! Deterministic CPU-only parchment art pipeline.
//!
//! Converts a raw NLS historic OS Ireland tile (typically cream/sepia line-art)
//! into a warm illustrated parchment style:
//!
//! 1. Box-blur the input to reduce scan/compression noise (optional).
//! 2. Sobel edge-extraction on the luma channel → ink-stroke mask.
//! 3. Per-pixel colour remap toward the Rundale parchment palette.
//! 4. Multiply-blend the ink mask onto the remapped image.
//! 5. Overlay deterministic Perlin paper grain seeded by tile coordinates.

use image::{ImageFormat, RgbImage};

use crate::{config::ParchmentConfig, error::ArtError};

#[derive(Clone)]
pub struct ParchmentPipeline {
    cfg: ParchmentConfig,
}

impl ParchmentPipeline {
    pub fn new(cfg: ParchmentConfig) -> Self {
        Self { cfg }
    }

    /// Synchronous paint — suitable for calling inside `tokio::task::spawn_blocking`.
    pub fn paint_sync(
        &self,
        png_bytes: &[u8],
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, ArtError> {
        let img = image::load_from_memory(png_bytes)?.to_rgb8();

        let smoothed = if self.cfg.smooth_radius > 0 {
            box_blur(&img)
        } else {
            img.clone()
        };

        let edges = sobel_magnitude(&smoothed);
        let mut out = remap_colors(&smoothed);
        blend_ink(&mut out, &edges, self.cfg.ink_strength);
        add_grain(&mut out, self.cfg.grain_strength, z, x, y);

        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(out)
            .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)?;
        Ok(buf)
    }
}

// ── Smoothing ───────────────────────────────────────────────────────────────

fn box_blur(img: &RgbImage) -> RgbImage {
    let (width, height) = img.dimensions();
    let mut out = img.clone();
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let p = img.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32);
                    r += p[0] as u32;
                    g += p[1] as u32;
                    b += p[2] as u32;
                }
            }
            out.put_pixel(
                x,
                y,
                image::Rgb([(r / 9) as u8, (g / 9) as u8, (b / 9) as u8]),
            );
        }
    }
    out
}

// ── Edge extraction (Sobel) ─────────────────────────────────────────────────

fn luma(p: &image::Rgb<u8>) -> f32 {
    p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114
}

/// Returns an edge-strength image in [0, 255]: bright = strong edge.
fn sobel_magnitude(img: &RgbImage) -> Vec<f32> {
    let (width, height) = img.dimensions();
    let (w, h) = (width as usize, height as usize);

    // Build luma plane
    let luma_plane: Vec<f32> = img.pixels().map(luma).collect();

    let mut mag = vec![0.0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let idx = |dy: i32, dx: i32| (y as i32 + dy) as usize * w + (x as i32 + dx) as usize;
            #[rustfmt::skip]
            let gx = -luma_plane[idx(-1, -1)] + luma_plane[idx(-1, 1)]
                     - 2.0 * luma_plane[idx(0, -1)] + 2.0 * luma_plane[idx(0, 1)]
                     - luma_plane[idx(1, -1)] + luma_plane[idx(1, 1)];
            #[rustfmt::skip]
            let gy = -luma_plane[idx(-1, -1)] - 2.0 * luma_plane[idx(-1, 0)] - luma_plane[idx(-1, 1)]
                     + luma_plane[idx(1, -1)] + 2.0 * luma_plane[idx(1, 0)] + luma_plane[idx(1, 1)];
            mag[y * w + x] = (gx * gx + gy * gy).sqrt().min(255.0);
        }
    }
    mag
}

// ── Colour remap ────────────────────────────────────────────────────────────

/// Parchment palette: colour targets for each region type.
/// Values chosen to match warm 1820s illustrated cartography (RDR2-inspired).
const PARCHMENT_BG: [u8; 3] = [0xf4, 0xe6, 0xc0]; // light cream
const INK_DARK: [u8; 3] = [0x2d, 0x1a, 0x0e]; // dark sepia-black
const INK_MID: [u8; 3] = [0x6b, 0x4a, 0x2a]; // medium sepia
const WATER: [u8; 3] = [0xb0, 0xbe, 0xc5]; // cool slate
const FIELD: [u8; 3] = [0xe8, 0xd5, 0xa0]; // mid parchment
const WOODLAND: [u8; 3] = [0x8a, 0x9a, 0x6a]; // muted sage
const SETTLEMENT: [u8; 3] = [0xb0, 0x78, 0x50]; // terracotta

fn to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta < 1e-6 {
        0.0
    } else if (max - rf).abs() < 1e-6 {
        let h = 60.0 * ((gf - bf) / delta);
        if h < 0.0 { h + 360.0 } else { h }
    } else if (max - gf).abs() < 1e-6 {
        60.0 * ((bf - rf) / delta + 2.0)
    } else {
        60.0 * ((rf - gf) / delta + 4.0)
    };
    (h, s, v)
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)) as u8
}

fn blend_rgb(base: [u8; 3], target: [u8; 3], t: f32) -> [u8; 3] {
    [
        lerp_u8(base[0], target[0], t),
        lerp_u8(base[1], target[1], t),
        lerp_u8(base[2], target[2], t),
    ]
}

fn remap_pixel(r: u8, g: u8, b: u8) -> [u8; 3] {
    let (h, s, _v) = to_hsv(r, g, b);
    let luma_val = (r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114) as u8;

    // Water: blue-shifted with meaningful saturation
    if s > 0.12 && (170.0..=235.0).contains(&h) {
        // Blend toward water colour; stronger blue bias = more water colour
        let t = (s * 4.0).min(1.0);
        return blend_rgb(FIELD, WATER, t);
    }

    // Woodland / hedgerow: green-shifted
    if s > 0.10 && (65.0..=158.0).contains(&h) {
        let t = (s * 5.0).min(1.0);
        return blend_rgb(FIELD, WOODLAND, t);
    }

    // Settlements: warm grey-brown (desaturated, mid-luma)
    if s > 0.05 && (15.0..40.0).contains(&h) && luma_val > 80 && luma_val < 170 {
        let t = (s * 6.0).min(1.0);
        return blend_rgb(FIELD, SETTLEMENT, t);
    }

    // Dark → ink (roads, boundaries, text)
    if luma_val < 80 {
        let t = 1.0 - luma_val as f32 / 80.0;
        return blend_rgb(INK_MID, INK_DARK, t);
    }

    // Mid-dark → medium sepia details
    if luma_val < 135 {
        let t = (luma_val as f32 - 80.0) / 55.0;
        return blend_rgb(INK_MID, FIELD, t);
    }

    // Light → parchment background
    let t = (luma_val as f32 - 135.0) / 120.0;
    blend_rgb(FIELD, PARCHMENT_BG, t)
}

fn remap_colors(img: &RgbImage) -> RgbImage {
    let (width, height) = img.dimensions();
    let mut out = img.clone();
    for y in 0..height {
        for x in 0..width {
            let p = img.get_pixel(x, y);
            let [r, g, b] = remap_pixel(p[0], p[1], p[2]);
            out.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    out
}

// ── Ink overlay ──────────────────────────────────────────────────────────────

fn blend_ink(img: &mut RgbImage, edges: &[f32], strength: f32) {
    let (width, height) = img.dimensions();
    for y in 0..height {
        for x in 0..width {
            let edge = edges[(y * width + x) as usize];
            if edge < 8.0 {
                continue;
            }
            // Gamma-curve the edge strength for sharper ink lines
            let t = (edge / 255.0).powf(0.7) * strength;
            let p = img.get_pixel(x, y);
            let r = lerp_u8(p[0], INK_DARK[0], t);
            let g = lerp_u8(p[1], INK_DARK[1], t);
            let b = lerp_u8(p[2], INK_DARK[2], t);
            img.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
}

// ── Paper grain ──────────────────────────────────────────────────────────────

fn add_grain(img: &mut RgbImage, strength: f32, tile_z: u32, tile_x: u32, tile_y: u32) {
    if strength < 1e-4 {
        return;
    }
    // Mix tile coords into the seed so adjacent tiles have distinct grain.
    let seed = (tile_z as i32).wrapping_mul(1_000_003)
        ^ (tile_x as i32).wrapping_mul(101_111)
        ^ (tile_y as i32);

    let mut noise = fastnoise_lite::FastNoiseLite::new();
    noise.set_seed(Some(seed));
    noise.set_noise_type(Some(fastnoise_lite::NoiseType::Perlin));
    // Frequency 0.05 at 256-pixel tiles → ~12 noise cells across the tile,
    // giving a medium-scale paper texture rather than fine pixel noise.
    noise.set_frequency(Some(0.05));

    let (width, height) = img.dimensions();
    for py in 0..height {
        for px in 0..width {
            let n = noise.get_noise_2d(px as f32, py as f32); // [-1.0, 1.0]
            let delta = (n * strength * 22.0) as i32;
            let p = img.get_pixel_mut(px, py);
            p[0] = (p[0] as i32 + delta).clamp(0, 255) as u8;
            p[1] = (p[1] as i32 + delta).clamp(0, 255) as u8;
            // Less blue variation to keep grain warm
            p[2] = (p[2] as i32 + (delta >> 1)).clamp(0, 255) as u8;
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_rgb_png(r: u8, g: u8, b: u8) -> Vec<u8> {
        let img = RgbImage::from_fn(256, 256, |_, _| image::Rgb([r, g, b]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn parchment_round_trips_png() {
        let cfg = ParchmentConfig::default();
        let pipeline = ParchmentPipeline::new(cfg);
        let input = solid_rgb_png(200, 190, 160);
        let output = pipeline.paint_sync(&input, 10, 500, 350).unwrap();
        // Output is a valid PNG
        let decoded = image::load_from_memory(&output).unwrap();
        assert_eq!(decoded.width(), 256);
        assert_eq!(decoded.height(), 256);
    }

    #[test]
    fn grain_is_deterministic() {
        let cfg = ParchmentConfig::default();
        let pipeline = ParchmentPipeline::new(cfg);
        let input = solid_rgb_png(200, 190, 160);
        let a = pipeline.paint_sync(&input, 14, 7811, 5284).unwrap();
        let b = pipeline.paint_sync(&input, 14, 7811, 5284).unwrap();
        assert_eq!(a, b, "same tile coords must produce identical output");
    }

    #[test]
    fn adjacent_tiles_have_different_grain() {
        let cfg = ParchmentConfig::default();
        let pipeline = ParchmentPipeline::new(cfg);
        let input = solid_rgb_png(200, 190, 160);
        let a = pipeline.paint_sync(&input, 14, 7811, 5284).unwrap();
        let b = pipeline.paint_sync(&input, 14, 7812, 5284).unwrap();
        assert_ne!(a, b, "adjacent tiles must differ in grain");
    }

    #[test]
    fn dark_pixels_remap_to_ink() {
        let [r, g, b] = remap_pixel(5, 5, 5);
        // Should be close to INK_DARK
        assert!(r < 60, "r={r} expected near {}", INK_DARK[0]);
        assert!(g < 40, "g={g} expected near {}", INK_DARK[1]);
        assert!(b < 30, "b={b} expected near {}", INK_DARK[2]);
    }

    #[test]
    fn light_pixels_remap_to_parchment() {
        let [r, g, b] = remap_pixel(240, 235, 225);
        // Should be in warm cream range
        assert!(r > 220, "r={r} expected warm cream");
        assert!(g > 200, "g={g} expected warm cream");
        assert!(b > 150 && b < 220, "b={b} expected slightly cool cream");
    }

    #[test]
    fn blue_pixels_remap_to_water() {
        // Simulate a blue-grey water wash
        let [_r, g, b] = remap_pixel(130, 165, 185);
        // Should shift toward WATER slate colour
        assert!(
            b >= g || (b as i16 - g as i16).abs() < 30,
            "water should be blue-ish"
        );
    }
}
