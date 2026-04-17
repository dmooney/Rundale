//! Slippy-map tile coordinate math.
//!
//! Converts between WGS-84 lon/lat, XYZ tile indices `(z, x, y)`, and
//! within-tile pixel offsets. All functions assume the standard 256 px
//! slippy scheme used by OpenStreetMap and the National Library of
//! Scotland OS-6" tile server. `y=0` is the northernmost tile row;
//! TMS-style sources must y-flip before calling into this module.

use super::super::overpass::BoundingBox;

/// Standard slippy-map tile size in pixels.
pub const TILE_SIZE_PX: u32 = 256;

/// A location expressed as `(tile_x, tile_y, pixel_x, pixel_y)` at a given zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TilePixel {
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub px: u32,
    pub py: u32,
}

/// Converts `(lat, lon)` in degrees to a `TilePixel` at zoom `z`.
///
/// Pixel coordinates are within the 256×256 tile identified by `(x, y)`.
pub fn lonlat_to_tile_pixel(lat: f64, lon: f64, z: u8) -> TilePixel {
    let n = (1u64 << z) as f64;
    let lat_rad = lat.to_radians();
    let x_frac = (lon + 180.0) / 360.0 * n;
    let y_frac =
        (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n;

    let x = x_frac.floor().clamp(0.0, n - 1.0) as u32;
    let y = y_frac.floor().clamp(0.0, n - 1.0) as u32;
    let px = ((x_frac - x as f64) * TILE_SIZE_PX as f64)
        .round()
        .clamp(0.0, (TILE_SIZE_PX - 1) as f64) as u32;
    let py = ((y_frac - y as f64) * TILE_SIZE_PX as f64)
        .round()
        .clamp(0.0, (TILE_SIZE_PX - 1) as f64) as u32;

    TilePixel { z, x, y, px, py }
}

/// Inverts [`lonlat_to_tile_pixel`]: converts `(z, x, y, px, py)` back to `(lat, lon)`.
pub fn tile_pixel_to_lonlat(z: u8, x: u32, y: u32, px: u32, py: u32) -> (f64, f64) {
    let n = (1u64 << z) as f64;
    let x_frac = x as f64 + (px as f64 / TILE_SIZE_PX as f64);
    let y_frac = y as f64 + (py as f64 / TILE_SIZE_PX as f64);
    let lon = x_frac / n * 360.0 - 180.0;
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * y_frac / n))
        .sinh()
        .atan();
    let lat = lat_rad.to_degrees();
    (lat, lon)
}

/// Returns the inclusive `(x, y)` tile index range that covers a bbox at zoom `z`.
///
/// Order is `(min_x, min_y, max_x, max_y)` with `min_y` being the northernmost
/// row (`y=0` convention).
pub fn tile_range_for_bbox(bbox: &BoundingBox, z: u8) -> (u32, u32, u32, u32) {
    let nw = lonlat_to_tile_pixel(bbox.north, bbox.west, z);
    let se = lonlat_to_tile_pixel(bbox.south, bbox.east, z);
    let min_x = nw.x.min(se.x);
    let max_x = nw.x.max(se.x);
    let min_y = nw.y.min(se.y);
    let max_y = nw.y.max(se.y);
    (min_x, min_y, max_x, max_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        // Duplicated from osm_model::haversine_distance to avoid a cross-module
        // cfg(test) dep cycle in tests.
        const R: f64 = 6_371_000.0;
        let dlat = (lat2 - lat1).to_radians();
        let dlon = (lon2 - lon1).to_radians();
        let a = (dlat / 2.0).sin().powi(2)
            + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
        R * 2.0 * a.sqrt().asin()
    }

    #[test]
    fn test_roundtrip_irish_coords_within_a_few_metres() {
        // Samples around Roscommon / Athlone / Ballinasloe.
        let samples = [
            (53.6320, -8.1020), // Kilteevan
            (53.5070, -7.9850), // Kiltoom
            (53.4246, -7.9407), // Athlone
            (53.3331, -8.2228), // Ballinasloe
        ];
        for z in [14u8, 15, 16, 17, 18] {
            for &(lat, lon) in &samples {
                let tp = lonlat_to_tile_pixel(lat, lon, z);
                let (rlat, rlon) = tile_pixel_to_lonlat(tp.z, tp.x, tp.y, tp.px, tp.py);
                let err = haversine_m(lat, lon, rlat, rlon);
                // At z=18 a pixel is <0.6 m; at z=14 it's ~10 m. Allow 12 m
                // across the tested zoom range.
                assert!(
                    err < 12.0,
                    "roundtrip error {err:.2}m at z={z} for ({lat}, {lon}) -> ({rlat}, {rlon})"
                );
            }
        }
    }

    #[test]
    fn test_tile_range_covers_bbox() {
        let bbox = BoundingBox {
            south: 53.60,
            west: -8.15,
            north: 53.65,
            east: -8.05,
        };
        let (min_x, min_y, max_x, max_y) = tile_range_for_bbox(&bbox, 15);
        assert!(
            min_x <= max_x && min_y <= max_y,
            "range must be well-ordered"
        );
        // The bbox corners should be inside the returned tile range.
        let corners = [
            (bbox.south, bbox.west),
            (bbox.south, bbox.east),
            (bbox.north, bbox.west),
            (bbox.north, bbox.east),
        ];
        for (lat, lon) in corners {
            let tp = lonlat_to_tile_pixel(lat, lon, 15);
            assert!(tp.x >= min_x && tp.x <= max_x, "x out of range");
            assert!(tp.y >= min_y && tp.y <= max_y, "y out of range");
        }
    }

    #[test]
    fn test_pixel_coords_respect_tile_size() {
        let tp = lonlat_to_tile_pixel(53.6, -8.1, 16);
        assert!(tp.px < TILE_SIZE_PX);
        assert!(tp.py < TILE_SIZE_PX);
    }
}
