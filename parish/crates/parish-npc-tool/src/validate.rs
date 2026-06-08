//! Database integrity validation (TD-027/TD-028 split from main.rs).

use anyhow::{Result, bail};
use rusqlite::{Connection, params};

pub(crate) fn validate_db(conn: &Connection, parish: Option<String>, all: bool) -> Result<()> {
    if parish.is_some() && all {
        bail!("choose either --parish or --all");
    }

    // #592 — Use a parameterized sub-query for the parish filter so the
    // caller-supplied name is never interpolated into SQL text.  The parish
    // name is passed as a bound value to `rusqlite`; the predicate columns
    // (`household_id`, `age`, …) are fixed literals that never originate from
    // user input, so they remain safe to format into the SQL string.
    let parish_lc: Option<String> = parish.as_deref().map(str::to_lowercase);

    let missing_households: i64 = count_with_optional_parish(
        conn,
        "household_id IS NULL OR household_id NOT IN (SELECT id FROM households)",
        parish_lc.as_deref(),
    )?;
    let invalid_age: i64 =
        count_with_optional_parish(conn, "age < 0 OR age > 110", parish_lc.as_deref())?;
    let elaborated_without_personality: i64 = count_with_optional_parish(
        conn,
        "data_tier >= 1 AND (personality IS NULL OR personality = '')",
        parish_lc.as_deref(),
    )?;
    let broken_relationships: i64 = conn.query_row(
        "
        SELECT COUNT(*) FROM npc_relationships r
        LEFT JOIN npcs a ON a.id = r.from_npc_id
        LEFT JOIN npcs b ON b.id = r.to_npc_id
        WHERE a.id IS NULL OR b.id IS NULL
    ",
        [],
        |r| r.get(0),
    )?;

    println!("Validation report:");
    println!("- missing_households: {missing_households}");
    println!("- invalid_age: {invalid_age}");
    println!("- elaborated_without_personality: {elaborated_without_personality}");
    println!("- broken_relationships: {broken_relationships}");

    if missing_households + invalid_age + elaborated_without_personality + broken_relationships > 0
    {
        bail!("validation failed");
    }
    println!("Validation passed");
    Ok(())
}

/// Counts `npcs` rows matching a fixed `predicate`, optionally scoped to a
/// parish by lowercase name.
///
/// #592 — the parish name (the only user-supplied value) is always bound as
/// a `?` parameter, never interpolated into SQL. `predicate` is a fixed
/// literal supplied by `validate_db` and never originates from user input,
/// so it is safe to format into the query string. Replaces the former
/// `with_filter` inner fn + `count!` macro (TD-027) so each validation rule
/// is a single call.
pub(crate) fn count_with_optional_parish(
    conn: &Connection,
    predicate: &str,
    parish_lc: Option<&str>,
) -> Result<i64> {
    let count = match parish_lc {
        Some(name) => {
            let sql = format!(
                "SELECT COUNT(*) FROM npcs n \
                 WHERE n.parish_id = (SELECT id FROM parishes WHERE name = ?) \
                 AND ({predicate})"
            );
            conn.query_row(&sql, params![name], |r| r.get::<_, i64>(0))?
        }
        None => {
            let sql = format!("SELECT COUNT(*) FROM npcs n WHERE {predicate}");
            conn.query_row(&sql, [], |r| r.get::<_, i64>(0))?
        }
    };
    Ok(count)
}
