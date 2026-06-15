//! Mod-selector endpoints — list and switch active base mods.

use std::sync::Arc;

use axum::Json;
use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::state::AppState;

// ── Mod selector ─────────────────────────────────────────────────────────────

/// Lightweight per-mod payload returned by `GET /api/mods`.
#[derive(serde::Serialize)]
pub struct ModEntry {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub version: String,
    pub description: String,
    pub active: bool,
}

/// Scans `root` for setting mods and returns them with an `active` flag.
pub fn collect_base_mods(root: &std::path::Path, active_id: &str) -> Vec<ModEntry> {
    use parish_core::game_mod::{ModKind, ModManifest};

    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut mods: Vec<ModEntry> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let manifest_path = e.path().join("mod.toml");
            let text = std::fs::read_to_string(&manifest_path).ok()?;
            let manifest = ModManifest::from_toml_str(&text).ok()?;
            if manifest.meta.kind != ModKind::Base {
                return None;
            }
            Some(ModEntry {
                active: manifest.meta.id == active_id,
                id: manifest.meta.id.clone(),
                name: manifest.meta.name.clone(),
                title: manifest.meta.title.clone(),
                version: manifest.meta.version.clone(),
                description: manifest.meta.description.clone(),
            })
        })
        .collect();
    mods.sort_by(|a, b| a.id.cmp(&b.id));
    mods
}

/// `GET /api/mods` — lists all discoverable base mods with an `active` flag.
pub async fn list_mods(Extension(state): Extension<Arc<AppState>>) -> Json<Vec<ModEntry>> {
    let root = state.mods_root();
    let active_id = state
        .game_mod
        .as_ref()
        .map(|gm| gm.manifest.meta.id.clone())
        .unwrap_or_default();
    let mods = tokio::task::spawn_blocking(move || collect_base_mods(&root, &active_id))
        .await
        .unwrap_or_default();
    Json(mods)
}

#[derive(serde::Deserialize)]
pub struct SwitchModBody {
    pub mod_id: String,
}

/// `POST /api/mods/switch` — updates `mods/mod-list.toml` to select a new
/// active base mod.
///
/// The running server continues with the currently-loaded mod until it is
/// restarted; the client should reload after a server restart to see the new
/// world, palette, and NPC roster.
pub async fn switch_mod(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<SwitchModBody>,
) -> impl IntoResponse {
    let root = state.mods_root();

    // Validate the requested id exists on disk before writing anything.
    let active_id = state
        .game_mod
        .as_ref()
        .map(|gm| gm.manifest.meta.id.clone())
        .unwrap_or_default();
    let available = tokio::task::spawn_blocking({
        let root = root.clone();
        let active_id = active_id.clone();
        move || collect_base_mods(&root, &active_id)
    })
    .await
    .unwrap_or_default();

    if !available.iter().any(|m| m.id == body.mod_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "error": "unknown mod id"})),
        );
    }

    let mod_list_path = root.join("mod-list.toml");
    let content = format!("active_base = {:?}\n", body.mod_id);
    if let Err(e) = tokio::fs::write(&mod_list_path, &content).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        );
    }

    tracing::info!("Mod switch requested: {} → {}", active_id, body.mod_id);
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}
