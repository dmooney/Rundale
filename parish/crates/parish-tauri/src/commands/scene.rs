//! Scene-diorama commands for the desktop frontend.

use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use parish_core::game_mod::{GameMod, canonical_scene_asset_path};
use parish_core::ipc::{SceneState, build_scene_state_relative, map_scene_state_asset_urls};

use crate::AppState;

/// Returns the active diorama scene state, or `None` when the `diorama`
/// kill switch is disabled or the current location has no scene.
#[tauri::command]
pub async fn get_scene_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<SceneState>, String> {
    let Some(game_mod) = state.game_mod.as_ref() else {
        return Ok(None);
    };

    let relative = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let flags = state.config.lock().await.flags.clone();
        build_scene_state_relative(&world, &npc_manager, game_mod.scenes.as_ref(), &flags)
    };

    Ok(relative.and_then(|scene| {
        map_scene_state_asset_urls(scene, &|rel| mod_scene_asset_data_url(game_mod, rel).ok())
    }))
}

pub(crate) fn mod_scene_asset_data_url(game_mod: &GameMod, rel: &str) -> Result<String, String> {
    scene_asset_data_url(&game_mod.mod_dir, rel)
}

pub(crate) fn scene_asset_data_url(mod_dir: &Path, rel: &str) -> Result<String, String> {
    let path = canonical_scene_asset_path(mod_dir, rel)?;
    let bytes =
        std::fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    Ok(format!(
        "data:{};base64,{}",
        mime_for_path(&path),
        STANDARD.encode(bytes)
    ))
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn scene_asset_data_url_reads_scene_asset_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let asset_dir = dir.path().join("assets/scenes/example");
        std::fs::create_dir_all(&asset_dir).unwrap();
        std::fs::write(asset_dir.join("sprite.png"), [1_u8, 2, 3, 4]).unwrap();

        let url = scene_asset_data_url(dir.path(), "assets/scenes/example/sprite.png").unwrap();
        let encoded = url
            .strip_prefix("data:image/png;base64,")
            .expect("expected image/png data URL");
        let bytes = STANDARD.decode(encoded).unwrap();
        assert_eq!(bytes, vec![1, 2, 3, 4]);
    }

    #[test]
    fn scene_asset_data_url_rejects_non_scene_assets() {
        let dir = tempfile::tempdir().unwrap();
        let asset_dir = dir.path().join("assets");
        std::fs::create_dir_all(&asset_dir).unwrap();
        std::fs::write(asset_dir.join("icon.png"), [1_u8]).unwrap();

        let err = scene_asset_data_url(dir.path(), "assets/icon.png").unwrap_err();
        assert!(err.contains("assets/scenes"), "unexpected error: {err}");
    }

    #[test]
    fn scene_asset_data_url_uses_svg_mime() {
        let dir = tempfile::tempdir().unwrap();
        let asset_dir = dir.path().join("assets/scenes/example");
        std::fs::create_dir_all(&asset_dir).unwrap();
        std::fs::write(asset_dir.join("atom.svg"), b"<svg></svg>").unwrap();

        let url = scene_asset_data_url(dir.path(), "assets/scenes/example/atom.svg").unwrap();

        assert!(url.starts_with("data:image/svg+xml;base64,"), "got: {url}");
    }
}
