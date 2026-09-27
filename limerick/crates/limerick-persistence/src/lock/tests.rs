//! Tests for advisory save-file locking. Cross-process behaviour is covered by
//! `tests/save_lock_processes.rs`.

use super::*;

fn save_in(dir: &tempfile::TempDir) -> PathBuf {
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();
    save
}

/// Holds the kernel lock through a separate open file, as another process
/// would: `SaveFileLock` in this process does not know about it.
fn foreign_holder(save: &Path) -> File {
    let file = File::create(SaveFileLock::lock_path_for(save)).unwrap();
    file.try_lock().unwrap();
    file
}

fn write_legacy_directory(save: &Path, pid: u32) -> PathBuf {
    let lock_path = SaveFileLock::lock_path_for(save);
    fs::create_dir(&lock_path).unwrap();
    fs::write(
        lock_path.join(LEGACY_OWNER_FILENAME),
        format!(r#"{{"version":1,"pid":{pid},"token":"{pid}-0"}}"#),
    )
    .unwrap();
    lock_path
}

#[test]
fn lock_path_for_save() {
    assert_eq!(
        SaveFileLock::lock_path_for(Path::new("saves/limerick_001.db")),
        PathBuf::from("saves/limerick_001.db.lock")
    );
}

#[test]
fn acquire_reenter_and_release() {
    let dir = tempfile::tempdir().unwrap();
    let save = save_in(&dir);
    let lock_path = SaveFileLock::lock_path_for(&save);

    let first = SaveFileLock::try_acquire(&save).expect("first acquire");
    assert!(lock_path.is_file());
    assert_eq!(
        fs::read_to_string(&lock_path).unwrap(),
        std::process::id().to_string(),
        "the lock file records its owner's PID"
    );
    assert!(is_locked(&save));

    let second = SaveFileLock::try_acquire(&save).expect("same-process reentrant acquire");
    drop(first);
    assert!(
        lock_path.is_file() && is_locked(&save),
        "dropping one reentrant guard must keep the lock"
    );
    drop(second);
    assert!(!lock_path.exists());
    assert!(!is_locked(&save));
}

#[test]
fn a_foreign_holder_refuses_acquisition() {
    let dir = tempfile::tempdir().unwrap();
    let save = save_in(&dir);
    let holder = foreign_holder(&save);

    assert!(SaveFileLock::try_acquire(&save).is_none());
    assert!(is_locked(&save));

    drop(holder);
    assert!(!is_locked(&save));
    let guard = SaveFileLock::try_acquire(&save).expect("released lock is free");
    drop(guard);
}

#[test]
fn a_held_lock_refuses_a_second_open_file() {
    let dir = tempfile::tempdir().unwrap();
    let save = save_in(&dir);
    let guard = SaveFileLock::try_acquire(&save).unwrap();

    let other = File::open(SaveFileLock::lock_path_for(&save)).unwrap();
    assert!(other.try_lock().is_err(), "the kernel lock is exclusive");
    drop(guard);
}

#[test]
fn a_recorded_pid_without_the_kernel_lock_does_not_lock() {
    // What a killed owner leaves behind, whatever process now has its PID:
    // our own, an unrelated live one, or one we may not signal.
    for pid in [std::process::id(), 1, u32::MAX] {
        let dir = tempfile::tempdir().unwrap();
        let save = save_in(&dir);
        fs::write(SaveFileLock::lock_path_for(&save), pid.to_string()).unwrap();

        assert!(!is_locked(&save), "pid {pid}");
        let guard = SaveFileLock::try_acquire(&save).expect("stale file is reclaimed");
        drop(guard);
        assert!(!SaveFileLock::lock_path_for(&save).exists());
    }
}

#[test]
fn a_replaced_lock_file_is_not_removed_by_the_previous_owner() {
    let dir = tempfile::tempdir().unwrap();
    let save = save_in(&dir);
    let lock_path = SaveFileLock::lock_path_for(&save);
    let guard = SaveFileLock::try_acquire(&save).unwrap();

    fs::remove_file(&lock_path).unwrap();
    fs::write(&lock_path, b"successor").unwrap();
    drop(guard);

    assert_eq!(fs::read(&lock_path).unwrap(), b"successor");
}

#[test]
fn legacy_directory_with_a_dead_owner_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let save = save_in(&dir);
    let lock_path = write_legacy_directory(&save, u32::MAX);

    assert!(!is_locked(&save));
    let guard = SaveFileLock::try_acquire(&save).expect("dead legacy owner is replaced");
    assert!(lock_path.is_file());
    drop(guard);
    assert!(!lock_path.exists());
}

#[test]
fn legacy_directory_with_a_live_owner_is_locked() {
    let dir = tempfile::tempdir().unwrap();
    let save = save_in(&dir);
    let lock_path = write_legacy_directory(&save, std::process::id());

    assert!(SaveFileLock::try_acquire(&save).is_none());
    assert!(is_locked(&save));
    assert!(lock_path.is_dir(), "a live legacy owner is never removed");
}

#[test]
fn incomplete_or_malformed_legacy_directory_is_locked() {
    for owner in [None, Some(&b"{not-json"[..])] {
        let dir = tempfile::tempdir().unwrap();
        let save = save_in(&dir);
        let lock_path = SaveFileLock::lock_path_for(&save);
        fs::create_dir(&lock_path).unwrap();
        if let Some(body) = owner {
            fs::write(lock_path.join(LEGACY_OWNER_FILENAME), body).unwrap();
        }

        assert!(SaveFileLock::try_acquire(&save).is_none());
        assert!(is_locked(&save));
        assert!(lock_path.is_dir());
    }
}

#[test]
fn concurrent_acquirers_in_one_process_share_one_lock() {
    use std::sync::Barrier;

    const THREADS: usize = 16;
    let dir = tempfile::tempdir().unwrap();
    let save = Arc::new(save_in(&dir));
    let start = Arc::new(Barrier::new(THREADS));
    let handles = (0..THREADS)
        .map(|_| {
            let save = Arc::clone(&save);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                SaveFileLock::try_acquire(&save)
            })
        })
        .collect::<Vec<_>>();
    let guards = handles
        .into_iter()
        .map(|handle| handle.join().unwrap().expect("same-process acquire"))
        .collect::<Vec<_>>();

    assert!(
        guards
            .iter()
            .all(|guard| Arc::ptr_eq(&guard.file, &guards[0].file))
    );
    drop(guards);
    assert!(!SaveFileLock::lock_path_for(&save).exists());
    assert!(!is_locked(&save));
}
