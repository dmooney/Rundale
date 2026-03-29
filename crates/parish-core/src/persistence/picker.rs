//! Save file picker — Papers Please-style save slot selection.
//!
//! Scans a `saves/` directory for `.db` files, opens each briefly to read
//! branch and snapshot metadata, then provides data for a numbered picker.
//! Each save file shows its branches with nesting and latest
//! snapshot info (location, game date, save count).

use std::path::{Path, PathBuf};

use crate::error::ParishError;
use crate::persistence::database::Database;
use crate::world::graph::WorldGraph;

/// Default directory for save files.
pub const SAVES_DIR: &str = "saves";

/// Prefix for auto-numbered save files.
const SAVE_PREFIX: &str = "parish_";

/// Extension for save files.
const SAVE_EXT: &str = "db";

/// A single snapshot cell for the grid display.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotCell {
    /// Snapshot database id (used when loading).
    pub id: i64,
    /// Formatted game date with time of day (e.g. "20 Mar 1820, Morning").
    pub game_date: String,
    /// Resolved location name (if available).
    pub location: Option<String>,
}

/// Information about a branch within a save file for display.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SaveBranchDisplay {
    /// Branch name.
    pub name: String,
    /// Branch database id.
    pub id: i64,
    /// Parent branch name (None for root branches).
    pub parent_name: Option<String>,
    /// Number of snapshots on this branch.
    pub snapshot_count: usize,
    /// Resolved location name from the latest snapshot (if available).
    pub latest_location: Option<String>,
    /// Formatted game date from the latest snapshot (if available).
    pub latest_game_date: Option<String>,
    /// All snapshots on this branch, oldest first (for grid display).
    pub snapshots: Vec<SnapshotCell>,
}

/// Information about a save file for display in the picker.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SaveFileInfo {
    /// Full path to the .db file.
    pub path: PathBuf,
    /// Just the filename (e.g. "parish_001.db").
    pub filename: String,
    /// Human-readable file size (e.g. "12 KB").
    pub file_size: String,
    /// Branches within the save file.
    pub branches: Vec<SaveBranchDisplay>,
}

/// Ensures the saves directory exists and returns its path.
///
/// Creates the directory if it doesn't exist. Also performs a one-time
/// migration of the legacy `parish_saves.db` file from the project root.
pub fn ensure_saves_dir() -> PathBuf {
    let saves_dir = PathBuf::from(SAVES_DIR);
    std::fs::create_dir_all(&saves_dir).ok();

    // One-time migration from legacy location
    let legacy = Path::new("parish_saves.db");
    if legacy.exists() {
        let target = saves_dir.join(format!("{}{:03}.{}", SAVE_PREFIX, 1, SAVE_EXT));
        if !target.exists() {
            if let Err(e) = std::fs::rename(legacy, &target) {
                eprintln!("Warning: Could not migrate {}: {}", legacy.display(), e);
            } else {
                println!("Migrated save file to {}", target.display());
            }
        }
    }

    saves_dir
}

/// Discovers all save files in the given directory and reads their metadata.
///
/// Opens each `.db` file briefly to list branches and their latest snapshots.
/// Location names are resolved using the provided world graph.
pub fn discover_saves(saves_dir: &Path, graph: &WorldGraph) -> Vec<SaveFileInfo> {
    let entries = match std::fs::read_dir(saves_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == SAVE_EXT))
        .collect();
    files.sort();

    let mut saves = Vec::new();
    for path in files {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let branches = match read_save_branches(&path, graph) {
            Ok(b) => b,
            Err(_) => continue, // Skip corrupt files
        };

        let file_size = std::fs::metadata(&path)
            .map(|m| format_file_size(m.len()))
            .unwrap_or_default();

        saves.push(SaveFileInfo {
            path,
            filename,
            file_size,
            branches,
        });
    }

    saves
}

/// Reads branch metadata from a save database file.
fn read_save_branches(
    db_path: &Path,
    graph: &WorldGraph,
) -> Result<Vec<SaveBranchDisplay>, ParishError> {
    let db = Database::open(db_path)?;
    let branches = db.list_branches()?;

    let mut displays = Vec::new();
    for branch in &branches {
        let log = db.branch_log(branch.id)?;
        let snapshot_count = log.len();

        // Build snapshot cells from the log (oldest first for grid display)
        let mut snapshots: Vec<SnapshotCell> = log
            .iter()
            .rev() // branch_log returns newest first, we want oldest first
            .map(|info| {
                let game_date = chrono::DateTime::parse_from_rfc3339(&info.game_time)
                    .map(|dt| format_game_date(&dt.with_timezone(&chrono::Utc)))
                    .unwrap_or_else(|_| info.game_time.clone());
                SnapshotCell {
                    id: info.id,
                    game_date,
                    location: None, // filled in for latest below
                }
            })
            .collect();

        // Read latest snapshot to get location
        let (latest_location, latest_game_date) =
            if let Ok(Some((_id, snapshot))) = db.load_latest_snapshot(branch.id) {
                let loc_name = graph
                    .get(snapshot.player_location)
                    .map(|ld| ld.name.clone());
                let game_date = format_game_date(&snapshot.clock.game_time);
                // Set location on the last cell
                if let Some(last) = snapshots.last_mut() {
                    last.location = loc_name.clone();
                }
                (loc_name, Some(game_date))
            } else {
                (None, None)
            };

        // Find parent name
        let parent_name = branch.parent_branch_id.and_then(|pid| {
            branches
                .iter()
                .find(|b| b.id == pid)
                .map(|b| b.name.clone())
        });

        displays.push(SaveBranchDisplay {
            name: branch.name.clone(),
            id: branch.id,
            parent_name,
            snapshot_count,
            latest_location,
            latest_game_date,
            snapshots,
        });
    }

    Ok(displays)
}

/// Formats a chrono DateTime into a short game-date string with time of day.
///
/// Example: "20 Mar 1820, Morning"
fn format_game_date(dt: &chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Timelike;
    let hour = dt.hour();
    let tod = match hour {
        5..=8 => "Morning",
        9..=11 => "Late Morning",
        12..=13 => "Midday",
        14..=16 => "Afternoon",
        17..=19 => "Dusk",
        20..=21 => "Evening",
        _ => "Night",
    };
    format!("{}, {}", dt.format("%-d %b %Y"), tod)
}

/// Formats a byte count into a human-readable file size.
///
/// Example: `12288` → `"12 KB"`
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}

/// Returns the next auto-numbered save filename.
///
/// Scans for `parish_NNN.db` files and returns one higher than the max.
pub fn next_save_number(saves_dir: &Path) -> u32 {
    let entries = match std::fs::read_dir(saves_dir) {
        Ok(entries) => entries,
        Err(_) => return 1,
    };

    let max = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            parse_save_number(&name)
        })
        .max()
        .unwrap_or(0);

    max + 1
}

/// Parses the number from a filename like "parish_003.db".
fn parse_save_number(filename: &str) -> Option<u32> {
    let stem = filename.strip_suffix(&format!(".{}", SAVE_EXT))?;
    let num_str = stem.strip_prefix(SAVE_PREFIX)?;
    num_str.parse().ok()
}

/// Creates a new save file path with the next auto-number.
pub fn new_save_path(saves_dir: &Path) -> PathBuf {
    let num = next_save_number(saves_dir);
    saves_dir.join(format!("{}{:03}.{}", SAVE_PREFIX, num, SAVE_EXT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_save_number() {
        assert_eq!(parse_save_number("parish_001.db"), Some(1));
        assert_eq!(parse_save_number("parish_042.db"), Some(42));
        assert_eq!(parse_save_number("parish_100.db"), Some(100));
        assert_eq!(parse_save_number("other.db"), None);
        assert_eq!(parse_save_number("parish_.db"), None);
        assert_eq!(parse_save_number("parish_abc.db"), None);
    }

    #[test]
    fn test_next_save_number_empty_dir() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(next_save_number(tmp.path()), 1);
    }

    #[test]
    fn test_next_save_number_sequential() {
        let tmp = TempDir::new().unwrap();
        // Create parish_001.db and parish_003.db
        std::fs::write(tmp.path().join("parish_001.db"), "").unwrap();
        std::fs::write(tmp.path().join("parish_003.db"), "").unwrap();
        assert_eq!(next_save_number(tmp.path()), 4);
    }

    #[test]
    fn test_next_save_number_ignores_non_matching() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("parish_002.db"), "").unwrap();
        std::fs::write(tmp.path().join("other.db"), "").unwrap();
        std::fs::write(tmp.path().join("notes.txt"), "").unwrap();
        assert_eq!(next_save_number(tmp.path()), 3);
    }

    #[test]
    fn test_new_save_path() {
        let tmp = TempDir::new().unwrap();
        let path = new_save_path(tmp.path());
        assert!(path.to_string_lossy().contains("parish_001.db"));

        std::fs::write(tmp.path().join("parish_001.db"), "").unwrap();
        let path2 = new_save_path(tmp.path());
        assert!(path2.to_string_lossy().contains("parish_002.db"));
    }

    #[test]
    fn test_discover_saves_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let graph = WorldGraph::new();
        let saves = discover_saves(tmp.path(), &graph);
        assert!(saves.is_empty());
    }

    #[test]
    fn test_discover_saves_finds_db_files() {
        let tmp = TempDir::new().unwrap();
        let graph = WorldGraph::new();

        // Create a real DB file
        let db_path = tmp.path().join("parish_001.db");
        let _db = Database::open(&db_path).unwrap();

        let saves = discover_saves(tmp.path(), &graph);
        assert_eq!(saves.len(), 1);
        assert_eq!(saves[0].filename, "parish_001.db");
        // Should have at least the "main" branch (auto-created by Database::open)
        assert!(!saves[0].branches.is_empty());
        assert_eq!(saves[0].branches[0].name, "main");
    }

    #[test]
    fn test_discover_saves_skips_non_db_files() {
        let tmp = TempDir::new().unwrap();
        let graph = WorldGraph::new();

        std::fs::write(tmp.path().join("notes.txt"), "hello").unwrap();
        std::fs::write(tmp.path().join("readme.md"), "readme").unwrap();
        let db_path = tmp.path().join("parish_001.db");
        let _db = Database::open(&db_path).unwrap();

        let saves = discover_saves(tmp.path(), &graph);
        assert_eq!(saves.len(), 1);
    }

    #[test]
    fn test_save_branch_display_from_real_db() {
        let tmp = TempDir::new().unwrap();
        let graph = WorldGraph::new();

        let db_path = tmp.path().join("test.db");
        let db = Database::open(&db_path).unwrap();

        // Create a second branch
        db.create_branch("alternate", Some(1)).unwrap();

        drop(db);

        let branches = read_save_branches(&db_path, &graph).unwrap();
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].parent_name.is_none());
        assert_eq!(branches[1].name, "alternate");
        assert_eq!(branches[1].parent_name, Some("main".to_string()));
    }

    #[test]
    fn test_format_game_date() {
        use chrono::{TimeZone, Utc};
        let dt = Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap();
        let formatted = format_game_date(&dt);
        assert_eq!(formatted, "20 Mar 1820, Morning");
    }

    #[test]
    fn test_ensure_saves_dir_creates_directory() {
        let original_dir = std::env::current_dir().unwrap();
        let tmp = TempDir::new().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let saves_dir = ensure_saves_dir();
        assert!(saves_dir.exists());

        std::env::set_current_dir(original_dir).unwrap();
    }
}
