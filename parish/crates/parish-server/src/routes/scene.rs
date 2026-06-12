//! Diorama scene routes.
//!
//! Covers:
//! - `GET /api/scene-state`   — current renderable scene (diorama M2)
//! - `GET /api/scene-asset/*rel` — serve mod asset bytes (plate PNGs, sprites)

use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// `GET /api/scene-state` — returns the current renderable diorama scene state.
///
/// Gated behind the `diorama` feature flag (rule #6); returns `Json(None)` when
/// the flag is off or no scene is defined for the current location.
///
/// Lock order: world → npc_manager → config (canonical order, §483).
pub async fn get_scene_state(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<Option<parish_core::ipc::SceneState>> {
    let Some(gm) = state.game_mod.as_ref() else {
        return Json(None);
    };
    let Some(scenes) = gm.scenes.as_ref() else {
        return Json(None);
    };

    let mod_dir = gm.mod_dir.clone();
    let scenes = scenes.clone();

    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let config = state.config.lock().await;

    let asset_url = |rel: &str| -> Option<String> {
        let p = parish_core::game_mod::canonical_mod_asset_path(&mod_dir, rel).ok()?;
        let v = std::fs::metadata(&p)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Some(format!("/api/scene-asset/{rel}?v={v}"))
    };

    let result = parish_core::ipc::build_scene_state(
        &world,
        &npc_manager,
        &scenes,
        &config.flags,
        &asset_url,
    );

    Json(result)
}

/// `GET /api/scene-asset/*rel` — serves a mod asset (plate PNG, sprite, etc.)
/// with an immutable cache header.
///
/// Security:
/// - `rel` is validated through [`parish_core::game_mod::canonical_mod_asset_path`],
///   which rejects absolute paths, traversal components, and anything not
///   under `assets/`.
/// - The canonical path must additionally be a descendant of
///   `<mod_dir>/assets/scenes/`; any path escaping that subtree is rejected
///   with 404.
///
/// Response: correct MIME type from `mime_guess`, `Cache-Control: public,
/// max-age=31536000, immutable` (the `?v=` cache-buster from `get_scene_state`
/// makes this safe).
pub async fn serve_scene_asset(
    Extension(state): Extension<Arc<AppState>>,
    Path(rel): Path<String>,
) -> Response {
    let Some(gm) = state.game_mod.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mod_dir = &gm.mod_dir;

    // Re-validate through the canonical guard.
    let canonical = match parish_core::game_mod::canonical_mod_asset_path(mod_dir, &rel) {
        Ok(p) => p,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Enforce that the asset lives under assets/scenes/ — sprites and plate PNGs
    // only; other mod assets (icons, etc.) are served by dedicated routes.
    let scenes_dir = mod_dir.join("assets/scenes");
    if !canonical.starts_with(&scenes_dir) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match tokio::fs::read(&canonical).await {
        Ok(bytes) => (
            [
                (
                    CONTENT_TYPE,
                    mime_guess::from_path(&canonical)
                        .first_or_octet_stream()
                        .to_string(),
                ),
                (
                    CACHE_CONTROL,
                    "public, max-age=31536000, immutable".to_string(),
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(
                path = %canonical.display(),
                error = %e,
                "failed to read scene asset"
            );
            StatusCode::NOT_FOUND.into_response()
        }
    }
}
