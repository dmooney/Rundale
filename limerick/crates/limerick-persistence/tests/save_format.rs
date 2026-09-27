//! Save format versioning over checked-in saves of every format.
//!
//! Fixtures (`tests/fixtures/`):
//!
//! - `pre_turn_journal_save.db`: format 1, a real `limerick-server` save from
//!   `main` f3e629304 (see `pre_turn_journal_save.rs`).
//! - `turn_journal_v2_save.db`: format 2, a real `limerick-server` save
//!   written by #2037 (2cb2682d9) after the `talk-and-task` proof scenario:
//!   six snapshots, five committed requests and their 24 transcript events.
//! - `future_format_unknown_event_save.db`: the format 2 save as a newer
//!   build would leave it: stamped format 4, an extra `harvest_festivals`
//!   table, a latest snapshot with a field this build does not know
//!   (`harvest_moon`) and a content identity (`rundale` 1.1.0), and a
//!   transcript event of a kind this build does not know
//!   (`harvest_festival`).
//! - `unreadable_state_save.db`: the format 2 save with its latest snapshot's
//!   `player_location` rewritten to a string, as a newer build that changed
//!   the field's type would write it. Its authoritative state cannot be
//!   read.

use std::path::{Path, PathBuf};

use limerick_persistence::{Database, SAVE_FORMAT_VERSION, inspect_save};
use limerick_types::LimerickError;
use rusqlite::Connection;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A copy of fixture `name` in a temporary directory.
fn copy_of(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("limerick_001.db");
    std::fs::copy(fixture(name), &path).unwrap();
    (dir, path)
}

fn user_version(path: &Path) -> u32 {
    Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn every_prior_format_opens_and_migrates_to_the_current_one() {
    for (name, format) in [
        ("pre_turn_journal_save.db", 1),
        ("turn_journal_v2_save.db", 2),
    ] {
        let (_dir, path) = copy_of(name);
        let inspection = inspect_save(&path).unwrap();
        assert_eq!(inspection.format_version, format, "{name}");
        assert_eq!(inspection.stamped_version, 0, "{name}: unmarked");
        assert!(inspection.needs_migration(), "{name}");

        let db = Database::open(&path).unwrap();
        let main = db.find_branch("main").unwrap().expect("main branch");
        let recovery = db.load_recovery_data(main.id).unwrap().expect("a snapshot");
        assert_eq!(recovery.snapshot.content, None, "{name}: no identity yet");
        drop(db);
        assert_eq!(user_version(&path), SAVE_FORMAT_VERSION, "{name}: stamped");
        assert_eq!(inspect_save(&path).unwrap().format_version, SAVE_FORMAT_VERSION);
    }
}

#[test]
fn the_format_2_save_keeps_its_turn_journal() {
    let (_dir, path) = copy_of("turn_journal_v2_save.db");
    let db = Database::open(&path).unwrap();
    let main = db.find_branch("main").unwrap().unwrap();
    assert_eq!(db.turn_requests(main.id).unwrap().len(), 5);
    assert!(db.open_turn_requests(main.id).unwrap().is_empty());
    let events = db.transcript_events(main.id, 0).unwrap();
    assert_eq!(events.len(), 24);
    assert!(events.windows(2).all(|pair| pair[0].sequence < pair[1].sequence));
    assert_eq!(db.branch_log(main.id).unwrap().len(), 6);
}

#[test]
fn a_newer_format_opens_unchanged_when_its_state_reads() {
    let (_dir, path) = copy_of("future_format_unknown_event_save.db");
    let inspection = inspect_save(&path).unwrap();
    assert_eq!(inspection.format_version, 4);
    assert!(!inspection.needs_migration());
    assert_eq!(
        inspection.branches[0].content.as_ref().map(|c| c.id.as_str()),
        Some("rundale")
    );

    let db = Database::open(&path).unwrap();
    let main = db.find_branch("main").unwrap().unwrap();
    assert!(db.load_recovery_data(main.id).unwrap().is_some());
    let unknown: Vec<_> = db
        .transcript_events(main.id, 0)
        .unwrap()
        .into_iter()
        .filter(|row| row.kind == "harvest_festival")
        .collect();
    assert_eq!(unknown.len(), 1, "the unknown event is kept");
    drop(db);
    assert_eq!(user_version(&path), 4, "a newer stamp is never lowered");
    let conn = Connection::open(&path).unwrap();
    let festivals: i64 = conn
        .query_row("SELECT COUNT(*) FROM harvest_festivals", [], |row| row.get(0))
        .unwrap();
    assert_eq!(festivals, 1, "tables this build does not know are kept");
}

fn assert_refused_and_untouched(path: &Path) -> String {
    let before = std::fs::read(path).unwrap();
    let inspected = inspect_save(path);
    let opened = Database::open(path);
    let reason = match (inspected, opened) {
        (Err(LimerickError::SaveIncompatible(a)), Err(LimerickError::SaveIncompatible(b))) => {
            assert_eq!(a, b);
            a
        }
        (inspected, opened) => panic!(
            "expected a refusal, got {:?} / {:?}",
            inspected.map(|_| ()),
            opened.map(|_| ())
        ),
    };
    assert_eq!(
        std::fs::read(path).unwrap(),
        before,
        "a refused save is byte-identical"
    );
    reason
}

#[test]
fn a_save_whose_state_cannot_be_read_is_refused_and_left_untouched() {
    let (_dir, path) = copy_of("unreadable_state_save.db");
    let reason = assert_refused_and_untouched(&path);
    assert!(
        reason.contains("saved state of branch 'main' cannot be read"),
        "{reason}"
    );
    // Also when a stale WAL sidecar is left beside it.
    let reason_again = assert_refused_and_untouched(&path);
    assert_eq!(reason, reason_again);
}

#[test]
fn a_file_that_is_not_a_save_is_refused_and_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let garbage = dir.path().join("limerick_001.db");
    std::fs::write(&garbage, b"this is not a sqlite database, just some bytes .....").unwrap();
    assert!(assert_refused_and_untouched(&garbage).contains("not a readable save database"));

    let other = dir.path().join("limerick_002.db");
    Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE recipes (id INTEGER PRIMARY KEY, name TEXT);")
        .unwrap();
    assert!(assert_refused_and_untouched(&other).contains("not a save database"));

    let stamped = dir.path().join("limerick_003.db");
    let conn = Connection::open(&stamped).unwrap();
    conn.execute_batch(
        "CREATE TABLE branches (id INTEGER PRIMARY KEY, name TEXT, created_at TEXT, parent_branch_id INTEGER);\n         PRAGMA user_version = 3;",
    )
    .unwrap();
    drop(conn);
    assert!(assert_refused_and_untouched(&stamped).contains("has no `snapshots` table"));
}

#[test]
fn a_table_missing_a_column_this_build_reads_is_refused() {
    let (_dir, path) = copy_of("turn_journal_v2_save.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch("ALTER TABLE requests DROP COLUMN phase;")
        .unwrap();
    let reason = assert_refused_and_untouched(&path);
    assert!(reason.contains("`requests` has no `phase`"), "{reason}");
}

#[test]
fn a_request_that_is_not_json_is_refused() {
    let (_dir, path) = copy_of("turn_journal_v2_save.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch("UPDATE requests SET record = 'not json' WHERE rowid = 1;")
        .unwrap();
    let reason = assert_refused_and_untouched(&path);
    assert!(reason.contains("turn request"), "{reason}");
}

#[test]
fn new_saves_are_stamped_and_a_current_save_reopens_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("limerick_001.db");
    let inspection = inspect_save(&path).unwrap();
    assert_eq!(inspection.format_version, 0, "a missing file is a new save");
    drop(Database::open(&path).unwrap());
    assert_eq!(user_version(&path), SAVE_FORMAT_VERSION);

    // Checkpoint so the main file holds everything, then reopen: a save
    // that is already current is not migrated again.
    Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let db = Database::open(&path).unwrap();
    assert!(db.find_branch("main").unwrap().is_some());
    drop(db);
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let memory = Database::open_memory().unwrap();
    drop(memory);
}
