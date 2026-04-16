//! Deterministic JSON formatting with atomic writes.
//!
//! The single most important invariant of the Parish Designer editor: editing
//! and re-saving `mods/rundale/npcs.json` *unchanged* must produce an empty
//! `git diff`. Any drift here silently corrupts source files over time.
//!
//! This module provides [`write_json_deterministic`], which formats JSON with
//! a stable **4-space indent** (matching the on-disk convention in
//! `mods/rundale/*.json`) and atomically writes the result by staging to a
//! `.tmp` sibling file and renaming on success.

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::ser::{PrettyFormatter, Serializer};

use parish_types::ParishError;

/// Writes a serializable value to `path` as pretty JSON, atomically.
///
/// 1. Serializes `value` with a 4-space-indent `PrettyFormatter` (matches
///    the on-disk `mods/rundale/*.json` convention).
/// 2. Appends a trailing newline (matches the existing file convention and
///    keeps `git diff` happy).
/// 3. Writes to `<path>.tmp` first.
/// 4. Renames `<path>.tmp` → `<path>` atomically.
///
/// Field order in the output is controlled by the `Serialize` impl — use
/// structs (not maps) where ordering matters, and prefer `BTreeMap` over
/// `HashMap` for any map-typed fields.
pub fn write_json_deterministic<T: Serialize>(path: &Path, value: &T) -> Result<(), ParishError> {
    let mut buf = Vec::with_capacity(4096);
    let formatter = PrettyFormatter::with_indent(b"    ");
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    value
        .serialize(&mut ser)
        .map_err(ParishError::Serialization)?;
    let mut body = String::from_utf8(buf)
        .map_err(|e| ParishError::Config(format!("JSON output is not UTF-8: {}", e)))?;
    body.push('\n');

    let tmp_path = tmp_path_for(path);
    fs::write(&tmp_path, body.as_bytes()).map_err(|e| {
        ParishError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to write {}: {}", tmp_path.display(), e),
        ))
    })?;

    fs::rename(&tmp_path, path).map_err(|e| {
        // Clean up the tmp file if the rename failed so we don't leave
        // orphans on disk.
        let _ = fs::remove_file(&tmp_path);
        ParishError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "failed to rename {} to {}: {}",
                tmp_path.display(),
                path.display(),
                e
            ),
        ))
    })?;

    Ok(())
}

/// Computes the staging path used by [`write_json_deterministic`].
fn tmp_path_for(path: &Path) -> std::path::PathBuf {
    let mut os_name = path.as_os_str().to_os_string();
    os_name.push(".tmp");
    std::path::PathBuf::from(os_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        count: u32,
        tags: Vec<String>,
    }

    #[test]
    fn atomic_write_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        let value = Sample {
            name: "alice".into(),
            count: 7,
            tags: vec!["a".into(), "b".into()],
        };
        write_json_deterministic(&path, &value).unwrap();
        assert!(path.exists());
        // Ensure the tmp file is gone after a successful rename.
        assert!(!tmp_path_for(&path).exists());
    }

    #[test]
    fn atomic_write_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        let value = Sample {
            name: "bob".into(),
            count: 1,
            tags: vec!["only".into()],
        };
        write_json_deterministic(&path, &value).unwrap();
        let first = fs::read(&path).unwrap();
        write_json_deterministic(&path, &value).unwrap();
        let second = fs::read(&path).unwrap();
        assert_eq!(first, second, "two writes must produce identical bytes");
    }

    #[test]
    fn atomic_write_ends_with_newline() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        let value = Sample {
            name: "c".into(),
            count: 0,
            tags: vec![],
        };
        write_json_deterministic(&path, &value).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn round_trip_preserves_content() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        let original = Sample {
            name: "d".into(),
            count: 42,
            tags: vec!["x".into(), "y".into(), "z".into()],
        };
        write_json_deterministic(&path, &original).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let parsed: Sample = serde_json::from_str(&text).unwrap();
        assert_eq!(original, parsed);
    }
}
