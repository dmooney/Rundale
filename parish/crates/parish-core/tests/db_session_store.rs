use std::path::Path;

use chrono::{TimeZone, Utc};
use parish_core::persistence::{Database, GameSnapshot, WorldEvent};
use parish_core::session_store::{
    DbSessionStore, SessionStore, TaskJournalTarget, append_task_mutations,
    append_task_mutations_or_rollback, load_recovery_bundle,
};
use parish_core::world::{LocationId, WorldState};
use parish_types::{NpcId, PlayerTask, PlayerTaskId, TaskStatus};
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
        active_session: None,
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

fn test_task(id: u64, status: TaskStatus) -> PlayerTask {
    let assigned_at = Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap();
    PlayerTask {
        id: PlayerTaskId(id),
        description: "Dig over the potato patch.".to_string(),
        assigned_by: NpcId(7),
        location: LocationId(1),
        assigned_at,
        status,
        started_at: (status != TaskStatus::Assigned)
            .then_some(Utc.with_ymd_and_hms(1820, 3, 20, 8, 5, 0).unwrap()),
        completed_at: (status == TaskStatus::Completed)
            .then_some(Utc.with_ymd_and_hms(1820, 3, 20, 8, 10, 0).unwrap()),
        last_matching_action: (status != TaskStatus::Assigned)
            .then(|| "I dig over the potato patch.".to_string()),
    }
}

fn seed_named_save(path: &Path, weather: &str) -> i64 {
    let db = Database::open(path).unwrap();
    let branch_id = db.find_branch("main").unwrap().unwrap().id;
    let mut snapshot = make_test_snapshot();
    snapshot.weather = weather.to_string();
    db.save_snapshot(branch_id, &snapshot).unwrap();
    branch_id
}

#[tokio::test]
async fn active_save_rebind_switches_exact_file() {
    let tmp = TempDir::new().unwrap();
    let session_id = "rebind-session";
    let session_dir = tmp.path().join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let first = session_dir.join("parish_001.db");
    let second = session_dir.join("parish_002.db");
    let first_branch = seed_named_save(&first, "Rain");
    let second_branch = seed_named_save(&second, "Clear");
    assert_eq!(first_branch, second_branch);

    let store = DbSessionStore::new(tmp.path().to_path_buf());
    store.set_active_save(session_id, &first).unwrap();
    let (_, first_snapshot) = store
        .load_latest_snapshot(session_id, first_branch)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_snapshot.weather, "Rain");

    store.set_active_save(session_id, &second).unwrap();
    let (_, second_snapshot) = store
        .load_latest_snapshot(session_id, second_branch)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second_snapshot.weather, "Clear");
    assert_eq!(
        std::fs::canonicalize(store.save_path(session_id).unwrap()).unwrap(),
        std::fs::canonicalize(second).unwrap()
    );
}

#[tokio::test]
async fn task_journal_targets_exact_branch_after_switch() {
    let tmp = TempDir::new().unwrap();
    let save_path = tmp.path().join("parish_001.db");
    let db = Database::open(&save_path).unwrap();
    let main = db.find_branch("main").unwrap().unwrap();
    let fork_id = db.create_branch("fork", Some(main.id)).unwrap();
    let snapshot = make_test_snapshot();
    let main_snapshot_id = db.save_snapshot(main.id, &snapshot).unwrap();
    let fork_snapshot_id = db.save_snapshot(fork_id, &snapshot).unwrap();
    drop(db);

    let store = DbSessionStore::new(tmp.path().to_path_buf());
    let main_target = TaskJournalTarget {
        session_id: String::new(),
        save_path: save_path.clone(),
        branch_id: main.id,
    };
    let fork_target = TaskJournalTarget {
        branch_id: fork_id,
        ..main_target.clone()
    };
    append_task_mutations(&store, &main_target, &[test_task(1, TaskStatus::Assigned)])
        .await
        .unwrap();
    append_task_mutations(
        &store,
        &fork_target,
        &[test_task(2, TaskStatus::InProgress)],
    )
    .await
    .unwrap();

    let main_events = store
        .read_journal("", main.id, main_snapshot_id)
        .await
        .unwrap();
    let fork_events = store
        .read_journal("", fork_id, fork_snapshot_id)
        .await
        .unwrap();
    assert_eq!(main_events.len(), 1);
    assert_eq!(fork_events.len(), 1);
    assert!(matches!(
        &main_events[0],
        WorldEvent::PlayerTaskStateChanged { task } if task.id == PlayerTaskId(1)
    ));
    assert!(matches!(
        &fork_events[0],
        WorldEvent::PlayerTaskStateChanged { task } if task.id == PlayerTaskId(2)
    ));
}

#[tokio::test]
async fn one_global_store_keeps_two_sessions_isolated_without_double_nesting() {
    let tmp = TempDir::new().unwrap();
    let first_session = "session-one";
    let second_session = "session-two";
    let first_dir = tmp.path().join(first_session);
    let second_dir = tmp.path().join(second_session);
    std::fs::create_dir_all(&first_dir).unwrap();
    std::fs::create_dir_all(&second_dir).unwrap();
    let first_path = first_dir.join("parish_001.db");
    let second_path = second_dir.join("parish_001.db");
    let first_branch = seed_named_save(&first_path, "Rain");
    let second_branch = seed_named_save(&second_path, "Clear");
    let store = DbSessionStore::new(tmp.path().to_path_buf());

    let first_target = TaskJournalTarget {
        session_id: first_session.to_string(),
        save_path: first_path.clone(),
        branch_id: first_branch,
    };
    let second_target = TaskJournalTarget {
        session_id: second_session.to_string(),
        save_path: second_path.clone(),
        branch_id: second_branch,
    };
    append_task_mutations(
        &store,
        &first_target,
        &[test_task(11, TaskStatus::Assigned)],
    )
    .await
    .unwrap();
    append_task_mutations(
        &store,
        &second_target,
        &[test_task(22, TaskStatus::Assigned)],
    )
    .await
    .unwrap();

    let first_bundle = load_recovery_bundle(&store, first_session, &first_path, first_branch)
        .await
        .unwrap()
        .unwrap();
    let second_bundle = load_recovery_bundle(&store, second_session, &second_path, second_branch)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        &first_bundle.journal[0],
        WorldEvent::PlayerTaskStateChanged { task } if task.id == PlayerTaskId(11)
    ));
    assert!(matches!(
        &second_bundle.journal[0],
        WorldEvent::PlayerTaskStateChanged { task } if task.id == PlayerTaskId(22)
    ));
    assert!(!tmp.path().join(first_session).join(first_session).exists());
    assert!(
        !tmp.path()
            .join(second_session)
            .join(second_session)
            .exists()
    );
}

#[tokio::test]
async fn recovery_bundle_replays_task_tail_after_snapshot_only_crash() {
    let tmp = TempDir::new().unwrap();
    let save_path = tmp.path().join("parish_001.db");
    let db = Database::open(&save_path).unwrap();
    let branch_id = db.find_branch("main").unwrap().unwrap().id;
    let mut snapshot = make_test_snapshot();
    snapshot
        .player_progress
        .apply_replayed_task(test_task(1, TaskStatus::Assigned))
        .unwrap();
    db.save_snapshot(branch_id, &snapshot).unwrap();
    drop(db);

    let store = DbSessionStore::new(tmp.path().to_path_buf());
    let progressed = test_task(1, TaskStatus::InProgress);
    append_task_mutations(
        &store,
        &TaskJournalTarget {
            session_id: String::new(),
            save_path: save_path.clone(),
            branch_id,
        },
        std::slice::from_ref(&progressed),
    )
    .await
    .unwrap();
    drop(store);

    let reopened = DbSessionStore::new(tmp.path().to_path_buf());
    let bundle = load_recovery_bundle(&reopened, "", &save_path, branch_id)
        .await
        .unwrap()
        .unwrap();
    let mut world = WorldState::new();
    let mut npc_manager = parish_core::npc::manager::NpcManager::new();
    bundle.restore(&mut world, &mut npc_manager);

    assert_eq!(
        world.player_progress.task(PlayerTaskId(1)),
        Some(&progressed)
    );
}

#[tokio::test]
async fn missing_task_target_rolls_back_the_in_memory_ledger() {
    let tmp = TempDir::new().unwrap();
    let store = DbSessionStore::new(tmp.path().to_path_buf());
    let mut initial_world = WorldState::new();
    let assigned = test_task(1, TaskStatus::Assigned);
    initial_world
        .player_progress
        .apply_replayed_task(assigned.clone())
        .unwrap();
    let before = initial_world.player_progress.clone();
    let progressed = test_task(1, TaskStatus::InProgress);
    initial_world
        .player_progress
        .apply_replayed_task(progressed.clone())
        .unwrap();
    let world = tokio::sync::Mutex::new(initial_world);

    let error =
        append_task_mutations_or_rollback(&store, None, &[progressed], &world, before.clone())
            .await
            .unwrap_err();

    assert!(error.to_string().contains("without an active save"));
    assert_eq!(world.lock().await.player_progress, before);
}
