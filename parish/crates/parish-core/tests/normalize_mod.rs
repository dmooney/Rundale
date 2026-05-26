use parish_core::editor::types::EditorDoc;
use parish_core::editor::{load_mod_snapshot, save_mod};
use std::path::PathBuf;

#[test]
#[ignore = "pass PARISH_NORMALIZE_MOD_DIR env var to run"]
fn normalize_mod_source_integration() {
    let dir = match std::env::var("PARISH_NORMALIZE_MOD_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => return,
    };
    if !dir.exists() {
        panic!("mod dir not found: {}", dir.display());
    }
    let mut snapshot = load_mod_snapshot(&dir).unwrap();
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
    eprintln!("Normalized: {}", dir.display());
}
