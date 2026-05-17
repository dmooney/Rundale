//! Save file picker — Papers Please-style save slot selection.
//!
//! Scans a `saves/` directory for `.db` files, opens each briefly to read
//! branch and snapshot metadata, then provides data for a numbered picker.
//! Each save file shows its branches with nesting and latest
//! snapshot info (location, game date, save count).

use std::path::{Path, PathBuf};

use crate::database::Database;
use parish_types::ParishError;
use parish_world::graph::WorldGraph;

/// Default directory for save files (leaf folder name under the user-data root).
pub const SAVES_DIR: &str = "saves";

/// Environment variable that overrides saves-dir resolution explicitly.
const SAVES_DIR_ENV: &str = "PARISH_SAVES_DIR";

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

/// Result of the player's choice in the save picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PickerChoice {
    /// Player chose an existing save file (index into the save list).
    Existing(usize),
    /// Player chose to start a new game.
    NewGame,
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
    /// Whether this save file is currently locked by another running instance.
    pub locked: bool,
}

/// Ensures the saves directory exists at `saves_dir`.
///
/// Also performs an idempotent migration of a legacy `parish_saves.db`
/// located in the parent of `saves_dir` into `saves_dir` as the first
/// numbered save file. The migration only runs when the target path does
/// not already exist, so repeated calls are safe.
/// Returns the same path back for chaining.
pub fn ensure_saves_dir_at(saves_dir: PathBuf) -> PathBuf {
    if let Err(e) = std::fs::create_dir_all(&saves_dir) {
        tracing::warn!(path = %saves_dir.display(), error = %e, "failed to create saves directory");
    }

    // One-time migration from the legacy location next to the saves dir.
    let legacy = saves_dir
        .parent()
        .map(|p| p.join("parish_saves.db"))
        .unwrap_or_else(|| PathBuf::from("parish_saves.db"));
    if legacy.exists() {
        let target = saves_dir.join(format!("{}{:03}.{}", SAVE_PREFIX, 1, SAVE_EXT));
        if !target.exists() {
            if let Err(e) = std::fs::rename(&legacy, &target) {
                tracing::warn!(path = %legacy.display(), target = %target.display(), error = %e, "could not migrate legacy save file");
            } else {
                tracing::info!(path = %target.display(), "migrated legacy save file");
            }
        }
    }

    saves_dir
}

/// Resolves the project's saves directory once at startup.
///
/// Resolution order:
/// 1. `PARISH_SAVES_DIR` environment variable — explicit operator/dev override.
///    Used verbatim (no anchoring); relative paths resolve against the cwd at
///    the time this is called, so callers must invoke at startup.
/// 2. `<user_data_dir>/saves` where `user_data_dir` is the platform-native
///    per-user data folder for `app_name` — typically the active mod's
///    [`crate::game_mod::ModMeta::app_name`]. See [`crate::paths::resolve_user_data_dir`].
///
/// The returned directory is created on disk if missing and is stable for the
/// lifetime of the process. Callers MUST resolve once at startup and store
/// the result on shared state. Do not call from request handlers —
/// `current_dir()` may differ at handler invocation time (packaged builds,
/// daemonised servers, working-directory changes), which is the bug behind #771.
pub fn resolve_project_saves_dir(app_name: &str) -> PathBuf {
    if let Ok(s) = std::env::var(SAVES_DIR_ENV)
        && !s.is_empty()
    {
        return ensure_saves_dir_at(PathBuf::from(s));
    }
    ensure_saves_dir_at(crate::paths::resolve_user_data_dir(app_name).join(SAVES_DIR))
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
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping unreadable/corrupt save file");
                continue;
            }
        };

        let file_size = std::fs::metadata(&path)
            .map(|m| format_file_size(m.len()))
            .unwrap_or_default();

        let locked = crate::lock::is_locked(&path);

        saves.push(SaveFileInfo {
            path,
            filename,
            file_size,
            branches,
            locked,
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

/// Prints a single branch line in the picker tree.
fn print_branch_line(connector: &str, branch: &SaveBranchDisplay) {
    let loc = branch
        .latest_location
        .as_deref()
        .unwrap_or("Unknown location");
    let date = branch.latest_game_date.as_deref().unwrap_or("Unknown date");
    let saves_label = if branch.snapshot_count == 1 {
        "1 save".to_string()
    } else {
        format!("{} saves", branch.snapshot_count)
    };
    println!(
        "     {} {} — {}, {}  ({})",
        connector, branch.name, loc, date, saves_label
    );
}

/// Displays the save picker in the terminal.
pub fn display_picker(saves: &[SaveFileInfo]) {
    println!();
    for (i, save) in saves.iter().enumerate() {
        println!("  {}. {}", i + 1, save.filename);

        // Separate root branches from child branches
        let roots: Vec<&SaveBranchDisplay> = save
            .branches
            .iter()
            .filter(|b| b.parent_name.is_none())
            .collect();
        let children: Vec<&SaveBranchDisplay> = save
            .branches
            .iter()
            .filter(|b| b.parent_name.is_some())
            .collect();

        for (j, branch) in roots.iter().enumerate() {
            let is_last_root = j == roots.len() - 1
                && children
                    .iter()
                    .all(|c| c.parent_name.as_deref() != Some(&branch.name));
            let connector = if is_last_root && children.is_empty() {
                "└─"
            } else {
                "├─"
            };
            print_branch_line(connector, branch);

            // Print children of this root
            let my_children: Vec<&&SaveBranchDisplay> = children
                .iter()
                .filter(|c| c.parent_name.as_deref() == Some(&branch.name))
                .collect();
            for (k, child) in my_children.iter().enumerate() {
                let child_connector = if k == my_children.len() - 1 {
                    "└─"
                } else {
                    "├─"
                };
                let indent = if is_last_root { "  " } else { "│ " };
                print!("     {}", indent);
                print_branch_line(child_connector, child);
            }
        }
    }
    println!();
    println!("  N. New Game");
    println!();
}

/// Reads the player's choice from stdin.
///
/// Returns `Ok(PickerChoice)` on valid input, or an error message string
/// for invalid input.
pub(crate) fn read_picker_choice(saves: &[SaveFileInfo]) -> Result<PickerChoice, String> {
    use std::io::{BufRead, Write};
    print!("Choose [1-{}, N]: ", saves.len());
    std::io::stdout().flush().ok();

    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_err() {
        return Err("Failed to read input.".to_string());
    }

    let input = line.trim().to_lowercase();
    if input == "n" || input == "new" {
        return Ok(PickerChoice::NewGame);
    }

    match input.parse::<usize>() {
        Ok(n) if n >= 1 && n <= saves.len() => Ok(PickerChoice::Existing(n - 1)),
        Ok(_) => Err(format!(
            "Please enter a number between 1 and {}, or N for new game.",
            saves.len()
        )),
        Err(_) => Err("Please enter a number or N.".to_string()),
    }
}

/// Runs the interactive save picker loop until the player makes a valid choice.
///
/// Returns the path to the chosen (or newly created) save file.
pub fn run_picker(saves_dir: &Path, graph: &WorldGraph) -> PathBuf {
    let saves = discover_saves(saves_dir, graph);

    // If no saves exist, start a new game automatically
    if saves.is_empty() {
        let path = new_save_path(saves_dir);
        println!("Starting new game: {}", path.display());
        return path;
    }

    display_picker(&saves);

    loop {
        match read_picker_choice(&saves) {
            Ok(PickerChoice::Existing(idx)) => {
                return saves[idx].path.clone();
            }
            Ok(PickerChoice::NewGame) => {
                let path = new_save_path(saves_dir);
                println!("Starting new game: {}", path.display());
                return path;
            }
            Err(msg) => {
                println!("{}", msg);
            }
        }
    }
}

/// Runs the picker for the `/load` command (mid-game save switching).
///
/// Shows the picker and returns the chosen path, or `None` if the player
/// cancels (enters empty input or the current save).
pub fn run_load_picker(saves_dir: &Path, graph: &WorldGraph) -> Option<PathBuf> {
    use std::io::{BufRead, Write};

    let saves = discover_saves(saves_dir, graph);

    if saves.is_empty() {
        println!("No save files found in {}.", saves_dir.display());
        return None;
    }

    display_picker(&saves);
    print!("Choose [1-{}, or Enter to cancel]: ", saves.len());
    std::io::stdout().flush().ok();

    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_err() {
        return None;
    }

    let input = line.trim();
    if input.is_empty() {
        return None;
    }

    if let Some(choice) = input.to_lowercase().strip_prefix("n")
        && (choice.is_empty() || choice == "ew")
    {
        let path = new_save_path(saves_dir);
        println!("Starting new game: {}", path.display());
        return Some(path);
    }

    match input.parse::<usize>() {
        Ok(n) if n >= 1 && n <= saves.len() => Some(saves[n - 1].path.clone()),
        _ => {
            println!("Invalid choice.");
            None
        }
    }
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

    /// Process-wide gate that serialises tests which mutate path-resolution
    /// env vars (`PARISH_SAVES_DIR`, `PARISH_USER_DATA_DIR`). Cargo runs unit
    /// tests within one binary in parallel threads by default; without this,
    /// two tests touching the env vars race with each other (and with
    /// anything else that reads them).
    fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// RAII helper that restores a captured env var to its previous value
    /// when dropped, even if the test panics.
    struct EnvGuard {
        key: &'static str,
        prev: Option<std::ffi::OsString>,
    }
    impl EnvGuard {
        fn capture(key: &'static str) -> Self {
            EnvGuard {
                key,
                prev: std::env::var_os(key),
            }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: env mutation is gated by `env_test_lock`.
            match self.prev.take() {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn test_resolve_project_saves_dir_env_override() {
        let _gate = env_test_lock();
        let _restore_saves = EnvGuard::capture(SAVES_DIR_ENV);
        let _restore_root = EnvGuard::capture(crate::paths::USER_DATA_DIR_ENV);

        let tmp = TempDir::new().unwrap();
        let explicit = tmp.path().join("custom_saves");
        // SAFETY: env mutation is gated by `env_test_lock`.
        unsafe { std::env::set_var(SAVES_DIR_ENV, &explicit) };

        let resolved = resolve_project_saves_dir("Rundale");
        assert_eq!(resolved, explicit);
        assert!(resolved.is_absolute());
        assert!(resolved.is_dir());
    }

    #[test]
    fn test_resolve_project_saves_dir_uses_user_data_dir() {
        let _gate = env_test_lock();
        let _restore_saves = EnvGuard::capture(SAVES_DIR_ENV);
        let _restore_root = EnvGuard::capture(crate::paths::USER_DATA_DIR_ENV);

        let tmp = TempDir::new().unwrap();
        // SAFETY: gated.
        unsafe {
            std::env::remove_var(SAVES_DIR_ENV);
            std::env::set_var(crate::paths::USER_DATA_DIR_ENV, tmp.path());
        }

        let resolved = resolve_project_saves_dir("Rundale");
        assert_eq!(resolved, tmp.path().join(SAVES_DIR));
        assert!(resolved.is_dir());
    }

    #[test]
    fn test_resolve_project_saves_dir_empty_env_ignored() {
        let _gate = env_test_lock();
        let _restore_saves = EnvGuard::capture(SAVES_DIR_ENV);
        let _restore_root = EnvGuard::capture(crate::paths::USER_DATA_DIR_ENV);

        let tmp = TempDir::new().unwrap();
        // SAFETY: gated.
        unsafe {
            std::env::set_var(SAVES_DIR_ENV, "");
            std::env::set_var(crate::paths::USER_DATA_DIR_ENV, tmp.path());
        }

        // Empty PARISH_SAVES_DIR must not be used as the path "".
        let resolved = resolve_project_saves_dir("Rundale");
        assert_eq!(resolved, tmp.path().join(SAVES_DIR));
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
    fn test_ensure_saves_dir_at_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("nested/saves");
        let returned = ensure_saves_dir_at(target.clone());
        assert_eq!(returned, target);
        assert!(target.is_dir());
    }
}
