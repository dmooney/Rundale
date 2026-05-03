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
pub async fn get_tile(
    State(global): State<Arc<GlobalState>>,
    Path(path): Path<String>,
) -> Response {
    // path = "source_id/z/x/y.png"
    let parts: Vec<&str> = path.splitn(4, '/').collect();
    if parts.len() != 4 {
        return (StatusCode::BAD_REQUEST, "invalid tile path").into_response();
    }
    let source_id = parts[0].to_string();
    let (Ok(z), Ok(x)) = (parts[1].parse::<u32>(), parts[2].parse::<u32>()) else {
        return (StatusCode::BAD_REQUEST, "invalid tile coordinates").into_response();
    };
    // Strip optional ".png" suffix from the y segment.
    let y_str = parts[3].strip_suffix(".png").unwrap_or(parts[3]);
    let Ok(y) = y_str.parse::<u32>() else {
        return (StatusCode::BAD_REQUEST, "invalid tile coordinates").into_response();
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
