//! Read / inspect / edit commands: list, show, search, edit, promote, stats, family-tree, relationships (TD-028 split from main.rs).

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::DataTier;

pub(crate) fn list_npcs(
    conn: &Connection,
    parish: Option<&str>,
    occupation: Option<&str>,
    tier: Option<DataTier>,
    limit: u32,
) -> Result<()> {
    let mut clauses: Vec<String> = Vec::new();
    let mut bind: Vec<String> = Vec::new();

    if let Some(p) = parish {
        clauses.push("p.name = ?".to_string());
        bind.push(p.to_lowercase());
    }
    if let Some(o) = occupation {
        clauses.push("n.occupation = ?".to_string());
        bind.push(o.to_string());
    }
    if let Some(t) = tier {
        clauses.push("n.data_tier = ?".to_string());
        bind.push(t.as_i64().to_string());
    }

    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {} ", clauses.join(" AND "))
    };
    let sql = format!(
        "SELECT n.id, n.name, p.name, n.occupation, n.data_tier \
         FROM npcs n JOIN parishes p ON p.id = n.parish_id \
         {}ORDER BY n.id LIMIT ?",
        where_clause
    );
    bind.push(limit.to_string());

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(bind.iter()))?;
    println!("id\tname\tparish\toccupation\ttier");
    while let Some(row) = rows.next()? {
        let tier_i: i64 = row.get(4)?;
        println!(
            "{}\t{}\t{}\t{}\t{}",
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            DataTier::from_i64(tier_i)
        );
    }
    Ok(())
}

pub(crate) fn show_npc(conn: &Connection, npc_id: i64) -> Result<()> {
    let row = conn
        .query_row(
            "
            SELECT n.id, n.name, n.age, p.name, n.occupation, n.data_tier, n.mood, n.personality
            FROM npcs n JOIN parishes p ON p.id = n.parish_id
            WHERE n.id = ?
            ",
            params![npc_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .optional()?;

    if let Some((id, name, age, parish, occupation, tier, mood, personality)) = row {
        println!("id: {id}");
        println!("name: {name}");
        println!("age: {age}");
        println!("parish: {parish}");
        println!("occupation: {occupation}");
        println!("tier: {}", DataTier::from_i64(tier));
        println!("mood: {}", mood.unwrap_or_else(|| "-".to_string()));
        println!(
            "personality: {}",
            personality.unwrap_or_else(|| "(none)".to_string())
        );
        Ok(())
    } else {
        bail!("NPC {} not found", npc_id)
    }
}

pub(crate) fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub(crate) fn search_npcs(conn: &Connection, query: &str, limit: u32) -> Result<()> {
    let like = format!("%{}%", escape_like(query));
    let mut stmt = conn.prepare(
        "
        SELECT n.id, n.name, p.name, n.occupation
        FROM npcs n JOIN parishes p ON p.id = n.parish_id
        WHERE n.name LIKE ? ESCAPE '\\'
        ORDER BY n.name
        LIMIT ?
    ",
    )?;
    let rows = stmt.query_map(params![like, limit], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;

    for row in rows {
        let (id, name, parish, occupation) = row?;
        println!("{id}: {name} ({occupation}, {parish})");
    }
    Ok(())
}

pub(crate) fn edit_npc(
    conn: &Connection,
    npc_id: i64,
    mood: Option<String>,
    occupation: Option<String>,
) -> Result<()> {
    if mood.is_none() && occupation.is_none() {
        bail!("provide at least one change (--mood or --occupation)");
    }
    if let Some(m) = mood {
        conn.execute("UPDATE npcs SET mood = ? WHERE id = ?", params![m, npc_id])?;
    }
    if let Some(o) = occupation {
        conn.execute(
            "UPDATE npcs SET occupation = ? WHERE id = ?",
            params![o, npc_id],
        )?;
    }
    println!("Updated NPC {}", npc_id);
    Ok(())
}

pub(crate) fn promote_npc(conn: &Connection, npc_id: i64) -> Result<()> {
    let changed = conn.execute(
        "
        UPDATE npcs
        SET data_tier = 1,
            personality = COALESCE(personality, 'A quietly observant parishioner with strong local ties.'),
            mood = COALESCE(mood, 'curious')
        WHERE id = ?
    ",
        params![npc_id],
    )?;
    if changed == 0 {
        bail!("NPC {} not found", npc_id);
    }
    println!("Promoted NPC {} to Elaborated", npc_id);
    Ok(())
}

pub(crate) fn stats(conn: &Connection) -> Result<()> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM npcs", [], |r| r.get(0))?;
    let sketched: i64 =
        conn.query_row("SELECT COUNT(*) FROM npcs WHERE data_tier = 0", [], |r| {
            r.get(0)
        })?;
    let elaborated: i64 =
        conn.query_row("SELECT COUNT(*) FROM npcs WHERE data_tier = 1", [], |r| {
            r.get(0)
        })?;
    let authored: i64 =
        conn.query_row("SELECT COUNT(*) FROM npcs WHERE data_tier = 2", [], |r| {
            r.get(0)
        })?;

    println!("Total NPCs: {total}");
    println!("Sketched: {sketched}");
    println!("Elaborated: {elaborated}");
    println!("Authored: {authored}");
    Ok(())
}

pub(crate) fn family_tree(conn: &Connection, npc_id: i64) -> Result<()> {
    // household_id is nullable in the schema (line 261). Fetch it as
    // Option<i64> so a NULL value — possible via import or manual
    // editing — doesn't surface as a misleading "NPC not found" error
    // (#435). An NPC without a household still exists; we just have
    // no tree to print.
    let (household_id, target_name, target_age): (Option<i64>, String, i64) = conn
        .query_row(
            "SELECT household_id, name, age FROM npcs WHERE id = ?",
            params![npc_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .context("NPC not found")?;

    let Some(household_id) = household_id else {
        println!("Family tree for {target_name}: no household assigned");
        return Ok(());
    };

    println!("Family tree for {target_name} (household #{household_id})");
    let mut stmt = conn
        .prepare("SELECT id, name, age FROM npcs WHERE household_id = ? ORDER BY age DESC, id")?;
    let rows = stmt.query_map(params![household_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;

    for row in rows {
        let (id, name, age) = row?;
        let relation = if id == npc_id {
            "self"
        } else if age >= target_age + 16 {
            "possible elder"
        } else if age + 16 <= target_age {
            "possible younger"
        } else {
            "peer"
        };
        println!("- {id}: {name}, age {age} ({relation})");
    }
    Ok(())
}

pub(crate) fn relationships(conn: &Connection, npc_id: i64) -> Result<()> {
    let exists: Option<String> = conn
        .query_row("SELECT name FROM npcs WHERE id = ?", params![npc_id], |r| {
            r.get(0)
        })
        .optional()?;
    let name = exists.context("NPC not found")?;

    println!("Relationships for {name} ({npc_id})");
    let mut stmt = conn.prepare(
        "
        SELECT r.to_npc_id, n.name, r.kind, r.strength
        FROM npc_relationships r
        JOIN npcs n ON n.id = r.to_npc_id
        WHERE r.from_npc_id = ?
        ORDER BY r.strength DESC
    ",
    )?;
    let rows = stmt.query_map(params![npc_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, f64>(3)?,
        ))
    })?;

    for row in rows {
        let (target_id, target_name, kind, strength) = row?;
        println!("- {target_id}: {target_name} [{kind}] {strength:.2}");
    }
    Ok(())
}
