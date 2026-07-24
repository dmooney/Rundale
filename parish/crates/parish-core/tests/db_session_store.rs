use std::path::Path;

use chrono::{TimeZone, Utc};
use parish_core::persistence::{Database, GameSnapshot, WorldEvent};
use parish_core::session_store::{DbSessionStore, SessionStore};
use tempfile::TempDir;

fn make_test_snapshot() -> GameSnapshot {
    use parish_core::persistence::snapshot::{ClockSnapshot, GameSnapshot};
    use parish_core::world::LocationId;
    GameSnapshot {
        player_location: LocationId(1),
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
        visited_locations: std::collections::HashSet::from([LocationId(1)]),
        visited_order: vec![LocationId(1)],
        edge_traversals: Default::default(),
        gossip_network: Default::default(),
        conversation_log: Default::default(),
        player_name: None,
        player_progress: Default::default(),
        npcs_who_know_player_name: Default::default(),
    }
}

fn seed_save_file(dir: &Path, session_id: &str) {
    let session_dir = dir.join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let save_path = session_dir.join("parish_001.db");
    let db = Database::open(&save_path).unwrap();
    drop(db);
}

fn seed_save_file_with_snapshot(dir: &Path, session_id: &str) -> (i64, GameSnapshot) {
    let session_dir = dir.join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let save_path = session_dir.join("parish_001.db");
    let db = Database::open(&save_path).unwrap();
    let main = db.find_branch("main").unwrap().unwrap();
    let snap = make_test_snapshot();
    let snap_id = db.save_snapshot(main.id, &snap).unwrap();
    (snap_id, snap)
}

#[tokio::test]
async fn ensure_db_creates_new_save_file_when_none_exists() {
    let tmp = TempDir::new().unwrap();
    let session_id = "a1b2c3d4-e5f6-4789-abcd-ef0123456789";
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let branches = store.list_branches(session_id).await.unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].name, "main");
}

#[tokio::test]
async fn save_and_load_latest_snapshot_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let session_id = "b1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let branches = store.list_branches(session_id).await.unwrap();
    let branch_id = branches[0].id;

    let snap = make_test_snapshot();
    let snap_id = store
        .save_snapshot(session_id, branch_id, &snap)
        .await
        .unwrap();
    assert!(snap_id > 0, "snapshot id must be positive");

    let loaded = store
        .load_latest_snapshot(session_id, branch_id)
        .await
        .unwrap();
    assert!(loaded.is_some(), "must find the saved snapshot");
    let (loaded_id, loaded_snap) = loaded.unwrap();
    assert_eq!(loaded_id, snap_id);
    assert_eq!(loaded_snap.player_location, snap.player_location);
    assert_eq!(loaded_snap.weather, snap.weather);
}

#[tokio::test]
async fn load_latest_snapshot_returns_none_when_empty() {
    let tmp = TempDir::new().unwrap();
    let session_id = "c1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let branches = store.list_branches(session_id).await.unwrap();
    let branch_id = branches[0].id;

    let loaded = store
        .load_latest_snapshot(session_id, branch_id)
        .await
        .unwrap();
    assert!(loaded.is_none(), "no snapshots saved yet");
}

#[tokio::test]
async fn create_and_load_branch_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let session_id = "d1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let fork_id = store
        .create_branch(session_id, "my-fork", None)
        .await
        .unwrap();
    assert!(fork_id > 0);

    let found = store.load_branch(session_id, "my-fork").await.unwrap();
    assert!(found.is_some(), "branch must exist after creation");
    let branch = found.unwrap();
    assert_eq!(branch.id, fork_id);
    assert_eq!(branch.name, "my-fork");
}

#[tokio::test]
async fn list_branches_returns_all_branches() {
    let tmp = TempDir::new().unwrap();
    let session_id = "e1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    store
        .create_branch(session_id, "alpha", None)
        .await
        .unwrap();
    store.create_branch(session_id, "beta", None).await.unwrap();

    let branches = store.list_branches(session_id).await.unwrap();
    let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"main"));
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
}

#[tokio::test]
async fn branch_log_returns_snapshots_most_recent_first() {
    let tmp = TempDir::new().unwrap();
    let session_id = "f1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let branches = store.list_branches(session_id).await.unwrap();
    let branch_id = branches[0].id;

    let snap = make_test_snapshot();
    let id1 = store
        .save_snapshot(session_id, branch_id, &snap)
        .await
        .unwrap();
    let id2 = store
        .save_snapshot(session_id, branch_id, &snap)
        .await
        .unwrap();

    let log = store.branch_log(session_id, branch_id).await.unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].id, id2, "most recent first");
    assert_eq!(log[1].id, id1);
}

#[tokio::test]
async fn acquire_save_lock_returns_some_on_existing_save() {
    let tmp = TempDir::new().unwrap();
    let session_id = "g1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let lock = store.acquire_save_lock(session_id).await;
    assert!(lock.is_some(), "should acquire lock on existing save file");
}

#[tokio::test]
async fn journal_append_and_read_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let session_id = "h1b2c3d4-e5f6-4789-abcd-ef0123456789";
    let (snap_id, _snap) = seed_save_file_with_snapshot(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let branches = store.list_branches(session_id).await.unwrap();
    let branch_id = branches[0].id;

    let event = WorldEvent::ClockAdvanced { minutes: 30 };
    store
        .append_journal_event(
            session_id,
            branch_id,
            snap_id,
            &event,
            "1820-03-20T08:00:00Z",
        )
        .await
        .unwrap();

    let events = store
        .read_journal(session_id, branch_id, snap_id)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], event);
}

#[tokio::test]
async fn journal_multiple_events_are_ordered() {
    let tmp = TempDir::new().unwrap();
    let session_id = "i1b2c3d4-e5f6-4789-abcd-ef0123456789";
    let (snap_id, _snap) = seed_save_file_with_snapshot(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let branches = store.list_branches(session_id).await.unwrap();
    let branch_id = branches[0].id;

    let e1 = WorldEvent::ClockAdvanced { minutes: 5 };
    let e2 = WorldEvent::WeatherChanged {
        new_weather: "Rain".to_string(),
    };

    store
        .append_journal_event(session_id, branch_id, snap_id, &e1, "1820-03-20T08:00:00Z")
        .await
        .unwrap();
    store
        .append_journal_event(session_id, branch_id, snap_id, &e2, "1820-03-20T08:05:00Z")
        .await
        .unwrap();

    let events = store
        .read_journal(session_id, branch_id, snap_id)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], e1);
    assert_eq!(events[1], e2);
}

#[tokio::test]
async fn save_path_resolves_to_existing_db_file() {
    let tmp = TempDir::new().unwrap();
    let session_id = "j1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let path = store.save_path(session_id);
    assert!(
        path.is_some(),
        "save_path must return Some with existing db"
    );
    let path = path.unwrap();
    assert!(path.exists(), "returned path must exist on disk");
    assert!(path.to_string_lossy().contains(session_id));
}

#[tokio::test]
async fn save_path_returns_none_for_new_session() {
    let tmp = TempDir::new().unwrap();
    let session_id = "k1b2c3d4-e5f6-4789-abcd-ef0123456789";
    let store = DbSessionStore::new(tmp.path().to_path_buf());
    let path = store.save_path(session_id);
    assert!(path.is_none(), "no save file yet for new session");
}

#[tokio::test]
async fn single_user_empty_session_id_resolves_flat_dir() {
    let tmp = TempDir::new().unwrap();
    let save_path = tmp.path().join("parish_001.db");
    {
        let db = Database::open(&save_path).unwrap();
        let main = db.find_branch("main").unwrap().unwrap();
        db.save_snapshot(main.id, &make_test_snapshot()).unwrap();
    }

    let store = DbSessionStore::new(tmp.path().to_path_buf());
    let branches = store.list_branches("").await.unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].name, "main");

    let path = store.save_path("");
    assert!(path.is_some());
    assert_eq!(path.unwrap(), save_path);
}

#[tokio::test]
async fn multiple_snapshots_loads_latest() {
    let tmp = TempDir::new().unwrap();
    let session_id = "l1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let branches = store.list_branches(session_id).await.unwrap();
    let branch_id = branches[0].id;

    let mut snap1 = make_test_snapshot();
    snap1.weather = "Cloudy".to_string();
    store
        .save_snapshot(session_id, branch_id, &snap1)
        .await
        .unwrap();

    let mut snap2 = make_test_snapshot();
    snap2.weather = "Sunny".to_string();
    let id2 = store
        .save_snapshot(session_id, branch_id, &snap2)
        .await
        .unwrap();

    let (loaded_id, loaded) = store
        .load_latest_snapshot(session_id, branch_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded_id, id2);
    assert_eq!(loaded.weather, "Sunny");
}

#[tokio::test]
async fn release_save_lock_is_noop_and_does_not_panic() {
    let tmp = TempDir::new().unwrap();
    let session_id = "m1b2c3d4-e5f6-4789-abcd-ef0123456789";
    seed_save_file(tmp.path(), session_id);
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let lock = store.acquire_save_lock(session_id).await;
    assert!(lock.is_some(), "lock must be acquired");

    store.release_save_lock(session_id, lock.unwrap());

    let lock2 = store.acquire_save_lock(session_id).await;
    assert!(
        lock2.is_some(),
        "should re-acquire lock after explicit release"
    );
}
