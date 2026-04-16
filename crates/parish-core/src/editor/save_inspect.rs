//! Read-only save file inspector for the Parish Designer.
//!
//! Lists save `.db` files and inspects their contents (branches, snapshots)
//! without the game loader's location-name resolution. Returns raw serde
//! JSON for snapshots so the frontend can render any section of the
//! GameSnapshot, even if the schema has evolved since the save was written.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use parish_persistence::database::Database;
use parish_types::ParishError;

/// Summary of a save file for the editor's save browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveFileSummary {
    /// Absolute path to the `.db` file.
    pub path: PathBuf,
    /// Just the filename (e.g. "parish_001.db").
    pub filename: String,
    /// Human-readable file size.
    pub file_size: String,
    /// Branch count in this file.
    pub branch_count: usize,
}

/// Summary of a branch within a save file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSummary {
    /// Database id for this branch.
    pub id: i64,
    /// Branch name (e.g. "main", "playthrough-1").
    pub name: String,
    /// Parent branch id, if forked.
    pub parent_branch_id: Option<i64>,
    /// Parent branch name, if forked.
    pub parent_branch_name: Option<String>,
    /// ISO 8601 creation timestamp (wall clock).
    pub created_at: String,
    /// Number of snapshots on this branch.
    pub snapshot_count: usize,
}

/// Summary of one snapshot on a branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    /// Database id for this snapshot.
    pub id: i64,
    /// ISO 8601 in-game clock time.
    pub game_time: String,
    /// ISO 8601 wall-clock time when this snapshot was written.
    pub real_time: String,
}

/// Full snapshot body returned by `read_snapshot`.
///
/// The snapshot body is kept as a `serde_json::Value` so the editor can
/// render older snapshot schemas without hard-failing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDetail {
    pub id: i64,
    pub branch_id: i64,
    pub game_time: String,
    pub real_time: String,
    /// Parsed `GameSnapshot` JSON body.
    pub world_state: serde_json::Value,
}

/// Scans a directory for save `.db` files.
pub fn list_saves(saves_dir: &Path) -> Result<Vec<SaveFileSummary>, ParishError> {
    if !saves_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut summaries = Vec::new();
    let entries = fs::read_dir(saves_dir).map_err(|e| {
        ParishError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to read saves dir: {e}"),
        ))
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("db") {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let file_size = fs::metadata(&path)
            .map(|m| format_file_size(m.len()))
            .unwrap_or_else(|_| "?".to_string());
        let branch_count = Database::open(&path)
            .and_then(|db| db.list_branches())
            .map(|b| b.len())
            .unwrap_or(0);
        summaries.push(SaveFileSummary {
            path,
            filename,
            file_size,
            branch_count,
        });
    }
    summaries.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(summaries)
}

/// Lists every branch in a save file.
pub fn list_branches(save_path: &Path) -> Result<Vec<BranchSummary>, ParishError> {
    let db = Database::open(save_path)?;
    let branches = db.list_branches()?;

    let mut out = Vec::new();
    for b in &branches {
        let parent_name = match b.parent_branch_id {
            Some(pid) => branches
                .iter()
                .find(|p| p.id == pid)
                .map(|p| p.name.clone()),
            None => None,
        };
        let snapshots = db.branch_log(b.id).map(|s| s.len()).unwrap_or(0);
        out.push(BranchSummary {
            id: b.id,
            name: b.name.clone(),
            parent_branch_id: b.parent_branch_id,
            parent_branch_name: parent_name,
            created_at: b.created_at.clone(),
            snapshot_count: snapshots,
        });
    }
    Ok(out)
}

/// Lists every snapshot on the given branch, oldest-first.
pub fn list_snapshots(
    save_path: &Path,
    branch_id: i64,
) -> Result<Vec<SnapshotSummary>, ParishError> {
    let db = Database::open(save_path)?;
    let snapshots = db.branch_log(branch_id)?;
    Ok(snapshots
        .into_iter()
        .map(|s| SnapshotSummary {
            id: s.id,
            game_time: s.game_time,
            real_time: s.real_time,
        })
        .collect())
}

/// Reads the latest snapshot on the branch as a raw JSON Value.
pub fn read_latest_snapshot(
    save_path: &Path,
    branch_id: i64,
) -> Result<Option<SnapshotDetail>, ParishError> {
    let db = Database::open(save_path)?;
    let snapshots = db.branch_log(branch_id)?;
    let Some(latest) = snapshots.last() else {
        return Ok(None);
    };
    // The Database API returns the full GameSnapshot (typed) via
    // load_latest_snapshot, but the editor wants the raw JSON so it can
    // display any schema version.
    let Some((id, parsed)) = db.load_latest_snapshot(branch_id)? else {
        return Ok(None);
    };
    let world_state = serde_json::to_value(&parsed).map_err(ParishError::Serialization)?;
    Ok(Some(SnapshotDetail {
        id,
        branch_id,
        game_time: latest.game_time.clone(),
        real_time: latest.real_time.clone(),
        world_state,
    }))
}

fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn list_saves_empty_dir() {
        let dir = TempDir::new().unwrap();
        let saves = list_saves(dir.path()).unwrap();
        assert!(saves.is_empty());
    }

    #[test]
    fn list_saves_missing_dir_returns_empty() {
        let saves = list_saves(Path::new("/nonexistent-path-xyz")).unwrap();
        assert!(saves.is_empty());
    }

    #[test]
    fn list_saves_finds_db_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("parish_001.db");
        Database::open(&path).unwrap();
        let saves = list_saves(dir.path()).unwrap();
        assert_eq!(saves.len(), 1);
        assert_eq!(saves[0].filename, "parish_001.db");
    }

    #[test]
    fn list_branches_returns_created_branches() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("save.db");
        let db = Database::open(&path).unwrap();
        // Database::open auto-creates a "main" branch; fork a named branch.
        let main_id = db.find_branch("main").unwrap().unwrap().id;
        db.create_branch("playthrough-1", Some(main_id)).unwrap();
        drop(db);
        let branches = list_branches(&path).unwrap();
        assert_eq!(branches.len(), 2);
        assert!(branches.iter().any(|b| b.name == "main"));
        assert!(branches.iter().any(|b| b.name == "playthrough-1"));
    }
}
