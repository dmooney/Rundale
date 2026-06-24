//! Scene-diorama endpoints.
//!
//! Scene state is built by `parish-core` so every runtime agrees on the model.
//! This module only maps mod-relative asset references to web URLs and serves
//! the checked scene assets through a narrowly scoped route.

use std::path::Path;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::extract::{Extension, Path as AxumPath};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::IntoResponse;
use parish_core::game_mod::{GameMod, canonical_scene_asset_path};
use parish_core::ipc::{SceneState, build_scene_state_relative, map_scene_state_asset_urls};

use crate::state::AppState;

const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// `GET /api/scene-state` — active diorama scene state, or null when disabled
/// by the `diorama` kill switch or absent for the current location.
pub async fn get_scene_state(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<Option<SceneState>> {
    let Some(game_mod) = state.game_mod.as_ref() else {
        return Json(None);
    };

    let relative = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let flags = state.config.lock().await.flags.clone();
        build_scene_state_relative(&world, &npc_manager, game_mod.scenes.as_ref(), &flags)
    };

    Json(
        relative.and_then(|scene| {
            map_scene_state_asset_urls(scene, &|rel| scene_asset_url(game_mod, rel))
        }),
    )
}

/// `GET /api/scene-asset/{*rel}` — serves active-mod files under
/// `assets/scenes/` only.
pub async fn get_scene_asset(
    AxumPath(rel): AxumPath<String>,
    Extension(state): Extension<Arc<AppState>>,
) -> axum::response::Response {
    let Some(game_mod) = state.game_mod.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let decoded = match percent_decode_path(&rel) {
        Ok(decoded) => decoded,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let path = match canonical_scene_asset_path(&game_mod.mod_dir, &decoded) {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!(rel = %decoded, error = %e, "rejected scene asset request");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [
                (
                    CONTENT_TYPE,
                    mime_guess::from_path(&path)
                        .first_or_octet_stream()
                        .to_string(),
                ),
                (CACHE_CONTROL, IMMUTABLE_CACHE_CONTROL.to_string()),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to read scene asset");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

pub(crate) fn scene_asset_url(game_mod: &GameMod, rel: &str) -> Option<String> {
    let path = canonical_scene_asset_path(&game_mod.mod_dir, rel).ok()?;
    let version = asset_version(&path);
    Some(format!(
        "/api/scene-asset/{}?v={version}",
        encode_scene_asset_rel(rel)
    ))
}

fn asset_version(path: &Path) -> u64 {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn percent_decode_path(input: &str) -> Result<String, ()> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' {
            if idx + 2 >= bytes.len() {
                return Err(());
            }
            let hi = hex_value(bytes[idx + 1]).ok_or(())?;
            let lo = hex_value(bytes[idx + 2]).ok_or(())?;
            out.push((hi << 4) | lo);
            idx += 3;
        } else {
            out.push(bytes[idx]);
            idx += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode_scene_asset_rel(rel: &str) -> String {
    rel.split('/')
        .map(percent_encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn percent_encode_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decode_path_decodes_encoded_traversal() {
        assert_eq!(
            percent_decode_path("assets/scenes/%2e%2e/world.json").unwrap(),
            "assets/scenes/../world.json"
        );
    }

    #[test]
    fn percent_decode_path_rejects_bad_escape() {
        assert!(percent_decode_path("assets/scenes/%xx").is_err());
    }
}
