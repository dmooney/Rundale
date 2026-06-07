//! Nominatim geocoding + name-suffix normalisation.
//!
//! Looks up a Real location's coordinates via the OSM Nominatim search API,
//! retrying with progressively simplified queries (including a type-suffix
//! strip). Split out of the realign binary's single-file body (#1200, TD-022).

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct NominatimHit {
    pub(crate) lat: String,
    pub(crate) lon: String,
}

pub(crate) async fn geocode_location(
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

pub(crate) fn strip_type_suffix(name: &str) -> Option<String> {
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
