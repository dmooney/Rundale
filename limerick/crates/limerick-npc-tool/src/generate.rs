//! World / parish population generation (TD-028 split from main.rs).

use anyhow::{Result, bail};
use rand::prelude::IndexedRandom;
use rand::{RngExt, SeedableRng, rngs::StdRng};
use rusqlite::{Connection, OptionalExtension, params};

use crate::WORLD_YEAR;
use crate::query::promote_npc;

pub(crate) const MALE_NAMES: &[&str] =
    &["Pádraig", "Seán", "Michael", "Thomas", "James", "Brendan"];

pub(crate) const FEMALE_NAMES: &[&str] =
    &["Mary", "Bridget", "Margaret", "Catherine", "Niamh", "Aoife"];

pub(crate) const SURNAMES: &[&str] =
    &["Kelly", "Murphy", "Brennan", "O'Brien", "Flanagan", "Darcy"];

pub(crate) const OCCUPATIONS: &[(&str, u8)] = &[
    ("Tenant Farmer", 35),
    ("Laborer", 30),
    ("Servant", 10),
    ("Craftsman", 8),
    ("Shopkeeper", 3),
    ("Clergy", 1),
    ("Gentry", 2),
    ("Other", 11),
];

pub(crate) fn generate_world(conn: &Connection, counties: &[String]) -> Result<()> {
    if counties.is_empty() {
        bail!("--counties is required (comma-separated)");
    }
    for county in counties {
        conn.execute(
            "INSERT OR IGNORE INTO counties(name) VALUES (?)",
            params![county.to_lowercase()],
        )?;
    }
    println!("Seeded {} counties", counties.len());
    Ok(())
}

pub(crate) fn generate_parish(
    conn: &Connection,
    parish: &str,
    pop: u32,
    seed: Option<u64>,
) -> Result<()> {
    // Ensure a county row exists before opening the generation transaction
    // so that the INSERT OR IGNORE below sees a valid county_id.
    //
    // TD-023: never auto-create a hard-coded county (it surprised authors of
    // mods set in Galway/Mayo) and never `.expect()`-panic on a read-only or
    // constrained DB. Require the caller to seed counties via `generate-world`
    // first, surfacing a clear recoverable error instead of a process abort.
    let county_id: i64 = match conn
        .query_row("SELECT id FROM counties ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .optional()?
    {
        Some(id) => id,
        None => bail!(
            "no county exists — run `generate-world --counties <name,...>` before `generate-parish`"
        ),
    };

    // Wrap all NPC/relationship inserts in a single transaction (#606).
    // If any insert fails (disk full, constraint violation, process crash)
    // the entire generation is rolled back, leaving no orphaned rows.
    // Matches the transaction pattern used by `import_npcs_inner`.
    let tx = conn.unchecked_transaction()?;

    let parish_lc = parish.to_lowercase();
    tx.execute(
        "INSERT OR IGNORE INTO parishes(county_id, name) VALUES (?, ?)",
        params![county_id, &parish_lc],
    )?;
    let parish_id: i64 = tx.query_row(
        "SELECT id FROM parishes WHERE name = ?",
        params![&parish_lc],
        |r| r.get(0),
    )?;

    let mut rng = StdRng::seed_from_u64(seed.unwrap_or(42));
    let household_count = (pop / 6).max(1);
    let now_year = WORLD_YEAR;

    for i in 0..household_count {
        tx.execute(
            "INSERT INTO households(parish_id, name) VALUES (?, ?)",
            params![parish_id, format!("{} Household {}", parish, i + 1)],
        )?;
        let household_id = tx.last_insert_rowid();
        let members = rng.random_range(4..=8);
        for _ in 0..members {
            let female = rng.random_bool(0.5);
            let first = if female {
                FEMALE_NAMES
                    .choose(&mut rng)
                    .expect("female names list is non-empty")
            } else {
                MALE_NAMES
                    .choose(&mut rng)
                    .expect("male names list is non-empty")
            };
            let surname = SURNAMES
                .choose(&mut rng)
                .expect("surname list is non-empty");
            let name = format!("{} {}", first, surname);
            let age: i64 = rng.random_range(0..=85);
            let birth_year = now_year - age;
            let occupation = weighted_occupation(&mut rng);
            tx.execute(
                "INSERT INTO npcs(name, sex, birth_year, age, parish_id, household_id, occupation, data_tier, mood) VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)",
                params![name, if female {"female"} else {"male"}, birth_year, age, parish_id, household_id, occupation, "neutral"],
            )?;
        }
    }

    let mut stmt = tx.prepare("SELECT id FROM npcs WHERE parish_id = ?")?;
    let npc_ids: Vec<i64> = stmt
        .query_map(params![parish_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut relationships: Vec<(i64, i64, f64)> = Vec::new();
    for id in &npc_ids {
        for _ in 0..2 {
            if let Some(other) = npc_ids.choose(&mut rng)
                && other != id
            {
                relationships.push((*id, *other, rng.random_range(-0.2..0.9)));
            }
        }
    }
    for (from, to, strength) in &relationships {
        tx.execute(
            "INSERT OR IGNORE INTO npc_relationships(from_npc_id, to_npc_id, kind, strength) VALUES (?, ?, ?, ?)",
            params![from, to, "Acquaintance", strength],
        )?;
    }

    let count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM npcs WHERE parish_id = ?",
        params![parish_id],
        |r| r.get(0),
    )?;
    tx.commit()?;
    println!("Generated parish '{}' with {} sketched NPCs", parish, count);
    Ok(())
}

pub(crate) fn weighted_occupation(rng: &mut StdRng) -> &'static str {
    let roll: u8 = rng.random_range(0..100);
    let mut acc = 0_u8;
    for (occ, weight) in OCCUPATIONS {
        acc = acc.saturating_add(*weight);
        if roll < acc {
            return occ;
        }
    }
    "Other"
}

pub(crate) fn elaborate_parish(conn: &Connection, parish: &str, batch: u32) -> Result<()> {
    let parish = parish.to_lowercase();
    let mut stmt = conn.prepare(
        "
        SELECT n.id
        FROM npcs n JOIN parishes p ON p.id = n.parish_id
        WHERE p.name = ? AND n.data_tier = 0
        ORDER BY n.id
        LIMIT ?
    ",
    )?;
    let ids: Vec<i64> = stmt
        .query_map(params![parish, batch], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for id in &ids {
        promote_npc(conn, *id)?;
    }
    println!("Elaborated {} NPCs in parish {}", ids.len(), parish);
    Ok(())
}
