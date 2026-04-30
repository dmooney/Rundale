//! Destructive one-shot maintenance tool.
//!
//! `normalize_rundale_source` rewrites `mods/rundale/*.json` through the
//! editor's save path. Only run it with `NORMALIZE_RUNDALE=1` to bring a
//! mod's canonical form up to whatever the editor currently produces.
//! Used during Phase 1a bring-up to normalize the source files so the
//! byte-identical round-trip acceptance test holds.

use super::mod_io::load_mod_snapshot;
use super::persist::save_mod;
use super::types::EditorDoc;
use std::path::PathBuf;

#[test]
#[ignore = "destructive: only run with NORMALIZE_RUNDALE=1"]
fn normalize_rundale_source() {
    if std::env::var("NORMALIZE_RUNDALE").is_err() {
        return;
    }
    let src_root = PathBuf::from("../../../mods/rundale");
    if !src_root.exists() {
        panic!("mods/rundale not found");
    }
    let mut snapshot = load_mod_snapshot(&src_root).unwrap();
    save_mod(
        &mut snapshot,
        &[
            EditorDoc::Npcs,
            EditorDoc::World,
            EditorDoc::Festivals,
            EditorDoc::Encounters,
            EditorDoc::Anachronisms,
        ],
    )
    .unwrap();
}
