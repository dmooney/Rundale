//! Historic tile-source abstraction.
//!
//! Implementations fetch 256 px PNG tiles at a given `(z, x, y)` in XYZ
//! conventions (`y=0` at north). Providers that use TMS y-flipping must
//! apply the flip internally so the trait always speaks XYZ.
//!
//! The default [`TileSourceRegistry`] resolves a [`BoundingBox`] to the
//! first registered source whose [`HistoricTileSource::covers`] returns
//! `true`. The `NlsRoscommonSource` is registered by default; add more
//! via [`TileSourceRegistry::with_source`] as island-wide sources come
//! online.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;

use super::super::overpass::BoundingBox;

/// Trait implemented by every historic raster-tile provider.
#[async_trait]
pub trait HistoricTileSource: Send + Sync {
    /// Short identifier used in cache paths and CLI flags (e.g. `"nls-roscommon"`).
    fn id(&self) -> &'static str;

    /// Human-readable attribution, written to each location's `geo_source`
    /// so downstream tools can cite the source sheet.
    fn attribution(&self) -> &str;

    /// Inclusive min / max native zoom for this source.
    fn zoom_range(&self) -> (u8, u8);

    /// Returns `true` if this source's raster coverage overlaps `bbox`.
    fn covers(&self, bbox: &BoundingBox) -> bool;

    /// Fetches a single PNG tile at `(z, x, y)` (XYZ convention).
    async fn fetch_tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>>;
}

/// Registry of available tile sources, in priority order.
pub struct TileSourceRegistry {
    sources: Vec<Arc<dyn HistoricTileSource>>,
}

impl Default for TileSourceRegistry {
    fn default() -> Self {
        Self::new().with_source(Arc::new(NlsRoscommonSource::new()))
    }
}

impl TileSourceRegistry {
    /// Creates an empty registry. See [`Self::default`] for the standard set.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Appends a source. Later lookups prefer earlier-registered sources.
    pub fn with_source(mut self, source: Arc<dyn HistoricTileSource>) -> Self {
        self.sources.push(source);
        self
    }

    /// Returns the first source whose `covers()` returns true for the bbox.
    pub fn for_bbox(&self, bbox: &BoundingBox) -> Option<&Arc<dyn HistoricTileSource>> {
        self.sources.iter().find(|s| s.covers(bbox))
    }

    /// Looks up a source by id. Useful when the user passes `--tile-source`
    /// explicitly.
    pub fn by_id(&self, id: &str) -> Option<&Arc<dyn HistoricTileSource>> {
        self.sources.iter().find(|s| s.id() == id)
    }
}

/// NLS-hosted OS 6-inch First Edition for Roscommon (ca. 1829–42).
///
/// The tile set referenced in `parish.example.toml` — a standard XYZ
/// scheme, no auth required. Coverage is approximately the historic
/// county of Roscommon plus a small border buffer.
pub struct NlsRoscommonSource {
    client: reqwest::Client,
    base_url: String,
}

impl Default for NlsRoscommonSource {
    fn default() -> Self {
        Self::new()
    }
}

impl NlsRoscommonSource {
    /// Canonical bounding box of the Roscommon sheet coverage.
    const ROSCOMMON_COVERAGE: BoundingBox = BoundingBox {
        south: 53.30,
        west: -8.80,
        north: 54.10,
        east: -7.80,
    };

    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Parish-GeoTool/0.1 (historic-discover)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            base_url: "https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1".to_string(),
        }
    }
}

#[async_trait]
impl HistoricTileSource for NlsRoscommonSource {
    fn id(&self) -> &'static str {
        "nls-roscommon"
    }

    fn attribution(&self) -> &str {
        "OS 6-inch First Edition, Roscommon sheet, ca. 1829\u{2013}42 (NLS)"
    }

    fn zoom_range(&self) -> (u8, u8) {
        (13, 17)
    }

    fn covers(&self, bbox: &BoundingBox) -> bool {
        let c = Self::ROSCOMMON_COVERAGE;
        // Reject if the query bbox is entirely outside the coverage rectangle.
        !(bbox.east < c.west || bbox.west > c.east || bbox.north < c.south || bbox.south > c.north)
    }

    async fn fetch_tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>> {
        let (zmin, zmax) = self.zoom_range();
        if z < zmin || z > zmax {
            return Err(anyhow!(
                "zoom {z} outside native range {zmin}..={zmax} for {}",
                self.id()
            ));
        }
        let url = format!("{}/{}/{}/{}.png", self.base_url, z, x, y);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("fetching tile {url}"))?
            .error_for_status()
            .with_context(|| format!("tile server returned error for {url}"))?;
        let bytes = resp
            .bytes()
            .await
            .with_context(|| format!("reading tile bytes from {url}"))?;
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roscommon_bbox() -> BoundingBox {
        BoundingBox {
            south: 53.60,
            west: -8.15,
            north: 53.65,
            east: -8.05,
        }
    }

    fn kerry_bbox() -> BoundingBox {
        BoundingBox {
            south: 52.00,
            west: -9.80,
            north: 52.05,
            east: -9.70,
        }
    }

    #[test]
    fn test_nls_roscommon_covers_only_roscommon() {
        let source = NlsRoscommonSource::new();
        assert!(source.covers(&roscommon_bbox()));
        assert!(!source.covers(&kerry_bbox()));
    }

    #[test]
    fn test_default_registry_resolves_roscommon() {
        let reg = TileSourceRegistry::default();
        let resolved = reg.for_bbox(&roscommon_bbox()).expect("should resolve");
        assert_eq!(resolved.id(), "nls-roscommon");
    }

    #[test]
    fn test_default_registry_misses_kerry() {
        let reg = TileSourceRegistry::default();
        assert!(reg.for_bbox(&kerry_bbox()).is_none());
    }

    #[test]
    fn test_registry_by_id() {
        let reg = TileSourceRegistry::default();
        assert!(reg.by_id("nls-roscommon").is_some());
        assert!(reg.by_id("nonexistent").is_none());
    }

    #[test]
    fn test_attribution_cites_sheet() {
        let source = NlsRoscommonSource::new();
        let attr = source.attribution();
        assert!(attr.contains("Roscommon"));
        assert!(attr.contains("NLS"));
    }
}
