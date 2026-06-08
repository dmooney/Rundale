//! SQLite schema initialisation (TD-028 split from main.rs).

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Initialises the standalone `parish-npc-tool` SQLite schema.
///
/// # Schema divergence from parish-persistence (#434)
///
/// This schema is **not** compatible with the main game's persistence
/// format in `parish-persistence` (which stores branch-based game
/// snapshots keyed by session id, not relational parish/household/NPC
/// rows). Databases created by `parish-npc-tool` cannot be loaded by the
/// running game, and save files created by the game cannot be opened
/// by `parish-npc-tool` commands.
///
/// That divergence is deliberate: `parish-npc-tool` is a world-*building*
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
/// 1. `parish-npc-tool generate-parish …` populates this schema.
/// 2. `parish-npc-tool export [--parish NAME]` emits the JSON blob the
///    game consumes, which can be hand-massaged into `npcs.json`.
/// 3. The game loads `npcs.json` into its own runtime structures —
///    no direct SQLite interop.
///
/// If you need to hold both formats in sync, treat the parish-npc-tool DB
/// as the source of truth at design time and re-export after every
/// authoring session. A proper conversion utility between this schema
/// and a gameplay save is out of scope; track additions there under
/// #434 follow-ups.
pub(crate) fn ensure_schema(conn: &Connection) -> Result<()> {
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
