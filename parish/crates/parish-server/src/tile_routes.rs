//! Tile proxy route — `GET /tiles/{source_id}/{z}/{x}/{y}.png`.
//!
//! Serves NLS historic-map tiles through the server-side [`TileCache`],
//! so the browser never connects to `mapseries-tilesets.s3.amazonaws.com`
//! directly.  Requires a valid session (injected by [`session_middleware`]).
//!
//! [`TileCache`]: parish_core::tile_cache::TileCache

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::session::GlobalState;

/// `GET /tiles/*path` — resolves to `GET /tiles/{source_id}/{z}/{x}/{y}.png`.
///
/// Axum 0.8 does not allow a static suffix (`.png`) in the same path segment
/// as a named parameter (`{y}`), so we capture the tail as a wildcard string
/// and parse the four components here.
///
/// Authentication is enforced by `cf_access_guard` and `session_middleware`
/// (both already in the middleware stack for every route on the router);
/// this handler does not need to perform auth checks itself.
///
/// `source_id` is validated against the tile source registry so that the
/// cache path cannot be exploited to traverse outside `tile_cache_dir`.
/// Parses a tile path of the form `source_id/z/x/y.png`.
///
/// Returns `Some((source_id, z, x, y))` when all four components are present
/// and the coordinates are valid unsigned integers.
fn parse_tile_path(path: &str) -> Option<(String, u32, u32, u32)> {
    let parts: Vec<&str> = path.splitn(4, '/').collect();
    if parts.len() != 4 {
        return None;
    }
    let source_id = parts[0].to_string();
    let z = parts[1].parse::<u32>().ok()?;
    let x = parts[2].parse::<u32>().ok()?;
    let y_str = parts[3].strip_suffix(".png").unwrap_or(parts[3]);
    let y = y_str.parse::<u32>().ok()?;
    Some((source_id, z, x, y))
}

pub async fn get_tile(
    State(global): State<Arc<GlobalState>>,
    Path(path): Path<String>,
) -> Response {
    let Some((source_id, z, x, y)) = parse_tile_path(&path) else {
        return (StatusCode::BAD_REQUEST, "invalid tile path or coordinates").into_response();
    };

    // ── Validate source_id against the registered tile sources ───────────
    // This prevents path traversal: only source ids that appear in the
    // engine config are accepted, and those ids are alphanumeric slugs.
    let known = global
        .template_config
        .tile_sources
        .iter()
        .any(|(id, _)| id == &source_id);

    if !known {
        tracing::warn!(source_id = %source_id, "tile proxy: unknown source id — rejecting");
        return (StatusCode::NOT_FOUND, "unknown tile source").into_response();
    }

    // ── Fetch from cache (or upstream) ───────────────────────────────────
    match global.tile_cache.get(&source_id, z, x, y).await {
        Ok(data) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "image/png")
            .header("Cache-Control", "public, max-age=86400, immutable")
            .body(Body::from(data))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::warn!(
                source_id = %source_id,
                z,
                x,
                y,
                error = %e,
                "tile proxy: upstream fetch or cache read failed"
            );
            (StatusCode::BAD_GATEWAY, "tile fetch failed").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tile_path_valid() {
        assert_eq!(
            parse_tile_path("osm/5/12/8.png"),
            Some(("osm".to_string(), 5, 12, 8))
        );
    }

    #[test]
    fn parse_tile_path_no_png_suffix() {
        assert_eq!(
            parse_tile_path("nls/3/1/2"),
            Some(("nls".to_string(), 3, 1, 2))
        );
    }

    #[test]
    fn parse_tile_path_too_few_segments() {
        assert_eq!(parse_tile_path("osm/5/12"), None);
    }

    #[test]
    fn parse_tile_path_too_many_segments() {
        // splitn(4, '/') yields at most 4 parts; extra slashes end up in the
        // last part, so y fails to parse.
        assert_eq!(parse_tile_path("osm/5/12/8/extra"), None);
    }

    #[test]
    fn parse_tile_path_non_numeric_coordinate() {
        assert_eq!(parse_tile_path("osm/5/abc/8.png"), None);
    }

    #[test]
    fn parse_tile_path_negative_coordinate() {
        // Negative numbers are not valid u32.
        assert_eq!(parse_tile_path("osm/5/-1/8.png"), None);
    }

    #[test]
    fn parse_tile_path_empty_source_id() {
        assert_eq!(
            parse_tile_path("/5/12/8.png"),
            Some(("".to_string(), 5, 12, 8))
        );
    }
}
