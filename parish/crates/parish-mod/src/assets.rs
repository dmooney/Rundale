//! Asset-path validation helpers used during mod loading.

use std::path::{Component, Path, PathBuf};

use parish_types::error::ParishError;

pub(crate) fn validate_optional_asset_ref(
    mod_dir: &Path,
    field: &str,
    rel: Option<&str>,
) -> Result<(), ParishError> {
    match rel {
        Some(path) => canonical_mod_asset_path(mod_dir, path)
            .map(|_| ())
            .map_err(|e| ParishError::Config(format!("{field}: {e}"))),
        None => Ok(()),
    }
}

/// Resolves a mod-relative asset path to a canonical absolute path, rejecting
/// absolute paths, anything outside `assets/`, traversal components, and paths
/// that escape the mod directory. Public so runtime asset-serving paths (the
/// diorama `scene-asset` route and the Tauri data-URL command) can re-validate
/// a request path through the same guard before reading bytes.
pub fn canonical_mod_asset_path(mod_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(format!("asset path {rel} must be relative"));
    }
    if !rel_path.starts_with("assets") {
        return Err(format!("asset path {rel} must live under assets/"));
    }
    if rel_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("asset path {rel} contains invalid path components"));
    }

    let candidate = mod_dir.join(rel_path);
    let canonical = candidate
        .canonicalize()
        .map_err(|e| format!("failed to resolve {}: {e}", candidate.display()))?;
    if !canonical.starts_with(mod_dir) {
        return Err(format!("asset path {rel} escapes mod directory"));
    }
    Ok(canonical)
}
