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
mod historic;
mod lod;
mod merge;
mod osm_model;
mod output;
mod overpass;
mod pipeline;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

/// Geographic data conversion tool for the Parish game engine.
///
/// Defaults to the OSM / Overpass pipeline for backwards compatibility.
/// Use the `historic-discover` subcommand for the vision-driven 1820s
/// OS 6-inch pipeline.
#[derive(Parser, Debug)]
#[command(name = "parish-geo-tool", version, about)]
struct Cli {
    /// Optional subcommand. If omitted, the top-level flags drive the
    /// legacy OSM pipeline.
    #[command(subcommand)]
    command: Option<Command>,

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

/// Subcommands layered on top of the legacy OSM flags.
#[derive(Subcommand, Debug)]
enum Command {
    /// AI-assisted first-pass world generation from historical map tiles.
    ///
    /// Fetches tiled raster imagery of the target bbox, feeds 2×2 tile
    /// chunks to a vision-capable LLM to identify man-made features, and
    /// writes a `parish.json` of candidate locations placed to the OS 6"
    /// First Edition (ca. 1829–42) positions.
    HistoricDiscover {
        /// Bounding box as south,west,north,east.
        #[arg(long, value_delimiter = ',', num_args = 4, required = true)]
        bbox: Vec<f64>,

        /// Tile zoom level. Higher = more detail, more tiles, more LLM calls.
        #[arg(long, default_value = "16")]
        zoom: u8,

        /// Explicit tile-source id (e.g. `nls-roscommon`). If omitted,
        /// the first source covering the bbox is used.
        #[arg(long)]
        tile_source: Option<String>,

        /// OpenAI-compatible provider base URL (OpenRouter / Ollama / ...).
        #[arg(long, default_value = "https://openrouter.ai/api")]
        provider_url: String,

        /// Vision-capable model id.
        #[arg(long, default_value = "anthropic/claude-opus-4-7")]
        model: String,

        /// Environment variable name holding the API key.
        #[arg(long, default_value = "OPENROUTER_API_KEY")]
        api_key_env: String,

        /// Drop vision features below this confidence (0.0..1.0).
        #[arg(long, default_value = "0.5")]
        confidence_floor: f32,

        /// Optional model id for the text-only naming call. Defaults to
        /// `--model` if omitted.
        #[arg(long)]
        naming_model: Option<String>,

        /// Cache directory for fetched PNG tiles.
        #[arg(long, default_value = "data/cache/historic")]
        cache_dir: PathBuf,

        /// Skip the tile cache and always re-download.
        #[arg(long)]
        no_cache: bool,

        /// Output file path for the generated parish.json.
        #[arg(long, short, default_value = "data/parish-historic.json")]
        output: PathBuf,

        /// Starting LocationId for generated features.
        #[arg(long, default_value = "1")]
        id_offset: u32,
    },
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

    if let Some(Command::HistoricDiscover {
        bbox,
        zoom,
        tile_source,
        provider_url,
        model,
        api_key_env,
        confidence_floor,
        naming_model,
        cache_dir,
        no_cache,
        output,
        id_offset,
    }) = cli.command
    {
        return run_historic_discover(HistoricDiscoverArgs {
            bbox,
            zoom,
            tile_source,
            provider_url,
            model,
            api_key_env,
            confidence_floor,
            naming_model,
            cache_dir,
            no_cache,
            output,
            id_offset,
        })
        .await
        .context("geo-tool historic-discover failed");
    }

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

/// Parsed arguments for the `historic-discover` subcommand.
struct HistoricDiscoverArgs {
    bbox: Vec<f64>,
    zoom: u8,
    tile_source: Option<String>,
    provider_url: String,
    model: String,
    api_key_env: String,
    confidence_floor: f32,
    naming_model: Option<String>,
    cache_dir: PathBuf,
    no_cache: bool,
    output: PathBuf,
    id_offset: u32,
}

async fn run_historic_discover(args: HistoricDiscoverArgs) -> Result<()> {
    use anyhow::{anyhow, bail};

    if args.bbox.len() != 4 {
        bail!("--bbox must be four comma-separated values: south,west,north,east");
    }
    let bbox = overpass::BoundingBox {
        south: args.bbox[0],
        west: args.bbox[1],
        north: args.bbox[2],
        east: args.bbox[3],
    };

    let registry = historic::tile_source::TileSourceRegistry::default();
    let source = match &args.tile_source {
        Some(id) => registry
            .by_id(id)
            .cloned()
            .ok_or_else(|| anyhow!("no tile source registered with id `{id}`"))?,
        None => registry
            .for_bbox(&bbox)
            .cloned()
            .ok_or_else(|| anyhow!("no registered tile source covers bbox {bbox}"))?,
    };

    let cache = historic::raster_cache::RasterCache::new(&args.cache_dir, args.no_cache);
    let fetcher = historic::discover::CachedTileFetcher {
        source: source.clone(),
        cache,
    };

    let api_key = std::env::var(&args.api_key_env).ok();
    if api_key.is_none() {
        tracing::warn!(
            "API key env var `{}` is unset; the vision call will fail unless \
             the provider accepts anonymous requests (e.g. a local Ollama)",
            args.api_key_env
        );
    }

    let openai =
        parish_inference::openai_client::OpenAiClient::new(&args.provider_url, api_key.as_deref());

    let vision = historic::discover::OpenAiVisionClient {
        client: openai.clone(),
        model: args.model.clone(),
        max_tokens: Some(4096),
    };

    let mut config = historic::discover::DiscoverConfig::new(bbox, args.zoom, args.id_offset);
    config.confidence_floor = args.confidence_floor;
    config.naming_model = args.naming_model.clone();

    let naming_model = args.naming_model.as_deref().unwrap_or(&args.model);
    let naming_client = Some((&openai, naming_model));

    tracing::info!(
        "historic-discover: source={} model={} zoom={} bbox={bbox}",
        source.id(),
        args.model,
        args.zoom,
    );

    let (tracked, audit) =
        historic::discover::run(&fetcher, &vision, naming_client, &config).await?;

    println!(
        "historic-discover: {} locations ({} dropped low-confidence, {} dropped as duplicates)",
        audit.features_emitted,
        audit.features_dropped_low_confidence,
        audit.features_dropped_duplicate,
    );

    output::write_output(&args.output, &tracked)?;
    match output::validate_output(&args.output) {
        Ok(()) => println!("Validation: PASSED"),
        Err(e) => println!("Validation: FAILED — {e}"),
    }
    output::print_summary(&tracked);

    // Write a tiny audit sidecar alongside the metadata written by `output`.
    let audit_path = args.output.with_extension("audit.json");
    std::fs::write(&audit_path, serde_json::to_string_pretty(&audit)?)
        .with_context(|| format!("writing audit sidecar {}", audit_path.display()))?;
    println!("Wrote audit sidecar: {}", audit_path.display());

    Ok(())
}
