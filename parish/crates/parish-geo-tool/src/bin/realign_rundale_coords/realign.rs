//! Relative-position resolution + fictional-location realignment.
//!
//! Resolves `relative_to` anchor chains into absolute coordinates and shifts
//! each fictional location by a graph-distance-weighted blend of the nearby
//! real-location deltas. Split out of the realign binary's single-file body
//! (#1200, TD-022).

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Context, Result, bail};
use parish_core::world::LocationId;
use parish_core::world::graph::{GeoKind, LocationData};
use parish_geo_tool::osm_model::EARTH_RADIUS_M;

/// Offsets a lat/lon by a signed (north, east) meters delta using a local
/// equirectangular approximation. Accurate to sub-metre at sub-kilometre
/// offsets and latitudes in the 40–60° range (all of Ireland).
pub(crate) fn offset_latlon(lat: f64, lon: f64, dnorth_m: f64, deast_m: f64) -> (f64, f64) {
    let dlat = (dnorth_m / EARTH_RADIUS_M).to_degrees();
    let dlon = (deast_m / (EARTH_RADIUS_M * lat.to_radians().cos())).to_degrees();
    (lat + dlat, lon + dlon)
}

/// Resolves every location with a `relative_to` reference by walking the
/// anchor chain and rewriting its `lat`/`lon` as `anchor + offset`.
/// Errors on cycles or anchors that don't exist in the slice.
pub(crate) fn resolve_relative_positions(locations: &mut [LocationData]) -> Result<()> {
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

pub(crate) fn realign_fictional_locations(
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

pub(crate) fn infer_delta(
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
