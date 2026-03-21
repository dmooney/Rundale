//! File-based response cache for Overpass API queries.
//!
//! Caches API responses to disk to avoid redundant downloads during
//! iterative development. Cache entries are keyed by a normalized
//! query identifier and stored as plain JSON files.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::debug;

/// File-based cache for Overpass API responses.
#[derive(Debug, Clone)]
pub struct ResponseCache {
    /// Directory where cached responses are stored.
    cache_dir: PathBuf,
}

impl ResponseCache {
    /// Creates a new cache backed by the given directory.
    ///
    /// The directory is created if it does not exist.
    pub fn new(cache_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(cache_dir)
            .with_context(|| format!("failed to create cache dir: {}", cache_dir.display()))?;
        Ok(Self {
            cache_dir: cache_dir.to_path_buf(),
        })
    }

    /// Returns the cached response for the given key, if it exists and is valid.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let path = self.key_path(key);
        if path.exists() {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read cache file: {}", path.display()))?;
            debug!("cache hit: {key}");
            Ok(Some(contents))
        } else {
            Ok(None)
        }
    }

    /// Stores a response in the cache under the given key.
    pub fn put(&self, key: &str, value: &str) -> Result<()> {
        let path = self.key_path(key);
        std::fs::write(&path, value)
            .with_context(|| format!("failed to write cache file: {}", path.display()))?;
        debug!("cached: {key}");
        Ok(())
    }

    /// Clears all cached entries.
    #[allow(dead_code)] // Public API for manual cache management
    pub fn clear(&self) -> Result<()> {
        if self.cache_dir.exists() {
            for entry in std::fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry.path().extension().is_some_and(|e| e == "json") {
                    std::fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }

    /// Returns the file path for a cache key.
    fn key_path(&self, key: &str) -> PathBuf {
        // Sanitize key: replace non-alphanumeric chars with underscores
        let safe_key: String = key
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.cache_dir.join(format!("{safe_key}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_put_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ResponseCache::new(dir.path()).unwrap();

        cache.put("test_key", r#"{"elements":[]}"#).unwrap();
        let result = cache.get("test_key").unwrap();
        assert_eq!(result, Some(r#"{"elements":[]}"#.to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ResponseCache::new(dir.path()).unwrap();

        let result = cache.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_cache_clear() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ResponseCache::new(dir.path()).unwrap();

        cache.put("key1", "value1").unwrap();
        cache.put("key2", "value2").unwrap();
        cache.clear().unwrap();

        assert!(cache.get("key1").unwrap().is_none());
        assert!(cache.get("key2").unwrap().is_none());
    }

    #[test]
    fn test_cache_key_sanitization() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ResponseCache::new(dir.path()).unwrap();

        // Keys with special chars should be sanitized
        cache
            .put("pois_Kiltoom_Parish", r#"{"elements":[]}"#)
            .unwrap();
        let result = cache.get("pois_Kiltoom_Parish").unwrap();
        assert!(result.is_some());
    }
}
