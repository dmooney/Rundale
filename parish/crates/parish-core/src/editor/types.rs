//! Serializable DTOs exchanged between the editor backend and the frontend.
//!
//! These types form the wire format for the `/editor` IPC commands. They
//! must remain stable and `#[derive(Serialize, Deserialize)]` so both Tauri
//! and the Axum web server can return them as JSON.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use parish_npc::NpcFile;
use parish_world::graph::LocationData;

use crate::game_mod::{AnachronismData, EncounterTable, FestivalDef, ModManifest};

/// Lightweight summary of a mod found on disk.
///
/// Returned by [`crate::editor::list_mods`] for the mod browser UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSummary {
    /// Machine-friendly identifier from `mod.toml`.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Display title shown on the splash screen.
    pub title: Option<String>,
    /// Semantic version string.
    pub version: String,
    /// Short description of the mod.
    pub description: String,
    /// Absolute path to the mod directory.
    pub path: PathBuf,
}

/// Serializable manifest mirror used by the editor.
///
/// A stripped-down copy of [`ModManifest`] that only includes the fields the
/// editor needs to display or round-trip. The full manifest is preserved
/// when saving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorManifest {
    /// Machine-friendly identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Display title.
    pub title: Option<String>,
    /// Semantic version string.
    pub version: String,
    /// Short description.
    pub description: String,
    /// ISO 8601 start date.
    pub start_date: String,
    /// Starting location id.
    pub start_location: u32,
    /// Anachronism cutoff year.
    pub period_year: u16,
}

impl From<&ModManifest> for EditorManifest {
    fn from(m: &ModManifest) -> Self {
        Self {
            id: m.meta.id.clone(),
            name: m.meta.name.clone(),
            title: m.meta.title.clone(),
            version: m.meta.version.clone(),
            description: m.meta.description.clone(),
            start_date: m.setting.start_date.clone(),
            start_location: m.setting.start_location,
            period_year: m.setting.period_year,
        }
    }
}

/// Everything the editor loads for a single mod.
///
/// Each field is loaded independently, so one malformed file (reported via
/// [`ValidationReport::errors`]) does not hide the rest of the mod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorModSnapshot {
    /// Absolute path to the mod directory.
    pub mod_path: PathBuf,
    /// Manifest metadata.
    pub manifest: EditorManifest,
    /// Raw NPC file — round-trip safe.
    pub npcs: NpcFile,
    /// World locations with connections.
    pub locations: Vec<LocationData>,
    /// Festival definitions.
    pub festivals: Vec<FestivalDef>,
    /// Encounter flavour text keyed by time-of-day.
    pub encounters: EncounterTable,
    /// Anachronism detection data.
    pub anachronisms: AnachronismData,
    /// Validation report computed at load time.
    pub validation: ValidationReport,
}

/// Validation report for a mod snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Hard errors that block saving.
    pub errors: Vec<ValidationIssue>,
    /// Warnings that inform but do not block saving.
    pub warnings: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Returns `true` if the report contains no errors or warnings.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }

    /// Returns `true` if the report has any error-level issues.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Pushes a new issue into the appropriate bucket based on severity.
    pub fn push(&mut self, issue: ValidationIssue) {
        match issue.severity {
            ValidationSeverity::Error => self.errors.push(issue),
            ValidationSeverity::Warning => self.warnings.push(issue),
        }
    }
}

/// A single validation issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// High-level category for grouping in the UI.
    pub category: ValidationCategory,
    /// Severity level.
    pub severity: ValidationSeverity,
    /// Which editable document the issue applies to.
    pub doc: EditorDoc,
    /// Dotted field path so the UI can jump to the offending entry,
    /// e.g. `"npcs[3].relationships[1].target_id"`.
    pub field_path: String,
    /// Human-readable message.
    pub message: String,
    /// Optional extra context (e.g., the offending id or value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Severity of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    /// Blocks saving.
    Error,
    /// Informational; does not block.
    Warning,
}

/// High-level category for grouping validation issues in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCategory {
    /// World graph structural problems.
    World,
    /// NPC field errors.
    Npc,
    /// NPC schedule errors.
    Schedule,
    /// Relationship errors.
    Relationship,
    /// Festival errors.
    Festival,
    /// Manifest errors.
    Manifest,
    /// File-level parse errors.
    Parse,
}

/// Which editable document an issue applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorDoc {
    /// `mod.toml` manifest.
    Manifest,
    /// `npcs.json`.
    Npcs,
    /// `world.json`.
    World,
    /// `festivals.json`.
    Festivals,
    /// `encounters.json`.
    Encounters,
    /// `anachronisms.json`.
    Anachronisms,
}
