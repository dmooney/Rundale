//! Tests for the Database and AsyncDatabase API surface.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::params;

use super::*;
use crate::snapshot::{ClockSnapshot, GameSnapshot};
use chrono::{TimeZone, Utc};

fn make_test_snapshot() -> GameSnapshot {
    GameSnapshot {
        player_location: parish_types::LocationId(1),
        weather: "Clear".to_string(),
        text_log: vec!["Hello".to_string()],
        clock: ClockSnapshot {
            game_time: Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap(),
            speed_factor: 36.0,
            paused: false,
        },
        npcs: Vec::new(),
        last_tier2_game_time: None,
        last_tier3_game_time: None,
        last_tier4_game_time: None,
        introduced_npcs: Default::default(),
        visited_locations: std::collections::HashSet::from([parish_types::LocationId(1)]),
        visited_order: vec![parish_types::LocationId(1)],
        edge_traversals: Default::default(),
        gossip_network: Default::default(),
        conversation_log: Default::default(),
        player_name: None,
        player_progress: Default::default(),
        npcs_who_know_player_name: Default::default(),
    }
}

#[test]
fn test_database_open_memory() {
    let db = Database::open_memory().unwrap();
    let branches = db.list_branches().unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].name, "main");
}

#[test]
fn test_snapshot_save_and_load() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snapshot = make_test_snapshot();

    let snap_id = db.save_snapshot(branch.id, &snapshot).unwrap();
    assert!(snap_id > 0);

    let loaded = db.load_latest_snapshot(branch.id).unwrap().unwrap();
    assert_eq!(loaded.0, snap_id);
    assert_eq!(loaded.1.player_location, snapshot.player_location);
    assert_eq!(loaded.1.weather, snapshot.weather);
}

#[test]
fn reaction_history_survives_snapshot_plus_journal_recovery() {
    use parish_npc::manager::NpcManager;
    use parish_types::{NpcId, ReactionDirection};

    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let mut snapshot = make_test_snapshot();
    let npc = parish_npc::Npc::new_test_npc();
    snapshot.npcs.push(crate::NpcSnapshot::from_npc(&npc));
    let snapshot_id = db.save_snapshot(branch.id, &snapshot).unwrap();
    let timestamp = Utc.with_ymd_and_hms(1820, 3, 20, 8, 5, 0).unwrap();
    db.append_event(
        branch.id,
        snapshot_id,
        &crate::WorldEvent::ReactionRecorded {
            npc_id: NpcId(1),
            direction: ReactionDirection::NpcToPlayer,
            emoji: "👀".to_string(),
            context: "I have no money".to_string(),
            timestamp,
        },
        &timestamp.to_rfc3339(),
    )
    .unwrap();

    let recovery = db.load_recovery_data(branch.id).unwrap().unwrap();
    let mut world = parish_world::WorldState::new();
    let mut npcs = NpcManager::new();
    recovery.snapshot.restore(&mut world, &mut npcs);
    crate::replay_journal(&mut world, &mut npcs, &recovery.journal);
    let context = npcs
        .get(NpcId(1))
        .unwrap()
        .reaction_log
        .prompt_context_string(5);
    assert!(context.contains("You raised an eyebrow"));
    assert!(!context.contains("The player raised an eyebrow"));
}

#[test]
fn test_load_latest_snapshot_none() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let result = db.load_latest_snapshot(branch.id).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_create_and_find_branch() {
    let db = Database::open_memory().unwrap();
    let id = db.create_branch("test-branch", Some(1)).unwrap();
    let found = db.find_branch("test-branch").unwrap().unwrap();
    assert_eq!(found.id, id);
    assert_eq!(found.name, "test-branch");
    assert_eq!(found.parent_branch_id, Some(1));
}

#[test]
fn test_create_branch_rejects_dangling_parent() {
    let db = Database::open_memory().unwrap();
    let result = db.create_branch("orphan", Some(999));
    assert!(
        result.is_err(),
        "branches must not reference a missing parent branch"
    );
    assert!(db.find_branch("orphan").unwrap().is_none());
}

#[test]
fn create_branch_with_snapshot_rolls_back_orphan_and_same_name_retries() {
    let db = Database::open_memory().unwrap();
    let main = db.find_branch("main").unwrap().unwrap();
    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_initial_fork_snapshot
             BEFORE INSERT ON snapshots
             BEGIN
                 SELECT RAISE(FAIL, 'injected snapshot failure');
             END;",
        )
        .unwrap();

    let error = db
        .create_branch_with_snapshot("retryable-fork", Some(main.id), &make_test_snapshot())
        .unwrap_err();
    assert!(error.to_string().contains("injected snapshot failure"));
    assert!(
        db.find_branch("retryable-fork").unwrap().is_none(),
        "failed initial snapshot must roll back the branch row"
    );

    db.conn
        .execute_batch("DROP TRIGGER fail_initial_fork_snapshot;")
        .unwrap();
    let (branch_id, snapshot_id) = db
        .create_branch_with_snapshot("retryable-fork", Some(main.id), &make_test_snapshot())
        .unwrap();
    assert_eq!(
        db.load_latest_snapshot(branch_id).unwrap().unwrap().0,
        snapshot_id
    );
}

#[test]
fn test_branch_parent_foreign_key_rejects_dangling_parent() {
    let db = Database::open_memory().unwrap();
    let result = db.conn.execute(
        "INSERT INTO branches (name, created_at, parent_branch_id)
         VALUES (?1, ?2, ?3)",
        params!["raw-orphan", chrono::Utc::now().to_rfc3339(), 999],
    );

    assert!(
        result.is_err(),
        "branch parent foreign key should reject a missing parent"
    );
}

fn seed_legacy_branches_without_parent_fk(path: &Path, orphan_parent_id: Option<i64>) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE branches (
            id INTEGER PRIMARY KEY,
            name TEXT UNIQUE NOT NULL,
            created_at TEXT NOT NULL,
            parent_branch_id INTEGER
         );",
    )
    .unwrap();

    conn.execute(
        "INSERT INTO branches (id, name, created_at, parent_branch_id)
         VALUES (1, 'main', ?1, NULL)",
        params![chrono::Utc::now().to_rfc3339()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO branches (id, name, created_at, parent_branch_id)
         VALUES (2, 'child', ?1, 1)",
        params![chrono::Utc::now().to_rfc3339()],
    )
    .unwrap();

    if let Some(parent_id) = orphan_parent_id {
        conn.execute(
            "INSERT INTO branches (id, name, created_at, parent_branch_id)
             VALUES (3, 'orphan', ?1, ?2)",
            params![chrono::Utc::now().to_rfc3339(), parent_id],
        )
        .unwrap();
    }
}

#[test]
fn test_migrate_legacy_branches_adds_parent_foreign_key() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    seed_legacy_branches_without_parent_fk(tmp.path(), None);

    let db = Database::open(tmp.path()).unwrap();
    assert!(schema::branch_parent_fk_present(&db.conn).unwrap());
    assert_eq!(
        db.find_branch("child").unwrap().unwrap().parent_branch_id,
        Some(1)
    );

    let result = db.conn.execute(
        "INSERT INTO branches (name, created_at, parent_branch_id)
         VALUES (?1, ?2, ?3)",
        params!["raw-orphan", chrono::Utc::now().to_rfc3339(), 999],
    );
    assert!(
        result.is_err(),
        "migrated legacy branches should reject missing parents"
    );
}

#[test]
fn test_migrate_legacy_branches_clears_dangling_parent_ids() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    seed_legacy_branches_without_parent_fk(tmp.path(), Some(999));

    let db = Database::open(tmp.path()).unwrap();
    assert!(schema::branch_parent_fk_present(&db.conn).unwrap());
    assert_eq!(
        db.find_branch("child").unwrap().unwrap().parent_branch_id,
        Some(1)
    );
    assert_eq!(
        db.find_branch("orphan").unwrap().unwrap().parent_branch_id,
        None
    );
}

#[test]
fn test_find_branch_not_found() {
    let db = Database::open_memory().unwrap();
    let result = db.find_branch("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_list_branches() {
    let db = Database::open_memory().unwrap();
    db.create_branch("alpha", None).unwrap();
    db.create_branch("beta", None).unwrap();
    let branches = db.list_branches().unwrap();
    assert_eq!(branches.len(), 3); // main + alpha + beta
}

#[test]
fn test_journal_append_and_query() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    let event1 = WorldEvent::PlayerMoved {
        from: parish_types::LocationId(1),
        to: parish_types::LocationId(2),
        minutes: None,
    };
    let event2 = WorldEvent::WeatherChanged {
        new_weather: "Rain".to_string(),
    };

    db.append_event(branch.id, snap_id, &event1, "1820-03-20T08:00:00Z")
        .unwrap();
    db.append_event(branch.id, snap_id, &event2, "1820-03-20T09:00:00Z")
        .unwrap();

    let events = db.events_since_snapshot(branch.id, snap_id).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], event1);
    assert_eq!(events[1], event2);
}

#[test]
fn test_journal_count() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 0);

    let event = WorldEvent::ClockAdvanced { minutes: 10 };
    db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
        .unwrap();
    assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 1);
}

#[test]
fn test_branch_log() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();

    db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
    db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    let log = db.branch_log(branch.id).unwrap();
    assert_eq!(log.len(), 2);
    // Most recent first
    assert!(log[0].id > log[1].id);
}

#[test]
fn test_clear_journal() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    let event = WorldEvent::ClockAdvanced { minutes: 10 };
    db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
        .unwrap();
    assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 1);

    db.clear_journal(branch.id, snap_id).unwrap();
    assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 0);
}

/// Compaction must be scoped to the `(branch_id, snapshot_id)` pair.
/// Events tied to an *earlier* snapshot on the same branch must survive
/// a `clear_journal` of a *later* snapshot (and vice versa) — otherwise
/// compaction would delete history it has no business touching.
#[test]
fn test_clear_journal_scoped_to_snapshot_id() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();

    // Two snapshots on the same branch, each with their own journal tail.
    let snap1 = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
    db.append_event(
        branch.id,
        snap1,
        &WorldEvent::ClockAdvanced { minutes: 5 },
        "1820-03-20T08:00:00Z",
    )
    .unwrap();
    db.append_event(
        branch.id,
        snap1,
        &WorldEvent::WeatherChanged {
            new_weather: "Fog".to_string(),
        },
        "1820-03-20T08:05:00Z",
    )
    .unwrap();

    let snap2 = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
    db.append_event(
        branch.id,
        snap2,
        &WorldEvent::ClockAdvanced { minutes: 10 },
        "1820-03-20T09:00:00Z",
    )
    .unwrap();

    assert_eq!(db.journal_count(branch.id, snap1).unwrap(), 2);
    assert_eq!(db.journal_count(branch.id, snap2).unwrap(), 1);

    // Pruning snap2's journal must leave snap1's events alone.
    db.clear_journal(branch.id, snap2).unwrap();
    assert_eq!(
        db.journal_count(branch.id, snap1).unwrap(),
        2,
        "pruning snap2 must not touch snap1's journal"
    );
    assert_eq!(db.journal_count(branch.id, snap2).unwrap(), 0);

    // Pruning snap1 now clears the remaining two events.
    db.clear_journal(branch.id, snap1).unwrap();
    assert_eq!(db.journal_count(branch.id, snap1).unwrap(), 0);
}

/// Compaction must be scoped to the `branch_id` as well: pruning the
/// journal for branch A must NEVER touch branch B's events.
#[test]
fn test_clear_journal_scoped_to_branch_id() {
    let db = Database::open_memory().unwrap();
    let main = db.find_branch("main").unwrap().unwrap();
    let fork_id = db.create_branch("fork", Some(main.id)).unwrap();

    let main_snap = db.save_snapshot(main.id, &make_test_snapshot()).unwrap();
    let fork_snap = db.save_snapshot(fork_id, &make_test_snapshot()).unwrap();

    db.append_event(
        main.id,
        main_snap,
        &WorldEvent::ClockAdvanced { minutes: 1 },
        "1820-03-20T08:00:00Z",
    )
    .unwrap();
    db.append_event(
        fork_id,
        fork_snap,
        &WorldEvent::ClockAdvanced { minutes: 2 },
        "1820-03-20T08:00:00Z",
    )
    .unwrap();

    db.clear_journal(main.id, main_snap).unwrap();

    assert_eq!(db.journal_count(main.id, main_snap).unwrap(), 0);
    assert_eq!(
        db.journal_count(fork_id, fork_snap).unwrap(),
        1,
        "pruning main's journal must not touch the fork"
    );
}

/// End-to-end compaction workflow:
///   1. save snapshot A
///   2. append N journal events after A
///   3. save snapshot B (which captures the state produced by those events)
///   4. clear_journal(A) — the tail is now redundant
///   5. load_latest_snapshot returns B, and events_since_snapshot(B) is empty
///
/// This is the exact lifecycle the CLI / server paths drive via `/save`.
#[test]
fn test_compaction_lifecycle_matches_save_path() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();

    let snap_a = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
    for i in 0..4 {
        db.append_event(
            branch.id,
            snap_a,
            &WorldEvent::ClockAdvanced { minutes: i + 1 },
            "1820-03-20T08:00:00Z",
        )
        .unwrap();
    }
    assert_eq!(db.journal_count(branch.id, snap_a).unwrap(), 4);

    // New snapshot captures the post-event state, old journal pruned.
    let snap_b = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
    db.clear_journal(branch.id, snap_a).unwrap();

    assert_eq!(db.journal_count(branch.id, snap_a).unwrap(), 0);
    assert_eq!(db.journal_count(branch.id, snap_b).unwrap(), 0);

    // Loading the latest snapshot gives us snap_b and no tail.
    let (loaded_id, _) = db.load_latest_snapshot(branch.id).unwrap().unwrap();
    assert_eq!(loaded_id, snap_b);
    let tail = db.events_since_snapshot(branch.id, snap_b).unwrap();
    assert!(tail.is_empty());
}

/// Regression: `clear_journal` with zero events must succeed silently —
/// the first-ever `/save` call on a fresh branch hits this path, since
/// `latest_snapshot_id` may still point at the bootstrap snapshot whose
/// journal tail is empty.
#[test]
fn test_clear_journal_on_empty_tail_is_noop() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 0);
    db.clear_journal(branch.id, snap_id).unwrap();
    assert_eq!(db.journal_count(branch.id, snap_id).unwrap(), 0);
}

#[test]
fn test_multiple_snapshots_loads_latest() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();

    let mut snap1 = make_test_snapshot();
    snap1.weather = "Cloudy".to_string();
    db.save_snapshot(branch.id, &snap1).unwrap();

    let mut snap2 = make_test_snapshot();
    snap2.weather = "Sunny".to_string();
    let id2 = db.save_snapshot(branch.id, &snap2).unwrap();

    let (loaded_id, loaded) = db.load_latest_snapshot(branch.id).unwrap().unwrap();
    assert_eq!(loaded_id, id2);
    assert_eq!(loaded.weather, "Sunny");
}

#[test]
fn test_open_file_database() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db = Database::open(tmp.path()).unwrap();
    let branches = db.list_branches().unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].name, "main");
}

#[test]
fn test_file_database_uses_wal_normal_synchronous_and_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.sqlite3");
    let db = Database::open(&path).unwrap();

    let journal_mode: String = db
        .conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    let synchronous: i64 = db
        .conn
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();
    let foreign_keys: i64 = db
        .conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();

    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(synchronous, 1, "SQLite NORMAL synchronous is encoded as 1");
    assert_eq!(foreign_keys, 1);
}

#[test]
fn test_reader_can_read_committed_state_during_write_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.sqlite3");
    let db = Database::open(&path).unwrap();
    let main = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(main.id, &make_test_snapshot()).unwrap();

    let writer = rusqlite::Connection::open(&path).unwrap();
    writer
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             BEGIN IMMEDIATE;",
        )
        .unwrap();
    writer
        .execute(
            "INSERT INTO branches (name, created_at, parent_branch_id)
             VALUES (?1, ?2, ?3)",
            params!["uncommitted-fork", chrono::Utc::now().to_rfc3339(), main.id],
        )
        .unwrap();

    let branches = db.list_branches().unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].name, "main");
    assert_eq!(
        db.load_latest_snapshot(main.id).unwrap().unwrap().0,
        snap_id
    );

    writer.execute_batch("COMMIT;").unwrap();
}

#[test]
fn test_duplicate_branch_name_fails() {
    let db = Database::open_memory().unwrap();
    db.create_branch("test", None).unwrap();
    let result = db.create_branch("test", None);
    assert!(result.is_err());
}

// --- AsyncDatabase wrapper ---

#[tokio::test]
async fn test_async_save_and_load_roundtrip() {
    let db = Database::open_memory().unwrap();
    let async_db = AsyncDatabase::new(db);

    let branch = async_db.find_branch("main").await.unwrap().unwrap();
    let snap = make_test_snapshot();
    let snap_id = async_db.save_snapshot(branch.id, &snap).await.unwrap();

    let loaded = async_db
        .load_latest_snapshot(branch.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.0, snap_id);
    assert_eq!(loaded.1.player_location, snap.player_location);
    assert_eq!(loaded.1.weather, snap.weather);
}

#[tokio::test]
async fn test_async_branch_crud() {
    let db = Database::open_memory().unwrap();
    let async_db = AsyncDatabase::new(db);

    let fork_id = async_db.create_branch("fork", None).await.unwrap();
    let found = async_db.find_branch("fork").await.unwrap();
    assert_eq!(found.unwrap().id, fork_id);

    let branches = async_db.list_branches().await.unwrap();
    assert_eq!(branches.len(), 2); // main + fork
}

#[tokio::test]
async fn test_async_journal_append_and_count() {
    let db = Database::open_memory().unwrap();
    let async_db = AsyncDatabase::new(db);

    let branch = async_db.find_branch("main").await.unwrap().unwrap();
    let snap_id = async_db
        .save_snapshot(branch.id, &make_test_snapshot())
        .await
        .unwrap();

    async_db
        .append_event(
            branch.id,
            snap_id,
            &WorldEvent::ClockAdvanced { minutes: 5 },
            "1820-03-20T08:00:00Z",
        )
        .await
        .unwrap();

    let count = async_db.journal_count(branch.id, snap_id).await.unwrap();
    assert_eq!(count, 1);

    let events = async_db
        .events_since_snapshot(branch.id, snap_id)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn test_async_clear_journal() {
    let db = Database::open_memory().unwrap();
    let async_db = AsyncDatabase::new(db);

    let branch = async_db.find_branch("main").await.unwrap().unwrap();
    let snap_id = async_db
        .save_snapshot(branch.id, &make_test_snapshot())
        .await
        .unwrap();

    async_db
        .append_event(
            branch.id,
            snap_id,
            &WorldEvent::ClockAdvanced { minutes: 5 },
            "1820-03-20T08:00:00Z",
        )
        .await
        .unwrap();

    async_db.clear_journal(branch.id, snap_id).await.unwrap();
    let count = async_db.journal_count(branch.id, snap_id).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_async_branch_log() {
    let db = Database::open_memory().unwrap();
    let async_db = AsyncDatabase::new(db);

    let branch = async_db.find_branch("main").await.unwrap().unwrap();
    async_db
        .save_snapshot(branch.id, &make_test_snapshot())
        .await
        .unwrap();
    async_db
        .save_snapshot(branch.id, &make_test_snapshot())
        .await
        .unwrap();

    let log = async_db.branch_log(branch.id).await.unwrap();
    assert_eq!(log.len(), 2);
}

/// Regression: corrupt JSON in the `world_state` column must produce a
/// recoverable error, not a panic.
#[test]
fn test_corrupt_world_state_json_is_recoverable() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db = Database::open(tmp.path()).unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    // Corrupt the world_state via a raw connection.
    let raw = rusqlite::Connection::open(tmp.path()).unwrap();
    raw.execute(
        "UPDATE snapshots SET world_state = 'this is not valid json'",
        [],
    )
    .unwrap();
    drop(raw);

    let result = db.load_latest_snapshot(branch.id);
    assert!(
        result.is_err(),
        "corrupt JSON should produce a recoverable error"
    );
}

/// Regression: corrupt journal event JSON must surface as a recoverable
/// error to the caller. Save recovery can then report the bad row instead
/// of panicking while replaying a damaged journal.
#[test]
fn test_corrupt_journal_event_json_is_recoverable() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db = Database::open(tmp.path()).unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    let raw = rusqlite::Connection::open(tmp.path()).unwrap();
    raw.execute(
        "INSERT INTO journal_events
         (branch_id, sequence, after_snapshot_id, event_type, event_data, game_time)
         VALUES (?1, 1, ?2, 'ClockAdvanced', '{not valid json', ?3)",
        params![branch.id, snap_id, "1820-03-20T08:00:00Z"],
    )
    .unwrap();
    drop(raw);

    let result = db.events_since_snapshot(branch.id, snap_id);
    assert!(
        result.is_err(),
        "corrupt journal JSON should produce a recoverable error"
    );
}

#[test]
fn test_batch_append_rolls_back_first_insert_when_second_insert_fails_then_retries() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_second_journal_insert
             BEFORE INSERT ON journal_events
             WHEN NEW.sequence = 2
             BEGIN
                 SELECT RAISE(ABORT, 'injected second insert failure');
             END;",
        )
        .unwrap();
    let events = vec![
        (
            WorldEvent::ClockAdvanced { minutes: 1 },
            "1820-03-20T08:01:00Z".to_string(),
        ),
        (
            WorldEvent::ClockAdvanced { minutes: 2 },
            "1820-03-20T08:02:00Z".to_string(),
        ),
    ];

    let error = db
        .append_events_to_latest_snapshot(branch.id, &events)
        .expect_err("the trigger must reject the second insert");
    assert!(error.to_string().contains("injected second insert failure"));
    assert_eq!(
        db.journal_count(branch.id, snap_id).unwrap(),
        0,
        "the first insert must roll back with the failed second insert"
    );

    db.conn
        .execute_batch("DROP TRIGGER fail_second_journal_insert;")
        .unwrap();
    assert_eq!(
        db.append_events_to_latest_snapshot(branch.id, &events)
            .unwrap(),
        Some(snap_id)
    );
    assert_eq!(
        db.events_since_snapshot(branch.id, snap_id).unwrap(),
        vec![
            WorldEvent::ClockAdvanced { minutes: 1 },
            WorldEvent::ClockAdvanced { minutes: 2 },
        ],
        "the unchanged batch must succeed on retry"
    );
}

#[test]
fn test_recovery_data_returns_snapshot_and_exact_tail() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let first = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
    db.append_event(
        branch.id,
        first,
        &WorldEvent::ClockAdvanced { minutes: 3 },
        "1820-03-20T08:03:00Z",
    )
    .unwrap();

    let recovery = db.load_recovery_data(branch.id).unwrap().unwrap();
    assert_eq!(recovery.snapshot_id, first);
    assert_eq!(
        recovery.journal,
        vec![WorldEvent::ClockAdvanced { minutes: 3 }]
    );
}

/// Concurrent `append_event` calls must produce non-overlapping sequences:
/// every event must be present, no duplicates, no gaps.
#[tokio::test]
async fn test_concurrent_append_events_produce_correct_sequences() {
    let db = Database::open_memory().unwrap();
    let async_db = AsyncDatabase::new(db);

    let branch = async_db.find_branch("main").await.unwrap().unwrap();
    let snap_id = async_db
        .save_snapshot(branch.id, &make_test_snapshot())
        .await
        .unwrap();

    let num_tasks = 4usize;
    let events_per_task = 25usize;
    let total = num_tasks * events_per_task;

    let mut handles = Vec::new();
    for t in 0..num_tasks {
        let adb = async_db.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..events_per_task {
                adb.append_event(
                    branch.id,
                    snap_id,
                    &WorldEvent::ClockAdvanced {
                        minutes: ((t * events_per_task + i) as i64) + 1,
                    },
                    "1820-03-20T08:00:00Z",
                )
                .await
                .unwrap();
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let count = async_db.journal_count(branch.id, snap_id).await.unwrap();
    assert_eq!(count, total, "all events must be present");

    let events = async_db
        .events_since_snapshot(branch.id, snap_id)
        .await
        .unwrap();
    assert_eq!(events.len(), total, "all events must be queryable");
}

// --- Issue #225: PRAGMA foreign_keys=ON enforcement ---

#[test]
fn test_foreign_key_snapshot_references_branch() {
    let db = Database::open_memory().unwrap();
    let snapshot = make_test_snapshot();
    // branch_id 999 does not exist; FK enforcement must reject the insert.
    let result = db.save_snapshot(999, &snapshot);
    assert!(
        result.is_err(),
        "save_snapshot with a non-existent branch_id should fail with FK violation"
    );
}

#[test]
fn test_foreign_key_journal_references_snapshot() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let event = WorldEvent::ClockAdvanced { minutes: 1 };
    // snapshot_id 999 does not exist; FK enforcement must reject the insert.
    let result = db.append_event(branch.id, 999, &event, "1820-03-20T08:00:00Z");
    assert!(
        result.is_err(),
        "append_event with a non-existent snapshot_id should fail with FK violation"
    );
}

// --- Issue #226: atomic sequence + UNIQUE constraint ---

#[test]
fn test_append_event_sequence_starts_at_one() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    let event = WorldEvent::ClockAdvanced { minutes: 5 };
    db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
        .unwrap();

    // Verify sequence=1 was assigned by querying the count (sequence numbers
    // are internal, but correct ordering is observable via events_since_snapshot).
    let events = db.events_since_snapshot(branch.id, snap_id).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], event);
}

#[test]
fn test_journal_sequences_are_contiguous() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let snap_id = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    for i in 1..=10i64 {
        let event = WorldEvent::ClockAdvanced { minutes: i };
        db.append_event(branch.id, snap_id, &event, "1820-03-20T08:00:00Z")
            .unwrap();
    }

    let events = db.events_since_snapshot(branch.id, snap_id).unwrap();
    assert_eq!(events.len(), 10);
    // Events must arrive in insertion order (ascending sequence).
    for (idx, ev) in events.iter().enumerate() {
        match ev {
            WorldEvent::ClockAdvanced { minutes } => {
                assert_eq!(*minutes, (idx as i64) + 1);
            }
            _ => panic!("unexpected event type"),
        }
    }
}

#[test]
fn test_sequences_are_independent_per_snapshot() {
    // Each snapshot has its own sequence counter starting from 1.
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();

    let snap1 = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();
    let snap2 = db.save_snapshot(branch.id, &make_test_snapshot()).unwrap();

    let ev = WorldEvent::ClockAdvanced { minutes: 1 };
    db.append_event(branch.id, snap1, &ev, "1820-03-20T08:00:00Z")
        .unwrap();
    db.append_event(branch.id, snap2, &ev, "1820-03-20T08:00:00Z")
        .unwrap();

    // Both snapshots should have exactly one event each.
    assert_eq!(db.journal_count(branch.id, snap1).unwrap(), 1);
    assert_eq!(db.journal_count(branch.id, snap2).unwrap(), 1);
}

/// Regression test for #82: a previous panic while holding the mutex
/// must not cascade into every subsequent database call panicking.
/// `lock_recovered` should transparently recover from the poisoned
/// state.
#[test]
fn test_lock_recovered_handles_poisoned_mutex() {
    let mutex: Arc<Mutex<u32>> = Arc::new(Mutex::new(42));

    // Poison the mutex by panicking in another thread while holding it.
    let mutex_clone = mutex.clone();
    let _ = std::thread::spawn(move || {
        let _guard = mutex_clone.lock().unwrap();
        panic!("intentional panic to poison the mutex");
    })
    .join();

    // The mutex is now poisoned; `.lock()` returns `Err(_)`.
    assert!(mutex.lock().is_err(), "mutex should be poisoned");

    // `lock_recovered` must still yield the inner value.
    let guard = schema::lock_recovered(&mutex);
    assert_eq!(*guard, 42);
}

/// Regression test for #82: poison recovery also works on the
/// `Arc<Mutex<Database>>` used inside `AsyncDatabase`.
#[test]
fn test_lock_recovered_with_database_after_poison() {
    let db = Database::open_memory().unwrap();
    let branch = db.find_branch("main").unwrap().unwrap();
    let inner: Arc<Mutex<Database>> = Arc::new(Mutex::new(db));

    // Poison the inner mutex.
    let inner_clone = inner.clone();
    let _ = std::thread::spawn(move || {
        let _guard = inner_clone.lock().unwrap();
        panic!("intentional panic");
    })
    .join();

    assert!(inner.lock().is_err(), "database mutex should be poisoned");

    // Recover and verify the database is still usable.
    let db = schema::lock_recovered(&inner);
    let loaded = db
        .find_branch(&branch.name)
        .expect("find_branch should still work after poison recovery");
    assert!(loaded.is_some());
}
