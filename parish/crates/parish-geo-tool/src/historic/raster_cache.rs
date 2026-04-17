//! On-disk PNG tile cache.
//!
//! Mirrors the URL layout for readability: `{cache_dir}/{source_id}/{z}/{x}/{y}.png`.
//! Sibling to the JSON Overpass cache in `super::super::cache`, with the
//! same `no_cache` semantics.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::debug;

/// Disk-backed cache of PNG tile bytes, keyed by `(source_id, z, x, y)`.
#[derive(Debug, Clone)]
pub struct RasterCache {
    root: PathBuf,
    no_cache: bool,
}

impl RasterCache {
    /// Creates a cache rooted at `cache_dir`. The directory is created
    /// lazily on first write.
    pub fn new(cache_dir: &Path, no_cache: bool) -> Self {
        Self {
            root: cache_dir.to_path_buf(),
            no_cache,
        }
    }

    fn tile_path(&self, source_id: &str, z: u8, x: u32, y: u32) -> PathBuf {
        self.root
            .join(source_id)
            .join(z.to_string())
            .join(x.to_string())
            .join(format!("{y}.png"))
    }

    /// Returns cached PNG bytes if present and not bypassed via `no_cache`.
    pub fn get(&self, source_id: &str, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>> {
        if self.no_cache {
            return Ok(None);
        }
        let path = self.tile_path(source_id, z, x, y);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)
            .with_context(|| format!("failed to read cached tile {}", path.display()))?;
        debug!("raster cache hit: {}/{}/{}/{}.png", source_id, z, x, y);
        Ok(Some(bytes))
    }

    /// Stores PNG bytes for a tile.
    pub fn put(&self, source_id: &str, z: u8, x: u32, y: u32, bytes: &[u8]) -> Result<()> {
        if self.no_cache {
            return Ok(());
        }
        let path = self.tile_path(source_id, z, x, y);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create tile cache dir {}", parent.display()))?;
        }
        std::fs::write(&path, bytes)
            .with_context(|| format!("failed to write cached tile {}", path.display()))?;
        debug!(
            "raster cache write: {}/{}/{}/{}.png ({} bytes)",
            source_id,
            z,
            x,
            y,
            bytes.len()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_path_layout() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RasterCache::new(dir.path(), false);
        let p = cache.tile_path("nls-roscommon", 16, 32191, 21587);
        let expected = dir
            .path()
            .join("nls-roscommon")
            .join("16")
            .join("32191")
            .join("21587.png");
        assert_eq!(p, expected);
    }

    #[test]
    fn test_put_and_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RasterCache::new(dir.path(), false);
        let bytes = b"\x89PNG\r\n\x1a\nfake".to_vec();
        cache.put("src", 15, 10, 20, &bytes).unwrap();
        let loaded = cache.get("src", 15, 10, 20).unwrap();
        assert_eq!(loaded.as_deref(), Some(bytes.as_slice()));
    }

    #[test]
    fn test_no_cache_bypasses_reads_and_writes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RasterCache::new(dir.path(), true);
        cache.put("src", 15, 1, 1, b"ignored").unwrap();
        // no_cache writes are no-ops, so the file must not exist.
        assert!(!cache.tile_path("src", 15, 1, 1).exists());
        // And reads always return None.
        assert!(cache.get("src", 15, 1, 1).unwrap().is_none());
    }

    #[test]
    fn test_miss_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RasterCache::new(dir.path(), false);
        assert!(cache.get("src", 15, 0, 0).unwrap().is_none());
    }
}
