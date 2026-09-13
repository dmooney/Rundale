//! JSON export / import round-trip (TD-028 split from main.rs).

use std::io::{self, Read};

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::{ExportBlob, ExportNpc, WORLD_YEAR};

pub(crate) fn export_npcs(conn: &Connection, parish: Option<&str>) -> Result<()> {
    // `sex` added in #436 so import can restore it rather than
    // hard-coding 'unknown'. Keep the column order stable so the
    // mapper indices are obvious.
    let sql = if parish.is_some() {
        "
        SELECT n.id, n.name, n.sex, n.age, p.name, n.occupation, n.data_tier, n.mood, n.personality
        FROM npcs n JOIN parishes p ON p.id = n.parish_id
        WHERE p.name = ?
        ORDER BY n.id
        "
    } else {
        "
        SELECT n.id, n.name, n.sex, n.age, p.name, n.occupation, n.data_tier, n.mood, n.personality
        FROM npcs n JOIN parishes p ON p.id = n.parish_id
        ORDER BY n.id
        "
    };
    let mut stmt = conn.prepare(sql)?;

    let mapper = |r: &rusqlite::Row<'_>| {
        Ok(ExportNpc {
            id: r.get(0)?,
            name: r.get(1)?,
            sex: r.get(2)?,
            age: r.get(3)?,
            parish: r.get(4)?,
            occupation: r.get(5)?,
            data_tier: r.get(6)?,
            mood: r.get(7)?,
            personality: r.get(8)?,
        })
    };
    let npcs = if let Some(p) = parish {
        stmt.query_map(params![p.to_lowercase()], mapper)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], mapper)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let blob = ExportBlob { npcs };
    println!("{}", serde_json::to_string_pretty(&blob)?);
    Ok(())
}

pub(crate) fn import_npcs_inner(conn: &Connection, npcs: Vec<ExportNpc>) -> Result<(u64, u64)> {
    let tx = conn.unchecked_transaction()?;
    let mut inserted = 0u64;
    let mut updated = 0u64;
    for npc in npcs {
        let parish_lc = npc.parish.to_lowercase();
        tx.execute(
            "INSERT OR IGNORE INTO parishes(county_id, name) VALUES ((SELECT id FROM counties LIMIT 1), ?)",
            params![&parish_lc],
        )?;
        let parish_id: i64 = tx.query_row(
            "SELECT id FROM parishes WHERE name = ?",
            params![&parish_lc],
            |r| r.get(0),
        )?;

        // #436: use INSERT … ON CONFLICT DO UPDATE instead of INSERT
        // OR REPLACE so columns that aren't in the export blob (most
        // importantly `household_id`) are preserved on existing rows.
        // INSERT OR REPLACE deletes the old row and inserts a new
        // one, silently losing household_id, personality when the
        // blob doesn't include it, etc. The `sex` column now comes
        // from the blob so export→import is a lossless round-trip.
        let row_existed_before: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM npcs WHERE id = ?)",
                params![npc.id],
                |r| r.get(0),
            )
            .unwrap_or(false);

        tx.execute(
            "INSERT INTO npcs(id, name, sex, birth_year, age, parish_id, occupation, data_tier, mood, personality)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name        = excluded.name,
                 sex         = excluded.sex,
                 birth_year  = excluded.birth_year,
                 age         = excluded.age,
                 parish_id   = excluded.parish_id,
                 occupation  = excluded.occupation,
                 data_tier   = excluded.data_tier,
                 mood        = excluded.mood,
                 personality = excluded.personality",
            params![
                npc.id,
                npc.name,
                npc.sex,
                WORLD_YEAR - npc.age,
                npc.age,
                parish_id,
                npc.occupation,
                npc.data_tier,
                npc.mood,
                npc.personality
            ],
        )?;

        if row_existed_before {
            updated += 1;
        } else {
            inserted += 1;
        }
    }
    tx.commit()?;
    Ok((inserted, updated))
}

/// Parses an export blob from any reader. Extracted from `import_npcs` so the
/// JSON-validation contract (the `"invalid JSON input"` context on malformed
/// bytes) is unit-testable without a live stdin (TD-025).
pub(crate) fn parse_import_blob(mut reader: impl Read) -> Result<ExportBlob> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    serde_json::from_str(&input).context("invalid JSON input")
}

pub(crate) fn import_npcs(conn: &Connection) -> Result<()> {
    let blob = parse_import_blob(io::stdin().lock())?;
    let (inserted, updated) = import_npcs_inner(conn, blob.npcs)?;
    println!(
        "Imported NPCs from stdin: inserted {inserted}, updated {updated} (household_id and other non-export columns preserved on updates)"
    );
    Ok(())
}
