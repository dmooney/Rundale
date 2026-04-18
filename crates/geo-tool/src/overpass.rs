//! Overpass API client for querying OpenStreetMap data.
//!
//! Builds and executes Overpass QL queries targeting Irish geographic
//! features at configurable administrative levels. Includes rate limiting
//! and response caching.

use anyhow::{Context, Result, bail};
use tracing::{info, warn};

use super::cache::ResponseCache;
use super::osm_model::OverpassResponse;
use crate::AdminLevel;

/// Default Overpass API endpoint.
const OVERPASS_API_URL: &str = "https://overpass-api.de/api/interpreter";

/// A geographic bounding box (south, west, north, east).
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    /// Southern latitude boundary.
    pub south: f64,
    /// Western longitude boundary.
    pub west: f64,
    /// Northern latitude boundary.
    pub north: f64,
    /// Eastern longitude boundary.
    pub east: f64,
}

impl std::fmt::Display for BoundingBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{},{},{},{}",
            self.south, self.west, self.north, self.east
        )
    }
}

/// Overpass API client with caching and rate limiting.
pub struct OverpassClient {
    /// HTTP client.
    client: reqwest::Client,
    /// Response cache.
    cache: ResponseCache,
    /// Whether to skip the cache.
    no_cache: bool,
}

impl OverpassClient {
    /// Creates a new Overpass client.
    pub fn new(cache: ResponseCache, no_cache: bool) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .user_agent("Parish-GeoTool/0.1 (https://github.com/parish-game)")
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            cache,
            no_cache,
        }
    }

    /// Queries for all points of interest within a named administrative area.
    ///
    /// Builds and executes an Overpass query that searches for the named area
    /// at the given administrative level, then finds all relevant features
    /// within its boundary.
    pub async fn query_area_pois(
        &self,
        area_name: &str,
        level: AdminLevel,
    ) -> Result<OverpassResponse> {
        let query = build_poi_query_by_area(area_name, level);
        self.execute_query(&query, &format!("pois_{area_name}_{level:?}"))
            .await
    }

    /// Queries for the road network within a named administrative area.
    ///
    /// Returns ways tagged as highways within the area, with full geometry
    /// for calculating distances and generating connections.
    pub async fn query_area_roads(
        &self,
        area_name: &str,
        level: AdminLevel,
    ) -> Result<OverpassResponse> {
        let query = build_road_query_by_area(area_name, level);
        self.execute_query(&query, &format!("roads_{area_name}_{level:?}"))
            .await
    }

    /// Queries for all points of interest within a bounding box.
    pub async fn query_bbox_pois(&self, bbox: BoundingBox) -> Result<OverpassResponse> {
        let query = build_poi_query_by_bbox(bbox);
        self.execute_query(&query, &format!("pois_bbox_{bbox}"))
            .await
    }

    /// Queries for the road network within a bounding box.
    pub async fn query_bbox_roads(&self, bbox: BoundingBox) -> Result<OverpassResponse> {
        let query = build_road_query_by_bbox(bbox);
        self.execute_query(&query, &format!("roads_bbox_{bbox}"))
            .await
    }

    /// Executes an Overpass QL query with caching and retries.
    async fn execute_query(&self, query: &str, cache_key: &str) -> Result<OverpassResponse> {
        // Check cache first
        if !self.no_cache
            && let Some(cached) = self.cache.get(cache_key)?
        {
            info!("cache hit for {cache_key}");
            let response: OverpassResponse = serde_json::from_str(&cached)
                .context("failed to parse cached Overpass response")?;
            return Ok(response);
        }

        info!("querying Overpass API for {cache_key}");

        let mut last_err = None;
        for attempt in 0..4u32 {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(2u64.pow(attempt));
                warn!(
                    "retrying Overpass query (attempt {}) after {delay:?}",
                    attempt + 1
                );
                tokio::time::sleep(delay).await;
            }

            match self.do_query(query).await {
                Ok(body) => {
                    // Cache the successful response
                    if let Err(e) = self.cache.put(cache_key, &body) {
                        warn!("failed to cache response: {e}");
                    }

                    let response: OverpassResponse = serde_json::from_str(&body)
                        .context("failed to parse Overpass API response")?;
                    info!(
                        "received {} elements from Overpass",
                        response.elements.len()
                    );
                    return Ok(response);
                }
                Err(e) => {
                    warn!("Overpass query attempt {} failed: {e}", attempt + 1);
                    last_err = Some(e);
                }
            }
        }

        bail!(
            "Overpass API query failed after 4 attempts: {}",
            last_err.unwrap()
        )
    }

    /// Performs a single Overpass API HTTP POST request.
    async fn do_query(&self, query: &str) -> Result<String> {
        let response = self
            .client
            .post(OVERPASS_API_URL)
            .form(&[("data", query)])
            .send()
            .await
            .context("failed to send Overpass request")?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            bail!("Overpass API rate limited (429)");
        }
        if status == reqwest::StatusCode::GATEWAY_TIMEOUT {
            bail!("Overpass API query timed out (504)");
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("Overpass API returned {status}: {body}");
        }

        response
            .text()
            .await
            .context("failed to read Overpass response body")
    }
}

/// Escapes a string for safe interpolation inside an Overpass QL double-quoted string.
///
/// Overpass QL uses `\` as an escape character and `"` as a string delimiter.
/// A `\` in the input must become `\\`, and a `"` must become `\"`, to prevent
/// the value from breaking out of the surrounding string literal.
fn escape_overpass(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            other => out.push(other),
        }
    }
    out
}

/// Builds an Overpass QL query for POIs within a named administrative area.
///
/// Searches for features relevant to an 1820s Irish setting: churches, pubs,
/// farms, historic sites, natural features, etc.
fn build_poi_query_by_area(area_name: &str, level: AdminLevel) -> String {
    let area_name = escape_overpass(area_name);
    let admin_level = level.osm_admin_level();
    format!(
        r#"[out:json][timeout:120];
area["name"~"{area_name}",i]["admin_level"="{admin_level}"]->.searchArea;
(
  // Religious buildings
  nwr["amenity"="place_of_worship"](area.searchArea);
  nwr["building"="church"](area.searchArea);
  nwr["building"="chapel"](area.searchArea);
  // Pubs and inns
  nwr["amenity"="pub"](area.searchArea);
  nwr["tourism"="hotel"](area.searchArea);
  // Shops and commerce
  nwr["shop"](area.searchArea);
  nwr["amenity"="post_office"](area.searchArea);
  // Education
  nwr["amenity"="school"](area.searchArea);
  // Farms
  nwr["building"="farm"](area.searchArea);
  nwr["building"="farmhouse"](area.searchArea);
  nwr["landuse"="farmyard"](area.searchArea);
  // Historic and archaeological
  nwr["historic"="archaeological_site"](area.searchArea);
  nwr["historic"="ring_fort"](area.searchArea);
  nwr["historic"="castle"](area.searchArea);
  nwr["historic"="ruins"](area.searchArea);
  nwr["historic"="monument"](area.searchArea);
  nwr["historic"="standing_stone"](area.searchArea);
  nwr["historic"="holy_well"](area.searchArea);
  nwr["historic"="ogham_stone"](area.searchArea);
  // Natural features
  nwr["natural"="water"]["name"](area.searchArea);
  nwr["natural"="wetland"](area.searchArea);
  nwr["natural"="wood"]["name"](area.searchArea);
  nwr["natural"="peak"](area.searchArea);
  nwr["natural"="spring"](area.searchArea);
  // Waterways
  nwr["waterway"="river"]["name"](area.searchArea);
  nwr["waterway"="stream"]["name"](area.searchArea);
  // Cemeteries
  nwr["landuse"="cemetery"](area.searchArea);
  nwr["amenity"="grave_yard"](area.searchArea);
  // Bridges and fords
  nwr["man_made"="bridge"]["name"](area.searchArea);
  nwr["ford"="yes"](area.searchArea);
  // Infrastructure
  nwr["man_made"="kiln"](area.searchArea);
  nwr["man_made"="watermill"](area.searchArea);
  nwr["craft"="blacksmith"](area.searchArea);
  // Named places and townlands
  node["place"~"hamlet|village|isolated_dwelling|locality|townland|town"](area.searchArea);
  // Harbours and quays
  nwr["leisure"="harbour"](area.searchArea);
  nwr["man_made"="pier"](area.searchArea);
);
out center;"#
    )
}

/// Builds an Overpass QL query for the road network within a named area.
fn build_road_query_by_area(area_name: &str, level: AdminLevel) -> String {
    let area_name = escape_overpass(area_name);
    let admin_level = level.osm_admin_level();
    format!(
        r#"[out:json][timeout:120];
area["name"~"{area_name}",i]["admin_level"="{admin_level}"]->.searchArea;
(
  way["highway"~"^(primary|secondary|tertiary|unclassified|residential|track|path|footway|bridleway|service)$"](area.searchArea);
);
out geom;"#
    )
}

/// Builds an Overpass QL query for POIs within a bounding box.
fn build_poi_query_by_bbox(bbox: BoundingBox) -> String {
    let bb = format!("{},{},{},{}", bbox.south, bbox.west, bbox.north, bbox.east);
    format!(
        r#"[out:json][timeout:120];
(
  nwr["amenity"="place_of_worship"]({bb});
  nwr["building"="church"]({bb});
  nwr["amenity"="pub"]({bb});
  nwr["shop"]({bb});
  nwr["amenity"="post_office"]({bb});
  nwr["amenity"="school"]({bb});
  nwr["building"="farm"]({bb});
  nwr["building"="farmhouse"]({bb});
  nwr["historic"="archaeological_site"]({bb});
  nwr["historic"="ring_fort"]({bb});
  nwr["historic"="castle"]({bb});
  nwr["historic"="ruins"]({bb});
  nwr["historic"="standing_stone"]({bb});
  nwr["historic"="holy_well"]({bb});
  nwr["natural"="water"]["name"]({bb});
  nwr["natural"="wetland"]({bb});
  nwr["natural"="peak"]({bb});
  nwr["waterway"="river"]["name"]({bb});
  nwr["landuse"="cemetery"]({bb});
  nwr["man_made"="bridge"]["name"]({bb});
  nwr["man_made"="kiln"]({bb});
  node["place"~"hamlet|village|isolated_dwelling|locality|townland|town"]({bb});
);
out center;"#
    )
}

/// Builds an Overpass QL query for roads within a bounding box.
fn build_road_query_by_bbox(bbox: BoundingBox) -> String {
    let bb = format!("{},{},{},{}", bbox.south, bbox.west, bbox.north, bbox.east);
    format!(
        r#"[out:json][timeout:120];
(
  way["highway"~"^(primary|secondary|tertiary|unclassified|residential|track|path|footway|bridleway|service)$"]({bb});
);
out geom;"#
    )
}

/// Returns the Overpass query strings without executing them (for dry-run mode).
pub fn dry_run_queries(
    area: Option<&str>,
    bbox: Option<BoundingBox>,
    level: AdminLevel,
) -> Vec<(String, String)> {
    let mut queries = Vec::new();
    if let Some(area_name) = area {
        queries.push((
            format!("POIs in {area_name} ({level:?})"),
            build_poi_query_by_area(area_name, level),
        ));
        queries.push((
            format!("Roads in {area_name} ({level:?})"),
            build_road_query_by_area(area_name, level),
        ));
    }
    if let Some(bbox) = bbox {
        queries.push((
            format!("POIs in bbox {bbox}"),
            build_poi_query_by_bbox(bbox),
        ));
        queries.push((
            format!("Roads in bbox {bbox}"),
            build_road_query_by_bbox(bbox),
        ));
    }
    queries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_poi_query_contains_area_name() {
        let query = build_poi_query_by_area("Kiltoom", AdminLevel::Parish);
        assert!(query.contains("Kiltoom"));
        assert!(query.contains("admin_level"));
        assert!(query.contains("place_of_worship"));
        assert!(query.contains("ring_fort"));
        assert!(query.contains("out center"));
    }

    #[test]
    fn test_build_road_query_contains_highway() {
        let query = build_road_query_by_area("Kiltoom", AdminLevel::Parish);
        assert!(query.contains("highway"));
        assert!(query.contains("out geom"));
    }

    #[test]
    fn test_build_bbox_query() {
        let bbox = BoundingBox {
            south: 53.45,
            west: -8.05,
            north: 53.55,
            east: -7.95,
        };
        let query = build_poi_query_by_bbox(bbox);
        assert!(query.contains("53.45"));
        assert!(query.contains("-8.05"));
    }

    #[test]
    fn test_admin_level_mapping() {
        assert_eq!(AdminLevel::Townland.osm_admin_level(), 10);
        assert_eq!(AdminLevel::Parish.osm_admin_level(), 8);
        assert_eq!(AdminLevel::Barony.osm_admin_level(), 7);
        assert_eq!(AdminLevel::County.osm_admin_level(), 6);
        assert_eq!(AdminLevel::Province.osm_admin_level(), 5);
    }

    #[test]
    fn test_dry_run_queries() {
        let queries = dry_run_queries(Some("Kiltoom"), None, AdminLevel::Parish);
        assert_eq!(queries.len(), 2);
        assert!(queries[0].0.contains("POIs"));
        assert!(queries[1].0.contains("Roads"));
    }

    #[test]
    fn test_bounding_box_display() {
        let bbox = BoundingBox {
            south: 53.45,
            west: -8.05,
            north: 53.55,
            east: -7.95,
        };
        let s = format!("{bbox}");
        assert_eq!(s, "53.45,-8.05,53.55,-7.95");
    }

    // ── escape_overpass tests ────────────────────────────────────────────────

    #[test]
    fn escape_overpass_normal_name_unchanged() {
        assert_eq!(escape_overpass("Killeen"), "Killeen");
        assert_eq!(escape_overpass("County Roscommon"), "County Roscommon");
    }

    #[test]
    fn escape_overpass_quotes_escaped() {
        // A quote must not be able to break out of the surrounding QL string.
        assert_eq!(escape_overpass(r#"Killeen"; bad;"#), r#"Killeen\"; bad;"#);
    }

    #[test]
    fn escape_overpass_backslash_escaped() {
        assert_eq!(escape_overpass(r"Foo\bar"), r"Foo\\bar");
    }

    #[test]
    fn escape_overpass_both_special_chars() {
        // backslash then quote
        assert_eq!(escape_overpass("a\\\"b"), r#"a\\\"b"#);
    }

    #[test]
    fn escape_overpass_injected_name_does_not_appear_raw_in_query() {
        let malicious = r#"Killeen"; out body;"#;
        let query = build_poi_query_by_area(malicious, AdminLevel::Parish);
        // The raw injection string must not appear verbatim in the query.
        assert!(!query.contains(r#"Killeen"; out body;"#));
        // But the escaped form should be present.
        assert!(query.contains(r#"Killeen\"; out body;"#));
    }
}
