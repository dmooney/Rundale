//! Seed a bundled tile directory with pre-stylized NLS map tiles.
//!
//! Enumerates all XYZ tiles covering a geographic bounding box at specified
//! zoom levels, fetches each raw tile from an upstream URL template, applies
//! the configured art pipeline, and writes the result to an output directory
//! structured as `{output}/{source_id}/{z}/{x}/{y}.png`.
//!
//! The output directory is intended to be set as `bundled_tiles_dir` in
//! `parish.toml` so `TileCache` serves pre-generated stylized tiles without
//! runtime processing.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use parish_tile_art::{ArtConfig, TileArtist};

pub struct SeedConfig {
    /// Upstream XYZ URL template, e.g. `"https://…/{z}/{x}/{y}.png"`.
    pub upstream_url: String,
    /// Bounding box (WGS-84 decimal degrees).
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
    /// Zoom levels to generate.
    pub zoom_levels: Vec<u32>,
    /// Source ID used as the subdirectory name, e.g. `"rundale-map"`.
    pub source_id: String,
    /// Root output directory (becomes the `bundled_tiles_dir`).
    pub output_dir: PathBuf,
    /// Maximum concurrent tile fetch+paint tasks.
    pub concurrency: usize,
    /// Local disk cache for raw downloaded tiles (avoids re-fetching on retry).
    pub raw_cache_dir: PathBuf,
    /// Art pipeline config.
    pub art_config: ArtConfig,
}

pub async fn run(cfg: SeedConfig) -> Result<()> {
    let artist = Arc::new(TileArtist::from_config(&cfg.art_config));
    let http = Arc::new(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Parish-TileSeeder/0.1")
            .build()
            .context("build HTTP client")?,
    );

    // Enumerate all tiles
    let mut all_tiles: Vec<(u32, u32, u32)> = Vec::new();
    for &zoom in &cfg.zoom_levels {
        let (min_x, max_y) = lon_lat_to_tile(cfg.west, cfg.south, zoom);
        let (max_x, min_y) = lon_lat_to_tile(cfg.east, cfg.north, zoom);
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                all_tiles.push((zoom, x, y));
            }
        }
    }

    let total = all_tiles.len();
    println!(
        "Seeding {} tiles across zoom levels {:?} → {}",
        total,
        cfg.zoom_levels,
        cfg.output_dir.display()
    );

    let sem = Arc::new(tokio::sync::Semaphore::new(cfg.concurrency));
    let cfg = Arc::new(cfg);

    let mut set = tokio::task::JoinSet::new();
    for (z, x, y) in all_tiles {
        let sem = Arc::clone(&sem);
        let artist = Arc::clone(&artist);
        let http = Arc::clone(&http);
        let cfg = Arc::clone(&cfg);
        set.spawn(async move {
            let _permit = sem.acquire_owned().await;
            process_tile(&artist, &http, &cfg, z, x, y).await
        });
    }

    let (mut done, mut skipped, mut errors) = (0u64, 0u64, 0u64);
    while let Some(result) = set.join_next().await {
        match result {
            Ok(Ok(TileResult::Written)) => done += 1,
            Ok(Ok(TileResult::Skipped)) => skipped += 1,
            Ok(Err(e)) => {
                errors += 1;
                tracing::warn!(error = %e, "tile error");
            }
            Err(e) => {
                errors += 1;
                tracing::warn!(error = %e, "task panic");
            }
        }
        let progress = done + skipped + errors;
        if progress % 100 == 0 || progress == total as u64 {
            println!("  {progress}/{total} — written={done} skipped={skipped} errors={errors}");
        }
    }

    println!("Done: {done} written, {skipped} skipped (already exist), {errors} errors");
    if errors > 0 {
        anyhow::bail!("{errors} tiles failed — check warnings above");
    }
    Ok(())
}

enum TileResult {
    Written,
    Skipped,
}

async fn process_tile(
    artist: &TileArtist,
    http: &reqwest::Client,
    cfg: &SeedConfig,
    z: u32,
    x: u32,
    y: u32,
) -> Result<TileResult> {
    let out_path = cfg
        .output_dir
        .join(&cfg.source_id)
        .join(z.to_string())
        .join(x.to_string())
        .join(format!("{y}.png"));

    if out_path.exists() {
        return Ok(TileResult::Skipped);
    }

    let raw = fetch_raw(http, &cfg.upstream_url, &cfg.raw_cache_dir, z, x, y).await?;
    let stylized = artist
        .paint_tile(&raw, z, x, y)
        .await
        .with_context(|| format!("painting tile z={z} x={x} y={y}"))?;

    // Atomic write: temp file then rename
    if let Some(parent) = out_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("create output dirs")?;
    }
    let tmp = out_path.with_extension("tmp");
    tokio::fs::write(&tmp, &stylized)
        .await
        .context("write tmp")?;
    tokio::fs::rename(&tmp, &out_path)
        .await
        .context("rename to final")?;

    Ok(TileResult::Written)
}

async fn fetch_raw(
    http: &reqwest::Client,
    url_template: &str,
    cache_dir: &std::path::Path,
    z: u32,
    x: u32,
    y: u32,
) -> Result<Vec<u8>> {
    // Check raw tile disk cache first
    let cache_path = cache_dir
        .join(z.to_string())
        .join(x.to_string())
        .join(format!("{y}.png"));

    if let Ok(bytes) = tokio::fs::read(&cache_path).await {
        return Ok(bytes);
    }

    let url = url_template
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string());

    let resp = http
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("upstream {} → HTTP {}", url, resp.status());
    }

    let bytes = resp.bytes().await.context("read response body")?.to_vec();

    // Write to raw cache (best-effort)
    if let Some(parent) = cache_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
        let tmp = cache_path.with_extension("tmp");
        if tokio::fs::write(&tmp, &bytes).await.is_ok() {
            let _ = tokio::fs::rename(&tmp, &cache_path).await;
        }
    }

    Ok(bytes)
}

/// Convert (longitude, latitude) to XYZ tile coordinates at the given zoom.
///
/// Standard Web Mercator (EPSG:3857) XYZ tiling: y=0 is at the north pole.
fn lon_lat_to_tile(lon: f64, lat: f64, zoom: u32) -> (u32, u32) {
    let n = 2u32.pow(zoom) as f64;
    let x = ((lon + 180.0) / 360.0 * n).floor() as u32;
    let lat_rad = lat.to_radians();
    let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor() as u32;
    (x.min(n as u32 - 1), y.min(n as u32 - 1))
}

/// Parse a zoom-range string like `"10-17"` or `"14"` into a `Vec<u32>`.
pub fn parse_zoom_range(s: &str) -> Result<Vec<u32>> {
    if let Some((a, b)) = s.split_once('-') {
        let min: u32 = a.trim().parse().context("zoom min")?;
        let max: u32 = b.trim().parse().context("zoom max")?;
        if min > max {
            anyhow::bail!("zoom min ({min}) must be ≤ max ({max})");
        }
        Ok((min..=max).collect())
    } else {
        let z: u32 = s.trim().parse().context("zoom level")?;
        Ok(vec![z])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_coords_roscommon() {
        // z=14, lon=-8.0, lat=53.5 (County Roscommon area)
        let (x, y) = lon_lat_to_tile(-8.0, 53.5, 14);
        assert_eq!(x, 7827, "expected x=7827 for lon=-8.0 z=14");
        assert_eq!(y, 5299, "expected y=5299 for lat=53.5 z=14");
    }

    #[test]
    fn parse_range() {
        assert_eq!(parse_zoom_range("14").unwrap(), vec![14]);
        assert_eq!(parse_zoom_range("12-14").unwrap(), vec![12, 13, 14]);
    }

    #[test]
    fn parse_range_invalid() {
        assert!(parse_zoom_range("15-12").is_err());
        assert!(parse_zoom_range("abc").is_err());
    }
}
