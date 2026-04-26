use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use parish_core::world::LocationId;
use parish_core::world::graph::{GeoKind, LocationData};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(
    name = "realign-rundale-coords",
    about = "Geocode real locations and realign connected fictional coordinates"
)]
struct Cli {
    #[arg(long, default_value = "mods/rundale/world.json")]
    world: PathBuf,
    #[arg(long)]
    in_place: bool,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long, default_value = "County Roscommon, Ireland")]
    context: String,
    #[arg(long)]
    no_geocode: bool,
    /// Optional baseline world file used to derive real-location deltas from
    /// already-updated coordinates in `--world`.
    #[arg(long)]
    baseline_world: Option<PathBuf>,
    /// Pin a location to an absolute coordinate, marking it as `Manual` so
    /// future runs won't try to geocode it. Repeatable. Format:
    /// `"Name=lat,lon"` (name must match the `name` field in world.json).
    #[arg(long = "set-coord", value_name = "NAME=LAT,LON")]
    set_coord: Vec<String>,
    /// Attach a provenance note to a location (typically one also pinned
    /// with `--set-coord`). Repeatable. Format: `"Name=note text"`.
    #[arg(long = "set-source", value_name = "NAME=TEXT")]
    set_source: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldFile {
    locations: Vec<LocationData>,
}

#[derive(Debug, Deserialize)]
struct NominatimHit {
    lat: String,
    lon: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let text = std::fs::read_to_string(&cli.world)
        .with_context(|| format!("failed to read {}", cli.world.display()))?;
    let mut world: WorldFile = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", cli.world.display()))?;

    let mut deltas: HashMap<LocationId, (f64, f64)> = cli
        .baseline_world
        .as_ref()
        .map(|path| derive_deltas_from_baseline(path, &world.locations))
        .transpose()?
        .unwrap_or_default();

    apply_set_coord_overrides(&cli.set_coord, &mut world.locations, &mut deltas)?;
    apply_set_source_overrides(&cli.set_source, &mut world.locations)?;

    let mut skipped: Vec<String> = Vec::new();
    if !cli.no_geocode {
        let client = Client::builder()
            .user_agent("parish-geo-tool/realign-rundale-coords (https://github.com/parish)")
            .build()
            .context("failed to build HTTP client")?;

        for loc in &mut world.locations {
            if loc.geo_kind != GeoKind::Real {
                continue;
            }
            match geocode_location(&client, &loc.name, &cli.context).await {
                Ok(Some((new_lat, new_lon))) => {
                    deltas.insert(loc.id, (new_lat - loc.lat, new_lon - loc.lon));
                    loc.lat = new_lat;
                    loc.lon = new_lon;
                }
                Ok(None) => {
                    eprintln!(
                        "warning: no geocoding result for '{}'; keeping existing ({:.6}, {:.6})",
                        loc.name, loc.lat, loc.lon
                    );
                    skipped.push(loc.name.clone());
                }
                Err(e) => {
                    return Err(e).with_context(|| format!("failed to geocode '{}'", loc.name));
                }
            }
        }
    }

    // Resolve relative_to references after geocoding so any location that
    // anchors to a moved Real (or edited Manual) position picks up the shift.
    resolve_relative_positions(&mut world.locations)
        .context("failed to resolve relative_to references")?;

    if deltas.is_empty() {
        bail!(
            "no real-location coordinate deltas available; {} locations were skipped. \
             Pass --no-geocode or --baseline-world to drive realignment from an existing world file.",
            skipped.len()
        );
    }

    let realigned = realign_fictional_locations(&mut world.locations, &deltas);
    println!(
        "updated {} anchor locations, skipped {} (kept existing coords), realigned {} fictional locations",
        deltas.len(),
        skipped.len(),
        realigned,
    );

    let out_path = if cli.in_place {
        cli.world.clone()
    } else {
        cli.output
            .clone()
            .unwrap_or_else(|| cli.world.with_extension("realigned.json"))
    };

    // Match the 4-space indent convention used by every other mod file and
    // by the editor's deterministic writer — keeps world.json byte-identical
    // through editor round-trips.
    let mut buf = Vec::with_capacity(8192);
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    world.serialize(&mut ser)?;
    let mut body = String::from_utf8(buf).context("realigned world.json is not UTF-8")?;
    body.push('\n');
    std::fs::write(&out_path, body)
        .with_context(|| format!("failed to write {}", out_path.display()))?;
    println!("wrote {}", out_path.display());

    Ok(())
}

fn apply_set_coord_overrides(
    entries: &[String],
    locations: &mut [LocationData],
    deltas: &mut HashMap<LocationId, (f64, f64)>,
) -> Result<()> {
    for raw in entries {
        let (name, lat, lon) = parse_set_coord(raw)?;
        let loc = locations
            .iter_mut()
            .find(|l| l.name == name)
            .with_context(|| format!("--set-coord: no location named '{name}' in world"))?;
        deltas.insert(loc.id, (lat - loc.lat, lon - loc.lon));
        loc.lat = lat;
        loc.lon = lon;
        loc.relative_to = None;
        loc.geo_kind = GeoKind::Manual;
    }
    Ok(())
}

fn apply_set_source_overrides(entries: &[String], locations: &mut [LocationData]) -> Result<()> {
    for raw in entries {
        let (name, note) = parse_set_source(raw)?;
        let loc = locations
            .iter_mut()
            .find(|l| l.name == name)
            .with_context(|| format!("--set-source: no location named '{name}' in world"))?;
        loc.geo_source = Some(note);
    }
    Ok(())
}

fn parse_set_coord(raw: &str) -> Result<(String, f64, f64)> {
    let (name, rest) = raw
        .split_once('=')
        .with_context(|| format!("--set-coord '{raw}' missing '=' separator"))?;
    let (lat_s, lon_s) = rest
        .split_once(',')
        .with_context(|| format!("--set-coord '{raw}' needs 'lat,lon' after '='"))?;
    let lat: f64 = lat_s
        .trim()
        .parse()
        .with_context(|| format!("--set-coord '{raw}': invalid latitude"))?;
    let lon: f64 = lon_s
        .trim()
        .parse()
        .with_context(|| format!("--set-coord '{raw}': invalid longitude"))?;
    Ok((name.trim().to_string(), lat, lon))
}

fn parse_set_source(raw: &str) -> Result<(String, String)> {
    let (name, note) = raw
        .split_once('=')
        .with_context(|| format!("--set-source '{raw}' missing '=' separator"))?;
    Ok((name.trim().to_string(), note.trim().to_string()))
}

fn derive_deltas_from_baseline(
    baseline_path: &PathBuf,
    current_locations: &[LocationData],
) -> Result<HashMap<LocationId, (f64, f64)>> {
    let baseline_text = std::fs::read_to_string(baseline_path)
        .with_context(|| format!("failed to read {}", baseline_path.display()))?;
    let baseline: WorldFile = serde_json::from_str(&baseline_text)
        .with_context(|| format!("failed to parse {}", baseline_path.display()))?;

    let current_by_id: HashMap<LocationId, &LocationData> =
        current_locations.iter().map(|loc| (loc.id, loc)).collect();
    let mut deltas = HashMap::new();

    for old in &baseline.locations {
        // Anchors for fictional realignment: locations whose position was
        // authored independently. Fictional locations get realigned (not
        // anchoring); relative_to locations derive from another anchor.
        if matches!(old.geo_kind, GeoKind::Fictional) || old.relative_to.is_some() {
            continue;
        }
        if let Some(new) = current_by_id.get(&old.id) {
            let delta = (new.lat - old.lat, new.lon - old.lon);
            if delta.0.abs() > 1e-12 || delta.1.abs() > 1e-12 {
                deltas.insert(old.id, delta);
            }
        }
    }
    Ok(deltas)
}

async fn geocode_location(
    client: &Client,
    name: &str,
    context: &str,
) -> Result<Option<(f64, f64)>> {
    let mut queries = vec![format!("{name}, {context}"), name.to_string()];
    if let Some(stripped) = strip_type_suffix(name) {
        queries.push(format!("{stripped}, {context}"));
        queries.push(stripped);
    }

    for query in queries {
        let response = client
            .get("https://nominatim.openstreetmap.org/search")
            .query(&[("q", query.as_str()), ("format", "jsonv2"), ("limit", "1")])
            .send()
            .await
            .context("nominatim request failed")?
            .error_for_status()
            .context("nominatim non-success status")?;

        let hits = response
            .json::<Vec<NominatimHit>>()
            .await
            .context("invalid nominatim response")?;
        if let Some(hit) = hits.first() {
            let lat: f64 = hit
                .lat
                .parse()
                .context("invalid latitude in nominatim response")?;
            let lon: f64 = hit
                .lon
                .parse()
                .context("invalid longitude in nominatim response")?;
            return Ok(Some((lat, lon)));
        }
    }

    Ok(None)
}

fn strip_type_suffix(name: &str) -> Option<String> {
    const SUFFIXES: &[&str] = &[
        "Village",
        "Town",
        "Parish",
        "Hamlet",
        "Townland",
        "Crossroads",
        "Cross",
    ];
    let trimmed = name.trim_end();
    for suffix in SUFFIXES {
        let Some(head) = trimmed.strip_suffix(suffix) else {
            continue;
        };
        if head.is_empty() || !head.ends_with(|c: char| c.is_whitespace() || c == ',') {
            continue;
        }
        let cleaned = head.trim_end_matches(|c: char| c == ',' || c.is_whitespace());
        if !cleaned.is_empty() {
            return Some(cleaned.to_string());
        }
    }
    None
}

/// Offsets a lat/lon by a signed (north, east) meters delta using a local
/// equirectangular approximation. Accurate to sub-metre at sub-kilometre
/// offsets and latitudes in the 40–60° range (all of Ireland).
fn offset_latlon(lat: f64, lon: f64, dnorth_m: f64, deast_m: f64) -> (f64, f64) {
    const EARTH_R_M: f64 = 6_371_000.0;
    let dlat = (dnorth_m / EARTH_R_M).to_degrees();
    let dlon = (deast_m / (EARTH_R_M * lat.to_radians().cos())).to_degrees();
    (lat + dlat, lon + dlon)
}

/// Resolves every location with a `relative_to` reference by walking the
/// anchor chain and rewriting its `lat`/`lon` as `anchor + offset`.
/// Errors on cycles or anchors that don't exist in the slice.
fn resolve_relative_positions(locations: &mut [LocationData]) -> Result<()> {
    let by_id: HashMap<LocationId, LocationData> =
        locations.iter().cloned().map(|l| (l.id, l)).collect();

    let mut resolved: HashMap<LocationId, (f64, f64)> = HashMap::new();
    for (id, loc) in &by_id {
        if loc.relative_to.is_none() {
            resolved.insert(*id, (loc.lat, loc.lon));
        }
    }

    for loc in locations.iter() {
        if loc.relative_to.is_some() {
            resolve_one(loc.id, &by_id, &mut resolved, &mut HashSet::new())?;
        }
    }

    for loc in locations.iter_mut() {
        if loc.relative_to.is_some() {
            let (lat, lon) = resolved[&loc.id];
            loc.lat = lat;
            loc.lon = lon;
        }
    }
    Ok(())
}

fn resolve_one(
    id: LocationId,
    by_id: &HashMap<LocationId, LocationData>,
    resolved: &mut HashMap<LocationId, (f64, f64)>,
    visiting: &mut HashSet<LocationId>,
) -> Result<(f64, f64)> {
    if let Some(&coord) = resolved.get(&id) {
        return Ok(coord);
    }
    if !visiting.insert(id) {
        bail!(
            "cyclic relative_to reference involving location id {}",
            id.0
        );
    }
    let loc = by_id
        .get(&id)
        .with_context(|| format!("relative_to references unknown location id {}", id.0))?;
    let coord = match loc.relative_to {
        Some(r) => {
            let (anchor_lat, anchor_lon) = resolve_one(r.anchor, by_id, resolved, visiting)?;
            offset_latlon(anchor_lat, anchor_lon, r.dnorth_m, r.deast_m)
        }
        None => (loc.lat, loc.lon),
    };
    visiting.remove(&id);
    resolved.insert(id, coord);
    Ok(coord)
}

fn realign_fictional_locations(
    locations: &mut [LocationData],
    real_deltas: &HashMap<LocationId, (f64, f64)>,
) -> usize {
    let snapshot = locations.to_vec();
    let graph: HashMap<LocationId, Vec<LocationId>> = snapshot
        .iter()
        .map(|loc| {
            (
                loc.id,
                loc.connections.iter().map(|c| c.target).collect::<Vec<_>>(),
            )
        })
        .collect();

    let mut updated = 0usize;
    for loc in locations
        .iter_mut()
        .filter(|l| l.geo_kind == GeoKind::Fictional && l.relative_to.is_none())
    {
        if let Some((d_lat, d_lon)) = infer_delta(loc.id, &graph, real_deltas, 6) {
            loc.lat += d_lat;
            loc.lon += d_lon;
            updated += 1;
        }
    }
    updated
}

fn infer_delta(
    origin: LocationId,
    graph: &HashMap<LocationId, Vec<LocationId>>,
    real_deltas: &HashMap<LocationId, (f64, f64)>,
    max_hops: usize,
) -> Option<(f64, f64)> {
    let mut queue: VecDeque<(LocationId, usize)> = VecDeque::from([(origin, 0)]);
    let mut visited: HashSet<LocationId> = HashSet::from([origin]);
    let mut weighted_lat = 0.0;
    let mut weighted_lon = 0.0;
    let mut total_weight = 0.0;

    while let Some((node, hops)) = queue.pop_front() {
        if hops > max_hops {
            continue;
        }
        if node != origin
            && let Some((d_lat, d_lon)) = real_deltas.get(&node)
        {
            let weight = 1.0 / hops as f64;
            weighted_lat += d_lat * weight;
            weighted_lon += d_lon * weight;
            total_weight += weight;
        }

        if let Some(neighbors) = graph.get(&node) {
            for next in neighbors {
                if visited.insert(*next) {
                    queue.push_back((*next, hops + 1));
                }
            }
        }
    }

    (total_weight > 0.0).then_some((weighted_lat / total_weight, weighted_lon / total_weight))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_core::world::graph::{Connection, RelativeRef};

    #[test]
    fn strip_type_suffix_strips_trailing_village() {
        assert_eq!(
            strip_type_suffix("Kilteevan Village"),
            Some("Kilteevan".to_string())
        );
        assert_eq!(strip_type_suffix("Foo Crossroads"), Some("Foo".to_string()));
        assert_eq!(strip_type_suffix("Bar Cross"), Some("Bar".to_string()));
    }

    #[test]
    fn strip_type_suffix_leaves_non_suffix_names_alone() {
        assert_eq!(strip_type_suffix("Hodson Bay"), None);
        assert_eq!(strip_type_suffix("Knockcroghery Road"), None);
        assert_eq!(strip_type_suffix("Curraghboy Road"), None);
    }

    #[test]
    fn strip_type_suffix_requires_word_boundary() {
        // "Cloncross" ends in "Cross" but without a space — must not strip.
        assert_eq!(strip_type_suffix("Cloncross"), None);
        // Bare suffix word — must not strip to empty.
        assert_eq!(strip_type_suffix("Village"), None);
        assert_eq!(strip_type_suffix(""), None);
    }

    #[test]
    fn infer_delta_prefers_nearer_real_nodes() {
        let a = LocationId(1);
        let b = LocationId(2);
        let c = LocationId(3);
        let d = LocationId(4);
        let graph = HashMap::from([(a, vec![b]), (b, vec![a, c]), (c, vec![b, d]), (d, vec![c])]);
        let real_deltas = HashMap::from([(b, (0.01, -0.02)), (d, (0.05, -0.08))]);

        let (lat, lon) = infer_delta(a, &graph, &real_deltas, 6).unwrap();
        assert!(
            lat > 0.01 && lat < 0.03,
            "weighted delta should lean to near node"
        );
        assert!(
            lon < -0.02 && lon > -0.05,
            "weighted delta should lean to near node"
        );
    }

    #[test]
    fn realign_updates_only_fictional_locations() {
        let mut locations = vec![
            LocationData {
                id: LocationId(1),
                name: "Real".to_string(),
                description_template: "".to_string(),
                indoor: false,
                public: true,
                connections: vec![Connection {
                    target: LocationId(2),
                    traversal_minutes: None,
                    path_description: "".to_string(),
                }],
                lat: 53.0,
                lon: -8.0,
                associated_npcs: vec![],
                mythological_significance: None,
                aliases: vec![],
                geo_kind: GeoKind::Real,
                relative_to: None,
                geo_source: None,
            },
            LocationData {
                id: LocationId(2),
                name: "Fiction".to_string(),
                description_template: "".to_string(),
                indoor: false,
                public: true,
                connections: vec![Connection {
                    target: LocationId(1),
                    traversal_minutes: None,
                    path_description: "".to_string(),
                }],
                lat: 53.1,
                lon: -8.1,
                associated_npcs: vec![],
                mythological_significance: None,
                aliases: vec![],
                geo_kind: GeoKind::Fictional,
                relative_to: None,
                geo_source: None,
            },
        ];

        let deltas = HashMap::from([(LocationId(1), (0.02, -0.03))]);
        let updated = realign_fictional_locations(&mut locations, &deltas);

        assert_eq!(updated, 1);
        assert_eq!(locations[0].lat, 53.0);
        assert!((locations[1].lat - 53.12).abs() < 1e-9);
        assert!((locations[1].lon - (-8.13)).abs() < 1e-9);
    }

    fn mk_loc(
        id: u32,
        name: &str,
        lat: f64,
        lon: f64,
        geo_kind: GeoKind,
        relative_to: Option<RelativeRef>,
    ) -> LocationData {
        LocationData {
            id: LocationId(id),
            name: name.to_string(),
            description_template: String::new(),
            indoor: false,
            public: true,
            connections: vec![],
            lat,
            lon,
            associated_npcs: vec![],
            mythological_significance: None,
            aliases: vec![],
            geo_kind,
            relative_to,
            geo_source: None,
        }
    }

    #[test]
    fn offset_latlon_translates_north_correctly() {
        // 1000 m north at 53°N ≈ 0.00899° latitude shift (1 deg ≈ 111.2 km).
        let (lat, lon) = offset_latlon(53.0, -8.0, 1000.0, 0.0);
        assert!(
            (lat - 53.008993).abs() < 1e-5,
            "expected ~53.00899, got {lat}"
        );
        assert!((lon - -8.0).abs() < 1e-9, "lon should not change");
    }

    #[test]
    fn offset_latlon_translates_east_correctly() {
        // 1000 m east at 53°N ≈ 0.01494° longitude shift (1 deg lon ≈ 66.9 km at 53°N).
        let (lat, lon) = offset_latlon(53.0, -8.0, 0.0, 1000.0);
        assert!((lat - 53.0).abs() < 1e-9, "lat should not change");
        assert!(
            (lon - -7.98506).abs() < 1e-4,
            "expected ~-7.98506, got {lon}"
        );
    }

    #[test]
    fn resolve_absolute_only_is_noop() {
        let mut locs = vec![
            mk_loc(1, "A", 53.0, -8.0, GeoKind::Manual, None),
            mk_loc(2, "B", 53.1, -8.1, GeoKind::Fictional, None),
        ];
        resolve_relative_positions(&mut locs).unwrap();
        assert_eq!(locs[0].lat, 53.0);
        assert_eq!(locs[1].lat, 53.1);
    }

    #[test]
    fn resolve_single_relative_ref_applies_offset() {
        let mut locs = vec![
            mk_loc(1, "Anchor", 53.0, -8.0, GeoKind::Manual, None),
            mk_loc(
                2,
                "Offset",
                0.0,
                0.0,
                GeoKind::Fictional,
                Some(RelativeRef {
                    anchor: LocationId(1),
                    dnorth_m: 1000.0,
                    deast_m: 0.0,
                }),
            ),
        ];
        resolve_relative_positions(&mut locs).unwrap();
        assert!((locs[1].lat - 53.008993).abs() < 1e-5);
        assert!((locs[1].lon - -8.0).abs() < 1e-9);
    }

    #[test]
    fn resolve_chain_resolves_transitively() {
        // A absolute → B = A + 1km east → C = B + 1km north.
        let mut locs = vec![
            mk_loc(1, "A", 53.0, -8.0, GeoKind::Manual, None),
            mk_loc(
                2,
                "B",
                0.0,
                0.0,
                GeoKind::Fictional,
                Some(RelativeRef {
                    anchor: LocationId(1),
                    dnorth_m: 0.0,
                    deast_m: 1000.0,
                }),
            ),
            mk_loc(
                3,
                "C",
                0.0,
                0.0,
                GeoKind::Fictional,
                Some(RelativeRef {
                    anchor: LocationId(2),
                    dnorth_m: 1000.0,
                    deast_m: 0.0,
                }),
            ),
        ];
        resolve_relative_positions(&mut locs).unwrap();
        // C should be ~1km east AND ~1km north of A.
        assert!((locs[2].lat - 53.008993).abs() < 1e-5);
        assert!((locs[2].lon - -7.98506).abs() < 1e-4);
    }

    #[test]
    fn resolve_detects_cycle() {
        let mut locs = vec![
            mk_loc(
                1,
                "A",
                0.0,
                0.0,
                GeoKind::Fictional,
                Some(RelativeRef {
                    anchor: LocationId(2),
                    dnorth_m: 100.0,
                    deast_m: 0.0,
                }),
            ),
            mk_loc(
                2,
                "B",
                0.0,
                0.0,
                GeoKind::Fictional,
                Some(RelativeRef {
                    anchor: LocationId(1),
                    dnorth_m: -100.0,
                    deast_m: 0.0,
                }),
            ),
        ];
        let err = resolve_relative_positions(&mut locs).unwrap_err();
        assert!(err.to_string().contains("cyclic"), "{err}");
    }

    #[test]
    fn resolve_detects_missing_anchor() {
        let mut locs = vec![mk_loc(
            1,
            "Orphan",
            0.0,
            0.0,
            GeoKind::Fictional,
            Some(RelativeRef {
                anchor: LocationId(99),
                dnorth_m: 0.0,
                deast_m: 0.0,
            }),
        )];
        let err = resolve_relative_positions(&mut locs).unwrap_err();
        assert!(err.to_string().contains("unknown"), "{err}");
    }

    #[test]
    fn parse_set_coord_round_trips_valid_input() {
        let (name, lat, lon) = parse_set_coord("Kilteevan Village=53.6321,-8.1021").unwrap();
        assert_eq!(name, "Kilteevan Village");
        assert!((lat - 53.6321).abs() < 1e-9);
        assert!((lon - -8.1021).abs() < 1e-9);
    }

    #[test]
    fn parse_set_coord_rejects_missing_separator() {
        assert!(parse_set_coord("no-equals-sign").is_err());
        assert!(parse_set_coord("Name=no-comma").is_err());
        assert!(parse_set_coord("Name=abc,def").is_err());
    }

    #[test]
    fn parse_set_source_accepts_multi_word_notes() {
        let (name, note) = parse_set_source("Kilteevan=OS 6-inch ca. 1837").unwrap();
        assert_eq!(name, "Kilteevan");
        assert_eq!(note, "OS 6-inch ca. 1837");
    }

    #[test]
    fn apply_set_coord_marks_manual_and_records_delta() {
        let mut locs = vec![mk_loc(1, "X", 53.0, -8.0, GeoKind::Real, None)];
        let mut deltas = HashMap::new();
        apply_set_coord_overrides(&["X=53.5,-8.2".to_string()], &mut locs, &mut deltas).unwrap();
        assert_eq!(locs[0].geo_kind, GeoKind::Manual);
        assert!((locs[0].lat - 53.5).abs() < 1e-9);
        assert!((locs[0].lon - -8.2).abs() < 1e-9);
        let delta = deltas[&LocationId(1)];
        assert!((delta.0 - 0.5).abs() < 1e-9);
        assert!((delta.1 - -0.2).abs() < 1e-9);
    }
}
