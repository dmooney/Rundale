//! parish-geo-tool — Download geographic data from OpenStreetMap and convert to Parish game data.
//!
//! A development tool that queries the Overpass API for real Irish geographic
//! features and converts them into the `parish.json` world graph format used
//! by the Parish game engine.
//!
//! # Usage
//!
//! ```sh
//! # Generate parish data for a specific area by name
//! cargo run -p parish-geo-tool -- --area "Kiltoom" --level parish
//!
//! # Generate for a bounding box
//! cargo run -p parish-geo-tool -- --bbox 53.45,-8.05,53.55,-7.95
//!
//! # Merge with existing hand-authored data
//! cargo run -p parish-geo-tool -- --area "Kiltoom" --merge data/parish.json
//!
//! # Generate for a full county
//! cargo run -p parish-geo-tool -- --area "Roscommon" --level county
//! ```

mod cache;
mod connections;
mod descriptions;
mod extract;
mod lod;
mod merge;
mod osm_model;
mod output;
mod overpass;
mod pipeline;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

/// Geographic data conversion tool for the Parish game engine.
///
/// Downloads real geographic features from OpenStreetMap and converts them
/// into the parish.json world graph format.
#[derive(Parser, Debug)]
#[command(name = "parish-geo-tool", version, about)]
struct Cli {
    /// Named area to query (e.g., "Kiltoom", "Roscommon", "Athlone").
    #[arg(long)]
    area: Option<String>,

    /// Bounding box as south,west,north,east (e.g., 53.45,-8.05,53.55,-7.95).
    #[arg(long, value_delimiter = ',', num_args = 4)]
    bbox: Option<Vec<f64>>,

    /// Administrative level to query at.
    #[arg(long, default_value = "parish")]
    level: AdminLevel,

    /// Level of detail for location extraction.
    #[arg(long, default_value = "full")]
    detail: lod::DetailLevel,

    /// Merge with an existing parish.json file (hand-authored locations preserved).
    #[arg(long)]
    merge: Option<PathBuf>,

    /// Output file path.
    #[arg(long, short, default_value = "data/parish-generated.json")]
    output: PathBuf,

    /// Cache directory for Overpass API responses.
    #[arg(long, default_value = "data/cache/geo")]
    cache_dir: PathBuf,

    /// Skip cache and always re-download.
    #[arg(long)]
    no_cache: bool,

    /// Dry run — show what would be queried without downloading.
    #[arg(long)]
    dry_run: bool,

    /// Starting location ID offset for generated locations.
    ///
    /// When merging, auto-detected from existing data. Otherwise defaults to 1.
    #[arg(long)]
    id_offset: Option<u32>,

    /// Maximum number of locations to generate (0 = unlimited).
    #[arg(long, default_value = "0")]
    max_locations: usize,
}

/// Administrative district level for geographic queries.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AdminLevel {
    /// Single townland (~50-200 acres).
    Townland,
    /// Civil parish (group of townlands).
    Parish,
    /// Barony (group of parishes).
    Barony,
    /// County.
    County,
    /// Province (Connacht, Leinster, Munster, Ulster).
    Province,
}

impl AdminLevel {
    /// Returns the OSM admin_level value for Overpass queries.
    ///
    /// Irish administrative boundaries in OSM use these levels:
    /// - 6 = county
    /// - 7 = barony (historical)
    /// - 8 = civil parish
    /// - 9 = electoral division
    /// - 10 = townland
    pub fn osm_admin_level(self) -> u8 {
        match self {
            Self::Townland => 10,
            Self::Parish => 8,
            Self::Barony => 7,
            Self::County => 6,
            Self::Province => 5,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    pipeline::run(pipeline::PipelineConfig {
        area: cli.area,
        bbox: cli.bbox.map(|v| overpass::BoundingBox {
            south: v[0],
            west: v[1],
            north: v[2],
            east: v[3],
        }),
        level: cli.level,
        detail: cli.detail,
        merge_path: cli.merge,
        output_path: cli.output,
        cache_dir: cli.cache_dir,
        no_cache: cli.no_cache,
        dry_run: cli.dry_run,
        id_offset: cli.id_offset,
        max_locations: cli.max_locations,
    })
    .await
    .context("parish-geo-tool pipeline failed")
}
