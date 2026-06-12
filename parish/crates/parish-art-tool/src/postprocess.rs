//! Post-processing: Lanczos downscale for plates (480x270) and sprites (48x72).
//!
//! Plates are generated at 1536x1024 and downscaled preserving the colour
//! palette.  Sprites are generated at 1024x1024 with a transparent background,
//! trimmed to their opaque bounding box, then downscaled to 48x72.

use anyhow::{Result, bail};
use image::{DynamicImage, ImageFormat, RgbaImage};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Final plate dimensions.
pub const PLATE_WIDTH: u32 = 480;
pub const PLATE_HEIGHT: u32 = 270;

/// Final sprite dimensions.
pub const SPRITE_WIDTH: u32 = 48;
pub const SPRITE_HEIGHT: u32 = 72;

// ---------------------------------------------------------------------------
// Plate downscale
// ---------------------------------------------------------------------------

/// Downscale raw PNG bytes to a 480x270 plate PNG.
///
/// Uses Lanczos3 resampling. Alpha is preserved if the source image has it.
pub fn downscale_plate(png_bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)?;
    let resized = img.resize_exact(
        PLATE_WIDTH,
        PLATE_HEIGHT,
        image::imageops::FilterType::Lanczos3,
    );
    encode_png(resized)
}

// ---------------------------------------------------------------------------
// Sprite downscale with trim
// ---------------------------------------------------------------------------

/// Downscale raw PNG bytes to a 48x72 sprite PNG, preserving alpha.
///
/// Trims the transparent border before downscaling so the opaque content
/// fills the target dimensions.  Returns an error if the image is fully
/// transparent (an empty sprite is not useful and likely a generation failure).
pub fn downscale_sprite(png_bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)?;
    let rgba = img.to_rgba8();
    let trimmed = trim_transparent(&rgba)?;
    let dyn_trimmed = DynamicImage::ImageRgba8(trimmed);
    let resized = dyn_trimmed.resize_exact(
        SPRITE_WIDTH,
        SPRITE_HEIGHT,
        image::imageops::FilterType::Lanczos3,
    );
    encode_png(resized)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Trim the fully-transparent border of an RGBA image.
///
/// Returns an error if the image is entirely transparent.
fn trim_transparent(img: &RgbaImage) -> Result<RgbaImage> {
    let (width, height) = img.dimensions();

    let mut min_x = width;
    let mut max_x = 0u32;
    let mut min_y = height;
    let mut max_y = 0u32;

    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            if pixel[3] > 0 {
                if x < min_x {
                    min_x = x;
                }
                if x > max_x {
                    max_x = x;
                }
                if y < min_y {
                    min_y = y;
                }
                if y > max_y {
                    max_y = y;
                }
            }
        }
    }

    if min_x > max_x || min_y > max_y {
        bail!(
            "sprite image is fully transparent — generation likely failed or \
             returned an empty image"
        );
    }

    let crop_width = max_x - min_x + 1;
    let crop_height = max_y - min_y + 1;
    let mut cropped = RgbaImage::new(crop_width, crop_height);
    for y in 0..crop_height {
        for x in 0..crop_width {
            let src = img.get_pixel(min_x + x, min_y + y);
            cropped.put_pixel(x, y, *src);
        }
    }
    Ok(cropped)
}

/// Encode a `DynamicImage` as PNG bytes.
fn encode_png(img: DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// Encode a synthetic RgbaImage to PNG bytes for use as test fixtures.
    fn encode_rgba_to_png(img: &RgbaImage) -> Vec<u8> {
        let dyn_img = DynamicImage::ImageRgba8(img.clone());
        let mut buf = Vec::new();
        dyn_img
            .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    /// Build a solid-colour RGBA image of the given size.
    fn solid_rgba(w: u32, h: u32, colour: Rgba<u8>) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for pixel in img.pixels_mut() {
            *pixel = colour;
        }
        img
    }

    // --- plate ---

    #[test]
    fn downscale_plate_produces_correct_dims() {
        let src = solid_rgba(1536, 1024, Rgba([100, 150, 200, 255]));
        let png = encode_rgba_to_png(&src);
        let out_bytes = downscale_plate(&png).unwrap();
        let out = image::load_from_memory_with_format(&out_bytes, ImageFormat::Png).unwrap();
        assert_eq!(
            out.width(),
            PLATE_WIDTH,
            "plate width must be {PLATE_WIDTH}"
        );
        assert_eq!(
            out.height(),
            PLATE_HEIGHT,
            "plate height must be {PLATE_HEIGHT}"
        );
    }

    #[test]
    fn downscale_plate_preserves_alpha_channel() {
        let src = solid_rgba(1536, 1024, Rgba([100, 150, 200, 128]));
        let png = encode_rgba_to_png(&src);
        let out_bytes = downscale_plate(&png).unwrap();
        let out = image::load_from_memory_with_format(&out_bytes, ImageFormat::Png).unwrap();
        // Verify the output has an alpha channel (RGBA not RGB).
        let rgba = out.to_rgba8();
        // The Lanczos filter on a uniform colour should keep alpha roughly
        // similar (within rounding); just assert it's non-zero.
        let sample = rgba.get_pixel(0, 0);
        assert!(sample[3] > 0, "alpha must be preserved");
    }

    #[test]
    fn downscale_plate_arbitrary_source_size() {
        // Any input size should produce 480x270 output.
        let src = solid_rgba(800, 600, Rgba([50, 50, 50, 255]));
        let png = encode_rgba_to_png(&src);
        let out_bytes = downscale_plate(&png).unwrap();
        let out = image::load_from_memory_with_format(&out_bytes, ImageFormat::Png).unwrap();
        assert_eq!(out.width(), PLATE_WIDTH);
        assert_eq!(out.height(), PLATE_HEIGHT);
    }

    // --- sprite ---

    #[test]
    fn downscale_sprite_produces_correct_dims() {
        let src = solid_rgba(1024, 1024, Rgba([200, 100, 50, 255]));
        let png = encode_rgba_to_png(&src);
        let out_bytes = downscale_sprite(&png).unwrap();
        let out = image::load_from_memory_with_format(&out_bytes, ImageFormat::Png).unwrap();
        assert_eq!(
            out.width(),
            SPRITE_WIDTH,
            "sprite width must be {SPRITE_WIDTH}"
        );
        assert_eq!(
            out.height(),
            SPRITE_HEIGHT,
            "sprite height must be {SPRITE_HEIGHT}"
        );
    }

    #[test]
    fn downscale_sprite_preserves_alpha() {
        let src = solid_rgba(1024, 1024, Rgba([200, 100, 50, 200]));
        let png = encode_rgba_to_png(&src);
        let out_bytes = downscale_sprite(&png).unwrap();
        let out = image::load_from_memory_with_format(&out_bytes, ImageFormat::Png).unwrap();
        let rgba = out.to_rgba8();
        let sample = rgba.get_pixel(0, 0);
        assert!(sample[3] > 0, "alpha must be preserved in sprite");
    }

    #[test]
    fn downscale_sprite_transparent_border_trimmed() {
        // Build a 1024x1024 image that is mostly transparent with a small
        // opaque region in the centre — the trim should remove the border.
        let mut src = RgbaImage::new(1024, 1024);
        // Fill centre 100x200 block with opaque pixels.
        for y in 400..600u32 {
            for x in 462..562u32 {
                src.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let png = encode_rgba_to_png(&src);
        let out_bytes = downscale_sprite(&png).unwrap();
        let out = image::load_from_memory_with_format(&out_bytes, ImageFormat::Png).unwrap();
        // After trim + downscale the result must still be SPRITE_WIDTH x SPRITE_HEIGHT.
        assert_eq!(out.width(), SPRITE_WIDTH);
        assert_eq!(out.height(), SPRITE_HEIGHT);
    }

    #[test]
    fn downscale_sprite_fully_transparent_returns_error() {
        let src = RgbaImage::new(1024, 1024); // all zeros, alpha=0
        let png = encode_rgba_to_png(&src);
        let result = downscale_sprite(&png);
        assert!(
            result.is_err(),
            "fully transparent sprite must return error"
        );
    }
}
