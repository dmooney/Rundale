//! Geocode real Rundale locations and realign connected fictional coordinates.
//!
//! Structure (#1200 decomposition, TD-022): the former single-file binary is
//! split into focused submodules —
//! - [`overrides`] — `--set-coord` / `--set-source` application and
//!   `--baseline-world` delta derivation;
//! - [`geocode`] — Nominatim lookup + name-suffix normalisation;
//! - [`realign`] — relative-position resolution + fictional realignment.
//!
//! This `main.rs` owns the CLI definition, the shared `WorldFile` type
//! (via `include!`), and the orchestration in `main`.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use parish_core::world::LocationId;
use parish_core::world::graph::{GeoKind, LocationData};
use reqwest::Client;
use serde::{Deserialize, Serialize};

mod geocode;
mod overrides;
mod realign;

use geocode::geocode_location;
use overrides::{
    apply_set_coord_overrides, apply_set_source_overrides, derive_deltas_from_baseline,
};
use realign::{realign_fictional_locations, resolve_relative_positions};

#[derive(Parser, Debug)]
#[command(
    name = "realign-rundale-coords",
    about = "Geocode real locations and realign connected fictional coordinates"
)]
struct Cli {
    #[arg(long, default_value = "../mods/rundale/world.json")]
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

include!("../../world_file_shared.inc");

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

#[cfg(test)]
mod tests {
    use super::*;
    use geocode::strip_type_suffix;
    use overrides::{parse_set_coord, parse_set_source};
    use parish_core::world::graph::{Connection, LocationData, RelativeRef};
    use realign::{infer_delta, offset_latlon};
    use std::collections::HashMap;

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
                landmarks: vec![],
                indoor: false,
                public: true,
                connections: vec![Connection {
                    target: LocationId(2),
                    path_description: "".to_string(),
                    hazard: Default::default(),
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
                landmarks: vec![],
                indoor: false,
                public: true,
                connections: vec![Connection {
                    target: LocationId(1),
                    path_description: "".to_string(),
                    hazard: Default::default(),
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

    #[test]
    fn realign_leaves_relative_locations_at_resolved_anchor_offset() {
        let mut locations = vec![
            mk_loc(1, "Anchor", 53.0, -8.0, GeoKind::Manual, None),
            mk_loc(
                2,
                "Relative Fiction",
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
        locations[1].connections.push(Connection {
            target: LocationId(1),
            path_description: "to anchor".to_string(),
            hazard: Default::default(),
        });

        resolve_relative_positions(&mut locations).unwrap();
        let resolved_lat = locations[1].lat;
        let resolved_lon = locations[1].lon;
        let deltas = HashMap::from([(LocationId(1), (0.02, -0.03))]);

        let updated = realign_fictional_locations(&mut locations, &deltas);

        assert_eq!(updated, 0);
        assert!((locations[1].lat - resolved_lat).abs() < 1e-9);
        assert!((locations[1].lon - resolved_lon).abs() < 1e-9);
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
            landmarks: vec![],
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

    #[test]
    fn apply_set_source_overrides_sets_geo_source() {
        let mut locs = vec![mk_loc(1, "X", 53.0, -8.0, GeoKind::Real, None)];
        apply_set_source_overrides(&["X=OS 6-inch ca. 1837".to_string()], &mut locs).unwrap();
        assert_eq!(locs[0].geo_source.as_deref(), Some("OS 6-inch ca. 1837"));
    }

    #[test]
    fn apply_set_source_overrides_fails_on_missing_name() {
        let mut locs = vec![mk_loc(1, "X", 53.0, -8.0, GeoKind::Real, None)];
        let err = apply_set_source_overrides(&["Y=some note".to_string()], &mut locs).unwrap_err();
        assert!(err.to_string().contains("no location named 'Y'"));
    }

    #[test]
    fn derive_deltas_from_baseline_computes_shifts() {
        let old_locs = vec![
            mk_loc(1, "A", 53.0, -8.0, GeoKind::Real, None),
            mk_loc(2, "B", 53.1, -8.1, GeoKind::Fictional, None),
        ];
        let new_locs = vec![
            mk_loc(1, "A", 53.001, -8.002, GeoKind::Real, None),
            mk_loc(2, "B", 53.1, -8.1, GeoKind::Fictional, None),
        ];

        let dir = tempfile::tempdir().unwrap();
        let baseline_path = dir.path().join("baseline.json");
        let baseline = WorldFile {
            locations: old_locs,
        };
        std::fs::write(&baseline_path, serde_json::to_string(&baseline).unwrap()).unwrap();

        let deltas = derive_deltas_from_baseline(&baseline_path, &new_locs).unwrap();
        assert_eq!(deltas.len(), 1);
        let delta = deltas[&LocationId(1)];
        assert!((delta.0 - 0.001).abs() < 1e-9);
        assert!((delta.1 - -0.002).abs() < 1e-9);
    }

    #[test]
    fn derive_deltas_from_baseline_ignores_fictional_and_relative() {
        let old_locs = vec![
            mk_loc(1, "A", 53.0, -8.0, GeoKind::Real, None),
            mk_loc(
                2,
                "B",
                53.1,
                -8.1,
                GeoKind::Real,
                Some(RelativeRef {
                    anchor: LocationId(1),
                    dnorth_m: 100.0,
                    deast_m: 0.0,
                }),
            ),
            mk_loc(3, "C", 53.2, -8.2, GeoKind::Fictional, None),
        ];
        let new_locs = vec![
            mk_loc(1, "A", 53.001, -8.0, GeoKind::Real, None),
            mk_loc(
                2,
                "B",
                53.1,
                -8.1,
                GeoKind::Real,
                Some(RelativeRef {
                    anchor: LocationId(1),
                    dnorth_m: 100.0,
                    deast_m: 0.0,
                }),
            ),
            mk_loc(3, "C", 53.2, -8.2, GeoKind::Fictional, None),
        ];

        let dir = tempfile::tempdir().unwrap();
        let baseline_path = dir.path().join("baseline.json");
        let baseline = WorldFile {
            locations: old_locs,
        };
        std::fs::write(&baseline_path, serde_json::to_string(&baseline).unwrap()).unwrap();

        let deltas = derive_deltas_from_baseline(&baseline_path, &new_locs).unwrap();
        assert_eq!(deltas.len(), 1);
        assert!(deltas.contains_key(&LocationId(1)));
        assert!(!deltas.contains_key(&LocationId(2)));
        assert!(!deltas.contains_key(&LocationId(3)));
    }
}
