use std::fs;
use std::path::PathBuf;

fn session_path() -> Option<PathBuf> {
    let state_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()));
    Some(state_dir.join("parish").join("session"))
}

pub fn load() -> Option<String> {
    let path = session_path()?;
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn save(sid: &str) -> std::io::Result<()> {
    if let Some(path) = session_path() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, sid)?;
    }
    Ok(())
}
