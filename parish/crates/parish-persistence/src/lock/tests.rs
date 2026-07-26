//! Tests for advisory save-file locking.

use super::*;

#[test]
fn lock_path_for_save() {
    assert_eq!(
        SaveFileLock::lock_path_for(Path::new("saves/parish_001.db")),
        PathBuf::from("saves/parish_001.db.lock")
    );
}

#[test]
fn acquire_reenter_and_release_owner_directory() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();
    let lock_path = SaveFileLock::lock_path_for(&save);

    let first = SaveFileLock::try_acquire(&save).expect("first acquire");
    let owner = match observe_lock(&lock_path) {
        LockObservation::Owner(owner) => owner,
        _ => panic!("complete owner record must be published"),
    };
    assert_eq!(owner.pid, std::process::id());
    assert!(is_locked(&save));

    let second = SaveFileLock::try_acquire(&save).expect("same-process reentrant acquire");
    drop(first);
    assert!(
        lock_path.is_dir(),
        "dropping one reentrant guard must preserve ownership"
    );
    drop(second);
    assert!(!lock_path.exists());
}

#[test]
fn transient_reentrant_guard_does_not_remove_owner() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    let first = SaveFileLock::try_acquire(&save).unwrap();
    let _ = SaveFileLock::try_acquire(&save);

    assert!(is_locked(&save));
    drop(first);
    assert!(!SaveFileLock::lock_path_for(&save).exists());
}

#[test]
fn incomplete_just_published_owner_is_never_stolen() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    let lock_path = SaveFileLock::lock_path_for(&save);
    fs::create_dir(&lock_path).unwrap();

    assert!(
        SaveFileLock::try_acquire(&save).is_none(),
        "missing owner.json may be a peer between mkdir and publication"
    );
    assert!(is_locked(&save), "incomplete owner state must fail closed");
    assert!(lock_path.is_dir(), "contender must not remove the owner");
}

#[test]
fn malformed_directory_owner_is_locked() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    let lock_path = SaveFileLock::lock_path_for(&save);
    fs::create_dir(&lock_path).unwrap();
    fs::write(lock_path.join(OWNER_FILENAME), b"{not-json").unwrap();

    assert!(SaveFileLock::try_acquire(&save).is_none());
    assert!(is_locked(&save));
}

#[test]
fn malformed_legacy_file_is_locked() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    let lock_path = SaveFileLock::lock_path_for(&save);
    fs::write(&lock_path, b"not-a-pid").unwrap();

    assert!(SaveFileLock::try_acquire(&save).is_none());
    assert!(is_locked(&save));
    assert!(lock_path.is_file());
}

#[test]
fn stale_legacy_file_is_migrated_safely() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    let lock_path = SaveFileLock::lock_path_for(&save);
    let stale_pid = u32::MAX;
    fs::write(&lock_path, stale_pid.to_string()).unwrap();

    let guard = SaveFileLock::try_acquire_with(&save, 41_001, |pid| pid != stale_pid)
        .expect("dead legacy owner should be replaced");

    assert!(lock_path.is_dir());
    assert!(matches!(
        observe_lock(&lock_path),
        LockObservation::Owner(OwnerRecord { pid: 41_001, .. })
    ));
    drop(guard);
}

#[test]
fn concurrent_stale_replacers_publish_exactly_one_owner() {
    use std::sync::{Arc, Barrier};

    const CONTENDERS: usize = 16;
    const STALE_PID: u32 = 99_999;

    let dir = tempfile::tempdir().unwrap();
    let save = Arc::new(dir.path().join("test.db"));
    fs::write(SaveFileLock::lock_path_for(&save), STALE_PID.to_string()).unwrap();
    let start = Arc::new(Barrier::new(CONTENDERS));
    let finish = Arc::new(Barrier::new(CONTENDERS));

    let handles = (0..CONTENDERS)
        .map(|index| {
            let save = Arc::clone(&save);
            let start = Arc::clone(&start);
            let finish = Arc::clone(&finish);
            std::thread::spawn(move || {
                start.wait();
                let my_pid = 50_000 + u32::try_from(index).unwrap();
                let guard = SaveFileLock::try_acquire_with(&save, my_pid, |pid| pid != STALE_PID);
                finish.wait();
                guard
            })
        })
        .collect::<Vec<_>>();

    let guards = handles
        .into_iter()
        .filter_map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        guards.len(),
        1,
        "cleanup mutex plus revalidation must allow one stale replacement"
    );
    let published = match observe_lock(&SaveFileLock::lock_path_for(&save)) {
        LockObservation::Owner(owner) => owner,
        _ => panic!("winner must leave one complete owner"),
    };
    assert_eq!(published, guards[0].owner);
}

#[test]
fn stale_parseable_owner_reports_reclaimable() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    let lock_path = SaveFileLock::lock_path_for(&save);
    let owner = OwnerRecord {
        version: OWNER_VERSION,
        pid: 999_999_999,
        token: "stale".to_string(),
    };
    fs::create_dir(&lock_path).unwrap();
    fs::write(
        lock_path.join(OWNER_FILENAME),
        serde_json::to_vec(&owner).unwrap(),
    )
    .unwrap();

    assert!(!is_locked(&save));
}
