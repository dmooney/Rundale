//! Map tile-source registry and active default (`[engine.map]`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Map tile source registry and active default.
///
/// A registry of named raster-tile sources (XYZ templates) that the frontend
/// can switch between at runtime via the `/map` slash command. Users can
/// override the baked-in defaults by adding `[engine.map.tile_sources.<id>]`
/// blocks in `parish.toml`.
///
/// **Partial overrides:** serde's BTreeMap deserialisation replaces the whole
/// map rather than merging. Call [`MapConfig::apply_defaults`] after parsing
/// to fold the baked defaults (OSM, Ireland Historic 6") back into a user-supplied
/// registry that only overrode a subset.
#[derive(Debug, Deserialize, Serialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapConfig {
    /// Id of the source used on first boot (pre-localStorage). Must match one
    /// of the keys in `tile_sources`.
    #[serde(default = "default_tile_source_id")]
    pub default_tile_source: String,
    /// Registry of available raster tile sources, keyed by id.
    #[serde(default = "default_tile_sources")]
    pub tile_sources: BTreeMap<String, TileSourceConfig>,
    /// Optional path to a pre-seeded tile directory. When set, `TileCache`
    /// checks this directory before hitting the upstream network, enabling
    /// offline play. Path is resolved at startup; relative paths are resolved
    /// against the mod/data directory (see CLAUDE.md rule #9). `None` means
    /// no bundled tiles — the cache falls through to the upstream fetch as
    /// normal. Can also be overridden at startup via `PARISH_BUNDLED_TILES_DIR`.
    #[serde(default)]
    pub bundled_tiles_dir: Option<std::path::PathBuf>,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            default_tile_source: default_tile_source_id(),
            tile_sources: default_tile_sources(),
            bundled_tiles_dir: None,
        }
    }
}

impl MapConfig {
    /// Fold baked-in defaults into the registry for any id the user didn't
    /// override. Call this after deserialising `parish.toml` so a partial
    /// `[engine.map.tile_sources.osm]` block doesn't wipe the historic entry.
    pub fn apply_defaults(&mut self) {
        for (id, source) in default_tile_sources() {
            self.tile_sources.entry(id).or_insert(source);
        }
    }

    /// Returns tile-source entries as `(id, label)` pairs, alphabetical by id.
    /// Used by backends to populate [`parish_core::ipc::GameConfig::tile_sources`]
    /// so the `/map` command handler can list and validate without needing
    /// a reference to the whole engine config.
    pub fn id_label_pairs(&self) -> Vec<(String, String)> {
        self.tile_sources
            .iter()
            .map(|(id, src)| (id.clone(), src.label.clone()))
            .collect()
    }
}

fn default_tile_source_id() -> String {
    "historic".to_string()
}

fn default_tile_sources() -> BTreeMap<String, TileSourceConfig> {
    let mut m = BTreeMap::new();
    m.insert(
        "osm".to_string(),
        TileSourceConfig {
            label: "OpenStreetMap".to_string(),
            url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png".to_string(),
            // OSM is fetched directly by the browser; no server-side proxying.
            upstream_url: String::new(),
            tile_size: 256,
            minzoom: 0,
            maxzoom: 19,
            attribution: "© OpenStreetMap contributors".to_string(),
            raster_saturation: -0.4,
            raster_opacity: 0.85,
            tms: false,
        },
    );
    m.insert(
        "historic".to_string(),
        TileSourceConfig {
            label: "Historic 6\" OS Ireland (1st ed., via NLS)".to_string(),
            // Ordnance Survey of Ireland First Edition 6-inch (surveyed 1829–1842),
            // scanned and hosted by the National Library of Scotland. NLS serves
            // Ireland as 32 separate per-county tilesets under `os/<county>1/`
            // — there is no seamless all-Ireland layer. Rundale is set in
            // County Roscommon, so we wire the Roscommon 1st-edition sheet;
            // expanding to whole-island coverage will require a multi-source
            // style (see issue #360).
            //
            // Terms: CC-BY per https://maps.nls.uk/copyright.html
            // (no version specified by NLS; CC-BY ≥ 3.0 implied). Per-sheet
            // viewers link the licence at the #noncommercial anchor.
            // Required attribution: "Reproduced with the permission of the
            // National Library of Scotland". Downstream may be relicensed
            // under CC-BY-SA per Creative Commons one-way compatibility.
            //
            // `url` is the same-origin proxy path the browser hits (issue #360);
            // `upstream_url` is the absolute NLS S3 URL the server-side
            // tile cache fetches from on a miss. The path segment after
            // `/tiles/` must match the registering key so `tile_routes`'s
            // validator accepts the request (PR #955).
            url: "/tiles/historic/{z}/{x}/{y}.png".to_string(),
            upstream_url:
                "https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png"
                    .to_string(),
            tile_size: 256,
            minzoom: 1,
            maxzoom: 17,
            attribution:
                "Historic 6\" OS Ireland (1829–1842) — Reproduced with the permission of the National Library of Scotland (CC-BY)"
                    .to_string(),
            raster_saturation: 0.0,
            raster_opacity: 1.0,
            tms: false,
        },
    );
    m
}

/// A single raster tile source — URL template plus display metadata.
#[derive(Debug, Deserialize, Serialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TileSourceConfig {
    /// Human-readable label displayed in `/map` listings.
    #[serde(default)]
    pub label: String,
    /// XYZ URL template the **frontend** uses to fetch tiles (e.g.
    /// `https://…/{z}/{x}/{y}.png`, or a same-origin proxy path like
    /// `/tiles/historic/{z}/{x}/{y}.png`). Empty string means the source is
    /// registered but not yet configured; the frontend falls back to a flat
    /// background.
    #[serde(default)]
    pub url: String,
    /// Upstream XYZ URL template the **server-side tile cache** fetches from
    /// on a cache miss (must be absolute, e.g.
    /// `https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`).
    /// Empty string means this source isn't proxied — the frontend's `url`
    /// is expected to be an absolute upstream URL the browser hits directly
    /// (the OSM case), and the tile-proxy route will refuse to serve it.
    ///
    /// Kept distinct from `url` because the two represent different layers:
    /// `url` is what MapLibre fetches, `upstream_url` is what `reqwest` fetches
    /// on the server. Conflating them is what caused PR #955.
    #[serde(default)]
    pub upstream_url: String,
    /// Tile edge length in pixels. 256 for classic OSM-style sources.
    #[serde(default = "default_tile_size")]
    pub tile_size: u32,
    /// Minimum zoom level the source serves tiles for.
    #[serde(default)]
    pub minzoom: u32,
    /// Maximum zoom level the source serves tiles for.
    #[serde(default = "default_tile_maxzoom")]
    pub maxzoom: u32,
    /// Attribution text shown in the MapLibre attribution control.
    #[serde(default)]
    pub attribution: String,
    /// MapLibre `raster-saturation` paint (-1.0 to 1.0). Negative values
    /// desaturate; 0.0 leaves colours untouched.
    #[serde(default = "default_raster_saturation")]
    pub raster_saturation: f32,
    /// MapLibre `raster-opacity` paint (0.0 to 1.0).
    #[serde(default = "default_raster_opacity")]
    pub raster_opacity: f32,
    /// When true, the frontend sets `scheme: 'tms'` on the MapLibre source,
    /// flipping the y-axis for ArcGIS-style tile services.
    #[serde(default)]
    pub tms: bool,
}

fn default_tile_size() -> u32 {
    256
}
fn default_tile_maxzoom() -> u32 {
    19
}
fn default_raster_saturation() -> f32 {
    -0.4
}
fn default_raster_opacity() -> f32 {
    0.85
}
