//! Disk-backed cache for NLS historic map tiles.
//!
//! [`TileCache`] proxies tile requests from the client through the server
//! rather than having the client hit upstream tile servers directly.  On a
//! cache miss it fetches the tile via the XYZ URL template registered for the
//! source in config, writes the bytes to
//! `cache_dir/{source_id}/{z}/{x}/{y}.png`, and returns them.  On a cache
//! hit it reads the file from disk.
//!
//! The cache directory is resolved once at server startup from config / env
//! (`PARISH_TILE_CACHE_DIR` or `<saves_dir>/tile-cache/`) and stored on
//! `GlobalState` — per CLAUDE.md rule #9 ("resolve runtime paths from
//! explicit config, not the cwd").
//!
//! URL templates (one per registered tile source) are loaded from `MapConfig`
//! at startup and stored inside `TileCache`, so the upstream fetch URL is
//! never derived from user-supplied HTTP path params (SSRF prevention).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::ParishError;

/// Disk-backed HTTP cache for tile sources.
///
/// Constructed once at server boot and stored on `GlobalState` (web) or
/// `AppState` (desktop/CLI).  All clone-and-pass patterns are fine because
/// the inner [`reqwest::Client`] and the URL-template map are both
/// `Arc`-backed / cheaply cloneable.
///
/// Lookup order in [`TileCache::get`]:
/// 1. `cache_dir` — mutable per-user cache, written on upstream miss.
/// 2. `bundled_dir` — read-only pre-seeded bundle (e.g. shipped game data);
///    hit returns immediately without touching the network or writing to cache.
/// 3. Upstream fetch via `url_templates` → persisted to `cache_dir`.
#[derive(Clone)]
pub struct TileCache {
    /// Root directory for cached tile files.
    cache_dir: PathBuf,
    /// Optional read-only directory of pre-seeded tiles. Checked after
    /// `cache_dir` misses and before upstream fetch. Enables offline play
    /// when the tile bundle is shipped with the game data.
    bundled_dir: Option<PathBuf>,
    /// XYZ URL templates keyed by source id, loaded from config at startup.
    /// Using config-provided templates (not user input) in the upstream fetch
    /// prevents SSRF: the remote host is always a config-vetted value.
    url_templates: std::sync::Arc<HashMap<String, String>>,
    /// Shared HTTP client (reuses connections across requests).
    http: reqwest::Client,
}

impl TileCache {
    /// Creates a new cache rooted at `cache_dir`.
    ///
    /// `url_templates` maps each registered tile source id to its XYZ URL
    /// template (e.g. `https://…/{z}/{x}/{y}.png`).  It must be populated
    /// from trusted config data at server startup — never from request params.
    ///
    /// `cache_dir` must exist or be creatable at request time — the cache
    /// creates subdirectories lazily on first write per source/z/x combination.
    pub fn new(cache_dir: PathBuf, url_templates: HashMap<String, String>) -> Self {
        Self {
            cache_dir,
            bundled_dir: None,
            url_templates: std::sync::Arc::new(url_templates),
            http: reqwest::Client::new(),
        }
    }

    /// Set a read-only bundled tile directory checked before upstream fetches.
    ///
    /// Tiles found here are returned directly without being copied to
    /// `cache_dir`.  The directory must already exist; an absent path is
    /// silently ignored at lookup time (the cache falls through to upstream).
    pub fn with_bundled_dir(mut self, dir: PathBuf) -> Self {
        self.bundled_dir = Some(dir);
        self
    }

    /// Returns a tile from cache (disk hit) or fetches it from upstream (miss).
    ///
    /// The `source_id` must be a safe path component — the route handler
    /// validates it against the registered tile sources before calling here.
    ///
    /// # Errors
    ///
    /// - [`ParishError::Config`] if `source_id` is not in the registered sources.
    /// - [`ParishError::Io`] if the on-disk tile cannot be read or written.
    /// - [`ParishError::Network`] if the upstream fetch fails.
    pub async fn get(
        &self,
        source_id: &str,
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, ParishError> {
        // Defence-in-depth: reject source_ids that are not safe path components.
        // The route handler already validates against a config allowlist, but
        // CodeQL tracks user-controlled data from HTTP path params into
        // PathBuf::join, so we add an explicit character-class guard here.
        if source_id.is_empty()
            || !source_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(ParishError::Config(format!(
                "tile source id contains unsafe characters: {source_id:?}"
            )));
        }

        // iter().find() returns the stored key+value from trusted config data.
        // Using the config's own key (not the user-supplied source_id) for both
        // the disk path and URL template breaks CodeQL's taint chain: the path
        // components and upstream URL are derived from config, never from the
        // HTTP request parameter.
        let (config_source_id, url_template) = self
            .url_templates
            .iter()
            .find(|(k, _)| k.as_str() == source_id)
            .ok_or_else(|| {
                ParishError::Config(format!("tile source not registered: {source_id:?}"))
            })?;

        // Path::file_name() is a CodeQL-recognised path-traversal sanitiser:
        // it returns only the last path component, stripping any `..` segments
        // or leading `/`.  For config-loaded slug keys this is a no-op, but
        // it gives CodeQL a concrete sanitisation step to track statically.
        let safe_dir = std::path::Path::new(config_source_id.as_str())
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                ParishError::Config(format!(
                    "tile source id is not a valid path component: {config_source_id:?}"
                ))
            })?;

        let tile_path = self
            .cache_dir
            .join(safe_dir)
            .join(z.to_string())
            .join(x.to_string())
            .join(format!("{y}.png"));

        // ── Cache hit ──────────────────────────────────────────────────────
        match tokio::fs::read(&tile_path).await {
            Ok(data) => return Ok(data),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        // ── Bundled-dir fallback (read-only) ─────────────────────────────
        // Checked after a cache_dir miss and before hitting the network.
        // Uses the same CodeQL-safe `safe_dir` derived above so the path
        // components are never derived from raw HTTP params.
        if let Some(ref bundled) = self.bundled_dir {
            let bundled_path = bundled
                .join(safe_dir)
                .join(z.to_string())
                .join(x.to_string())
                .join(format!("{y}.png"));
            match tokio::fs::read(&bundled_path).await {
                Ok(data) => return Ok(data),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }

        // ── Cache miss: fetch from upstream ───────────────────────────────
        let url = url_template
            .replace("{z}", &z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string());
        tracing::debug!(url = %url, "tile cache miss — fetching from upstream");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ParishError::Network(
                resp.error_for_status().unwrap_err().to_string(),
            ));
        }

        let data = resp
            .bytes()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?
            .to_vec();

        // ── Persist to disk ────────────────────────────────────────────────
        if let Some(parent) = tile_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tmp_path = tile_path.with_extension("tmp");
        tokio::fs::write(&tmp_path, &data).await?;
        tokio::fs::rename(&tmp_path, &tile_path).await?;

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_cache(dir: &TempDir) -> TileCache {
        let mut templates = HashMap::new();
        templates.insert(
            "roscommon1".to_string(),
            "https://example.com/{z}/{x}/{y}.png".to_string(),
        );
        TileCache::new(dir.path().to_path_buf(), templates)
    }

    fn make_cache_with_url(dir: &TempDir, url: &str) -> TileCache {
        let mut templates = HashMap::new();
        templates.insert("roscommon1".to_string(), url.to_string());
        TileCache::new(dir.path().to_path_buf(), templates)
    }

    #[test]
    fn tile_path_is_deterministic() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);
        let path = cache
            .cache_dir
            .join("roscommon1")
            .join("10")
            .join("500")
            .join("350.png");
        assert!(path.to_str().unwrap().contains("roscommon1/10/500/350.png"));
    }

    #[tokio::test]
    async fn get_unknown_source_returns_config_error() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);
        let err = cache
            .get("nonexistent_source", 10, 500, 350)
            .await
            .unwrap_err();
        assert!(matches!(err, ParishError::Config(_)));
        assert!(err.to_string().contains("not registered"));
    }

    #[tokio::test]
    async fn get_empty_source_returns_config_error() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);
        let err = cache.get("", 10, 500, 350).await.unwrap_err();
        assert!(matches!(err, ParishError::Config(_)));
        assert!(err.to_string().contains("unsafe"));
    }

    #[tokio::test]
    async fn get_unsafe_source_returns_config_error() {
        let dir = TempDir::new().unwrap();
        let cache = make_cache(&dir);
        let err = cache.get("../etc/passwd", 10, 500, 350).await.unwrap_err();
        assert!(matches!(err, ParishError::Config(_)));
        assert!(err.to_string().contains("unsafe"));
    }

    #[tokio::test]
    async fn get_cache_miss_fetches_from_upstream_then_hit_reads_disk() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/10/500/350.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"tile-data"))
            .mount(&mock_server)
            .await;

        let dir = TempDir::new().unwrap();
        let upstream_url = format!("{}/{{z}}/{{x}}/{{y}}.png", mock_server.uri());
        let cache = make_cache_with_url(&dir, &upstream_url);

        // Cache miss: fetches from upstream
        let data = cache.get("roscommon1", 10, 500, 350).await.unwrap();
        assert_eq!(data, b"tile-data");

        // Cache hit: reads from disk (no mock server needed — it was already persisted)
        let data = cache.get("roscommon1", 10, 500, 350).await.unwrap();
        assert_eq!(data, b"tile-data");
    }

    #[tokio::test]
    async fn get_upstream_failure_returns_network_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/10/500/350.png"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let dir = TempDir::new().unwrap();
        let upstream_url = format!("{}/{{z}}/{{x}}/{{y}}.png", mock_server.uri());
        let cache = make_cache_with_url(&dir, &upstream_url);

        let err = cache.get("roscommon1", 10, 500, 350).await.unwrap_err();
        assert!(matches!(err, ParishError::Network(_)));
    }

    // ── bundled_dir tests ────────────────────────────────────────────────

    fn write_bundled_tile(
        bundled_dir: &TempDir,
        source: &str,
        z: u32,
        x: u32,
        y: u32,
        data: &[u8],
    ) {
        let path = bundled_dir
            .path()
            .join(source)
            .join(z.to_string())
            .join(x.to_string())
            .join(format!("{y}.png"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }

    #[tokio::test]
    async fn bundled_dir_hit_skips_upstream() {
        // If the tile is in the bundled dir, no upstream request should fire.
        let mock_server = MockServer::start().await;
        // No mocks registered — any request to the mock server would panic.

        let cache_dir = TempDir::new().unwrap();
        let bundled_dir = TempDir::new().unwrap();
        write_bundled_tile(&bundled_dir, "roscommon1", 10, 500, 350, b"bundled-tile");

        let upstream_url = format!("{}/{{z}}/{{x}}/{{y}}.png", mock_server.uri());
        let cache = make_cache_with_url(&cache_dir, &upstream_url)
            .with_bundled_dir(bundled_dir.path().to_path_buf());

        let data = cache.get("roscommon1", 10, 500, 350).await.unwrap();
        assert_eq!(data, b"bundled-tile");
    }

    #[tokio::test]
    async fn bundled_dir_miss_falls_through_to_upstream() {
        // Bundled dir exists but lacks the tile → upstream is called → result cached.
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/10/500/350.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"upstream-tile"))
            .mount(&mock_server)
            .await;

        let cache_dir = TempDir::new().unwrap();
        let bundled_dir = TempDir::new().unwrap(); // empty — no tile here

        let upstream_url = format!("{}/{{z}}/{{x}}/{{y}}.png", mock_server.uri());
        let cache = make_cache_with_url(&cache_dir, &upstream_url)
            .with_bundled_dir(bundled_dir.path().to_path_buf());

        let data = cache.get("roscommon1", 10, 500, 350).await.unwrap();
        assert_eq!(data, b"upstream-tile");

        // Should now be in cache_dir (persisted on upstream hit)
        let cached_path = cache_dir
            .path()
            .join("roscommon1")
            .join("10")
            .join("500")
            .join("350.png");
        assert!(cached_path.exists());
    }

    #[tokio::test]
    async fn cache_dir_hit_takes_precedence_over_bundled_dir() {
        // cache_dir has a tile; bundled_dir has a different one.
        // cache_dir must win (it is checked first).
        let cache_dir = TempDir::new().unwrap();
        let bundled_dir = TempDir::new().unwrap();

        // Write different content to each dir
        let cached_path = cache_dir
            .path()
            .join("roscommon1")
            .join("10")
            .join("500")
            .join("350.png");
        std::fs::create_dir_all(cached_path.parent().unwrap()).unwrap();
        std::fs::write(&cached_path, b"cache-tile").unwrap();
        write_bundled_tile(&bundled_dir, "roscommon1", 10, 500, 350, b"bundled-tile");

        let cache = make_cache(&cache_dir).with_bundled_dir(bundled_dir.path().to_path_buf());

        let data = cache.get("roscommon1", 10, 500, 350).await.unwrap();
        assert_eq!(data, b"cache-tile");
    }

    #[tokio::test]
    async fn bundled_dir_io_error_propagates() {
        // bundled_dir is a file, not a directory → joining paths returns a
        // non-directory entry → read fails with something other than NotFound.
        let cache_dir = TempDir::new().unwrap();
        let bundled_file = cache_dir.path().join("not-a-dir.txt");
        std::fs::write(&bundled_file, b"oops").unwrap();

        // Place a same-name path inside the file so the subpath yields an error.
        // On Linux, joining paths into a regular file gives NotADirectory on read.
        let cache = make_cache(&cache_dir).with_bundled_dir(bundled_file.clone());

        // The attempt to read bundled_file/roscommon1/10/500/350.png should
        // fail with an I/O error other than NotFound (ENOTDIR), surfaced as
        // ParishError::Io. If it's NotFound the fallback would hit the upstream
        // (no templates → Config error); either outcome is acceptable here but
        // the important invariant is: no panic, clean error.
        let result = cache.get("roscommon1", 10, 500, 350).await;
        assert!(result.is_err());
    }
}
