//! Durable pointer to the last successfully selected save file and branch.
//!
//! Save databases contain the game state, but a runtime still needs to know
//! which database and branch to resume when several exist. This small
//! sidecar stores only a validated filename plus exact branch identity. It is
//! staged and renamed so a crash cannot expose partially-written JSON.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use parish_types::ParishError;
use serde::{Deserialize, Serialize};

use crate::IntoParishDbError as _;

const ACTIVE_IDENTITY_FILENAME: &str = ".active-save.json";
const ACTIVE_IDENTITY_VERSION: u8 = 1;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSaveIdentity {
    pub save_path: PathBuf,
    pub branch_id: i64,
    pub branch_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredActiveSaveIdentity {
    version: u8,
    save_filename: String,
    branch_id: i64,
    branch_name: String,
}

fn config_error(message: impl Into<String>) -> ParishError {
    ParishError::Config(message.into())
}

fn validated_filename(save_path: &Path) -> Result<String, ParishError> {
    if save_path
        .extension()
        .is_none_or(|extension| extension != "db")
    {
        return Err(config_error(format!(
            "active save must be a .db file: {}",
            save_path.display()
        )));
    }
    save_path
        .file_name()
        .and_then(|filename| filename.to_str())
        .filter(|filename| !filename.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| config_error("active save filename is not valid UTF-8"))
}

fn validate_direct_child(saves_dir: &Path, save_path: &Path) -> Result<(), ParishError> {
    let canonical_dir = fs::canonicalize(saves_dir)?;
    let canonical_save = fs::canonicalize(save_path)?;
    if canonical_save.parent() != Some(canonical_dir.as_path()) {
        return Err(config_error(format!(
            "active save {} is outside saves directory {}",
            canonical_save.display(),
            canonical_dir.display()
        )));
    }
    Ok(())
}

/// Atomically records the exact save file and branch to resume.
pub fn write_active_save_identity(
    saves_dir: &Path,
    save_path: &Path,
    branch_id: i64,
    branch_name: &str,
) -> Result<(), ParishError> {
    write_active_save_identity_with_parent_sync(
        saves_dir,
        save_path,
        branch_id,
        branch_name,
        |directory| {
            #[cfg(unix)]
            fs::File::open(directory)?.sync_all()?;
            #[cfg(not(unix))]
            let _ = directory;
            Ok(())
        },
    )
}

fn write_active_save_identity_with_parent_sync(
    saves_dir: &Path,
    save_path: &Path,
    branch_id: i64,
    branch_name: &str,
    sync_parent: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), ParishError> {
    if branch_id <= 0 {
        return Err(config_error("active branch id must be positive"));
    }
    if branch_name.trim().is_empty() {
        return Err(config_error("active branch name must not be blank"));
    }
    validate_direct_child(saves_dir, save_path)?;
    let stored = StoredActiveSaveIdentity {
        version: ACTIVE_IDENTITY_VERSION,
        save_filename: validated_filename(save_path)?,
        branch_id,
        branch_name: branch_name.to_string(),
    };
    let mut body = serde_json::to_vec_pretty(&stored)?;
    body.push(b'\n');

    let marker_path = saves_dir.join(ACTIVE_IDENTITY_FILENAME);
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_path = saves_dir.join(format!(
        "{ACTIVE_IDENTITY_FILENAME}.{}.{}.tmp",
        std::process::id(),
        counter
    ));
    let stage_result = (|| -> Result<(), ParishError> {
        let mut temp_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)?;
        temp_file.write_all(&body)?;
        temp_file.sync_all()?;
        fs::rename(&temp_path, &marker_path)?;
        Ok(())
    })();
    if stage_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return stage_result;
    }

    // `rename` is the commit point: from here readers observe the complete new
    // identity. A parent-directory sync improves crash durability, but failure
    // cannot roll the rename back and therefore must not report an uncommitted
    // `Err` to callers (which could delete the now-active save candidate).
    if let Err(error) = sync_parent(saves_dir) {
        tracing::warn!(
            path = %marker_path.display(),
            %error,
            "active-save marker committed, but parent-directory sync failed"
        );
    }
    Ok(())
}

fn filename_is_safe(filename: &str) -> bool {
    let mut components = Path::new(filename).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

/// Reads and validates the exact save file and branch selected last time.
///
/// A missing marker returns `Ok(None)`. Malformed, stale, or path-escaping
/// markers return an error and must fail closed. Legacy fallback is valid only
/// when this function returns `Ok(None)`.
pub fn read_active_save_identity(
    saves_dir: &Path,
) -> Result<Option<ActiveSaveIdentity>, ParishError> {
    let Some(identity) = read_active_save_identity_candidate(saves_dir)? else {
        return Ok(None);
    };
    let db = rusqlite::Connection::open_with_flags(
        &identity.save_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .db_err()?;
    let branch_matches: bool = db
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM branches WHERE id = ?1 AND name = ?2
             )",
            rusqlite::params![identity.branch_id, &identity.branch_name],
            |row| row.get(0),
        )
        .db_err()?;
    if !branch_matches {
        return Err(config_error(format!(
            "active branch {} ({}) does not exist in {}",
            identity.branch_name,
            identity.branch_id,
            identity.save_path.display()
        )));
    }
    Ok(Some(identity))
}

/// Reads and path-validates the active marker without opening its database.
///
/// Startup lifecycle code uses this first, acquires the save-file lock, and
/// only then opens SQLite to validate the branch. This keeps advisory locking
/// ahead of every database read, migration, and recovery operation.
pub fn read_active_save_identity_candidate(
    saves_dir: &Path,
) -> Result<Option<ActiveSaveIdentity>, ParishError> {
    let marker_path = saves_dir.join(ACTIVE_IDENTITY_FILENAME);
    let body = match fs::read(&marker_path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let stored: StoredActiveSaveIdentity = serde_json::from_slice(&body)?;
    if stored.version != ACTIVE_IDENTITY_VERSION {
        return Err(config_error(format!(
            "unsupported active-save identity version {}",
            stored.version
        )));
    }
    if !filename_is_safe(&stored.save_filename) {
        return Err(config_error(
            "active save marker contains an unsafe filename",
        ));
    }
    if stored.branch_id <= 0 || stored.branch_name.trim().is_empty() {
        return Err(config_error(
            "active save marker contains an invalid branch identity",
        ));
    }

    let save_path = saves_dir.join(&stored.save_filename);
    validate_direct_child(saves_dir, &save_path)?;
    validated_filename(&save_path)?;
    Ok(Some(ActiveSaveIdentity {
        save_path,
        branch_id: stored.branch_id,
        branch_name: stored.branch_name,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, GameSnapshot};
    use parish_npc::manager::NpcManager;
    use parish_world::WorldState;
    use tempfile::TempDir;

    #[test]
    fn exact_save_and_branch_round_trip() {
        let temp = TempDir::new().unwrap();
        let save_path = temp.path().join("parish_002.db");
        let db = Database::open(&save_path).unwrap();
        let main = db.find_branch("main").unwrap().unwrap();
        let fork = db.create_branch("fork", Some(main.id)).unwrap();
        db.save_snapshot(
            fork,
            &GameSnapshot::capture(&WorldState::new(), &NpcManager::new()),
        )
        .unwrap();

        write_active_save_identity(temp.path(), &save_path, fork, "fork").unwrap();
        let identity = read_active_save_identity(temp.path()).unwrap().unwrap();

        assert_eq!(
            fs::canonicalize(identity.save_path).unwrap(),
            fs::canonicalize(save_path).unwrap()
        );
        assert_eq!(identity.branch_id, fork);
        assert_eq!(identity.branch_name, "fork");
    }

    #[test]
    fn rewriting_marker_replaces_the_previous_branch_atomically() {
        let temp = TempDir::new().unwrap();
        let save_path = temp.path().join("parish_001.db");
        let db = Database::open(&save_path).unwrap();
        let main = db.find_branch("main").unwrap().unwrap();
        let fork = db.create_branch("fork", Some(main.id)).unwrap();

        write_active_save_identity(temp.path(), &save_path, main.id, "main").unwrap();
        write_active_save_identity(temp.path(), &save_path, fork, "fork").unwrap();

        let identity = read_active_save_identity(temp.path()).unwrap().unwrap();
        assert_eq!(identity.branch_id, fork);
        assert_eq!(identity.branch_name, "fork");
        assert!(
            fs::read_dir(temp.path())
                .unwrap()
                .filter_map(Result::ok)
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".tmp"))
        );
    }

    #[test]
    fn unsafe_filename_is_rejected() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join(ACTIVE_IDENTITY_FILENAME),
            br#"{"version":1,"save_filename":"../escape.db","branch_id":1,"branch_name":"main"}"#,
        )
        .unwrap();

        let error = read_active_save_identity(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unsafe filename"));
    }

    #[test]
    fn stale_branch_is_rejected() {
        let temp = TempDir::new().unwrap();
        let save_path = temp.path().join("parish_001.db");
        Database::open(&save_path).unwrap();
        fs::write(
            temp.path().join(ACTIVE_IDENTITY_FILENAME),
            br#"{"version":1,"save_filename":"parish_001.db","branch_id":99,"branch_name":"gone"}"#,
        )
        .unwrap();

        let error = read_active_save_identity(temp.path()).unwrap_err();

        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn post_rename_parent_sync_failure_reports_committed_success() {
        let temp = TempDir::new().unwrap();
        let save_path = temp.path().join("parish_001.db");
        let db = Database::open(&save_path).unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();

        let result = write_active_save_identity_with_parent_sync(
            temp.path(),
            &save_path,
            branch.id,
            &branch.name,
            |_| Err(std::io::Error::other("injected directory sync failure")),
        );

        assert!(
            result.is_ok(),
            "the marker was already committed by rename: {result:?}"
        );
        let identity = read_active_save_identity(temp.path()).unwrap().unwrap();
        assert_eq!(identity.branch_id, branch.id);
        assert_eq!(identity.branch_name, branch.name);
    }
}
