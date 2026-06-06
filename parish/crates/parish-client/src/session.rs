use std::fs;
use std::path::{Path, PathBuf};

fn session_path() -> Option<PathBuf> {
    let state_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()));
    Some(state_dir.join("parish").join("session"))
}

/// Read a session id from `path`, trimming surrounding whitespace and treating
/// an empty (or whitespace-only) file as "no session".
fn load_from(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Persist `sid` to `path`, creating parent directories as needed.
fn save_to(path: &Path, sid: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, sid)
}

pub fn load() -> Option<String> {
    load_from(&session_path()?)
}

pub fn save(sid: &str) -> std::io::Result<()> {
    if let Some(path) = session_path() {
        save_to(&path, sid)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("session");
        save_to(&path, "abc123").expect("save creates parents + writes");
        assert_eq!(load_from(&path), Some("abc123".to_string()));
    }

    #[test]
    fn load_trims_surrounding_whitespace() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session");
        save_to(&path, "  sid-with-newline\n").expect("save");
        assert_eq!(load_from(&path), Some("sid-with-newline".to_string()));
    }

    #[test]
    fn load_treats_empty_file_as_no_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session");
        save_to(&path, "   \n\t").expect("save");
        assert_eq!(load_from(&path), None);
    }

    #[test]
    fn load_missing_file_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(load_from(&dir.path().join("does-not-exist")), None);
    }
}
