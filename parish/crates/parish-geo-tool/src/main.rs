//! parish-geo-tool — geographic data tools for the Parish game engine.
//!
//! # Subcommands
//!
//! ## `generate` — Download OSM data and convert to Parish world graph
//!
//! ```sh
//! cargo run -p parish-geo-tool -- generate --area "Kiltoom" --level parish
//! cargo run -p parish-geo-tool -- generate --bbox 53.45,-8.05,53.55,-7.95
//! cargo run -p parish-geo-tool -- generate --area "Kiltoom" --merge data/parish.json
//! ```
//!
//! ## `seed-tiles` — Pre-generate stylized map tiles into a bundled directory
//!
//! ```sh
//! cargo run -p parish-geo-tool -- seed-tiles \
//!   --style parchment \
//!   --bbox 53.45,-8.15,53.65,-7.85 \
//!   --zoom 10-17 \
//!   --source-id rundale-map \
//!   --output mods/rundale/tiles/
//!
//! # AI-enhanced (requires local Stable Diffusion WebUI):
//! cargo run -p parish-geo-tool -- seed-tiles \
//!   --style diffusion \
//!   --diffusion-endpoint http://localhost:7860 \
//!   --bbox 53.45,-8.15,53.65,-7.85 \
//!   --zoom 14-17 \
//!   --output mods/rundale/tiles/
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
#[cfg(test)]
mod test_utils;
mod tile_seeder;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

/// Geographic data tools for the Parish game engine.
#[derive(Parser, Debug)]
#[command(name = "parish-geo-tool", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Download OSM geographic data and convert to a Parish world graph.
    Generate(GenerateCli),
    /// Pre-generate stylized map tiles from an NLS upstream source.
    SeedTiles(SeedTilesCli),
}

// ── generate ─────────────────────────────────────────────────────────────────

/// Generate Parish world graph data from OpenStreetMap.
#[derive(Args, Debug)]
struct GenerateCli {
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

    /// Skip reading from cache (responses are still written to cache).
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

// ── seed-tiles ───────────────────────────────────────────────────────────────

/// Pre-generate stylized NLS map tiles into a bundled directory.
#[derive(Args, Debug)]
struct SeedTilesCli {
    /// Upstream XYZ tile URL template.
    ///
    /// Defaults to the NLS OS Ireland 1st edition Roscommon tileset.
    #[arg(
        long,
        default_value = "https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png"
    )]
    upstream: String,

    /// Bounding box as south,west,north,east.
    #[arg(long, value_delimiter = ',', num_args = 4)]
    bbox: Vec<f64>,

    /// Zoom levels to seed, e.g. "10-17" or "14".
    #[arg(long, default_value = "10-17")]
    zoom: String,

    /// Source ID used as the output subdirectory name.
    #[arg(long, default_value = "rundale-map")]
    source_id: String,

    /// Output root directory (set this as bundled_tiles_dir in parish.toml).
    #[arg(long, short, default_value = "data/tiles")]
    output: PathBuf,

    /// Art style backend.
    #[arg(long, default_value = "parchment")]
    style: StyleArg,

    /// Parchment: ink overlay strength (0.0–1.0).
    #[arg(long, default_value = "0.40")]
    ink_strength: f32,

    /// Parchment: paper grain strength (0.0–1.0).
    #[arg(long, default_value = "0.07")]
    grain_strength: f32,

    /// Diffusion: Stable Diffusion WebUI base URL (required for --style diffusion).
    #[arg(long)]
    diffusion_endpoint: Option<String>,

    /// Diffusion: checkpoint model name (omit to use SD's currently loaded model).
    #[arg(long)]
    diffusion_model: Option<String>,

    /// Diffusion: denoising strength (0.0–1.0).
    #[arg(long, default_value = "0.65")]
    denoising_strength: f32,

    /// Diffusion: ControlNet Canny weight (0.0–1.0).
    #[arg(long, default_value = "0.85")]
    controlnet_strength: f32,

    /// Number of concurrent tile fetch + paint tasks.
    #[arg(long, default_value = "4")]
    concurrency: usize,

    /// Cache directory for downloaded raw tiles (avoids re-fetching on retry runs).
    #[arg(long, default_value = "data/cache/tiles")]
    raw_cache: PathBuf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StyleArg {
    Parchment,
    Diffusion,
}

// ── admin levels (unchanged) ─────────────────────────────────────────────────

/// Administrative district level for geographic queries.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AdminLevel {
    /// Single townland (~50–200 acres).
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

// ── main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Command::Generate(g) => run_generate(g).await,
        Command::SeedTiles(s) => run_seed_tiles(s).await,
    }
}

async fn run_generate(g: GenerateCli) -> Result<()> {
    pipeline::run(pipeline::PipelineConfig {
        area: g.area,
        bbox: g.bbox.map(|v| overpass::BoundingBox {
            south: v[0],
            west: v[1],
            north: v[2],
            east: v[3],
        }),
        level: g.level,
        detail: g.detail,
        merge_path: g.merge,
        output_path: g.output,
        cache_dir: g.cache_dir,
        no_cache: g.no_cache,
        dry_run: g.dry_run,
        id_offset: g.id_offset,
        max_locations: g.max_locations,
    })
    .await
    .context("parish-geo-tool generate failed")
}

async fn run_seed_tiles(s: SeedTilesCli) -> Result<()> {
    use parish_tile_art::{ArtConfig, ParchmentConfig};

    if s.bbox.len() != 4 {
        anyhow::bail!("--bbox requires exactly 4 values: south,west,north,east");
    }

    let zoom_levels = tile_seeder::parse_zoom_range(&s.zoom)?;

    let art_config = match s.style {
        StyleArg::Parchment => ArtConfig::Parchment(ParchmentConfig {
            ink_strength: s.ink_strength,
            grain_strength: s.grain_strength,
            smooth_radius: 1,
        }),
        StyleArg::Diffusion => {
            let endpoint = s.diffusion_endpoint.ok_or_else(|| {
                anyhow::anyhow!("--diffusion-endpoint is required for --style diffusion")
            })?;
            ArtConfig::Diffusion {
                endpoint,
                model: s.diffusion_model,
                prompt: parish_tile_art::config::default_diffusion_prompt_pub(),
                denoising_strength: s.denoising_strength,
                controlnet_strength: s.controlnet_strength,
                fallback: ParchmentConfig {
                    ink_strength: s.ink_strength,
                    grain_strength: s.grain_strength,
                    smooth_radius: 1,
                },
            }
        }
    };

    tile_seeder::run(tile_seeder::SeedConfig {
        upstream_url: s.upstream,
        south: s.bbox[0],
        west: s.bbox[1],
        north: s.bbox[2],
        east: s.bbox[3],
        zoom_levels,
        source_id: s.source_id,
        output_dir: s.output,
        concurrency: s.concurrency,
        raw_cache_dir: s.raw_cache,
        art_config,
    })
    .await
    .context("seed-tiles failed")
}
