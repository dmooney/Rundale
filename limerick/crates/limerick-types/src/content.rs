//! Stable identity of the game content a save was played against.

use serde::{Deserialize, Serialize};

/// The content (mod) a save was played against: its manifest id and version.
///
/// Saves record this instead of a hash of the content files, so editing
/// content never makes a save unreadable, and a save cannot silently open
/// against a different world whose places and people reuse the same numeric
/// ids (ADR-025 §4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentIdentity {
    /// The mod's stable id (`[mod] id` in `mod.toml`, for example `rundale`).
    pub id: String,
    /// The mod's version (`[mod] version` in `mod.toml`).
    pub version: String,
}

impl ContentIdentity {
    /// Whether a save played against `self` can be opened with `current`.
    ///
    /// The id must match; any version of the same content is compatible,
    /// because content versions change world data, not the ids the save
    /// refers to.
    pub fn is_compatible_with(&self, current: &ContentIdentity) -> bool {
        self.id == current.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(id: &str, version: &str) -> ContentIdentity {
        ContentIdentity {
            id: id.to_string(),
            version: version.to_string(),
        }
    }

    #[test]
    fn same_content_at_any_version_is_compatible() {
        assert!(identity("rundale", "1.0.0").is_compatible_with(&identity("rundale", "1.2.0")));
        assert!(!identity("rundale", "1.0.0").is_compatible_with(&identity("testbed", "1.0.0")));
    }
}
