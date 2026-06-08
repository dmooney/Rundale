//! Database path resolution and connection setup (TD-028 split from main.rs).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::ensure_schema;

/// Leaf filename of the NPC-tool database, relative to the `data/` directory.
pub(crate) const DB_FILENAME: &str = "parish-world.db";

/// Environment variable that pins the DB path directly (overrides project-root
/// anchor but is overridden by an explicit `--db` flag on the command line).
pub(crate) const NPC_TOOL_DB_ENV: &str = "PARISH_NPC_TOOL_DB";

/// Resolves the default NPC-tool DB path when `--db` is not given on the
/// command line.  Resolution order (Rule 9 — never bare cwd-relative):
///
///  1. `PARISH_NPC_TOOL_DB` env var — explicit operator/test override.
///  2. `PARISH_DATA_DIR` env var — mirrors the data-dir convention used by
///     the server and Tauri entry-points; appends `parish-world.db`.
///  3. Walk up to 4 ancestor directories of the startup cwd looking for
///     `Cargo.toml` (project root sentinel); returns
///     `<root>/data/parish-world.db` when found.
///  4. Bare `data/parish-world.db` relative to the startup cwd as a last
///     resort — matches the previous hard-coded default so single-shot runs
///     from the repo root still work.
pub fn resolve_default_db() -> PathBuf {
    // 1. Explicit env-var override.
    if let Ok(s) = std::env::var(NPC_TOOL_DB_ENV) {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    // 2. PARISH_DATA_DIR mirrors the convention in parish-tauri / parish-server.
    if let Ok(data_dir) = std::env::var("PARISH_DATA_DIR") {
        let trimmed = data_dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(DB_FILENAME);
        }
    }

    // 3. Walk ancestors for Cargo.toml (project root).
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut p = cwd.clone();
    for _ in 0..4 {
        if p.join("Cargo.toml").exists() {
            return p.join("data").join(DB_FILENAME);
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }

    // 4. Last resort: startup-cwd relative (prior behaviour).
    cwd.join("data").join(DB_FILENAME)
}

pub(crate) fn open_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create DB parent directory {}", parent.display())
        })?;
    }
    let conn =
        Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    ensure_schema(&conn)?;
    Ok(conn)
}
