//! Diorama scene-state command (M2).
//!
//! `get_scene_state` mirrors `get_engine_state`: it locks world + npc_manager,
//! resolves mod assets as `data:image/png;base64,...` data URLs using the
//! shared `mod_asset_data_url` helper, and delegates to the backend-agnostic
//! `parish_core::ipc::build_scene_state` builder (rule #12).
//!
//! Returns `Ok(None)` when:
//! - no mod is loaded (no `game_mod` on `AppState`),
//! - the mod declares no scene index, or
//! - the `diorama` feature flag is disabled (the builder gates on it).

use std::sync::Arc;

use crate::AppState;

/// Returns the current diorama scene state, or `None` when the diorama is
/// unavailable for the current location / configuration.
///
/// Backs both the `parish_scene_state` MCP tool and the `/api/scene-state`
/// route on the MCP bridge. Lock order: world → npc_manager → config
/// (matches the server's `get_scene_state` handler).
#[tauri::command]
pub async fn get_scene_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<parish_core::ipc::SceneState>, String> {
    let Some(gm) = state.game_mod.as_ref() else {
        return Ok(None);
    };
    let Some(scenes) = gm.scenes.as_ref() else {
        return Ok(None);
    };

    let mod_dir = gm.mod_dir.clone();
    let scenes = scenes.clone();

    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let config = state.config.lock().await;

    let asset_url = |rel: &str| -> Option<String> {
        let p = parish_core::game_mod::canonical_mod_asset_path(&mod_dir, rel).ok()?;
        crate::mod_asset_data_url(Some(p))
    };

    Ok(parish_core::ipc::build_scene_state(
        &world,
        &npc_manager,
        &scenes,
        &config.flags,
        &asset_url,
    ))
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;

    /// The `mod_asset_data_url` helper round-trips a real PNG to a valid
    /// `data:image/png;base64,...` URL.
    ///
    /// Uses one of the committed Rundale scene plates so the test does not
    /// need a temp-dir or test fixture generation.
    #[test]
    fn mod_asset_data_url_produces_valid_data_url() {
        use std::path::PathBuf;

        // Locate the test asset relative to CARGO_MANIFEST_DIR.
        // CARGO_MANIFEST_DIR is parish/crates/parish-tauri.
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // Navigate to the workspace root, then into mods/rundale.
        let asset_path = manifest_dir
            .join("../../..") // → workspace root (Rundale/)
            .join("mods/rundale/assets/icons/app/icon-16.png");

        // If the asset is missing (e.g. sparse checkout), skip the content
        // check but still assert the None path works.
        if !asset_path.exists() {
            let result = crate::mod_asset_data_url(None);
            assert!(result.is_none(), "None path must return None");
            return;
        }

        let expected_bytes = std::fs::read(&asset_path).expect("read test asset");

        // Exercise the function under test.
        let data_url = crate::mod_asset_data_url(Some(asset_path.clone()))
            .expect("mod_asset_data_url must return Some for a real file");

        // Must start with the correct prefix.
        assert!(
            data_url.starts_with("data:image/png;base64,"),
            "data URL must begin with 'data:image/png;base64,' — got: {data_url:.60}"
        );

        // The base64 payload must decode back to the original file bytes.
        let b64_payload = data_url
            .strip_prefix("data:image/png;base64,")
            .expect("prefix already checked");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64_payload)
            .expect("base64 must be valid");

        assert_eq!(
            decoded, expected_bytes,
            "decoded bytes must match original file"
        );
    }

    /// `mod_asset_data_url(None)` must return `None` without panicking.
    #[test]
    fn mod_asset_data_url_none_input_returns_none() {
        assert!(crate::mod_asset_data_url(None).is_none());
    }

    /// `mod_asset_data_url` returns `None` for a path that does not exist.
    #[test]
    fn mod_asset_data_url_missing_file_returns_none() {
        let result =
            crate::mod_asset_data_url(Some(std::path::PathBuf::from("/nonexistent/file.png")));
        assert!(result.is_none());
    }
}
