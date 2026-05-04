//! `ModSource` trait — abstraction over mod-content loading.
//!
//! The trait decouples call sites from the local-disk implementation so that
//! future S3, HTTP, or embedded sources can be swapped in without touching any
//! of the three entry points (Tauri, web server, headless CLI).
//!
//! Today only [`LocalDiskModSource`] exists.  Nothing in this module performs
//! any behavior change relative to the pre-trait code paths.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::ParishError;
use crate::game_mod::{DiscoveredMod, GameMod, ModKind, discover_mods_in, find_mods_root};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Lightweight summary of a discovered mod — enough to decide whether to load
/// it and to pass an identifier to [`ModSource::load_mod`].
///
/// Derived from [`DiscoveredMod`]; the setting mod (always `kind = Setting`) is
/// also represented here so callers have a uniform list.
#[derive(Debug, Clone)]
pub struct ModSummary {
    /// Machine-friendly mod identifier (e.g. `"rundale"`).
    pub id: String,
    /// Mod kind as declared in `mod.toml`.
    pub kind: ModKind,
    /// Absolute path to the mod directory (implementation detail; may not be
    /// meaningful for non-disk sources).
    pub path: PathBuf,
}

/// A fully loaded mod bundle.  Today this is exactly [`GameMod`]; the alias
/// keeps call sites readable and leaves room to add metadata fields later
/// without changing every function signature.
pub type ModBundle = GameMod;

// Helper type alias used in trait return types.
type ModFuture<T> = Pin<Box<dyn std::future::Future<Output = Result<T, ParishError>> + Send>>;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstracts over where mod content is stored.
///
/// The trait is object-safe and `Send + Sync` so implementations can be placed
/// behind `Arc<dyn ModSource>` and shared across async tasks.
///
/// Return types use `Pin<Box<dyn Future>>` rather than `impl Future` so the
/// trait is dyn-compatible (object-safe).
///
/// # Implementing
///
/// The contract is intentionally minimal:
///
/// - `list_mods` must return every discoverable mod in a deterministic order.
///   The first entry with `kind == Setting` is considered the primary mod.
/// - `load_mod` must accept any `id` returned by `list_mods` and produce the
///   corresponding fully-loaded [`ModBundle`].
pub trait ModSource: Send + Sync {
    /// Return a summary of every available mod.
    ///
    /// The order is deterministic (lexicographic by directory / registry key)
    /// so integration tests can rely on stable indices.
    fn list_mods(&self) -> ModFuture<Vec<ModSummary>>;

    /// Load a single mod by its `id` (as returned by [`list_mods`]).
    fn load_mod(&self, mod_id: &str) -> ModFuture<ModBundle>;
}

// ---------------------------------------------------------------------------
// LocalDiskModSource
// ---------------------------------------------------------------------------

/// Loads mods from a local `mods/` directory tree.
///
/// This is the only concrete implementation today.  It wraps the existing
/// [`discover_mods_in`] / [`GameMod::load`] logic one-for-one so behavior is
/// identical to the pre-trait code.
///
/// # Construction
///
/// ```ignore
/// // Auto-detect from PARISH_MODS_DIR env var or cwd-walk (dev default)
/// let src = LocalDiskModSource::new()?;
///
/// // Explicit root for tests
/// let src = LocalDiskModSource::with_root(PathBuf::from("/path/to/mods"));
/// ```
#[derive(Debug, Clone)]
pub struct LocalDiskModSource {
    /// Resolved path to the `mods/` root directory.
    pub root: PathBuf,
}

impl LocalDiskModSource {
    /// Construct using the standard resolution order:
    ///
    /// 1. `PARISH_MODS_DIR` env var.
    /// 2. cwd-walk searching for a `mods/` directory.
    ///
    /// Returns `Err` when no `mods/` directory can be located.  Callers that
    /// want a graceful fallback (no mod installed) should treat the error as
    /// `None` rather than hard-failing.
    pub fn new() -> Result<Self, ParishError> {
        let root = find_mods_root()
            .ok_or_else(|| ParishError::Config("No `mods/` directory found".to_string()))?;
        Ok(Self { root })
    }

    /// Construct from an explicit `mods/` root.  Used by tests and packaged
    /// builds that resolve the path at startup rather than walking the cwd.
    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// Convenience: construct and wrap in an `Arc<dyn ModSource>` in one step.
    pub fn new_arc() -> Result<Arc<dyn ModSource>, ParishError> {
        Ok(Arc::new(Self::new()?))
    }
}

impl ModSource for LocalDiskModSource {
    fn list_mods(&self) -> ModFuture<Vec<ModSummary>> {
        let root = self.root.clone();
        Box::pin(async move {
            let discovered = discover_mods_in(&root)?;

            // Setting mod is first in the list; auxiliary follow in lex order.
            let mut summaries = Vec::with_capacity(1 + discovered.auxiliary.len());
            // Peek at the setting manifest to obtain its id.
            let setting_id = peek_mod_id(&discovered.setting).unwrap_or_else(|| "unknown".into());
            summaries.push(ModSummary {
                id: setting_id,
                kind: ModKind::Setting,
                path: discovered.setting,
            });
            for aux in discovered.auxiliary {
                summaries.push(ModSummary {
                    id: aux.id,
                    kind: aux.kind,
                    path: aux.path,
                });
            }
            Ok(summaries)
        })
    }

    fn load_mod(&self, mod_id: &str) -> ModFuture<ModBundle> {
        let root = self.root.clone();
        let mod_id = mod_id.to_owned();
        Box::pin(async move {
            // Discover to find the directory for this id.
            let discovered = discover_mods_in(&root)?;
            let setting_id = peek_mod_id(&discovered.setting).unwrap_or_else(|| "unknown".into());

            let mod_dir = if setting_id == mod_id {
                discovered.setting
            } else {
                discovered
                    .auxiliary
                    .into_iter()
                    .find(|m: &DiscoveredMod| m.id == mod_id)
                    .map(|m| m.path)
                    .ok_or_else(|| {
                        ParishError::Config(format!("No mod with id '{mod_id}' found"))
                    })?
            };

            GameMod::load(&mod_dir)
        })
    }
}

// ---------------------------------------------------------------------------
// Sync convenience helper (used by synchronous entry points such as Tauri)
// ---------------------------------------------------------------------------

/// Load the primary setting mod from the local disk, returning `None` on any
/// error.
///
/// This is the synchronous equivalent of constructing a
/// [`LocalDiskModSource`] and awaiting `list_mods` + `load_mod`.  It is
/// provided as a free function for entry points that cannot `.await` (Tauri's
/// synchronous `run()` function, for example).
///
/// Behavior is identical to the old `find_default_mod().and_then(|dir|
/// GameMod::load(&dir).ok())` pattern so there is no behavior change.
pub fn load_setting_mod_sync() -> Option<ModBundle> {
    let source = LocalDiskModSource::new().ok()?;
    let root = source.root.clone();
    let discovered = discover_mods_in(&root).ok()?;
    let setting_id = peek_mod_id(&discovered.setting).unwrap_or_else(|| "unknown".into());
    match GameMod::load(&discovered.setting) {
        Ok(gm) => {
            tracing::info!(
                "Loaded game mod '{}' via LocalDiskModSource (sync)",
                gm.manifest.meta.name
            );
            let _ = setting_id; // suppress unused warning; id is available for logging above
            Some(gm)
        }
        Err(e) => {
            tracing::warn!("Failed to load setting mod '{}': {}", setting_id, e);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read only the `[mod].id` field from `mod.toml` without a full parse.
///
/// Returns `None` on I/O or parse error rather than propagating — callers
/// fall back to `"unknown"` which is acceptable for discovery purposes.
fn peek_mod_id(mod_dir: &std::path::Path) -> Option<String> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct MetaOnly {
        #[serde(rename = "mod")]
        meta: MetaId,
    }
    #[derive(Deserialize)]
    struct MetaId {
        id: String,
    }

    let text = std::fs::read_to_string(mod_dir.join("mod.toml")).ok()?;
    let parsed: MetaOnly = toml::from_str(&text).ok()?;
    Some(parsed.meta.id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_minimal_manifest(dir: &std::path::Path, id: &str, kind: Option<&str>) {
        fs::create_dir_all(dir).unwrap();
        let kind_line = kind
            .map(|k| format!("kind = \"{k}\"\n"))
            .unwrap_or_default();
        let body = format!(
            "[mod]\nname = \"{id}\"\nid = \"{id}\"\nversion = \"0.0.0\"\ndescription = \"x\"\n{kind_line}"
        );
        fs::write(dir.join("mod.toml"), body).unwrap();
    }

    #[tokio::test]
    async fn list_mods_returns_setting_first() {
        let tmp = TempDir::new().unwrap();
        let mods = tmp.path().join("mods");
        write_minimal_manifest(&mods.join("rundale"), "rundale", Some("setting"));
        write_minimal_manifest(&mods.join("solarized"), "solarized", Some("asset"));

        let src = LocalDiskModSource::with_root(mods);
        let summaries = src.list_mods().await.expect("list_mods should succeed");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, "rundale");
        assert_eq!(summaries[0].kind, ModKind::Setting);
        assert_eq!(summaries[1].id, "solarized");
        assert_eq!(summaries[1].kind, ModKind::Asset);
    }

    #[tokio::test]
    async fn load_mod_returns_error_for_unknown_id() {
        let tmp = TempDir::new().unwrap();
        let mods = tmp.path().join("mods");
        write_minimal_manifest(&mods.join("rundale"), "rundale", Some("setting"));

        let src = LocalDiskModSource::with_root(mods);
        let result = src.load_mod("no-such-mod").await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no-such-mod") || msg.contains("No mod"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn list_mods_no_mods_root_returns_error() {
        let result = LocalDiskModSource::with_root(PathBuf::from("/nonexistent_mods_path_abc123"))
            .list_mods()
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mod_source_is_dyn_compatible() {
        // Ensure the trait can be used as a trait object (Arc<dyn ModSource>).
        let tmp = TempDir::new().unwrap();
        let mods = tmp.path().join("mods");
        write_minimal_manifest(&mods.join("rundale"), "rundale", Some("setting"));

        let src: Arc<dyn ModSource> = Arc::new(LocalDiskModSource::with_root(mods));
        let summaries = src
            .list_mods()
            .await
            .expect("list_mods via dyn should succeed");
        assert!(!summaries.is_empty());
    }
}
