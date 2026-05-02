//! Runtime feature flags for safe deployment of in-progress features.
//!
//! Feature flags let you ship code to `main` with new functionality disabled
//! by default. Use `/flag enable <name>` in-game to turn a feature on, and
//! `/flag disable <name>` to turn it off. Changes are persisted to
//! `parish-flags.json` in the data directory so they survive restarts.
//!
//! All unknown flags are treated as disabled (`false`), so checking a flag
//! that has never been set is always safe.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Runtime feature flags — toggle code paths without redeploying.
///
/// Backed by a [`BTreeMap`] for deterministic, alphabetically-sorted output
/// in both listings and JSON serialisation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeatureFlags {
    #[serde(default)]
    flags: BTreeMap<String, bool>,
}

impl FeatureFlags {
    /// Returns `true` if the named flag is enabled, `false` otherwise
    /// (including when the flag has never been set).
    pub fn is_enabled(&self, name: &str) -> bool {
        *self.flags.get(name).unwrap_or(&false)
    }

    /// Enables the named flag.
    pub fn enable(&mut self, name: &str) {
        self.flags.insert(name.to_string(), true);
    }

    /// Disables the named flag.
    pub fn disable(&mut self, name: &str) {
        self.flags.insert(name.to_string(), false);
    }

    /// Returns `true` only when the flag has been **explicitly** disabled.
    ///
    /// Unknown flags return `false` — features are considered on by default.
    /// Use this (instead of `is_enabled`) for features that should ship
    /// enabled and be kill-switched off in production:
    ///
    /// ```ignore
    /// if !config.flags.is_disabled("new-npc-schedules") {
    ///     // feature code
    /// }
    /// ```
    pub fn is_disabled(&self, name: &str) -> bool {
        self.flags.get(name).copied() == Some(false)
    }

    /// Returns all flags in alphabetical order as `(name, enabled)` pairs.
    pub fn list(&self) -> Vec<(&str, bool)> {
        self.flags.iter().map(|(k, v)| (k.as_str(), *v)).collect()
    }

    /// Returns `true` if no flags have been set yet.
    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
    }

    /// Loads flags from a JSON file.
    ///
    /// Returns `Default::default()` if the file does not exist or cannot be
    /// parsed — this is not an error, since a missing file simply means no
    /// flags have been persisted yet. A parse failure (corrupt/stale JSON)
    /// is unexpected, so it is logged as a warning.
    pub fn load_from_file(path: &Path) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "could not read feature-flags file");
                return Self::default();
            }
        };
        match serde_json::from_str(&content) {
            Ok(flags) => flags,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "feature-flags file contains invalid JSON; using defaults");
                Self::default()
            }
        }
    }

    /// Saves the current flag state to a JSON file, creating parent
    /// directories as needed.
    pub fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_default_disabled() {
        let flags = FeatureFlags::default();
        assert!(!flags.is_enabled("any-flag"));
        assert!(!flags.is_enabled(""));
    }

    #[test]
    fn test_flag_enable() {
        let mut flags = FeatureFlags::default();
        flags.enable("new-weather");
        assert!(flags.is_enabled("new-weather"));
    }

    #[test]
    fn test_flag_disable_after_enable() {
        let mut flags = FeatureFlags::default();
        flags.enable("feature-x");
        flags.disable("feature-x");
        assert!(!flags.is_enabled("feature-x"));
    }

    #[test]
    fn test_flag_disable_nonexistent_is_noop() {
        let mut flags = FeatureFlags::default();
        flags.disable("never-existed");
        // disable on an unknown flag inserts it as false
        assert!(!flags.is_enabled("never-existed"));
    }

    #[test]
    fn test_flag_list_sorted() {
        let mut flags = FeatureFlags::default();
        flags.enable("zebra");
        flags.enable("apple");
        flags.disable("mango");
        let list = flags.list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].0, "apple");
        assert_eq!(list[1].0, "mango");
        assert_eq!(list[2].0, "zebra");
        assert!(list[0].1);
        assert!(!list[1].1);
        assert!(list[2].1);
    }

    #[test]
    fn test_flag_roundtrip_json() {
        let mut flags = FeatureFlags::default();
        flags.enable("alpha");
        flags.disable("beta");

        let json = serde_json::to_string(&flags).unwrap();
        let restored: FeatureFlags = serde_json::from_str(&json).unwrap();
        assert_eq!(flags, restored);
        assert!(restored.is_enabled("alpha"));
        assert!(!restored.is_enabled("beta"));
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let path = std::path::Path::new("/nonexistent/path/parish-flags.json");
        let flags = FeatureFlags::load_from_file(path);
        assert!(flags.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish-flags.json");

        let mut flags = FeatureFlags::default();
        flags.enable("persist-test");
        flags.disable("off-by-default");
        flags.save_to_file(&path).unwrap();

        let loaded = FeatureFlags::load_from_file(&path);
        assert!(loaded.is_enabled("persist-test"));
        assert!(!loaded.is_enabled("off-by-default"));
        assert_eq!(loaded.list().len(), 2);
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/subdir/parish-flags.json");
        let flags = FeatureFlags::default();
        flags.save_to_file(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_load_corrupt_json_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish-flags.json");
        std::fs::write(&path, b"not valid json {{{{").unwrap();
        // Should not panic; should return a default (empty) FeatureFlags.
        let flags = FeatureFlags::load_from_file(&path);
        assert!(flags.is_empty(), "corrupt JSON must yield default flags");
    }

    #[test]
    fn test_load_wrong_schema_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish-flags.json");
        // Valid JSON but wrong shape (array instead of object).
        std::fs::write(&path, b"[1, 2, 3]").unwrap();
        let flags = FeatureFlags::load_from_file(&path);
        assert!(
            flags.is_empty(),
            "wrong-schema JSON must yield default flags"
        );
    }

    #[test]
    fn test_is_disabled_unknown_flag_returns_false() {
        // Unknown flags are NOT disabled — features ship enabled by default.
        let flags = FeatureFlags::default();
        assert!(!flags.is_disabled("never-set"));
    }

    #[test]
    fn test_is_disabled_after_disable() {
        let mut flags = FeatureFlags::default();
        flags.disable("kill-switch");
        assert!(flags.is_disabled("kill-switch"));
    }

    #[test]
    fn test_is_disabled_after_enable() {
        let mut flags = FeatureFlags::default();
        flags.enable("re-enabled");
        assert!(!flags.is_disabled("re-enabled"));
    }

    #[test]
    fn test_is_empty_on_default() {
        let flags = FeatureFlags::default();
        assert!(flags.is_empty());
    }

    #[test]
    fn test_is_not_empty_after_set() {
        let mut flags = FeatureFlags::default();
        flags.enable("x");
        assert!(!flags.is_empty());
    }
}
