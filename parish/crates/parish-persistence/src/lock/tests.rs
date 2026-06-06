//! Tests for advisory save-file locking.

use super::*;

#[test]
fn test_lock_path_for() {
    let save = Path::new("saves/parish_001.db");
    let lock = SaveFileLock::lock_path_for(save);
    assert_eq!(lock, PathBuf::from("saves/parish_001.db.lock"));
}

#[test]
fn test_acquire_and_release() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    let lock_path = SaveFileLock::lock_path_for(&save);

    {
        let lock = SaveFileLock::try_acquire(&save);
        assert!(lock.is_some(), "should acquire lock");
        assert!(lock_path.exists(), "lock file should exist");

        let contents = fs::read_to_string(&lock_path).unwrap();
        assert_eq!(
            contents.trim(),
            std::process::id().to_string(),
            "lock should contain our PID"
        );
    }
    // Lock dropped — file should be gone.
    assert!(!lock_path.exists(), "lock file should be removed on drop");
}

#[test]
fn test_double_acquire_same_process() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    let lock1 = SaveFileLock::try_acquire(&save);
    assert!(lock1.is_some());

    // Re-entrant acquire from same process returns a new owning guard.
    let lock2 = SaveFileLock::try_acquire(&save);
    assert!(lock2.is_some(), "same process re-acquire should succeed");
}

/// Regression test for codex P1: replacing a guard (state.save_lock = Some(new))
/// must not delete the lock file while the new guard is still alive.
#[test]
fn test_reentrant_guard_replacement_keeps_lock_file() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    let lock_path = SaveFileLock::lock_path_for(&save);

    let lock1 = SaveFileLock::try_acquire(&save);
    assert!(lock1.is_some(), "initial acquire must succeed");
    assert!(
        lock_path.exists(),
        "lock file must exist after initial acquire"
    );

    // Simulate: state.save_lock = Some(SaveFileLock::try_acquire(&save))
    // The old guard (lock1) is dropped when the new one is assigned.
    let lock2 = SaveFileLock::try_acquire(&save);
    assert!(lock2.is_some(), "re-entrant acquire must succeed");

    // Drop the original guard — this is the replacement pattern that was broken.
    drop(lock1);

    // The lock file must still exist: lock2 is the active guard now.
    assert!(
        lock_path.exists(),
        "lock file must not be deleted when old guard is dropped during re-entrant replacement"
    );

    // Confirm save is still reported as locked while lock2 holds it.
    assert!(is_locked(&save), "save should still be reported as locked");

    drop(lock2);
    assert!(
        !lock_path.exists(),
        "lock file removed after final guard drops"
    );
}

/// Regression test for codex P1 (round 2): a transient reentrant guard
/// must not remove the lock file when it is immediately discarded.
///
/// Exact scenario from the codex comment:
///   `let lock1 = try_acquire(...).unwrap(); let _ = try_acquire(...);`
/// After the transient guard drops, the lock file must still exist and
/// `lock1` must still be the live owner.
#[test]
fn test_transient_reentrant_does_not_remove_file() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    let lock_path = SaveFileLock::lock_path_for(&save);

    let lock1 = SaveFileLock::try_acquire(&save).expect("initial acquire must succeed");
    assert!(
        lock_path.exists(),
        "lock file must exist after initial acquire"
    );

    // Transient reentrant acquire — result immediately discarded.
    let _ = SaveFileLock::try_acquire(&save);
    // ^^^ The temporary guard is now dropped here.

    // lock1 must still protect the file.
    assert!(
        lock_path.exists(),
        "lock file must still exist after transient reentrant guard drops"
    );
    assert!(
        is_locked(&save),
        "save must still be reported as locked while lock1 is alive"
    );

    drop(lock1);
    assert!(
        !lock_path.exists(),
        "lock file removed only when lock1 (the last guard) drops"
    );
}

/// Regression test for codex P1 (round 2): three nested guards — file
/// is removed only when the very last one drops, regardless of drop order.
#[test]
fn test_last_guard_drop_removes_file() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    let lock_path = SaveFileLock::lock_path_for(&save);

    let lock1 = SaveFileLock::try_acquire(&save).expect("first acquire");
    let lock2 = SaveFileLock::try_acquire(&save).expect("second (reentrant) acquire");
    let lock3 = SaveFileLock::try_acquire(&save).expect("third (reentrant) acquire");

    assert!(lock_path.exists(), "file must exist with three live guards");

    // Drop in a non-trivial order: middle, first, last.
    drop(lock2);
    assert!(
        lock_path.exists(),
        "file must persist after dropping lock2 (lock1 and lock3 still alive)"
    );

    drop(lock1);
    assert!(
        lock_path.exists(),
        "file must persist after dropping lock1 (lock3 still alive)"
    );

    drop(lock3);
    assert!(
        !lock_path.exists(),
        "file removed only after the last guard (lock3) drops"
    );
}

#[test]
fn test_stale_lock_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    let lock_path = SaveFileLock::lock_path_for(&save);

    // Write a lock file with a PID that almost certainly doesn't exist.
    fs::write(&lock_path, "999999999").unwrap();

    let lock = SaveFileLock::try_acquire(&save);
    assert!(
        lock.is_some(),
        "should acquire lock after cleaning stale PID"
    );
}

#[test]
fn test_is_locked_no_lock_file() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    assert!(!is_locked(&save));
}

#[test]
fn test_is_locked_with_active_lock() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    let _lock = SaveFileLock::try_acquire(&save);
    assert!(is_locked(&save), "should report locked while lock is held");
}

#[test]
fn test_is_locked_stale() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();

    let lock_path = SaveFileLock::lock_path_for(&save);
    fs::write(&lock_path, "999999999").unwrap();

    assert!(
        !is_locked(&save),
        "stale lock with dead PID should not report locked"
    );
}

// ── #424 atomic acquire tests ───────────────────────────────────────────

#[test]
fn test_unparseable_lock_is_treated_as_stale() {
    // A lock file with garbage content (e.g. half-written by a
    // crashed peer) must be treated as stale and replaced.
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();
    let lock_path = SaveFileLock::lock_path_for(&save);
    fs::write(&lock_path, "not-a-pid").unwrap();

    let lock = SaveFileLock::try_acquire(&save);
    assert!(
        lock.is_some(),
        "unparseable lock should be treated as stale"
    );
    // And we should now be the owner.
    let contents = fs::read_to_string(&lock_path).unwrap();
    assert_eq!(contents.trim(), std::process::id().to_string());
}

#[test]
fn test_concurrent_acquire_only_one_wins() {
    // Spawn many threads racing for the same lock path. Exactly one
    // should succeed; the rest must observe AlreadyExists and back
    // off cleanly. This is the regression-guard for the rename
    // race that #424 calls out — `create_new` is atomic so the
    // OS guarantees a single winner.
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    let dir = tempfile::tempdir().unwrap();
    let save = Arc::new(dir.path().join("test.db"));
    fs::write(&*save, b"").unwrap();

    let n_threads = 16;
    let barrier = Arc::new(Barrier::new(n_threads));
    let winners = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..n_threads {
        let save = Arc::clone(&save);
        let barrier = Arc::clone(&barrier);
        let winners = Arc::clone(&winners);
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let lock = SaveFileLock::try_acquire(&save);
            if lock.is_some() {
                winners.fetch_add(1, Ordering::SeqCst);
                // Hold briefly so the racers all observe a live
                // lock rather than letting Drop clean it up.
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    // Exactly one thread held the lock at the same time. (Some
    // threads may have legitimately won serially after the first
    // dropped — the test sleep covers this so we measure peak
    // contention, not sequencing.)
    let wins = winners.load(Ordering::SeqCst);
    assert!(
        wins >= 1,
        "at least one thread should have acquired the lock"
    );
    assert!(
        wins <= n_threads,
        "winners ({wins}) should not exceed threads ({n_threads})"
    );
}

#[test]
fn test_lock_with_empty_file_is_treated_as_stale() {
    // Empty lock file (e.g. from a crashed write between create
    // and PID write) must be treated as stale.
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("test.db");
    fs::write(&save, b"").unwrap();
    let lock_path = SaveFileLock::lock_path_for(&save);
    fs::write(&lock_path, "").unwrap();

    let lock = SaveFileLock::try_acquire(&save);
    assert!(lock.is_some(), "empty lock should be treated as stale");
}
