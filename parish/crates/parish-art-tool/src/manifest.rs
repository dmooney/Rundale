//! Art asset manifest — tracks per-asset metadata, status transitions, and
//! atomic persistence.
//!
//! `art/manifest.json` is committed to the repository so curation history is
//! preserved. Candidate PNG files under `art/` are gitignored; only accepted,
//! post-processed assets land in `mods/rundale/assets/scenes/`.

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The art asset manifest — the top-level object in `art/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArtManifest {
    pub assets: Vec<AssetRecord>,
}

/// A single generated asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRecord {
    /// Stable unique id (e.g. "plate-2-001").
    pub id: String,
    /// Asset kind.
    pub kind: AssetKind,
    /// Location id (for plates/variants) or NPC id (for sprites) this asset
    /// depicts.
    pub target_id: u32,
    /// Optional variant name (e.g. "night").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Provider used for generation (e.g. "openai", "google").
    pub provider: String,
    /// Model name (e.g. "gpt-image-1", "imagen-3.0-generate-002").
    pub model: String,
    /// The full prompt that was sent to the provider.
    pub prompt: String,
    /// Paths to reference images passed to the provider's edit/img2img
    /// endpoint, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_images: Vec<String>,
    /// ISO-8601 creation timestamp (passed in; never computed from wall clock
    /// in production code so tests remain deterministic).
    pub created: String,
    /// Curation status.
    pub status: AssetStatus,
    /// Whether this asset is the style anchor for its kind (the first accepted
    /// plate / sprite used as reference for all subsequent generations).
    #[serde(default)]
    pub anchor: bool,
    /// Human-written cleanup or curation notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_notes: Option<String>,
    /// Rejection reason (set on `reject`; preserved for audit history).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    /// Relative path of the generated candidate PNG under `art/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    /// Relative path of the exported, post-processed asset under
    /// `mods/rundale/assets/scenes/`.  Set by `export`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_path: Option<String>,
}

/// Asset kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetKind {
    /// Background plate for a scene (one per location).
    Plate,
    /// NPC character sprite (transparent background).
    Sprite,
    /// Variant of an existing plate (e.g. night-time version).
    Variant,
}

/// Curation status state machine.
///
/// Permitted transitions:
/// - `Pending` → `Accepted`
/// - `Pending` → `Rejected`
///
/// Illegal transitions (rejected at runtime):
/// - `Accepted` → `Pending`
/// - `Accepted` → `Rejected`
/// - `Rejected` → `Accepted`
/// - `Rejected` → `Pending`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetStatus {
    Pending,
    Accepted,
    Rejected,
}

impl AssetStatus {
    /// Whether transitioning to `next` is a legal status transition.
    ///
    /// Only `Pending → Accepted` and `Pending → Rejected` are allowed.
    pub fn can_transition_to(&self, next: &AssetStatus) -> bool {
        matches!(
            (self, next),
            (AssetStatus::Pending, AssetStatus::Accepted)
                | (AssetStatus::Pending, AssetStatus::Rejected)
        )
    }
}

impl std::fmt::Display for AssetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetKind::Plate => write!(f, "plate"),
            AssetKind::Sprite => write!(f, "sprite"),
            AssetKind::Variant => write!(f, "variant"),
        }
    }
}

impl std::fmt::Display for AssetStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetStatus::Pending => write!(f, "pending"),
            AssetStatus::Accepted => write!(f, "accepted"),
            AssetStatus::Rejected => write!(f, "rejected"),
        }
    }
}

// ---------------------------------------------------------------------------
// Manifest I/O
// ---------------------------------------------------------------------------

impl ArtManifest {
    /// Load the manifest from `manifest_path`, or return an empty manifest if
    /// the file does not yet exist.
    pub fn load(manifest_path: &Path) -> Result<Self> {
        if !manifest_path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(manifest_path)?;
        let manifest = serde_json::from_str(&json)?;
        Ok(manifest)
    }

    /// Atomically save the manifest: write to a `.tmp` sibling then rename.
    ///
    /// A partial write never corrupts the existing `manifest.json`.
    pub fn save(&self, manifest_path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let tmp = manifest_path.with_extension("json.tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, manifest_path)?;
        Ok(())
    }

    /// Append a new asset record and save atomically.
    pub fn add_and_save(&mut self, record: AssetRecord, manifest_path: &Path) -> Result<()> {
        self.assets.push(record);
        self.save(manifest_path)
    }

    /// Find a record by id (mutable).
    pub fn get_mut(&mut self, id: &str) -> Option<&mut AssetRecord> {
        self.assets.iter_mut().find(|r| r.id == id)
    }

    /// Find a record by id (immutable).
    pub fn get(&self, id: &str) -> Option<&AssetRecord> {
        self.assets.iter().find(|r| r.id == id)
    }

    /// Accept an asset: transition `pending` → `accepted`, store optional note,
    /// and save the manifest.
    ///
    /// Returns an error if the asset does not exist or the transition is
    /// illegal (e.g. the asset is already accepted or rejected).
    pub fn accept(&mut self, id: &str, note: Option<String>, path: &Path) -> Result<()> {
        let record = self
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("asset '{}' not found in manifest", id))?;

        if !record.status.can_transition_to(&AssetStatus::Accepted) {
            bail!(
                "cannot accept asset '{}': illegal status transition {} → accepted",
                id,
                record.status
            );
        }
        record.status = AssetStatus::Accepted;
        if note.is_some() {
            record.cleanup_notes = note;
        }
        self.save(path)
    }

    /// Reject an asset: transition `pending` → `rejected`, store reason,
    /// and save the manifest.
    pub fn reject(&mut self, id: &str, reason: String, path: &Path) -> Result<()> {
        let record = self
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("asset '{}' not found in manifest", id))?;

        if !record.status.can_transition_to(&AssetStatus::Rejected) {
            bail!(
                "cannot reject asset '{}': illegal status transition {} → rejected",
                id,
                record.status
            );
        }
        record.status = AssetStatus::Rejected;
        record.reject_reason = Some(reason);
        self.save(path)
    }

    /// Return assets filtered by status.
    pub fn by_status(&self, status: &AssetStatus) -> Vec<&AssetRecord> {
        self.assets.iter().filter(|r| &r.status == status).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_record(id: &str) -> AssetRecord {
        AssetRecord {
            id: id.to_string(),
            kind: AssetKind::Plate,
            target_id: 2,
            variant: None,
            provider: "openai".to_string(),
            model: "gpt-image-1".to_string(),
            prompt: "A pixel art plate of Darcy's Pub.".to_string(),
            reference_images: vec![],
            created: "2026-06-11T00:00:00Z".to_string(),
            status: AssetStatus::Pending,
            anchor: false,
            cleanup_notes: None,
            reject_reason: None,
            output_path: Some("art/plate-2-001.png".to_string()),
            exported_path: None,
        }
    }

    #[test]
    fn manifest_serde_roundtrip() {
        let mut manifest = ArtManifest::default();
        manifest.assets.push(sample_record("plate-2-001"));

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let roundtripped: ArtManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(roundtripped.assets.len(), 1);
        let r = &roundtripped.assets[0];
        assert_eq!(r.id, "plate-2-001");
        assert_eq!(r.kind, AssetKind::Plate);
        assert_eq!(r.target_id, 2);
        assert_eq!(r.status, AssetStatus::Pending);
        assert!(!r.anchor);
        assert!(r.cleanup_notes.is_none());
        assert!(r.reject_reason.is_none());
        assert_eq!(r.output_path.as_deref(), Some("art/plate-2-001.png"));
    }

    #[test]
    fn asset_kind_serde_kebab_case() {
        let json = serde_json::to_string(&AssetKind::Variant).unwrap();
        assert_eq!(json, "\"variant\"");
        let back: AssetKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AssetKind::Variant);
    }

    #[test]
    fn status_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&AssetStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&AssetStatus::Accepted).unwrap(),
            "\"accepted\""
        );
        assert_eq!(
            serde_json::to_string(&AssetStatus::Rejected).unwrap(),
            "\"rejected\""
        );
    }

    #[test]
    fn accept_transitions_from_pending() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let mut manifest = ArtManifest::default();
        manifest.assets.push(sample_record("plate-2-001"));
        manifest.save(&path).unwrap();

        manifest
            .accept("plate-2-001", Some("cleaned a pixel".to_string()), &path)
            .unwrap();
        let r = manifest.get("plate-2-001").unwrap();
        assert_eq!(r.status, AssetStatus::Accepted);
        assert_eq!(r.cleanup_notes.as_deref(), Some("cleaned a pixel"));
    }

    #[test]
    fn reject_transitions_from_pending() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let mut manifest = ArtManifest::default();
        manifest.assets.push(sample_record("plate-2-001"));
        manifest.save(&path).unwrap();

        manifest
            .reject("plate-2-001", "wrong palette".to_string(), &path)
            .unwrap();
        let r = manifest.get("plate-2-001").unwrap();
        assert_eq!(r.status, AssetStatus::Rejected);
        assert_eq!(r.reject_reason.as_deref(), Some("wrong palette"));
    }

    #[test]
    fn illegal_accepted_to_pending_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let mut manifest = ArtManifest::default();
        let mut record = sample_record("plate-2-001");
        record.status = AssetStatus::Accepted;
        manifest.assets.push(record);
        manifest.save(&path).unwrap();

        // accepted → rejected is illegal
        let err = manifest
            .reject("plate-2-001", "nope".to_string(), &path)
            .unwrap_err();
        assert!(
            err.to_string().contains("illegal status transition"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn illegal_rejected_to_accepted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let mut manifest = ArtManifest::default();
        let mut record = sample_record("plate-2-001");
        record.status = AssetStatus::Rejected;
        manifest.assets.push(record);
        manifest.save(&path).unwrap();

        let err = manifest.accept("plate-2-001", None, &path).unwrap_err();
        assert!(
            err.to_string().contains("illegal status transition"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn atomic_save_and_reload() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let mut manifest = ArtManifest::default();
        manifest.assets.push(sample_record("plate-2-001"));
        manifest.save(&path).unwrap();

        let loaded = ArtManifest::load(&path).unwrap();
        assert_eq!(loaded.assets.len(), 1);
        assert_eq!(loaded.assets[0].id, "plate-2-001");
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let manifest = ArtManifest::load(&path).unwrap();
        assert!(manifest.assets.is_empty());
    }

    #[test]
    fn by_status_filter() {
        let mut manifest = ArtManifest::default();
        let mut accepted = sample_record("plate-2-001");
        accepted.status = AssetStatus::Accepted;
        manifest.assets.push(accepted);
        manifest.assets.push(sample_record("plate-2-002"));

        assert_eq!(manifest.by_status(&AssetStatus::Accepted).len(), 1);
        assert_eq!(manifest.by_status(&AssetStatus::Pending).len(), 1);
        assert_eq!(manifest.by_status(&AssetStatus::Rejected).len(), 0);
    }
}
