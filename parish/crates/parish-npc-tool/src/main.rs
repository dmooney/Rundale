mod art_inputs;
mod catalog;
mod db;
mod generate;
mod import_export;
mod query;
mod schema;
mod validate;

// Bring the split command implementations into the crate root so `main()`
// and the `#[cfg(test)] mod tests` (via `use super::*`) keep their existing
// unqualified call sites after the TD-028 module split.
use art_inputs::export_art_inputs;
use db::{open_db, resolve_default_db};
use generate::{elaborate_parish, generate_parish, generate_world};
use import_export::{export_npcs, import_npcs};
use query::{
    edit_npc, family_tree, list_npcs, promote_npc, relationships, search_npcs, show_npc, stats,
};
use schema::ensure_schema;
use validate::validate_db;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

/// Current world year for the simulation. Used to derive `birth_year` from an
/// NPC's age in both `generate_parish` and `import_npcs_inner`; keeping it in
/// one place prevents the two call sites from silently diverging.
pub(crate) const WORLD_YEAR: i64 = 1820;

#[derive(Parser, Debug)]
#[command(name = "parish-npc-tool")]
#[command(about = "NPC world builder and inspection utility")]
struct Cli {
    /// SQLite database path.  Defaults to `data/parish-world.db` anchored at
    /// the project root (or the `PARISH_NPC_TOOL_DB` / `PARISH_DATA_DIR` env
    /// vars — see `resolve_default_db`).
    #[arg(long, global = true)]
    db: Option<PathBuf>,
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
    /// Split a monolithic `npcs.json` catalogue into one file per NPC (TD-001).
    /// These commands operate on JSON files and ignore `--db`.
    SplitCatalog {
        /// Path to the monolithic `npcs.json` to split.
        #[arg(long)]
        input: PathBuf,
        /// Directory to write per-NPC `npc-NNNN-slug.json` files into.
        #[arg(long)]
        out_dir: PathBuf,
    },
    /// Re-join per-NPC files back into a canonical, byte-identical `npcs.json`.
    JoinCatalog {
        /// Directory of per-NPC `*.json` files (output of `split-catalog`).
        #[arg(long)]
        in_dir: PathBuf,
        /// Path to write the re-joined `npcs.json` to.
        #[arg(long)]
        output: PathBuf,
    },
    /// Validate a `npcs.json` catalogue (unique ids, non-empty names, internal
    /// relationship targets resolve). Exits non-zero on the first error.
    ValidateCatalog {
        /// Path to the `npcs.json` to validate.
        #[arg(long)]
        input: PathBuf,
    },
    /// Export generator-ready notebook person-art inputs by merging NPC data,
    /// world/location context, and a reviewed art-direction supplement.
    /// This command operates on JSON files and ignores `--db`.
    ArtInputs {
        /// Path to the source `npcs.json` catalogue.
        #[arg(long)]
        npcs: PathBuf,
        /// Path to the source `world.json` file.
        #[arg(long)]
        world: PathBuf,
        /// Path to the reviewed NPC art-direction supplement.
        #[arg(long)]
        art_direction: PathBuf,
        /// Path to write the merged art-input dataset.
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum DataTier {
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
pub(crate) struct ExportBlob {
    npcs: Vec<ExportNpc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ExportNpc {
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

pub(crate) fn default_sex() -> String {
    "unknown".to_string()
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Catalogue (JSON-file) commands do not touch the SQLite DB — dispatch
    // them before opening it so they work without a `data/parish-world.db`.
    match &cli.command {
        Command::SplitCatalog { input, out_dir } => {
            let n = catalog::split_catalog(input, out_dir)?;
            println!("split {} NPCs into {}", n, out_dir.display());
            return Ok(());
        }
        Command::JoinCatalog { in_dir, output } => {
            let n = catalog::join_catalog(in_dir, output)?;
            println!("joined {} NPCs into {}", n, output.display());
            return Ok(());
        }
        Command::ValidateCatalog { input } => {
            let file = catalog::load_catalog(input)?;
            println!("ok: {} NPCs, no integrity errors", file.npcs.len());
            return Ok(());
        }
        Command::ArtInputs {
            npcs,
            world,
            art_direction,
            output,
        } => {
            let n = export_art_inputs(npcs, world, art_direction, output)?;
            println!("wrote {} NPC art inputs to {}", n, output.display());
            return Ok(());
        }
        _ => {}
    }

    let db_path = cli.db.unwrap_or_else(resolve_default_db);
    let conn = open_db(&db_path)?;

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
        // Handled before the DB was opened (early `return` above).
        Command::SplitCatalog { .. }
        | Command::JoinCatalog { .. }
        | Command::ValidateCatalog { .. }
        | Command::ArtInputs { .. } => unreachable!("file commands dispatched earlier"),
    }
}

#[cfg(test)]
mod tests;
