//! Export: copy an accepted post-processed asset into the mod's scenes
//! directory and upsert its path in `scenes.json`.
//!
//! Export is restricted to `mods/rundale/assets/scenes/` — any destination
//! that does not resolve under that tree is rejected (AGENTS rule, path
//! traversal guard).

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

// ---------------------------------------------------------------------------
// Path guard
// ---------------------------------------------------------------------------

/// The allowed export root, relative to the project root.
const ALLOWED_EXPORT_ROOT: &str = "mods/rundale/assets/scenes";

/// Validate that `dest` resolves under `allowed_root`.
///
/// Both paths must be canonicalisable (i.e. all intermediate directories must
/// exist or be created before this is called for the `dest` itself to
/// canonicalise; we canonicalise the parent directory instead).
fn guard_export_path(allowed_root: &Path, dest: &Path) -> Result<()> {
    // The destination file may not exist yet (we're about to create it), so
    // canonicalise its *parent* directory instead.
    let dest_parent = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("export destination has no parent directory"))?;

    if !dest_parent.exists() {
        std::fs::create_dir_all(dest_parent)?;
    }

    let canonical_root = allowed_root.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "cannot canonicalize export root {}: {e}",
            allowed_root.display()
        )
    })?;
    let canonical_parent = dest_parent.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "cannot canonicalize destination parent {}: {e}",
            dest_parent.display()
        )
    })?;

    if !canonical_parent.starts_with(&canonical_root) {
        bail!(
            "export destination {} is outside the allowed export root {}",
            dest.display(),
            allowed_root.display()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Copy `src_path` (the post-processed PNG) to `dest_path` inside the mod's
/// `assets/scenes/` directory.
///
/// `project_root` is the repository root.
/// `dest_rel` is the path relative to `project_root` (e.g.
/// `"mods/rundale/assets/scenes/darcys-pub/plate.png"`).
///
/// Returns the absolute destination path on success.
pub fn export_asset(project_root: &Path, src_path: &Path, dest_rel: &str) -> Result<PathBuf> {
    let allowed_root = project_root.join(ALLOWED_EXPORT_ROOT);
    let dest = project_root.join(dest_rel);

    // Enforce path guard before writing anything.
    guard_export_path(&allowed_root, &dest)?;

    std::fs::copy(src_path, &dest)?;
    Ok(dest)
}

/// Upsert a plate or sprite path in `scenes.json`.
///
/// For plates: finds the scene entry with the matching `location_id` (or
/// `slug`) and updates the `plate` field.
/// For sprites: finds the sprite entry with the matching `npc_id` or adds a
/// new entry.
///
/// Uses a raw `serde_json::Value` parse-modify-write cycle so that fields
/// not known to this crate are preserved without a full scenes-schema dep.
pub fn upsert_scenes_json(
    scenes_json_path: &Path,
    entry_type: SceneEntryType,
    asset_rel_path: &str,
) -> Result<()> {
    let json = if scenes_json_path.exists() {
        std::fs::read_to_string(scenes_json_path)?
    } else {
        r#"{"scenes":[],"sprites":[]}"#.to_string()
    };

    let mut value: serde_json::Value = serde_json::from_str(&json)?;

    match entry_type {
        SceneEntryType::Plate {
            location_id,
            slug: _,
        } => {
            if let Some(scenes) = value.get_mut("scenes").and_then(|v| v.as_array_mut()) {
                let found = scenes.iter_mut().find(|s| {
                    s.get("location_id")
                        .and_then(|v| v.as_u64())
                        .is_some_and(|id| id == location_id as u64)
                });
                if let Some(scene) = found {
                    scene["plate"] = serde_json::Value::String(asset_rel_path.to_string());
                }
                // If no scene entry exists for this location we don't create
                // one here — scene entries require hotspots/slots that are
                // hand-authored.
            }
        }
        SceneEntryType::Sprite { npc_id } => {
            // Ensure "sprites" key exists as an array.
            if let Some(obj) = value.as_object_mut() {
                obj.entry("sprites")
                    .or_insert_with(|| serde_json::Value::Array(vec![]));
            }
            let sprites = value.get_mut("sprites").expect("just inserted");
            if let Some(arr) = sprites.as_array_mut() {
                let found = arr.iter_mut().find(|s| {
                    s.get("npc_id")
                        .and_then(|v| v.as_u64())
                        .is_some_and(|id| id == npc_id as u64)
                });
                if let Some(entry) = found {
                    entry["image"] = serde_json::Value::String(asset_rel_path.to_string());
                } else {
                    let new_entry = serde_json::json!({
                        "npc_id": npc_id,
                        "image": asset_rel_path
                    });
                    arr.push(new_entry);
                }
            }
        }
        SceneEntryType::Variant {
            location_id,
            variant,
        } => {
            if let Some(scenes) = value.get_mut("scenes").and_then(|v| v.as_array_mut()) {
                let found = scenes.iter_mut().find(|s| {
                    s.get("location_id")
                        .and_then(|v| v.as_u64())
                        .is_some_and(|id| id == location_id as u64)
                });
                if let Some(scene) = found {
                    // Ensure "variants" key exists as an object.
                    if let Some(s_obj) = scene.as_object_mut() {
                        s_obj
                            .entry("variants")
                            .or_insert_with(|| serde_json::Value::Object(Default::default()));
                    }
                    let variants = scene.get_mut("variants").expect("just inserted");
                    if let Some(obj) = variants.as_object_mut() {
                        obj.insert(
                            variant,
                            serde_json::Value::String(asset_rel_path.to_string()),
                        );
                    }
                }
            }
        }
    }

    let out = serde_json::to_string_pretty(&value)?;
    std::fs::write(scenes_json_path, out)?;
    Ok(())
}

/// Describes what kind of entry to upsert in `scenes.json`.
#[derive(Debug)]
pub enum SceneEntryType {
    // `_slug` is authoring metadata recorded in the manifest; not currently
    // used by the upsert logic (which keys on `location_id`), but retained
    // for future use (e.g. creating a new scene entry from scratch in M5).
    #[allow(dead_code)]
    Plate {
        location_id: u32,
        #[allow(dead_code)]
        slug: String,
    },
    Sprite {
        npc_id: u32,
    },
    Variant {
        location_id: u32,
        variant: String,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_dummy_png(path: &Path) {
        // Minimal 1x1 transparent PNG for path-guard tests.
        // PNG header + IHDR + IDAT + IEND for a 1x1 RGBA image.
        let png: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // PNG signature
            0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, // IHDR length + type
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // width=1, height=1
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, // bit depth + colour + crc
            0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, // IDAT
            0x78, 0x9c, 0x62, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xe2, 0x21, 0xbc, 0x33, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82, // IEND
        ];
        fs::write(path, png).unwrap();
    }

    #[test]
    fn export_asset_copies_to_correct_location() {
        let dir = tempdir().unwrap();
        // Set up a fake project root with the allowed export root present.
        let project_root = dir.path();
        let allowed_root = project_root.join("mods/rundale/assets/scenes");
        fs::create_dir_all(allowed_root.join("darcys-pub")).unwrap();

        let src = project_root.join("art/plate-2-001.png");
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        write_dummy_png(&src);

        let dest_rel = "mods/rundale/assets/scenes/darcys-pub/plate.png";
        let dest = export_asset(project_root, &src, dest_rel).unwrap();

        assert!(dest.exists(), "exported file must exist");
    }

    #[test]
    fn export_asset_rejects_path_outside_scenes() {
        let dir = tempdir().unwrap();
        let project_root = dir.path();
        let allowed_root = project_root.join("mods/rundale/assets/scenes");
        fs::create_dir_all(&allowed_root).unwrap();

        let src = project_root.join("art/plate-2-001.png");
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        write_dummy_png(&src);

        // Traversal attempt
        let dest_rel = "mods/rundale/assets/sprites/malicious.png";
        let result = export_asset(project_root, &src, dest_rel);
        assert!(
            result.is_err(),
            "export outside assets/scenes must be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("outside the allowed export root"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn export_asset_rejects_traversal_to_etc() {
        let dir = tempdir().unwrap();
        let project_root = dir.path();
        let allowed_root = project_root.join("mods/rundale/assets/scenes");
        fs::create_dir_all(&allowed_root).unwrap();

        let src = project_root.join("art/plate.png");
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        write_dummy_png(&src);

        // Attempt to escape via traversal — will try to create dirs under /tmp
        // which should still fail the path guard.
        let dest_rel = "../../etc/malicious.png";
        let result = export_asset(project_root, &src, dest_rel);
        assert!(result.is_err(), "traversal export must be rejected");
    }

    #[test]
    fn upsert_scenes_json_updates_plate_path() {
        let dir = tempdir().unwrap();
        let scenes_path = dir.path().join("scenes.json");

        let initial = serde_json::json!({
            "scenes": [
                {
                    "location_id": 2,
                    "slug": "darcys-pub",
                    "plate": "assets/scenes/darcys-pub/placeholder.png",
                    "hotspots": [],
                    "slots": []
                }
            ],
            "sprites": []
        });
        fs::write(
            &scenes_path,
            serde_json::to_string_pretty(&initial).unwrap(),
        )
        .unwrap();

        upsert_scenes_json(
            &scenes_path,
            SceneEntryType::Plate {
                location_id: 2,
                slug: "darcys-pub".to_string(),
            },
            "assets/scenes/darcys-pub/plate.png",
        )
        .unwrap();

        let out: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&scenes_path).unwrap()).unwrap();
        assert_eq!(
            out["scenes"][0]["plate"],
            "assets/scenes/darcys-pub/plate.png"
        );
    }

    #[test]
    fn upsert_scenes_json_adds_new_sprite() {
        let dir = tempdir().unwrap();
        let scenes_path = dir.path().join("scenes.json");
        fs::write(&scenes_path, r#"{"scenes":[],"sprites":[]}"#).unwrap();

        upsert_scenes_json(
            &scenes_path,
            SceneEntryType::Sprite { npc_id: 1 },
            "assets/scenes/sprites/padraig-darcy.png",
        )
        .unwrap();

        let out: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&scenes_path).unwrap()).unwrap();
        assert_eq!(out["sprites"][0]["npc_id"], 1);
        assert_eq!(
            out["sprites"][0]["image"],
            "assets/scenes/sprites/padraig-darcy.png"
        );
    }
}
