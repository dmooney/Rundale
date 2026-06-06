//! Branch CRUD operations and row mapping.

use rusqlite::{Connection, OptionalExtension, params};

use crate::IntoParishDbError as _;
use parish_types::ParishError;

/// Information about a save branch.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BranchInfo {
    /// Database row id.
    pub id: i64,
    /// Human-readable branch name (unique).
    pub name: String,
    /// When the branch was created (ISO 8601).
    pub created_at: String,
    /// Parent branch id, if forked.
    pub parent_branch_id: Option<i64>,
}

/// Maps a rusqlite `Row` columns (id, name, created_at, parent_branch_id) into a `BranchInfo`.
pub(super) fn branch_info_from_row(row: &rusqlite::Row) -> rusqlite::Result<BranchInfo> {
    Ok(BranchInfo {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        parent_branch_id: row.get(3)?,
    })
}

/// Creates a new branch with the given name.
///
/// Returns the new branch row id.
pub(super) fn create_branch(
    conn: &Connection,
    name: &str,
    parent_branch_id: Option<i64>,
) -> Result<i64, ParishError> {
    if let Some(parent_id) = parent_branch_id {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM branches WHERE id = ?1)",
                params![parent_id],
                |row| row.get(0),
            )
            .db_err()?;
        if !exists {
            return Err(ParishError::Database(format!(
                "parent branch id {parent_id} does not exist"
            )));
        }
    }

    let created_at = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO branches (name, created_at, parent_branch_id) VALUES (?1, ?2, ?3)",
        params![name, created_at, parent_branch_id],
    )
    .db_err()?;
    Ok(conn.last_insert_rowid())
}

/// Finds a branch by name.
pub(super) fn find_branch(
    conn: &Connection,
    name: &str,
) -> Result<Option<BranchInfo>, ParishError> {
    conn.query_row(
        "SELECT id, name, created_at, parent_branch_id FROM branches WHERE name = ?1",
        params![name],
        branch_info_from_row,
    )
    .optional()
    .db_err()
}

/// Lists all branches.
pub(super) fn list_branches(conn: &Connection) -> Result<Vec<BranchInfo>, ParishError> {
    let mut stmt = conn
        .prepare("SELECT id, name, created_at, parent_branch_id FROM branches ORDER BY id")
        .db_err()?;
    let rows = stmt.query_map([], branch_info_from_row).db_err()?;
    let mut branches = Vec::new();
    for row in rows {
        branches.push(row.db_err()?);
    }
    Ok(branches)
}
