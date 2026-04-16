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
    if !cli.no_geocode {
        let client = Client::builder()
            .user_agent("parish-geo-tool/realign-rundale-coords (https://github.com/parish)")
            .build()
            .context("failed to build HTTP client")?;

        for loc in &mut world.locations {
            if loc.geo_kind != GeoKind::Real {
                continue;
            }
            let (new_lat, new_lon) = geocode_location(&client, &loc.name, &cli.context)
                .await
                .with_context(|| format!("failed to geocode '{}'", loc.name))?;
            deltas.insert(loc.id, (new_lat - loc.lat, new_lon - loc.lon));
            loc.lat = new_lat;
            loc.lon = new_lon;
        }
    }

    if deltas.is_empty() {
        bail!("no real-location coordinate deltas available; geocode at least one real location");
    }

    let realigned = realign_fictional_locations(&mut world.locations, &deltas);
    println!(
        "updated {} real locations and realigned {} fictional locations",
        deltas.len(),
        realigned
    );

    let out_path = if cli.in_place {
        cli.world.clone()
    } else {
        cli.output
            .clone()
            .unwrap_or_else(|| cli.world.with_extension("realigned.json"))
    };

    let json = serde_json::to_string_pretty(&world)?;
    std::fs::write(&out_path, format!("{json}\n"))
        .with_context(|| format!("failed to write {}", out_path.display()))?;
    println!("wrote {}", out_path.display());

    Ok(())
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
        if old.geo_kind != GeoKind::Real {
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

async fn geocode_location(client: &Client, name: &str, context: &str) -> Result<(f64, f64)> {
    let queries = [format!("{name}, {context}"), name.to_string()];

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
            return Ok((lat, lon));
        }
    }

    bail!("no geocoding results")
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
        .filter(|l| l.geo_kind == GeoKind::Fictional)
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
        if node != origin {
            if let Some((d_lat, d_lon)) = real_deltas.get(&node) {
                let weight = 1.0 / hops as f64;
                weighted_lat += d_lat * weight;
                weighted_lon += d_lon * weight;
                total_weight += weight;
            }
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
    use parish_core::world::graph::Connection;

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
            },
        ];

        let deltas = HashMap::from([(LocationId(1), (0.02, -0.03))]);
        let updated = realign_fictional_locations(&mut locations, &deltas);

        assert_eq!(updated, 1);
        assert_eq!(locations[0].lat, 53.0);
        assert!((locations[1].lat - 53.12).abs() < 1e-9);
        assert!((locations[1].lon - (-8.13)).abs() < 1e-9);
    }
}
