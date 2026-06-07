//! NPC catalogue (`npcs.json`) split / join / validate tooling.
//!
//! `mods/rundale/npcs.json` is a single ~175 KB file (TD-001). Editing one NPC
//! risks large review diffs, field-order churn, and cascading id mistakes.
//! This module provides per-NPC source files plus a canonical re-join that
//! reproduces the checked-in `npcs.json` **byte-for-byte**, so authors can edit
//! a single small file and regenerate the monolith without diff noise.
//!
//! Byte-identity is guaranteed by:
//! * the typed [`NpcFile`] / [`NpcFileEntry`] schema (fixed field order — a
//!   `serde_json::Value` round-trip would shuffle keys without `preserve_order`),
//!   and
//! * [`write_catalog_deterministic`], a 4-space `PrettyFormatter` + trailing
//!   newline that matches the `mods/rundale/*.json` on-disk convention (the same
//!   format the Parish Designer editor uses).

use std::path::Path;

use anyhow::{Context, Result, bail};
use parish_npc::data::{NpcFile, NpcFileEntry};
use serde::Serialize;
use serde_json::ser::{PrettyFormatter, Serializer};

/// Serializes `value` as 4-space-indent JSON with a trailing newline, matching
/// the canonical `mods/rundale/*.json` format.
pub fn to_canonical_json<T: Serialize>(value: &T) -> Result<String> {
    let mut buf = Vec::with_capacity(4096);
    let formatter = PrettyFormatter::with_indent(b"    ");
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser).context("serialize JSON")?;
    let mut body = String::from_utf8(buf).context("JSON output is not UTF-8")?;
    body.push('\n');
    Ok(body)
}

/// Writes `value` to `path` as canonical JSON.
fn write_catalog_deterministic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let body = to_canonical_json(value)?;
    std::fs::write(path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Per-NPC source filename: `npc-NNNN-slug.json` (zero-padded id keeps the
/// directory listing sorted in id order).
fn npc_filename(entry: &NpcFileEntry) -> String {
    format!("npc-{:04}-{}.json", entry.id, slugify(&entry.name))
}

/// Lower-cases, replaces non-alphanumerics with `-`, and collapses runs.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

/// Loads and validates the monolithic catalogue, returning the typed model.
pub fn load_catalog(input: &Path) -> Result<NpcFile> {
    let raw = std::fs::read_to_string(input)
        .with_context(|| format!("read NPC catalogue {}", input.display()))?;
    let file: NpcFile = serde_json::from_str(&raw)
        .with_context(|| format!("parse NPC catalogue {}", input.display()))?;
    validate_entries(&file)?;
    Ok(file)
}

/// Structural validation of the catalogue: unique ids, non-empty names, and
/// reciprocal-friendly relationship targets (every relationship target id must
/// be an id that exists in the catalogue). Cross-file world references are
/// validated separately by the `rundale_data_consistency` test in `parish-npc`.
pub fn validate_entries(file: &NpcFile) -> Result<()> {
    use std::collections::HashSet;
    let mut seen: HashSet<u32> = HashSet::new();
    let ids: HashSet<u32> = file.npcs.iter().map(|e| e.id).collect();
    for entry in &file.npcs {
        if !seen.insert(entry.id) {
            bail!("duplicate NPC id: {}", entry.id);
        }
        if entry.name.trim().is_empty() {
            bail!("NPC id {} has an empty name", entry.id);
        }
        for rel in &entry.relationships {
            if rel.target_id == entry.id {
                bail!("NPC id {} relates to itself", entry.id);
            }
            if !ids.contains(&rel.target_id) {
                bail!(
                    "NPC id {} ({}) relates to non-existent NPC id {}",
                    entry.id,
                    entry.name,
                    rel.target_id
                );
            }
        }
    }
    Ok(())
}

/// Splits the monolithic catalogue at `input` into one file per NPC under
/// `out_dir` (created if missing). Each per-NPC file is canonical JSON of a
/// single [`NpcFileEntry`]. Returns the number of NPC files written.
pub fn split_catalog(input: &Path, out_dir: &Path) -> Result<usize> {
    let file = load_catalog(input)?;
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("create split dir {}", out_dir.display()))?;
    for entry in &file.npcs {
        let path = out_dir.join(npc_filename(entry));
        write_catalog_deterministic(&path, entry)?;
    }
    Ok(file.npcs.len())
}

/// Re-joins per-NPC files under `in_dir` into a single canonical catalogue
/// written to `output`.
///
/// NPCs are ordered by ascending id so the output is independent of directory
/// iteration order — and matches the id-ascending order of the checked-in
/// `mods/rundale/npcs.json`, which is what makes the round-trip byte-identical.
pub fn join_catalog(in_dir: &Path, output: &Path) -> Result<usize> {
    let mut entries: Vec<NpcFileEntry> = Vec::new();
    let read_dir = std::fs::read_dir(in_dir)
        .with_context(|| format!("read split dir {}", in_dir.display()))?;
    for dirent in read_dir {
        let path = dirent?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let entry: NpcFileEntry =
            serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
        entries.push(entry);
    }
    entries.sort_by_key(|e| e.id);
    let file = NpcFile { npcs: entries };
    validate_entries(&file)?;
    write_catalog_deterministic(output, &file)?;
    Ok(file.npcs.len())
}

/// Convenience for tests: produce the canonical catalogue bytes from a typed
/// model (so a round-trip can be byte-compared without disk I/O).
#[cfg(test)]
fn canonical_catalog_bytes(file: &NpcFile) -> Result<String> {
    to_canonical_json(file)
}

/// Returns the split per-NPC source filename for an entry (exposed for tests).
#[cfg(test)]
fn entry_filename(entry: &NpcFileEntry) -> String {
    npc_filename(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("repo root via ../../..")
    }

    fn rundale_npcs() -> PathBuf {
        repo_root().join("mods/rundale/npcs.json")
    }

    // AC-9 / AC-10: parsing the real catalogue and re-serializing it with the
    // canonical formatter reproduces the on-disk bytes exactly.
    #[test]
    fn canonical_reserialize_is_byte_identical() {
        let path = rundale_npcs();
        let original = std::fs::read_to_string(&path).expect("read npcs.json");
        let file: NpcFile = serde_json::from_str(&original).expect("parse npcs.json");
        let round = canonical_catalog_bytes(&file).expect("serialize");
        assert_eq!(
            round, original,
            "canonical re-serialize must reproduce mods/rundale/npcs.json byte-for-byte"
        );
    }

    // AC-9 / AC-10: split → join reproduces the original catalogue byte-for-byte.
    #[test]
    fn split_then_join_round_trips_byte_identical() {
        let src = rundale_npcs();
        let original = std::fs::read_to_string(&src).expect("read npcs.json");

        let tmp = tempfile::tempdir().unwrap();
        let split_dir = tmp.path().join("npcs");
        let rejoined = tmp.path().join("npcs.json");

        let n_split = split_catalog(&src, &split_dir).expect("split");
        let n_join = join_catalog(&split_dir, &rejoined).expect("join");
        assert_eq!(n_split, n_join, "split/join NPC counts must match");

        let produced = std::fs::read_to_string(&rejoined).expect("read rejoined");
        assert_eq!(
            produced, original,
            "split→join must reproduce npcs.json byte-for-byte"
        );
    }

    // AC-9: split produces one file per NPC, named by id+slug.
    #[test]
    fn split_writes_one_file_per_npc() {
        let src = rundale_npcs();
        let file = load_catalog(&src).expect("load");
        let tmp = tempfile::tempdir().unwrap();
        let split_dir = tmp.path().join("npcs");
        split_catalog(&src, &split_dir).expect("split");

        for entry in &file.npcs {
            let p = split_dir.join(entry_filename(entry));
            assert!(p.is_file(), "missing per-NPC file {}", p.display());
        }
        let count = std::fs::read_dir(&split_dir).unwrap().count();
        assert_eq!(count, file.npcs.len());
    }

    // Validation rejects a duplicate id.
    #[test]
    fn validate_rejects_duplicate_id() {
        let json = r#"{"npcs":[
            {"id":1,"name":"A","age":30,"occupation":"X","personality":"p","home":1,"mood":"m","relationships":[]},
            {"id":1,"name":"B","age":31,"occupation":"Y","personality":"q","home":1,"mood":"n","relationships":[]}
        ]}"#;
        let file: NpcFile = serde_json::from_str(json).unwrap();
        let err = validate_entries(&file).unwrap_err().to_string();
        assert!(err.contains("duplicate NPC id"), "{err}");
    }

    // Validation rejects a relationship to a missing NPC.
    #[test]
    fn validate_rejects_dangling_relationship() {
        let json = r#"{"npcs":[
            {"id":1,"name":"A","age":30,"occupation":"X","personality":"p","home":1,"mood":"m",
             "relationships":[{"target_id":99,"kind":"Friend","strength":0.5}]}
        ]}"#;
        let file: NpcFile = serde_json::from_str(json).unwrap();
        let err = validate_entries(&file).unwrap_err().to_string();
        assert!(err.contains("non-existent NPC id 99"), "{err}");
    }

    #[test]
    fn slugify_handles_apostrophes_and_spaces() {
        assert_eq!(slugify("Niamh O'Brien"), "niamh-o-brien");
        assert_eq!(slugify("Padraig Darcy"), "padraig-darcy");
        assert_eq!(slugify(""), "unnamed");
    }
}
