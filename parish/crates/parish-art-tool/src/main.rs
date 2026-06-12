//! `parish-art-tool` — developer-side asset pipeline for the Parish Diorama.
//!
// Public items in the submodules (manifest, postprocess, export, providers,
// prompt) are part of the tool's documented API surface and are exercised by
// unit tests.  The binary does not call every item from `main` — many are
// invoked via tests or reserved for future subcommands.  The `dead_code`
// allowance is justified here rather than scattered across each module.
#![allow(dead_code)]
//!
//! Generates, curates, post-processes, and exports AI-generated pixel-art
//! plates and sprites for the Rundale diorama scene system.
//!
//! ## Workflow
//!
//! 1. `parish-art-tool init` — create `art/style-bible.md` + empty manifest.
//! 2. `parish-art-tool gen-plate <id>` — generate candidate plates (requires
//!    `OPENAI_API_KEY` or `GEMINI_API_KEY`; dry-runs without a key).
//! 3. `parish-art-tool list --pending` — review the queue.
//! 4. `parish-art-tool review <id>` — inspect a specific candidate.
//! 5. `parish-art-tool accept <id>` — mark a candidate as accepted.
//! 6. `parish-art-tool reject <id> --reason "..."` — reject with a reason.
//!
//! ## Providers
//!
//! - `openai` (default): `gpt-image-1` via `OPENAI_API_KEY`
//! - `google`: Imagen 3 via `GEMINI_API_KEY`

mod export;
mod manifest;
mod postprocess;
mod prompt;
mod providers;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use manifest::{ArtManifest, AssetKind, AssetRecord, AssetStatus};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "parish-art-tool")]
#[command(about = "Developer-side art pipeline for the Parish Diorama")]
struct Cli {
    /// Repository root directory (defaults to the current working directory).
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialise the art pipeline: create art/style-bible.md + art/manifest.json.
    Init,

    /// Generate a background plate for a location.
    GenPlate {
        /// Location id from world.json.
        location_id: u32,
        /// Image provider ("openai" or "google").
        #[arg(long, default_value = "openai")]
        provider: String,
        /// Reference image paths (anchors) to pass to the provider.
        #[arg(long = "ref", num_args = 1..)]
        reference_images: Vec<PathBuf>,
        /// Number of candidates to generate.
        #[arg(long, default_value_t = 1)]
        n: u32,
        /// Dry-run: print the prompt and write a manifest entry without calling
        /// the API.  Active automatically when no API key is present.
        #[arg(long)]
        dry_run: bool,
    },

    /// Generate an NPC character sprite.
    GenSprite {
        /// NPC id from npcs.json.
        npc_id: u32,
        /// Image provider.
        #[arg(long, default_value = "openai")]
        provider: String,
        /// Reference images for style consistency.
        #[arg(long = "ref", num_args = 1..)]
        reference_images: Vec<PathBuf>,
        /// Number of candidates.
        #[arg(long, default_value_t = 1)]
        n: u32,
        /// Dry-run.
        #[arg(long)]
        dry_run: bool,
    },

    /// Generate a variant (e.g. night) of an existing plate.
    GenVariant {
        /// Location id.
        location_id: u32,
        /// Variant name (e.g. "night").
        #[arg(long)]
        variant: String,
        /// Reference images for style consistency.
        #[arg(long = "ref", num_args = 1..)]
        reference_images: Vec<PathBuf>,
        /// Image provider.
        #[arg(long, default_value = "openai")]
        provider: String,
        /// Number of candidates.
        #[arg(long, default_value_t = 1)]
        n: u32,
        /// Dry-run.
        #[arg(long)]
        dry_run: bool,
    },

    /// List all manifest entries.
    List {
        /// Show only pending entries.
        #[arg(long, conflicts_with = "accepted")]
        pending: bool,
        /// Show only accepted entries.
        #[arg(long, conflicts_with = "pending")]
        accepted: bool,
    },

    /// Show all details for a single asset.
    Review {
        /// Asset id (e.g. "plate-2-001").
        asset_id: String,
    },

    /// Accept a pending asset.
    Accept {
        /// Asset id to accept.
        asset_id: String,
        /// Optional cleanup note.
        #[arg(long)]
        note: Option<String>,
    },

    /// Reject a pending asset.
    Reject {
        /// Asset id to reject.
        asset_id: String,
        /// Rejection reason (required).
        #[arg(long)]
        reason: String,
    },

    /// Post-process and export an accepted asset to the mod's scenes directory.
    Export {
        /// Asset id (must be accepted).
        asset_id: String,
        /// Destination path relative to the project root, e.g.
        /// "mods/rundale/assets/scenes/darcys-pub/plate.png".
        #[arg(long)]
        dest: String,
        /// Whether to mark this asset as an anchor (style reference for future
        /// generations).
        #[arg(long, default_value_t = false)]
        anchor: bool,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli
        .root
        .unwrap_or_else(|| std::env::current_dir().expect("current dir"));

    match cli.command {
        Command::Init => cmd_init(&root),
        Command::GenPlate {
            location_id,
            provider,
            reference_images,
            n,
            dry_run,
        } => cmd_gen_plate(&root, location_id, &provider, reference_images, n, dry_run),
        Command::GenSprite {
            npc_id,
            provider,
            reference_images,
            n,
            dry_run,
        } => cmd_gen_sprite(&root, npc_id, &provider, reference_images, n, dry_run),
        Command::GenVariant {
            location_id,
            variant,
            reference_images,
            provider,
            n,
            dry_run,
        } => cmd_gen_variant(
            &root,
            location_id,
            &variant,
            &provider,
            reference_images,
            n,
            dry_run,
        ),
        Command::List { pending, accepted } => cmd_list(&root, pending, accepted),
        Command::Review { asset_id } => cmd_review(&root, &asset_id),
        Command::Accept { asset_id, note } => cmd_accept(&root, &asset_id, note),
        Command::Reject { asset_id, reason } => cmd_reject(&root, &asset_id, reason),
        Command::Export {
            asset_id,
            dest,
            anchor,
        } => cmd_export(&root, &asset_id, &dest, anchor),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn manifest_path(root: &std::path::Path) -> PathBuf {
    root.join("art").join("manifest.json")
}

fn timestamp_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn next_asset_id(manifest: &ArtManifest, prefix: &str) -> String {
    let count = manifest
        .assets
        .iter()
        .filter(|r| r.id.starts_with(prefix))
        .count();
    format!("{}-{:03}", prefix, count + 1)
}

fn load_world_graph(root: &std::path::Path) -> Result<parish_world::graph::WorldGraph> {
    let world_path = root.join("mods/rundale/world.json");
    parish_world::graph::WorldGraph::load_from_file(&world_path)
        .with_context(|| format!("failed to load world.json from {}", world_path.display()))
}

fn load_npcs(root: &std::path::Path) -> Result<Vec<parish_npc::Npc>> {
    let npcs_path = root.join("mods/rundale/npcs.json");
    let json = std::fs::read_to_string(&npcs_path)
        .with_context(|| format!("failed to read {}", npcs_path.display()))?;
    parish_npc::data::load_npcs_from_str(&json)
        .with_context(|| "failed to parse npcs.json".to_string())
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

fn cmd_init(root: &std::path::Path) -> Result<()> {
    let art_dir = root.join("art");
    std::fs::create_dir_all(&art_dir)?;

    let bible_path = art_dir.join("style-bible.md");
    if bible_path.exists() {
        println!("style-bible.md already exists — skipping.");
    } else {
        let template = include_str!("../../../../art/style-bible.md");
        std::fs::write(&bible_path, template)?;
        println!("created {}", bible_path.display());
    }

    let manifest_path = art_dir.join("manifest.json");
    if manifest_path.exists() {
        println!("manifest.json already exists — skipping.");
    } else {
        let manifest = ArtManifest::default();
        manifest.save(&manifest_path)?;
        println!("created {}", manifest_path.display());
    }

    println!("art pipeline initialised. Set OPENAI_API_KEY or GEMINI_API_KEY to generate assets.");
    Ok(())
}

fn cmd_gen_plate(
    root: &std::path::Path,
    location_id: u32,
    provider_name: &str,
    _reference_images: Vec<PathBuf>,
    _n: u32,
    dry_run: bool,
) -> Result<()> {
    let graph = load_world_graph(root)?;
    let loc = graph
        .get(parish_world::LocationId(location_id))
        .ok_or_else(|| anyhow::anyhow!("location id {} not found in world.json", location_id))?;

    let bible_path = root.join("art").join("style-bible.md");
    let style_bible = if bible_path.exists() {
        std::fs::read_to_string(&bible_path)?
    } else {
        prompt::load_style_bible(root)?
    };

    let full_prompt = prompt::build_plate_prompt(&style_bible, loc);

    let manifest_p = manifest_path(root);
    let mut manifest = ArtManifest::load(&manifest_p)?;
    let prefix = format!("plate-{location_id}");
    let asset_id = next_asset_id(&manifest, &prefix);
    let output_path = format!("art/{asset_id}.png");

    let record = AssetRecord {
        id: asset_id.clone(),
        kind: AssetKind::Plate,
        target_id: location_id,
        variant: None,
        provider: provider_name.to_string(),
        model: model_for(provider_name),
        prompt: full_prompt.clone(),
        reference_images: vec![],
        created: timestamp_now(),
        status: AssetStatus::Pending,
        anchor: false,
        cleanup_notes: None,
        reject_reason: None,
        output_path: Some(output_path.clone()),
        exported_path: None,
    };

    // Always write the manifest entry first.
    manifest.add_and_save(record, &manifest_p)?;
    println!("manifest entry created: {asset_id}");
    println!();
    println!("--- PROMPT ---");
    println!("{full_prompt}");
    println!("--- END PROMPT ---");

    if dry_run || no_api_key_for(provider_name) {
        println!();
        println!(
            "[dry-run] no API call made (dry_run={dry_run}, key_present={}).",
            !no_api_key_for(provider_name)
        );
        println!(
            "Run `parish-art-tool accept {asset_id}` after placing the image at {output_path}."
        );
    } else {
        println!("[live] API generation would run here (not implemented in M4).");
    }

    Ok(())
}

fn cmd_gen_sprite(
    root: &std::path::Path,
    npc_id: u32,
    provider_name: &str,
    _reference_images: Vec<PathBuf>,
    _n: u32,
    dry_run: bool,
) -> Result<()> {
    let npcs = load_npcs(root)?;
    let npc = npcs
        .iter()
        .find(|n| n.id.0 == npc_id)
        .ok_or_else(|| anyhow::anyhow!("NPC id {} not found in npcs.json", npc_id))?;

    let bible_path = root.join("art").join("style-bible.md");
    let style_bible = if bible_path.exists() {
        std::fs::read_to_string(&bible_path)?
    } else {
        prompt::load_style_bible(root)?
    };

    let full_prompt = prompt::build_sprite_prompt(&style_bible, npc);

    let manifest_p = manifest_path(root);
    let mut manifest = ArtManifest::load(&manifest_p)?;
    let prefix = format!("sprite-{npc_id}");
    let asset_id = next_asset_id(&manifest, &prefix);
    let output_path = format!("art/{asset_id}.png");

    let record = AssetRecord {
        id: asset_id.clone(),
        kind: AssetKind::Sprite,
        target_id: npc_id,
        variant: None,
        provider: provider_name.to_string(),
        model: model_for(provider_name),
        prompt: full_prompt.clone(),
        reference_images: vec![],
        created: timestamp_now(),
        status: AssetStatus::Pending,
        anchor: false,
        cleanup_notes: None,
        reject_reason: None,
        output_path: Some(output_path.clone()),
        exported_path: None,
    };

    manifest.add_and_save(record, &manifest_p)?;
    println!("manifest entry created: {asset_id}");
    println!();
    println!("--- PROMPT ---");
    println!("{full_prompt}");
    println!("--- END PROMPT ---");

    if dry_run || no_api_key_for(provider_name) {
        println!();
        println!(
            "[dry-run] no API call made (dry_run={dry_run}, key_present={}).",
            !no_api_key_for(provider_name)
        );
    }

    Ok(())
}

fn cmd_gen_variant(
    root: &std::path::Path,
    location_id: u32,
    variant: &str,
    provider_name: &str,
    _reference_images: Vec<PathBuf>,
    _n: u32,
    dry_run: bool,
) -> Result<()> {
    let graph = load_world_graph(root)?;
    let loc = graph
        .get(parish_world::LocationId(location_id))
        .ok_or_else(|| anyhow::anyhow!("location id {} not found", location_id))?;

    let bible_path = root.join("art").join("style-bible.md");
    let style_bible = if bible_path.exists() {
        std::fs::read_to_string(&bible_path)?
    } else {
        prompt::load_style_bible(root)?
    };

    let full_prompt = prompt::build_variant_prompt(&style_bible, loc, variant, None);

    let manifest_p = manifest_path(root);
    let mut manifest = ArtManifest::load(&manifest_p)?;
    let prefix = format!("variant-{location_id}-{variant}");
    let asset_id = next_asset_id(&manifest, &prefix);
    let output_path = format!("art/{asset_id}.png");

    let record = AssetRecord {
        id: asset_id.clone(),
        kind: AssetKind::Variant,
        target_id: location_id,
        variant: Some(variant.to_string()),
        provider: provider_name.to_string(),
        model: model_for(provider_name),
        prompt: full_prompt.clone(),
        reference_images: vec![],
        created: timestamp_now(),
        status: AssetStatus::Pending,
        anchor: false,
        cleanup_notes: None,
        reject_reason: None,
        output_path: Some(output_path),
        exported_path: None,
    };

    manifest.add_and_save(record, &manifest_p)?;
    println!("manifest entry created: {asset_id}");
    println!();
    println!("--- PROMPT ---");
    println!("{full_prompt}");
    println!("--- END PROMPT ---");

    if dry_run || no_api_key_for(provider_name) {
        println!();
        println!("[dry-run] no API call made.");
    }

    Ok(())
}

fn cmd_list(root: &std::path::Path, pending_only: bool, accepted_only: bool) -> Result<()> {
    let manifest_p = manifest_path(root);
    let manifest = ArtManifest::load(&manifest_p)?;

    let entries: Vec<_> = manifest
        .assets
        .iter()
        .filter(|r| {
            if pending_only {
                r.status == AssetStatus::Pending
            } else if accepted_only {
                r.status == AssetStatus::Accepted
            } else {
                true
            }
        })
        .collect();

    if entries.is_empty() {
        println!("no entries");
        return Ok(());
    }

    println!(
        "{:<24} {:<8} {:<8} {:<10}",
        "id", "kind", "target", "status"
    );
    println!("{}", "-".repeat(56));
    for r in entries {
        println!(
            "{:<24} {:<8} {:<8} {:<10}",
            r.id, r.kind, r.target_id, r.status
        );
    }
    Ok(())
}

fn cmd_review(root: &std::path::Path, asset_id: &str) -> Result<()> {
    let manifest_p = manifest_path(root);
    let manifest = ArtManifest::load(&manifest_p)?;
    let record = manifest
        .get(asset_id)
        .ok_or_else(|| anyhow::anyhow!("asset '{}' not found in manifest", asset_id))?;

    println!("id:               {}", record.id);
    println!("kind:             {}", record.kind);
    println!("target_id:        {}", record.target_id);
    if let Some(v) = &record.variant {
        println!("variant:          {v}");
    }
    println!("provider:         {}", record.provider);
    println!("model:            {}", record.model);
    println!("created:          {}", record.created);
    println!("status:           {}", record.status);
    println!("anchor:           {}", record.anchor);
    if let Some(p) = &record.output_path {
        println!("output_path:      {p}");
    }
    if let Some(p) = &record.exported_path {
        println!("exported_path:    {p}");
    }
    if !record.reference_images.is_empty() {
        println!("reference_images: {}", record.reference_images.join(", "));
    }
    if let Some(n) = &record.cleanup_notes {
        println!("cleanup_notes:    {n}");
    }
    if let Some(r) = &record.reject_reason {
        println!("reject_reason:    {r}");
    }
    println!();
    println!("--- PROMPT ---");
    println!("{}", record.prompt);
    println!("--- END PROMPT ---");
    Ok(())
}

fn cmd_accept(root: &std::path::Path, asset_id: &str, note: Option<String>) -> Result<()> {
    let manifest_p = manifest_path(root);
    let mut manifest = ArtManifest::load(&manifest_p)?;
    manifest.accept(asset_id, note, &manifest_p)?;
    println!("accepted: {asset_id}");
    Ok(())
}

fn cmd_reject(root: &std::path::Path, asset_id: &str, reason: String) -> Result<()> {
    let manifest_p = manifest_path(root);
    let mut manifest = ArtManifest::load(&manifest_p)?;
    manifest.reject(asset_id, reason, &manifest_p)?;
    println!("rejected: {asset_id}");
    Ok(())
}

fn cmd_export(
    root: &std::path::Path,
    asset_id: &str,
    dest_rel: &str,
    mark_anchor: bool,
) -> Result<()> {
    let manifest_p = manifest_path(root);
    let mut manifest = ArtManifest::load(&manifest_p)?;

    let record = manifest
        .get(asset_id)
        .ok_or_else(|| anyhow::anyhow!("asset '{}' not found", asset_id))?;

    if record.status != AssetStatus::Accepted {
        anyhow::bail!(
            "asset '{}' is not accepted (status: {}) — accept it before exporting",
            asset_id,
            record.status
        );
    }

    let src_path = record
        .output_path
        .as_ref()
        .map(|p| root.join(p))
        .ok_or_else(|| anyhow::anyhow!("asset '{}' has no output_path", asset_id))?;

    let kind = record.kind.clone();
    let target_id = record.target_id;
    let variant = record.variant.clone();

    // Post-process the raw candidate PNG.
    let raw_bytes = std::fs::read(&src_path)
        .with_context(|| format!("failed to read source image {}", src_path.display()))?;

    let processed = match kind {
        AssetKind::Plate | AssetKind::Variant => postprocess::downscale_plate(&raw_bytes)?,
        AssetKind::Sprite => postprocess::downscale_sprite(&raw_bytes)?,
    };

    // Write the post-processed PNG to a temp path.
    let tmp_path = src_path.with_extension("processed.png");
    std::fs::write(&tmp_path, &processed)?;

    // Export to the mod's scenes directory.
    let dest = export::export_asset(root, &tmp_path, dest_rel)?;
    std::fs::remove_file(&tmp_path)?;

    // Upsert the path in scenes.json.
    let scenes_json = root.join("mods/rundale/scenes.json");
    let entry_type = match kind {
        AssetKind::Plate => export::SceneEntryType::Plate {
            location_id: target_id,
            slug: slug_from_dest(dest_rel),
        },
        AssetKind::Sprite => export::SceneEntryType::Sprite { npc_id: target_id },
        AssetKind::Variant => export::SceneEntryType::Variant {
            location_id: target_id,
            variant: variant.unwrap_or_else(|| "night".to_string()),
        },
    };
    export::upsert_scenes_json(&scenes_json, entry_type, dest_rel)?;

    // Update manifest: set exported_path and optionally anchor flag.
    if let Some(r) = manifest.get_mut(asset_id) {
        r.exported_path = Some(dest_rel.to_string());
        if mark_anchor {
            r.anchor = true;
        }
    }
    manifest.save(&manifest_p)?;

    println!("exported: {asset_id} → {}", dest.display());
    Ok(())
}

fn slug_from_dest(dest_rel: &str) -> String {
    // Extract slug from e.g. "mods/rundale/assets/scenes/darcys-pub/plate.png"
    dest_rel
        .split('/')
        .rev()
        .nth(1)
        .unwrap_or("unknown")
        .to_string()
}

// ---------------------------------------------------------------------------
// Provider helpers
// ---------------------------------------------------------------------------

fn model_for(provider: &str) -> String {
    match provider {
        "google" => "imagen-3.0-generate-002".to_string(),
        _ => "gpt-image-1".to_string(),
    }
}

fn no_api_key_for(provider: &str) -> bool {
    match provider {
        "google" => std::env::var("GEMINI_API_KEY").is_err(),
        _ => std::env::var("OPENAI_API_KEY").is_err(),
    }
}
