//! Replay determinism for `--script` (#2033).
//!
//! A script must produce the same bytes every run: NPC lists in id order,
//! game time moved only by commands, rolls seeded from game state, and
//! saves stamped with a fixed time. Two runs in one process also get
//! different `HashMap` seeds, so any list still built in hash order fails
//! here.

use std::path::{Path, PathBuf};

use limerick_engine::testing::{GameTestHarness, run_script_to};

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/fixtures");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures dir")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("test_") && name.ends_with(".txt"))
        })
        .collect();
    paths.sort();
    paths
}

fn run(path: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    run_script_to(path, GameTestHarness::new(), &mut out).expect("script runs");
    out
}

#[test]
fn every_script_fixture_replays_byte_identically() {
    let paths = fixtures();
    assert!(
        paths.len() >= 20,
        "expected the fixture set, found {}",
        paths.len()
    );
    let mut differing = Vec::new();
    for path in &paths {
        let first = run(path);
        let second = run(path);
        assert!(!first.is_empty(), "{} produced no output", path.display());
        if first != second {
            differing.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        differing.is_empty(),
        "fixtures that did not replay: {differing:?}"
    );
}
