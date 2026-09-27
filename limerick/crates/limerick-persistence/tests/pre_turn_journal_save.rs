//! A desktop save written before the turn journal existed keeps opening.
//!
//! `fixtures/pre_turn_journal_save.db` is a real `limerick-server` save from
//! `main` at f3e629304 (before #2037), after the `talk-and-task` proof
//! scenario: the `main` branch, two snapshots (the new-game snapshot and an
//! autosave holding the accepted task), and one `PlayerTaskStateChanged`
//! journal event anchored to the first. It has no `requests` or
//! `transcript_events` table.

use std::path::{Path, PathBuf};

use limerick_persistence::{Database, NewTranscriptEvent, TurnRequestRow, WorldEvent};
use rusqlite::Connection;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pre_turn_journal_save.db")
}

fn tables(path: &Path) -> Vec<String> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn pre_turn_journal_save_opens_and_migrates() {
    let original = std::fs::read(fixture()).unwrap();
    assert!(!tables(&fixture()).contains(&"requests".to_string()));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("limerick_001.db");
    std::fs::copy(fixture(), &path).unwrap();

    let db = Database::open(&path).unwrap();
    let main = db.find_branch("main").unwrap().expect("main branch");
    assert_eq!(db.list_branches().unwrap().len(), 1);
    assert_eq!(db.branch_log(main.id).unwrap().len(), 2);

    // The saved game restores exactly as before: the latest snapshot holds
    // the accepted task; the task event stays on the first snapshot.
    let recovery = db.load_recovery_data(main.id).unwrap().expect("a snapshot");
    assert!(recovery.journal.is_empty());
    let tasks = recovery.snapshot.player_progress.tasks();
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].description.contains("potato patch"), "{tasks:?}");
    let first = db.branch_log(main.id).unwrap().last().unwrap().id;
    let events = db.events_since_snapshot(main.id, first).unwrap();
    assert!(matches!(
        events.as_slice(),
        [WorldEvent::PlayerTaskStateChanged { .. }]
    ));

    // Opening added empty turn-journal tables and nothing else changed.
    let migrated = tables(&path);
    for table in ["requests", "transcript_events"] {
        assert!(migrated.contains(&table.to_string()), "{migrated:?}");
    }
    assert!(db.turn_requests(main.id).unwrap().is_empty());
    assert!(db.transcript_events(main.id, 0).unwrap().is_empty());

    // The migrated save takes turn-journal writes and reopens with them.
    db.turn_journal_transaction(|writer| {
        writer.put_request(&TurnRequestRow {
            request_id: "r1".to_string(),
            branch_id: main.id,
            phase: "accepted".to_string(),
            is_open: true,
            committed_revision: None,
            record: "{}".to_string(),
        })?;
        writer
            .append_event(
                main.id,
                NewTranscriptEvent {
                    event_id: "r1/cmd",
                    request_id: Some("r1"),
                    kind: "player_command",
                    event: "{}",
                },
            )
            .map(|_| ())
    })
    .unwrap();
    drop(db);
    let reopened = Database::open(&path).unwrap();
    assert_eq!(reopened.turn_requests(main.id).unwrap().len(), 1);
    assert_eq!(reopened.transcript_events(main.id, 0).unwrap().len(), 1);
    assert_eq!(
        reopened
            .load_recovery_data(main.id)
            .unwrap()
            .unwrap()
            .snapshot,
        recovery.snapshot
    );

    assert_eq!(
        std::fs::read(fixture()).unwrap(),
        original,
        "the checked-in fixture itself is never modified"
    );
}
