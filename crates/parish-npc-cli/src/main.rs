use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng, rngs::StdRng};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

const DEFAULT_DB: &str = "data/parish-world.db";
const MALE_NAMES: &[&str] = &["Pádraig", "Seán", "Michael", "Thomas", "James", "Brendan"];
const FEMALE_NAMES: &[&str] = &["Mary", "Bridget", "Margaret", "Catherine", "Niamh", "Aoife"];
const SURNAMES: &[&str] = &["Kelly", "Murphy", "Brennan", "O'Brien", "Flanagan", "Darcy"];
const OCCUPATIONS: &[(&str, u8)] = &[
    ("Tenant Farmer", 35),
    ("Laborer", 30),
    ("Servant", 10),
    ("Craftsman", 8),
    ("Shopkeeper", 3),
    ("Clergy", 1),
    ("Gentry", 2),
    ("Other", 11),
];

#[derive(Parser, Debug)]
#[command(name = "parish-npc")]
#[command(about = "NPC world builder and inspection utility")]
struct Cli {
    /// SQLite database path.
    #[arg(long, global = true, default_value = DEFAULT_DB)]
    db: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    GenerateWorld {
        #[arg(long, value_delimiter = ',')]
        counties: Vec<String>,
    },
    GenerateParish {
        parish: String,
        #[arg(long)]
        pop: u32,
        #[arg(long)]
        seed: Option<u64>,
    },
    List {
        #[arg(long)]
        parish: Option<String>,
        #[arg(long)]
        occupation: Option<String>,
        #[arg(long)]
        tier: Option<DataTier>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    Show {
        npc_id: i64,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    Edit {
        npc_id: i64,
        #[arg(long)]
        mood: Option<String>,
        #[arg(long)]
        occupation: Option<String>,
    },
    Promote {
        npc_id: i64,
    },
    Elaborate {
        #[arg(long)]
        parish: String,
        #[arg(long, default_value_t = 50)]
        batch: u32,
    },
    Validate {
        #[arg(long)]
        parish: Option<String>,
        #[arg(long, default_value_t = false)]
        all: bool,
    },
    Stats,
    Export {
        #[arg(long)]
        parish: Option<String>,
    },
    Import,
    FamilyTree {
        npc_id: i64,
    },
    Relationships {
        npc_id: i64,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DataTier {
    Sketched,
    Elaborated,
    Authored,
}

impl DataTier {
    fn as_i64(self) -> i64 {
        match self {
            Self::Sketched => 0,
            Self::Elaborated => 1,
            Self::Authored => 2,
        }
    }

    fn from_i64(v: i64) -> &'static str {
        match v {
            0 => "Sketched",
            1 => "Elaborated",
            2 => "Authored",
            _ => "Unknown",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportBlob {
    npcs: Vec<ExportNpc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportNpc {
    id: i64,
    name: String,
    /// NPC sex ("female" | "male" | "unknown"). Added in #436 so
    /// export→import round-trips are lossless. Defaults to "unknown"
    /// when a caller feeds import a legacy blob that predates this
    /// field.
    #[serde(default = "default_sex")]
    sex: String,
    age: i64,
    parish: String,
    occupation: String,
    data_tier: i64,
    mood: Option<String>,
    personality: Option<String>,
}

fn default_sex() -> String {
    "unknown".to_string()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let conn = open_db(&cli.db)?;

    match cli.command {
        Command::GenerateWorld { counties } => generate_world(&conn, &counties),
        Command::GenerateParish { parish, pop, seed } => generate_parish(&conn, &parish, pop, seed),
        Command::List {
            parish,
            occupation,
            tier,
            limit,
        } => list_npcs(&conn, parish.as_deref(), occupation.as_deref(), tier, limit),
        Command::Show { npc_id } => show_npc(&conn, npc_id),
        Command::Search { query, limit } => search_npcs(&conn, &query, limit),
        Command::Edit {
            npc_id,
            mood,
            occupation,
        } => edit_npc(&conn, npc_id, mood, occupation),
        Command::Promote { npc_id } => promote_npc(&conn, npc_id),
        Command::Elaborate { parish, batch } => elaborate_parish(&conn, &parish, batch),
        Command::Validate { parish, all } => validate_db(&conn, parish, all),
        Command::Stats => stats(&conn),
        Command::Export { parish } => export_npcs(&conn, parish.as_deref()),
        Command::Import => import_npcs(&conn),
        Command::FamilyTree { npc_id } => family_tree(&conn, npc_id),
        Command::Relationships { npc_id } => relationships(&conn, npc_id),
    }
}

fn open_db(path: &Path) -> Result<Connection> {
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

/// Initialises the standalone `parish-npc` SQLite schema.
///
/// # Schema divergence from parish-persistence (#434)
///
/// This schema is **not** compatible with the main game's persistence
/// format in `parish-persistence` (which stores branch-based game
/// snapshots keyed by session id, not relational parish/household/NPC
/// rows). Databases created by `parish-npc` cannot be loaded by the
/// running game, and save files created by the game cannot be opened
/// by `parish-npc` commands.
///
/// That divergence is deliberate: `parish-npc` is a world-*building*
/// tool that authors use at design time to generate large populations
/// with relational constraints (households, relationships, validation
/// sweeps). The runtime engine only needs read-only NPC data and
/// materialises it into the in-memory `NpcManager` from
/// `mods/<name>/npcs.json`. The two codepaths have different
/// workloads and different shape — forcing them into one schema would
/// burden the runtime with author-time fields (`data_tier`, parish
/// joins) it doesn't use, or would starve the CLI of the relational
/// structure it depends on.
///
/// The practical round-trip is:
///
/// 1. `parish-npc generate-parish …` populates this schema.
/// 2. `parish-npc export [--parish NAME]` emits the JSON blob the
///    game consumes, which can be hand-massaged into `npcs.json`.
/// 3. The game loads `npcs.json` into its own runtime structures —
///    no direct SQLite interop.
///
/// If you need to hold both formats in sync, treat the parish-npc DB
/// as the source of truth at design time and re-export after every
/// authoring session. A proper conversion utility between this schema
/// and a gameplay save is out of scope; track additions there under
/// #434 follow-ups.
fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS counties (
            id INTEGER PRIMARY KEY,
            name TEXT UNIQUE NOT NULL
        );
        CREATE TABLE IF NOT EXISTS parishes (
            id INTEGER PRIMARY KEY,
            county_id INTEGER,
            name TEXT UNIQUE NOT NULL,
            FOREIGN KEY(county_id) REFERENCES counties(id)
        );
        CREATE TABLE IF NOT EXISTS households (
            id INTEGER PRIMARY KEY,
            parish_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            FOREIGN KEY(parish_id) REFERENCES parishes(id)
        );
        CREATE TABLE IF NOT EXISTS npcs (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            sex TEXT NOT NULL,
            birth_year INTEGER NOT NULL,
            age INTEGER NOT NULL,
            parish_id INTEGER NOT NULL,
            household_id INTEGER,
            occupation TEXT NOT NULL,
            data_tier INTEGER NOT NULL DEFAULT 0,
            mood TEXT,
            personality TEXT,
            FOREIGN KEY(parish_id) REFERENCES parishes(id),
            FOREIGN KEY(household_id) REFERENCES households(id)
        );
        CREATE TABLE IF NOT EXISTS npc_relationships (
            from_npc_id INTEGER NOT NULL,
            to_npc_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            strength REAL NOT NULL,
            PRIMARY KEY (from_npc_id, to_npc_id),
            FOREIGN KEY(from_npc_id) REFERENCES npcs(id),
            FOREIGN KEY(to_npc_id) REFERENCES npcs(id)
        );
        CREATE INDEX IF NOT EXISTS idx_npcs_parish ON npcs(parish_id);
        CREATE INDEX IF NOT EXISTS idx_npcs_occupation ON npcs(occupation);
        CREATE INDEX IF NOT EXISTS idx_npcs_tier ON npcs(data_tier);
        CREATE INDEX IF NOT EXISTS idx_npcs_name ON npcs(name);
    ",
    )
    .context("failed to create schema")?;
    Ok(())
}

fn generate_world(conn: &Connection, counties: &[String]) -> Result<()> {
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

fn generate_parish(conn: &Connection, parish: &str, pop: u32, seed: Option<u64>) -> Result<()> {
    let county_id: i64 = conn
        .query_row("SELECT id FROM counties ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .optional()?
        .unwrap_or_else(|| {
            conn.execute("INSERT INTO counties(name) VALUES ('roscommon')", [])
                .expect("inserting default county should succeed");
            conn.last_insert_rowid()
        });

    conn.execute(
        "INSERT OR IGNORE INTO parishes(county_id, name) VALUES (?, ?)",
        params![county_id, parish],
    )?;
    let parish_id: i64 = conn.query_row(
        "SELECT id FROM parishes WHERE name = ?",
        params![parish],
        |r| r.get(0),
    )?;

    let mut rng = StdRng::seed_from_u64(seed.unwrap_or(42));
    let household_count = (pop / 6).max(1);
    let now_year = 1820_i64;

    for i in 0..household_count {
        conn.execute(
            "INSERT INTO households(parish_id, name) VALUES (?, ?)",
            params![parish_id, format!("{} Household {}", parish, i + 1)],
        )?;
        let household_id = conn.last_insert_rowid();
        let members = rng.gen_range(4..=8);
        for _ in 0..members {
            let female = rng.gen_bool(0.5);
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
            let age: i64 = rng.gen_range(0..=85);
            let birth_year = now_year - age;
            let occupation = weighted_occupation(&mut rng);
            conn.execute(
                "INSERT INTO npcs(name, sex, birth_year, age, parish_id, household_id, occupation, data_tier, mood) VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)",
                params![name, if female {"female"} else {"male"}, birth_year, age, parish_id, household_id, occupation, "neutral"],
            )?;
        }
    }

    let mut stmt = conn.prepare("SELECT id FROM npcs WHERE parish_id = ?")?;
    let npc_ids: Vec<i64> = stmt
        .query_map(params![parish_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in &npc_ids {
        for _ in 0..2 {
            if let Some(other) = npc_ids.choose(&mut rng)
                && other != id
            {
                let strength: f64 = rng.gen_range(-0.2..0.9);
                conn.execute(
                    "INSERT OR IGNORE INTO npc_relationships(from_npc_id, to_npc_id, kind, strength) VALUES (?, ?, ?, ?)",
                    params![id, other, "Acquaintance", strength],
                )?;
            }
        }
    }

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM npcs WHERE parish_id = ?",
        params![parish_id],
        |r| r.get(0),
    )?;
    println!("Generated parish '{}' with {} sketched NPCs", parish, count);
    Ok(())
}

fn weighted_occupation(rng: &mut StdRng) -> &'static str {
    let roll: u8 = rng.gen_range(0..100);
    let mut acc = 0_u8;
    for (occ, weight) in OCCUPATIONS {
        acc = acc.saturating_add(*weight);
        if roll < acc {
            return occ;
        }
    }
    "Other"
}

fn list_npcs(
    conn: &Connection,
    parish: Option<&str>,
    occupation: Option<&str>,
    tier: Option<DataTier>,
    limit: u32,
) -> Result<()> {
    let mut sql = "
        SELECT n.id, n.name, p.name, n.occupation, n.data_tier
        FROM npcs n JOIN parishes p ON p.id = n.parish_id
        WHERE 1=1"
        .to_string();
    let mut bind: Vec<String> = Vec::new();

    if let Some(p) = parish {
        sql.push_str(" AND p.name = ?");
        bind.push(p.to_string());
    }
    if let Some(o) = occupation {
        sql.push_str(" AND n.occupation = ?");
        bind.push(o.to_string());
    }
    if let Some(t) = tier {
        sql.push_str(" AND n.data_tier = ?");
        bind.push(t.as_i64().to_string());
    }
    sql.push_str(" ORDER BY n.id LIMIT ?");
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

fn show_npc(conn: &Connection, npc_id: i64) -> Result<()> {
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

fn search_npcs(conn: &Connection, query: &str, limit: u32) -> Result<()> {
    let like = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "
        SELECT n.id, n.name, p.name, n.occupation
        FROM npcs n JOIN parishes p ON p.id = n.parish_id
        WHERE n.name LIKE ?
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

fn edit_npc(
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

fn promote_npc(conn: &Connection, npc_id: i64) -> Result<()> {
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

fn elaborate_parish(conn: &Connection, parish: &str, batch: u32) -> Result<()> {
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

fn validate_db(conn: &Connection, parish: Option<String>, all: bool) -> Result<()> {
    if parish.is_some() && all {
        bail!("choose either --parish or --all");
    }

    let parish_filter = parish.as_deref().map(|name| {
        format!(
            "n.parish_id = (SELECT id FROM parishes WHERE name = '{}')",
            name.replace('"', "")
        )
    });
    let with_filter = |predicate: &str| {
        if let Some(ref filter) = parish_filter {
            format!("SELECT COUNT(*) FROM npcs n WHERE {filter} AND ({predicate})")
        } else {
            format!("SELECT COUNT(*) FROM npcs n WHERE {predicate}")
        }
    };

    let missing_households: i64 = conn.query_row(
        &with_filter("household_id IS NULL OR household_id NOT IN (SELECT id FROM households)"),
        [],
        |r| r.get(0),
    )?;
    let invalid_age: i64 =
        conn.query_row(&with_filter("age < 0 OR age > 110"), [], |r| r.get(0))?;
    let elaborated_without_personality: i64 = conn.query_row(
        &with_filter("data_tier >= 1 AND (personality IS NULL OR personality = '')"),
        [],
        |r| r.get(0),
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

fn stats(conn: &Connection) -> Result<()> {
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

fn export_npcs(conn: &Connection, parish: Option<&str>) -> Result<()> {
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
        stmt.query_map(params![p], mapper)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], mapper)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let blob = ExportBlob { npcs };
    println!("{}", serde_json::to_string_pretty(&blob)?);
    Ok(())
}

fn import_npcs(conn: &Connection) -> Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let blob: ExportBlob = serde_json::from_str(&input).context("invalid JSON input")?;

    let tx = conn.unchecked_transaction()?;
    let mut inserted = 0u64;
    let mut updated = 0u64;
    for npc in blob.npcs {
        tx.execute(
            "INSERT OR IGNORE INTO parishes(county_id, name) VALUES ((SELECT id FROM counties LIMIT 1), ?)",
            params![npc.parish],
        )?;
        let parish_id: i64 = tx.query_row(
            "SELECT id FROM parishes WHERE name = ?",
            params![npc.parish],
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
                1820 - npc.age,
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
    println!(
        "Imported NPCs from stdin: inserted {inserted}, updated {updated} (household_id and other non-export columns preserved on updates)"
    );
    Ok(())
}

fn family_tree(conn: &Connection, npc_id: i64) -> Result<()> {
    let (household_id, target_name, target_age): (i64, String, i64) = conn
        .query_row(
            "SELECT household_id, name, age FROM npcs WHERE id = ?",
            params![npc_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .context("NPC not found")?;

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

fn relationships(conn: &Connection, npc_id: i64) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_bootstrap_and_generation() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
        ensure_schema(&conn).expect("schema should initialize");
        generate_world(&conn, &["roscommon".to_string()]).expect("world generation should work");
        generate_parish(&conn, "Kiltoom", 30, Some(1)).expect("parish generation should work");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM npcs", [], |r| r.get(0))
            .expect("count query should succeed");
        assert!(count > 0);
    }

    #[test]
    fn test_promote_sets_personality() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
        ensure_schema(&conn).expect("schema should initialize");
        generate_world(&conn, &["roscommon".to_string()]).expect("world generation should work");
        generate_parish(&conn, "Kiltoom", 20, Some(2)).expect("parish generation should work");

        let npc_id: i64 = conn
            .query_row("SELECT id FROM npcs ORDER BY id LIMIT 1", [], |r| r.get(0))
            .expect("must have one NPC");
        promote_npc(&conn, npc_id).expect("promotion should succeed");

        let (tier, personality): (i64, Option<String>) = conn
            .query_row(
                "SELECT data_tier, personality FROM npcs WHERE id = ?",
                params![npc_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("should read promoted NPC");
        assert_eq!(tier, 1);
        assert!(personality.is_some());
    }

    #[test]
    fn test_validate_detects_missing_personality() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
        ensure_schema(&conn).expect("schema should initialize");
        generate_world(&conn, &["roscommon".to_string()]).expect("world generation should work");
        generate_parish(&conn, "Kiltoom", 20, Some(3)).expect("parish generation should work");

        conn.execute(
            "UPDATE npcs SET data_tier = 1, personality = '' WHERE id = (SELECT id FROM npcs LIMIT 1)",
            [],
        )
        .expect("update should succeed");

        let result = validate_db(&conn, None, true);
        assert!(result.is_err());
    }

    // ── #436 import preserves non-export columns + sex round-trips ──────────

    /// Seeds one NPC with a known sex and household_id, then simulates
    /// the import path on a blob that represents re-importing that NPC
    /// with updated personality. household_id must survive untouched and
    /// sex must come from the blob (not hard-coded 'unknown').
    #[test]
    fn test_import_preserves_household_and_restores_sex() {
        let conn = Connection::open_in_memory().expect("in-memory SQLite should open");
        ensure_schema(&conn).expect("schema should initialize");
        generate_world(&conn, &["roscommon".to_string()]).expect("world generation should work");

        let parish_id: i64 = conn
            .query_row("SELECT id FROM parishes LIMIT 1", [], |r| r.get(0))
            .ok()
            .unwrap_or_else(|| {
                conn.execute(
                    "INSERT INTO parishes(county_id, name) VALUES ((SELECT id FROM counties LIMIT 1), 'Testshire')",
                    [],
                )
                .unwrap();
                conn.last_insert_rowid()
            });

        // Insert a household so we have a non-NULL household_id to preserve.
        conn.execute(
            "INSERT INTO households(parish_id, name) VALUES (?, 'Darcy')",
            params![parish_id],
        )
        .unwrap();
        let household_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO npcs(id, name, sex, birth_year, age, parish_id, household_id, occupation, data_tier, mood)\n             VALUES (42, 'Pádraig Darcy', 'male', 1762, 58, ?, ?, 'Publican', 1, 'content')",
            params![parish_id, household_id],
        )
        .unwrap();

        // Build an import blob that updates personality but carries the
        // NPC's existing id. `sex` is present (no longer hard-coded).
        // parish is reused so we don't hit unrelated lookup paths.
        let parish_name: String = conn
            .query_row(
                "SELECT name FROM parishes WHERE id = ?",
                params![parish_id],
                |r| r.get(0),
            )
            .unwrap();
        let blob = ExportBlob {
            npcs: vec![ExportNpc {
                id: 42,
                name: "Pádraig Darcy".to_string(),
                sex: "male".to_string(),
                age: 58,
                parish: parish_name,
                occupation: "Publican".to_string(),
                data_tier: 1,
                mood: Some("content".to_string()),
                personality: Some("Warm-hearted publican.".to_string()),
            }],
        };

        // Call the import path directly (same SQL as import_npcs, but
        // without reading stdin). We replicate the minimal work here
        // because import_npcs reads stdin and that's awkward to fake
        // cleanly in a unit test.
        let tx = conn.unchecked_transaction().unwrap();
        for npc in blob.npcs {
            let pid: i64 = tx
                .query_row(
                    "SELECT id FROM parishes WHERE name = ?",
                    params![npc.parish],
                    |r| r.get(0),
                )
                .unwrap();
            tx.execute(
                "INSERT INTO npcs(id, name, sex, birth_year, age, parish_id, occupation, data_tier, mood, personality)\n                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)\n                 ON CONFLICT(id) DO UPDATE SET\n                     name        = excluded.name,\n                     sex         = excluded.sex,\n                     birth_year  = excluded.birth_year,\n                     age         = excluded.age,\n                     parish_id   = excluded.parish_id,\n                     occupation  = excluded.occupation,\n                     data_tier   = excluded.data_tier,\n                     mood        = excluded.mood,\n                     personality = excluded.personality",
                params![
                    npc.id,
                    npc.name,
                    npc.sex,
                    1820 - npc.age,
                    npc.age,
                    pid,
                    npc.occupation,
                    npc.data_tier,
                    npc.mood,
                    npc.personality
                ],
            )
            .unwrap();
        }
        tx.commit().unwrap();

        // household_id must still be set (would be NULL if INSERT OR
        // REPLACE were used — that was the #436 regression).
        let (hh, sex, personality): (Option<i64>, String, Option<String>) = conn
            .query_row(
                "SELECT household_id, sex, personality FROM npcs WHERE id = 42",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(hh, Some(household_id), "household_id must survive import");
        assert_eq!(sex, "male", "sex must come from the blob, not 'unknown'");
        assert_eq!(
            personality.as_deref(),
            Some("Warm-hearted publican."),
            "personality must update on import",
        );
    }

    /// A blob serialized *before* #436 (no `sex` field) must still
    /// deserialize cleanly, defaulting to "unknown" — so we don't
    /// break users with saved export files from earlier versions.
    #[test]
    fn test_export_blob_deserializes_legacy_missing_sex() {
        let legacy = r#"{
            "npcs": [{
                "id": 1,
                "name": "Legacy Mary",
                "age": 40,
                "parish": "Kiltoom",
                "occupation": "Servant",
                "data_tier": 0,
                "mood": null,
                "personality": null
            }]
        }"#;
        let blob: ExportBlob = serde_json::from_str(legacy).expect("legacy blob should parse");
        assert_eq!(blob.npcs.len(), 1);
        assert_eq!(blob.npcs[0].sex, "unknown");
    }
}
