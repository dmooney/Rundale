//! Historic-map discovery orchestrator.
//!
//! Stitches 2×2 tile chunks into 512×512 images, feeds each chunk to a
//! vision-capable LLM, normalises the resulting features into
//! [`TrackedLocation`]s, and builds the connection graph. The orchestrator
//! is mockable via the [`TileFetcher`] and [`VisionClient`] traits so
//! unit tests can exercise the full pipeline without network access.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use image::{ImageBuffer, ImageEncoder, RgbaImage};
use parish_core::world::LocationId;
use parish_core::world::graph::{Connection, GeoKind, LocationData};
use parish_inference::openai_client::{ImageInput, OpenAiClient};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::super::descriptions::DescriptionSource;
use super::super::merge::TrackedLocation;
use super::super::osm_model::{LocationType, haversine_distance};
use super::super::overpass::BoundingBox;
use super::naming::{NamedFeature, NamingRequestFeature, generate_names};
use super::raster_cache::RasterCache;
use super::tile_math::{TILE_SIZE_PX, tile_pixel_to_lonlat, tile_range_for_bbox};
use super::tile_source::HistoricTileSource;
use super::vision_prompt::{
    PxSegment, VISION_SYSTEM_PROMPT, VisionFeature, VisionFeatureKind, VisionResponse,
    user_instruction,
};

/// Side length (in tiles) of a stitched vision chunk.
const CHUNK_TILE_SIDE: u32 = 2;

/// Pixel side of a stitched chunk image (CHUNK_TILE_SIDE × TILE_SIZE_PX).
const CHUNK_PX: u32 = CHUNK_TILE_SIDE * TILE_SIZE_PX;

/// Drop vision features with confidence below this threshold.
pub const DEFAULT_CONFIDENCE_FLOOR: f32 = 0.5;

/// Features closer than this are treated as duplicates during dedup (metres).
const DEDUP_DISTANCE_M: f64 = 30.0;

/// Snap a vision-reported road segment endpoint to the nearest feature within this range (metres).
const SEGMENT_SNAP_M: f64 = 60.0;

/// Fall back to straight-line connections within this range if segments are absent (metres).
const FALLBACK_CONNECTION_M: f64 = 2_000.0;

/// Abstract source of PNG tile bytes. Real callers use a `HistoricTileSource`
/// wrapped in [`CachedTileFetcher`]; tests use in-memory mocks.
#[async_trait]
pub trait TileFetcher: Send + Sync {
    async fn fetch(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>>;
    fn attribution(&self) -> &str;
    fn source_id(&self) -> &str;
}

/// Abstract vision-capable JSON chat client. Real callers use
/// [`OpenAiVisionClient`]; tests use scripted mocks.
#[async_trait]
pub trait VisionClient: Send + Sync {
    /// Sends a stitched chunk image plus instructions, returns the parsed
    /// vision schema.
    async fn analyse(&self, user_text: &str, image_png: Vec<u8>) -> Result<VisionResponse>;
}

/// Cache-wrapping tile fetcher backed by a `HistoricTileSource`.
pub struct CachedTileFetcher {
    pub source: Arc<dyn HistoricTileSource>,
    pub cache: RasterCache,
}

#[async_trait]
impl TileFetcher for CachedTileFetcher {
    async fn fetch(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>> {
        if let Some(bytes) = self.cache.get(self.source.id(), z, x, y)? {
            return Ok(bytes);
        }
        let bytes = self.source.fetch_tile(z, x, y).await?;
        self.cache.put(self.source.id(), z, x, y, &bytes)?;
        Ok(bytes)
    }

    fn attribution(&self) -> &str {
        self.source.attribution()
    }

    fn source_id(&self) -> &str {
        self.source.id()
    }
}

/// Production `VisionClient` using an `OpenAiClient` behind the
/// `generate_json_with_images` path.
pub struct OpenAiVisionClient {
    pub client: OpenAiClient,
    pub model: String,
    pub max_tokens: Option<u32>,
}

#[async_trait]
impl VisionClient for OpenAiVisionClient {
    async fn analyse(&self, user_text: &str, image_png: Vec<u8>) -> Result<VisionResponse> {
        let image = ImageInput::png(image_png);
        let resp = self
            .client
            .generate_json_with_images::<VisionResponse>(
                &self.model,
                user_text,
                Some(VISION_SYSTEM_PROMPT),
                std::slice::from_ref(&image),
                self.max_tokens,
                Some(0.0),
            )
            .await
            .map_err(|e| anyhow!("vision call failed: {e}"))?;
        Ok(resp)
    }
}

/// Configuration for [`run`].
#[derive(Debug, Clone)]
pub struct DiscoverConfig {
    /// Area to discover features in.
    pub bbox: BoundingBox,
    /// Tile zoom to fetch at.
    pub zoom: u8,
    /// Drop features below this confidence (default [`DEFAULT_CONFIDENCE_FLOOR`]).
    pub confidence_floor: f32,
    /// Starting LocationId for the generated features.
    pub id_offset: u32,
    /// Optional model name used for naming unnamed features (text-only call).
    pub naming_model: Option<String>,
}

impl DiscoverConfig {
    pub fn new(bbox: BoundingBox, zoom: u8, id_offset: u32) -> Self {
        Self {
            bbox,
            zoom,
            confidence_floor: DEFAULT_CONFIDENCE_FLOOR,
            id_offset,
            naming_model: None,
        }
    }
}

/// Per-run audit record, emitted alongside `TrackedLocation`s so the
/// caller can write a sidecar metadata file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverAudit {
    pub source_id: String,
    pub attribution: String,
    pub tile_zoom: u8,
    pub features_emitted: usize,
    pub features_dropped_low_confidence: usize,
    pub features_dropped_duplicate: usize,
    /// Dropped because the projected `(lat, lon)` landed outside `config.bbox`.
    /// Happens at the bbox edge when the tile grid covers one or two extra
    /// rows/columns beyond the requested area.
    pub features_dropped_out_of_bbox: usize,
}

/// Returns true if `(lat, lon)` falls inside `bbox` (inclusive edges).
fn bbox_contains(bbox: &BoundingBox, lat: f64, lon: f64) -> bool {
    lat >= bbox.south && lat <= bbox.north && lon >= bbox.west && lon <= bbox.east
}

/// Intermediate record: a vision feature already normalised to WGS-84.
///
/// `map_labelled` records whether the *map* carried a label for this feature
/// (i.e. the vision model transcribed a name off the engraving). Labels
/// invented later by the naming pass are written to `label_text` but keep
/// `map_labelled = false`, so downstream `GeoKind` stays `Fictional`.
#[derive(Debug, Clone)]
struct GeolocatedVisionFeature {
    lat: f64,
    lon: f64,
    label_text: Option<String>,
    feature_kind: VisionFeatureKind,
    /// True iff the label originated on the map (not from the naming LLM).
    map_labelled: bool,
    /// Vision-reported confidence (0..1). Kept so dedup can prefer higher-
    /// confidence detections when collapsing near-duplicates.
    confidence: f32,
    /// Road/path endpoints in lat/lon — already projected from pixel coords.
    connected_segments: Vec<(f64, f64)>,
}

/// Entry point for the historic-discover pipeline.
///
/// - Iterates the covering tile grid in 2×2 chunks
/// - Fetches tiles (cache-first), stitches each chunk to a 512×512 PNG
/// - Calls the vision client per chunk, collects features
/// - Filters by confidence, converts pixel→lat/lon, dedupes by proximity
/// - Optionally names unlabelled features via the text-only client
/// - Builds bidirectional connections from vision-reported segments +
///   a straight-line nearest-neighbour fallback
pub async fn run<F, V>(
    tiles: &F,
    vision: &V,
    naming_client: Option<(&OpenAiClient, &str)>,
    config: &DiscoverConfig,
) -> Result<(Vec<TrackedLocation>, DiscoverAudit)>
where
    F: TileFetcher + ?Sized,
    V: VisionClient + ?Sized,
{
    let (min_x, min_y, max_x, max_y) = tile_range_for_bbox(&config.bbox, config.zoom);
    if min_x > max_x || min_y > max_y {
        bail!(
            "empty tile range for bbox {:?} at z={}",
            config.bbox,
            config.zoom
        );
    }
    info!(
        "historic-discover: bbox {:?} -> tiles x={min_x}..={max_x} y={min_y}..={max_y} at z={}",
        config.bbox, config.zoom
    );

    let mut collected: Vec<GeolocatedVisionFeature> = Vec::new();
    let mut dropped_low_conf = 0usize;
    let mut dropped_out_of_bbox = 0usize;

    // Iterate 2-tile-by-2-tile chunks aligned to the min corner.
    //
    // Edge chunks may overlap neighbours by one tile (or fetch tiles that
    // lie just outside the bbox tile range) when the range has an odd span.
    // We fetch the full 2x2 regardless — the source's own coverage check
    // already rejected clearly out-of-region bboxes — and filter any
    // projected features that land outside `config.bbox` after projection.
    let mut cy = min_y;
    while cy <= max_y {
        let mut cx = min_x;
        while cx <= max_x {
            let chunk_tiles = [(cx, cy), (cx + 1, cy), (cx, cy + 1), (cx + 1, cy + 1)];
            let png_bytes = fetch_and_stitch_chunk(tiles, config.zoom, &chunk_tiles).await?;
            let instruction = user_instruction();
            let resp = match vision.analyse(&instruction, png_bytes).await {
                Ok(r) => r,
                Err(e) => {
                    warn!("vision call failed for chunk ({cx},{cy}): {e:?}; skipping chunk");
                    cx += CHUNK_TILE_SIDE;
                    continue;
                }
            };

            for f in resp.features {
                if f.confidence < config.confidence_floor {
                    dropped_low_conf += 1;
                    continue;
                }
                let projected = project_feature(f, config.zoom, cx, cy);
                if !bbox_contains(&config.bbox, projected.lat, projected.lon) {
                    dropped_out_of_bbox += 1;
                    continue;
                }
                collected.push(projected);
            }

            cx += CHUNK_TILE_SIDE;
        }
        cy += CHUNK_TILE_SIDE;
    }

    info!(
        "collected {} features from vision ({} dropped below confidence)",
        collected.len(),
        dropped_low_conf,
    );

    let before_dedup = collected.len();
    let deduped = dedupe_by_proximity(collected, DEDUP_DISTANCE_M);
    let dropped_dup = before_dedup - deduped.len();
    info!("dedup: {} -> {} features", before_dedup, deduped.len());

    let named = resolve_names(deduped, naming_client, config.naming_model.as_deref()).await?;

    let tracked = build_tracked_locations(named, tiles.attribution(), config.id_offset);

    let audit = DiscoverAudit {
        source_id: tiles.source_id().to_string(),
        attribution: tiles.attribution().to_string(),
        tile_zoom: config.zoom,
        features_emitted: tracked.len(),
        features_dropped_low_confidence: dropped_low_conf,
        features_dropped_duplicate: dropped_dup,
        features_dropped_out_of_bbox: dropped_out_of_bbox,
    };

    Ok((tracked, audit))
}

/// Fetches four tiles and stitches them into a 512×512 PNG.
///
/// Tiles are pasted at their correct offset inside the chunk. If any tile
/// fails to decode, the function returns an error — partial stitches would
/// leave the LLM with black quadrants that could hallucinate features.
async fn fetch_and_stitch_chunk<F: TileFetcher + ?Sized>(
    tiles: &F,
    z: u8,
    chunk_tiles: &[(u32, u32); 4],
) -> Result<Vec<u8>> {
    let mut canvas: RgbaImage = ImageBuffer::new(CHUNK_PX, CHUNK_PX);

    for (tile_x, tile_y) in chunk_tiles {
        let bytes = tiles
            .fetch(z, *tile_x, *tile_y)
            .await
            .with_context(|| format!("fetching tile z={z} x={tile_x} y={tile_y}"))?;
        let img = image::load_from_memory(&bytes)
            .with_context(|| format!("decoding tile z={z} x={tile_x} y={tile_y}"))?
            .to_rgba8();

        // Position within the stitched chunk: min corner = chunk_tiles[0].
        let rel_x = (tile_x - chunk_tiles[0].0) * TILE_SIZE_PX;
        let rel_y = (tile_y - chunk_tiles[0].1) * TILE_SIZE_PX;

        paste_into(&mut canvas, &img, rel_x, rel_y);
    }

    let mut png: Vec<u8> = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            canvas.as_raw(),
            canvas.width(),
            canvas.height(),
            image::ExtendedColorType::Rgba8,
        )
        .context("encoding stitched chunk PNG")?;
    Ok(png)
}

fn paste_into(canvas: &mut RgbaImage, src: &RgbaImage, off_x: u32, off_y: u32) {
    for sy in 0..src.height() {
        for sx in 0..src.width() {
            let dx = off_x + sx;
            let dy = off_y + sy;
            if dx < canvas.width() && dy < canvas.height() {
                let pixel = src.get_pixel(sx, sy);
                canvas.put_pixel(dx, dy, *pixel);
            }
        }
    }
}

/// Converts a `VisionFeature`'s chunk-local pixel coordinates to WGS-84.
fn project_feature(
    f: VisionFeature,
    z: u8,
    chunk_x0: u32,
    chunk_y0: u32,
) -> GeolocatedVisionFeature {
    let (lat, lon) = chunk_px_to_lonlat(f.px, f.py, z, chunk_x0, chunk_y0);
    let connected_segments: Vec<(f64, f64)> = f
        .connected_px_segments
        .iter()
        .map(|seg: &PxSegment| chunk_px_to_lonlat(seg.to_px, seg.to_py, z, chunk_x0, chunk_y0))
        .collect();
    let label_text = f.label_text.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let map_labelled = label_text.is_some();
    GeolocatedVisionFeature {
        lat,
        lon,
        label_text,
        feature_kind: f.feature_kind,
        map_labelled,
        confidence: f.confidence,
        connected_segments,
    }
}

/// Map a chunk-local `(px, py)` (where `(0, 0)` is the top-left of the
/// stitched 512×512 image) to `(lat, lon)`.
fn chunk_px_to_lonlat(px: u32, py: u32, z: u8, chunk_x0: u32, chunk_y0: u32) -> (f64, f64) {
    let px = px.min(CHUNK_PX - 1);
    let py = py.min(CHUNK_PX - 1);
    let tile_dx = px / TILE_SIZE_PX;
    let tile_dy = py / TILE_SIZE_PX;
    let inner_px = px % TILE_SIZE_PX;
    let inner_py = py % TILE_SIZE_PX;
    tile_pixel_to_lonlat(
        z,
        chunk_x0 + tile_dx,
        chunk_y0 + tile_dy,
        inner_px,
        inner_py,
    )
}

/// Drops features whose position is within `threshold_m` of an earlier feature
/// of the same kind. Keeps the earliest entry per cluster.
/// Drops features whose position is within `threshold_m` of an earlier feature
/// of the same kind. Vision output order is not stable, so we first sort to
/// put preferred detections (map-labelled, then higher confidence) ahead of
/// unlabelled / lower-confidence ones — this way the "keep-first" collapse
/// never discards a labelled detection in favour of an unlabelled duplicate.
fn dedupe_by_proximity(
    features: Vec<GeolocatedVisionFeature>,
    threshold_m: f64,
) -> Vec<GeolocatedVisionFeature> {
    let mut sorted = features;
    sorted.sort_by(|a, b| {
        // `map_labelled` first (true > false), then confidence desc.
        b.map_labelled.cmp(&a.map_labelled).then_with(|| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let mut kept: Vec<GeolocatedVisionFeature> = Vec::new();
    for f in sorted {
        let is_dup = kept.iter().any(|k| {
            k.feature_kind == f.feature_kind
                && haversine_distance(k.lat, k.lon, f.lat, f.lon) < threshold_m
        });
        if !is_dup {
            kept.push(f);
        }
    }
    kept
}

/// Populates `label_text` for features that arrived unlabelled by asking
/// the naming client for plausible 1820s-style names in a single batch.
/// If no naming client is configured, unlabelled features remain so and
/// will be emitted with `geo_kind: Fictional`.
async fn resolve_names(
    features: Vec<GeolocatedVisionFeature>,
    naming_client: Option<(&OpenAiClient, &str)>,
    naming_model: Option<&str>,
) -> Result<Vec<GeolocatedVisionFeature>> {
    let Some((client, fallback_model)) = naming_client else {
        return Ok(features);
    };
    let model = naming_model.unwrap_or(fallback_model);

    let context_labels: Vec<String> = features
        .iter()
        .filter_map(|f| f.label_text.clone())
        .collect();
    let unnamed: Vec<(usize, NamingRequestFeature)> = features
        .iter()
        .enumerate()
        .filter(|(_, f)| f.label_text.is_none())
        .map(|(i, f)| {
            (
                i,
                NamingRequestFeature {
                    idx: i,
                    feature_kind: f.feature_kind,
                },
            )
        })
        .collect();

    if unnamed.is_empty() {
        return Ok(features);
    }

    let request_items: Vec<NamingRequestFeature> =
        unnamed.iter().map(|(_, req)| req.clone()).collect();
    let named: Vec<NamedFeature> = match generate_names(
        client,
        model,
        &context_labels,
        &request_items,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(
                "naming pass failed ({e}); unlabelled features will be emitted as Fictional without generated names"
            );
            return Ok(features);
        }
    };

    let mut name_by_idx: HashMap<usize, String> = HashMap::new();
    for nf in named {
        name_by_idx.insert(nf.idx, nf.name);
    }

    let mut out = features;
    for (idx, f) in out.iter_mut().enumerate() {
        if f.label_text.is_none()
            && let Some(name) = name_by_idx.remove(&idx)
        {
            f.label_text = Some(name);
        }
    }
    Ok(out)
}

/// Turns the deduped + named feature set into `TrackedLocation`s with
/// bidirectional connections.
fn build_tracked_locations(
    features: Vec<GeolocatedVisionFeature>,
    attribution: &str,
    id_offset: u32,
) -> Vec<TrackedLocation> {
    // Assign IDs up-front so we can reference them in connections.
    let mut ids: Vec<LocationId> = Vec::with_capacity(features.len());
    for (i, _) in features.iter().enumerate() {
        ids.push(LocationId(id_offset + i as u32));
    }

    // Build adjacency from vision-reported segments and a nearest-K fallback.
    let adjacency = build_adjacency(&features);

    let mut out: Vec<TrackedLocation> = Vec::with_capacity(features.len());
    for (i, f) in features.iter().enumerate() {
        // `Manual` is reserved for features whose label was *on the map*
        // (authoritative 1820s pin). Features that the vision model left
        // unlabelled — even if the naming pass later invented a plausible
        // name — stay `Fictional` so downstream tooling doesn't treat
        // LLM-hallucinated placenames as historical ground truth.
        let (name, geo_kind, geo_source) = if f.map_labelled {
            let label = f
                .label_text
                .clone()
                .expect("map_labelled=true implies label_text is Some");
            (label, GeoKind::Manual, Some(attribution.to_string()))
        } else {
            let name = f
                .label_text
                .clone()
                .unwrap_or_else(|| default_placeholder_name(f.feature_kind, i));
            (
                name,
                GeoKind::Fictional,
                Some(format!("{attribution} (unlabelled feature)")),
            )
        };

        let location_type = f.feature_kind.to_location_type();
        let mut connections: Vec<Connection> = Vec::new();
        for &peer_idx in adjacency.get(&i).into_iter().flatten() {
            let peer_name = features[peer_idx].label_text.clone().unwrap_or_else(|| {
                default_placeholder_name(features[peer_idx].feature_kind, peer_idx)
            });
            connections.push(Connection {
                target: ids[peer_idx],
                traversal_minutes: None,
                path_description: format!("toward {peer_name}"),
                hazard: Default::default(),
            });
        }

        let template = match location_type {
            LocationType::Crossroads => {
                "A crossroads. It is {time}. {weather}. {npcs_present}.".to_string()
            }
            LocationType::Bridge => {
                "A bridge over the water. It is {time}. {weather}. {npcs_present}.".to_string()
            }
            _ => format!(
                "{name} in the 1820s parish. It is {{time}}. {{weather}}. {{npcs_present}}."
            ),
        };

        let data = LocationData {
            id: ids[i],
            name: name.clone(),
            description_template: template,
            indoor: location_type.is_indoor(),
            public: location_type.is_public(),
            lat: f.lat,
            lon: f.lon,
            connections,
            associated_npcs: Vec::new(),
            mythological_significance: None,
            aliases: Vec::new(),
            geo_kind,
            relative_to: None,
            geo_source,
        };

        out.push(TrackedLocation {
            data,
            description_source: DescriptionSource::Template,
            osm_id: None,
            lat: f.lat,
            lon: f.lon,
        });
    }

    out
}

fn default_placeholder_name(kind: VisionFeatureKind, idx: usize) -> String {
    let stem = match kind {
        VisionFeatureKind::Village => "Small settlement",
        VisionFeatureKind::Church => "Small chapel",
        VisionFeatureKind::Mill => "Old mill",
        VisionFeatureKind::Forge => "Old forge",
        VisionFeatureKind::School => "Hedge school",
        VisionFeatureKind::PubOrInn => "Roadside inn",
        VisionFeatureKind::HolyWell => "Holy well",
        VisionFeatureKind::RingFort => "Ring fort",
        VisionFeatureKind::Farmstead => "Farmstead",
        VisionFeatureKind::Crossroads => "Crossroads",
        VisionFeatureKind::Bridge => "Stone bridge",
        VisionFeatureKind::Graveyard => "Old graveyard",
        VisionFeatureKind::Other => "Named place",
    };
    format!("{stem} {idx}")
}

/// Builds an adjacency map `feature_idx -> neighbour indices` from
/// vision-reported segments first, falling back to straight-line
/// nearest-K within `FALLBACK_CONNECTION_M`.
fn build_adjacency(features: &[GeolocatedVisionFeature]) -> HashMap<usize, Vec<usize>> {
    let mut adj: HashMap<usize, HashSet<usize>> = HashMap::new();

    // Seed from vision segments — snap each endpoint to the nearest feature
    // within SEGMENT_SNAP_M.
    for (i, f) in features.iter().enumerate() {
        for (slat, slon) in &f.connected_segments {
            let mut best: Option<(usize, f64)> = None;
            for (j, g) in features.iter().enumerate() {
                if j == i {
                    continue;
                }
                let d = haversine_distance(*slat, *slon, g.lat, g.lon);
                if d <= SEGMENT_SNAP_M && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((j, d));
                }
            }
            if let Some((j, _)) = best {
                adj.entry(i).or_default().insert(j);
                adj.entry(j).or_default().insert(i);
            }
        }
    }

    // Fallback: any feature with no neighbours gets linked to its two nearest
    // features within FALLBACK_CONNECTION_M. This mirrors the curated-to-
    // generated linking used in the OSM pipeline and keeps the graph
    // validation happy.
    for i in 0..features.len() {
        if adj.get(&i).is_some_and(|s| !s.is_empty()) {
            continue;
        }
        let mut distances: Vec<(usize, f64)> = features
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(j, g)| {
                (
                    j,
                    haversine_distance(features[i].lat, features[i].lon, g.lat, g.lon),
                )
            })
            .filter(|(_, d)| *d <= FALLBACK_CONNECTION_M)
            .collect();
        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        for (j, _) in distances.into_iter().take(2) {
            adj.entry(i).or_default().insert(j);
            adj.entry(j).or_default().insert(i);
        }
    }

    adj.into_iter()
        .map(|(k, v)| (k, v.into_iter().collect::<Vec<_>>()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::sync::Mutex;

    /// Produces a deterministic 256×256 PNG tile seeded by `(z, x, y)` so the
    /// stitch step has something non-trivial to work with.
    fn synthetic_tile(z: u8, x: u32, y: u32) -> Vec<u8> {
        let mut img: RgbaImage = ImageBuffer::new(TILE_SIZE_PX, TILE_SIZE_PX);
        let seed = (z as u32).wrapping_add(x).wrapping_add(y);
        for py in 0..TILE_SIZE_PX {
            for px in 0..TILE_SIZE_PX {
                let r = ((px ^ seed) & 0xff) as u8;
                let g = ((py ^ seed) & 0xff) as u8;
                let b = ((px.wrapping_add(py) ^ seed) & 0xff) as u8;
                img.put_pixel(px, py, Rgba([r, g, b, 255]));
            }
        }
        let mut png: Vec<u8> = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        png
    }

    struct SyntheticTiles;

    #[async_trait]
    impl TileFetcher for SyntheticTiles {
        async fn fetch(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>> {
            Ok(synthetic_tile(z, x, y))
        }
        fn attribution(&self) -> &str {
            "synthetic test tiles"
        }
        fn source_id(&self) -> &str {
            "synthetic"
        }
    }

    /// Vision mock: returns a pre-scripted list of features on the Nth call,
    /// and empty responses for any subsequent calls. Tracks call count.
    struct ScriptedVision {
        responses: Mutex<Vec<VisionResponse>>,
        calls: Mutex<usize>,
    }

    #[async_trait]
    impl VisionClient for ScriptedVision {
        async fn analyse(&self, _user_text: &str, image_png: Vec<u8>) -> Result<VisionResponse> {
            // Sanity: the stitched chunk must be a valid 512x512 PNG.
            let img = image::load_from_memory(&image_png)?;
            assert_eq!(img.width(), CHUNK_PX);
            assert_eq!(img.height(), CHUNK_PX);

            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                Ok(VisionResponse { features: vec![] })
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    #[tokio::test]
    async fn test_discover_pipeline_end_to_end_with_mocks() {
        // Pick a bbox a little larger than one z=15 tile (~1.2 km) so the
        // tile grid resolves to (min_x, min_y) = (max_x, max_y) + 1 in each
        // dim. Then position each scripted vision feature at a known lat/lon
        // inside the bbox and convert that to chunk-local pixels — this way
        // the bbox filter definitely keeps them.
        // Anchor the bbox inside a single z=15 tile by centring on a known
        // point and extending ±0.002° (~200m). One tile at z=15 is ~1.2 km,
        // so the bbox cannot straddle a tile boundary, guaranteeing the
        // pipeline makes exactly one vision call.
        let bbox = BoundingBox {
            south: 53.633,
            west: -8.102,
            north: 53.637,
            east: -8.098,
        };
        let z = 15u8;

        let (min_x, min_y, max_x, max_y) = tile_range_for_bbox(&bbox, z);
        assert!(max_x - min_x <= 1 && max_y - min_y <= 1);
        let mk_px = |lat: f64, lon: f64| {
            let tp = crate::historic::tile_math::lonlat_to_tile_pixel(lat, lon, z);
            let px = (tp.x - min_x) * TILE_SIZE_PX + tp.px;
            let py = (tp.y - min_y) * TILE_SIZE_PX + tp.py;
            (px, py)
        };
        let (k_px, k_py) = mk_px(53.636, -8.101);
        let (c_px, c_py) = mk_px(53.635, -8.099);
        let (f_px, f_py) = mk_px(53.634, -8.0985);
        let (g_px, g_py) = mk_px(53.6345, -8.1005);

        // Two labelled features + one unlabelled, one below threshold, and
        // one connected pair between the two labelled ones.
        let scripted = VisionResponse {
            features: vec![
                VisionFeature {
                    px: k_px,
                    py: k_py,
                    label_text: Some("Kilteevan".to_string()),
                    feature_kind: VisionFeatureKind::Village,
                    confidence: 0.95,
                    connected_px_segments: vec![PxSegment {
                        to_px: c_px,
                        to_py: c_py,
                    }],
                },
                VisionFeature {
                    px: c_px,
                    py: c_py,
                    label_text: Some("Ch.".to_string()),
                    feature_kind: VisionFeatureKind::Church,
                    confidence: 0.9,
                    connected_px_segments: vec![PxSegment {
                        to_px: k_px,
                        to_py: k_py,
                    }],
                },
                VisionFeature {
                    px: f_px,
                    py: f_py,
                    label_text: None,
                    feature_kind: VisionFeatureKind::Forge,
                    confidence: 0.8,
                    connected_px_segments: vec![],
                },
                VisionFeature {
                    px: g_px,
                    py: g_py,
                    label_text: Some("ghost".to_string()),
                    feature_kind: VisionFeatureKind::Other,
                    confidence: 0.1, // dropped
                    connected_px_segments: vec![],
                },
            ],
        };

        let vision = ScriptedVision {
            responses: Mutex::new(vec![scripted]),
            calls: Mutex::new(0),
        };
        let tiles = SyntheticTiles;
        let config = DiscoverConfig::new(bbox, z, 1);
        // No naming client, so the unlabelled feature gets a placeholder name
        // and `geo_kind: Fictional`.
        let (tracked, audit) = run(&tiles, &vision, None, &config)
            .await
            .expect("discover run");

        assert_eq!(audit.features_emitted, 3);
        assert_eq!(audit.features_dropped_low_confidence, 1);
        assert_eq!(audit.features_dropped_duplicate, 0);
        assert_eq!(audit.source_id, "synthetic");

        // Labelled features should be Manual; the unlabelled one Fictional.
        let kilteevan = tracked
            .iter()
            .find(|t| t.data.name == "Kilteevan")
            .expect("Kilteevan present");
        assert!(matches!(kilteevan.data.geo_kind, GeoKind::Manual));
        assert!(kilteevan.data.geo_source.as_deref() == Some("synthetic test tiles"));

        let forge = tracked
            .iter()
            .find(|t| matches!(t.data.geo_kind, GeoKind::Fictional))
            .expect("unlabelled feature becomes Fictional");
        assert!(forge.data.name.starts_with("Old forge"));
        assert!(
            forge
                .data
                .geo_source
                .as_deref()
                .is_some_and(|s| s.contains("unlabelled"))
        );

        // Vision-reported segment should have produced a bidirectional edge
        // between Kilteevan and the church.
        let church = tracked
            .iter()
            .find(|t| t.data.name == "Ch.")
            .expect("church present");
        assert!(
            kilteevan
                .data
                .connections
                .iter()
                .any(|c| c.target == church.data.id)
        );
        assert!(
            church
                .data
                .connections
                .iter()
                .any(|c| c.target == kilteevan.data.id)
        );

        // The synthetic tiles cover exactly one chunk (1×1 × 2-tile step).
        assert_eq!(*vision.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_discover_fails_vision_gracefully() {
        // A vision client that always errors: we expect the run to succeed
        // but emit zero features. This is the "chunk skip on error" path.
        struct AlwaysErr;
        #[async_trait]
        impl VisionClient for AlwaysErr {
            async fn analyse(&self, _: &str, _: Vec<u8>) -> Result<VisionResponse> {
                Err(anyhow!("simulated vision failure"))
            }
        }

        let bbox = BoundingBox {
            south: 53.632,
            west: -8.103,
            north: 53.634,
            east: -8.100,
        };
        let config = DiscoverConfig::new(bbox, 15, 1);
        let (tracked, audit) = run(&SyntheticTiles, &AlwaysErr, None, &config)
            .await
            .expect("pipeline tolerates per-chunk vision errors");
        assert_eq!(tracked.len(), 0);
        assert_eq!(audit.features_emitted, 0);
    }

    /// Helper for building dense dedupe/adjacency fixtures.
    fn mk_feature(
        lat: f64,
        lon: f64,
        label: Option<&str>,
        kind: VisionFeatureKind,
        confidence: f32,
        segments: Vec<(f64, f64)>,
    ) -> GeolocatedVisionFeature {
        GeolocatedVisionFeature {
            lat,
            lon,
            label_text: label.map(str::to_string),
            feature_kind: kind,
            map_labelled: label.is_some(),
            confidence,
            connected_segments: segments,
        }
    }

    #[test]
    fn test_dedupe_collapses_close_features_of_same_kind() {
        let a = mk_feature(53.5, -8.0, Some("A"), VisionFeatureKind::Mill, 0.9, vec![]);
        // 10m north of A — same kind → should be dropped.
        let b = mk_feature(
            53.5001,
            -8.0,
            Some("B"),
            VisionFeatureKind::Mill,
            0.8,
            vec![],
        );
        // Same spot but different kind — kept.
        let c = mk_feature(
            53.5001,
            -8.0,
            Some("C"),
            VisionFeatureKind::Church,
            0.9,
            vec![],
        );
        let out = dedupe_by_proximity(vec![a, b, c], 30.0);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|f| f.label_text.as_deref() == Some("A")));
        assert!(out.iter().any(|f| f.label_text.as_deref() == Some("C")));
    }

    #[test]
    fn test_dedupe_prefers_labelled_over_unlabelled_duplicate() {
        // Vision returned the unlabelled detection first and the labelled
        // one second; dedup must keep the labelled detection either way.
        let unlabelled_first = mk_feature(53.5, -8.0, None, VisionFeatureKind::Mill, 0.95, vec![]);
        let labelled_second = mk_feature(
            53.5001, // ~11m away
            -8.0,
            Some("Murphy's Mill"),
            VisionFeatureKind::Mill,
            0.7,
            vec![],
        );
        let out = dedupe_by_proximity(vec![unlabelled_first, labelled_second], 30.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].label_text.as_deref(), Some("Murphy's Mill"));
        assert!(out[0].map_labelled);
    }

    #[test]
    fn test_dedupe_prefers_higher_confidence_among_unlabelled() {
        let low = mk_feature(53.5, -8.0, None, VisionFeatureKind::Forge, 0.55, vec![]);
        let high = mk_feature(53.5, -8.0, None, VisionFeatureKind::Forge, 0.92, vec![]);
        let out = dedupe_by_proximity(vec![low, high], 30.0);
        assert_eq!(out.len(), 1);
        assert!((out[0].confidence - 0.92).abs() < 1e-6);
    }

    #[test]
    fn test_bbox_contains_inclusive_edges() {
        let bbox = BoundingBox {
            south: 53.60,
            west: -8.15,
            north: 53.65,
            east: -8.05,
        };
        assert!(bbox_contains(&bbox, 53.625, -8.10));
        // Corners are inclusive.
        assert!(bbox_contains(&bbox, 53.60, -8.15));
        assert!(bbox_contains(&bbox, 53.65, -8.05));
        // Just outside.
        assert!(!bbox_contains(&bbox, 53.66, -8.10));
        assert!(!bbox_contains(&bbox, 53.625, -8.04));
    }

    #[tokio::test]
    async fn test_discover_drops_features_outside_bbox() {
        // Use the known centre of a tight bbox as the "inside" feature, and
        // a pixel near the SE corner of the 2×2 chunk (which at z=15 covers
        // ~2.4 km — well outside a 200 m bbox) as the "outside" feature.
        let bbox = BoundingBox {
            south: 53.633,
            west: -8.103,
            north: 53.634,
            east: -8.102,
        };
        let z = 15u8;

        // Find the chunk origin + the pixel of a known point inside the bbox.
        let (min_x, min_y, _, _) = tile_range_for_bbox(&bbox, z);
        let centre = crate::historic::tile_math::lonlat_to_tile_pixel(53.6335, -8.1025, z);
        let inside_px = (centre.x - min_x) * TILE_SIZE_PX + centre.px;
        let inside_py = (centre.y - min_y) * TILE_SIZE_PX + centre.py;

        let resp = VisionResponse {
            features: vec![
                VisionFeature {
                    px: inside_px,
                    py: inside_py,
                    label_text: Some("Inside".to_string()),
                    feature_kind: VisionFeatureKind::Village,
                    confidence: 0.95,
                    connected_px_segments: vec![],
                },
                // SE corner of the 2×2 chunk — far from the tight bbox.
                VisionFeature {
                    px: CHUNK_PX - 1,
                    py: CHUNK_PX - 1,
                    label_text: Some("Outside".to_string()),
                    feature_kind: VisionFeatureKind::Church,
                    confidence: 0.95,
                    connected_px_segments: vec![],
                },
            ],
        };
        let vision = ScriptedVision {
            responses: Mutex::new(vec![resp]),
            calls: Mutex::new(0),
        };
        let config = DiscoverConfig::new(bbox, z, 1);
        let (tracked, audit) = run(&SyntheticTiles, &vision, None, &config)
            .await
            .expect("discover run");
        assert_eq!(audit.features_dropped_out_of_bbox, 1);
        assert_eq!(tracked.len(), 1);
        assert_eq!(tracked[0].data.name, "Inside");
    }

    #[test]
    fn test_llm_named_features_stay_fictional() {
        // One labelled village + one unlabelled forge. A naming client that
        // invents names must not cause the forge to be emitted as `Manual`.
        let unnamed = mk_feature(53.633, -8.102, None, VisionFeatureKind::Forge, 0.8, vec![]);
        // Ask resolve_names with a nil client — it must short-circuit and
        // leave provenance untouched. We use `build_tracked_locations` directly
        // with a pre-named feature that carries `map_labelled = false` to
        // assert the Manual/Fictional branch:
        let mut after_naming = unnamed.clone();
        after_naming.label_text = Some("Murphy's Forge".to_string());
        // map_labelled stays false — this is the naming-pass mutation.
        assert!(!after_naming.map_labelled);

        let tracked = build_tracked_locations(vec![after_naming], "test-attr", 1);
        assert_eq!(tracked.len(), 1);
        assert_eq!(tracked[0].data.name, "Murphy's Forge");
        assert!(matches!(tracked[0].data.geo_kind, GeoKind::Fictional));
        assert!(
            tracked[0]
                .data
                .geo_source
                .as_deref()
                .is_some_and(|s| s.contains("unlabelled"))
        );

        // And a map-labelled feature at the same point still becomes Manual.
        let labelled = mk_feature(
            53.633,
            -8.102,
            Some("Kilteevan"),
            VisionFeatureKind::Village,
            0.95,
            vec![],
        );
        let tracked = build_tracked_locations(vec![labelled], "test-attr", 1);
        assert!(matches!(tracked[0].data.geo_kind, GeoKind::Manual));
        assert_eq!(tracked[0].data.geo_source.as_deref(), Some("test-attr"));
    }

    #[test]
    fn test_build_adjacency_snaps_vision_segments_and_falls_back() {
        // Two features within SEGMENT_SNAP_M of each other with an explicit
        // vision segment pointing between them — the segment should win.
        let a = mk_feature(
            53.50,
            -8.00,
            Some("A"),
            VisionFeatureKind::Village,
            0.9,
            vec![(53.5002, -8.0002)], // ~30m from B
        );
        let b = mk_feature(
            53.5003,
            -8.0003,
            Some("B"),
            VisionFeatureKind::Church,
            0.9,
            vec![],
        );
        // C is isolated → should pick up fallback edges.
        let c = mk_feature(
            53.501,
            -8.001,
            Some("C"),
            VisionFeatureKind::Mill,
            0.9,
            vec![],
        );
        let adj = build_adjacency(&[a, b, c]);
        assert!(adj.get(&0).unwrap().contains(&1));
        assert!(adj.get(&1).unwrap().contains(&0));
        // C must end up connected to somebody via fallback.
        assert!(adj.get(&2).is_some_and(|peers| !peers.is_empty()));
    }
}
