//! CLI override parsing + baseline-delta derivation.
//!
//! `--set-coord` / `--set-source` application and the `--baseline-world`
//! delta inference. Split out of the realign binary's single-file body
//! (#1200, TD-022).

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use parish_core::world::LocationId;
use parish_core::world::graph::{GeoKind, LocationData};

use crate::WorldFile;

pub(crate) fn apply_set_coord_overrides(
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

pub(crate) fn apply_set_source_overrides(
    entries: &[String],
    locations: &mut [LocationData],
) -> Result<()> {
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

pub(crate) fn parse_set_coord(raw: &str) -> Result<(String, f64, f64)> {
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

pub(crate) fn parse_set_source(raw: &str) -> Result<(String, String)> {
    let (name, note) = raw
        .split_once('=')
        .with_context(|| format!("--set-source '{raw}' missing '=' separator"))?;
    Ok((name.trim().to_string(), note.trim().to_string()))
}

pub(crate) fn derive_deltas_from_baseline(
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
