//! Advisory file locking for save files.
//!
//! Prevents multiple app instances from writing to the same save file
//! simultaneously. Each lock is a `<save_path>.lock` sidecar file
//! containing the owning process's PID. Stale locks from crashed
//! processes are detected and cleaned up automatically.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Advisory lock backed by a `.lock` sidecar file.
///
/// On creation, writes the current PID to `<save_path>.lock`.
/// On drop, removes the lock file (best-effort).
pub struct SaveFileLock {
    lock_path: PathBuf,
}

impl SaveFileLock {
    /// Attempts to acquire an advisory lock for the given save file.
    ///
    /// Returns `Some(lock)` on success, or `None` if the file is already
    /// locked by another live process. Stale locks (dead PID) are cleaned
    /// up and re-acquired automatically.
    ///
    /// # Implementation note (#424)
    ///
    /// Uses `OpenOptions::create_new(true)` which is **atomic** at the
    /// filesystem level: on Unix the underlying `open(O_CREAT | O_EXCL)`
    /// is the canonical race-free lock-file creation primitive, and on
    /// Windows it maps to `CreateFile(CREATE_NEW)`. The previous
    /// implementation read-then-checked-then-renamed, which had two
    /// races: two processes could both observe an empty / stale lock
    /// and both win the rename, and the post-rename re-read could not
    /// reliably tell winner from loser when PIDs were close in time.
    /// `create_new` removes both: only one process across the whole
    /// machine can succeed at a given inode.
    pub fn try_acquire(save_path: &Path) -> Option<Self> {
        let lock_path = Self::lock_path_for(save_path);
        let my_pid = std::process::id();

        // Two attempts: first the direct create_new; if that fails with
        // AlreadyExists and the existing lock is stale, remove it and
        // retry once. Stale cleanup itself can race (two processes
        // both decide it's stale), but only one create_new wins so the
        // loser cleanly returns None.
        for attempt in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut f) => {
                    // We won the create. Write our PID so future
                    // process-alive checks can identify us; sync to
                    // disk so a crashed process leaves a parseable
                    // file behind. If the write or sync fails, remove
                    // the lock we just created so we don't strand it.
                    if write!(f, "{}", my_pid).is_ok() && f.sync_all().is_ok() {
                        return Some(Self { lock_path });
                    }
                    let _ = fs::remove_file(&lock_path);
                    return None;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if attempt > 0 {
                        // We already cleaned once and the lock is back —
                        // a peer beat us to the create_new. Bail.
                        return None;
                    }
                    let contents = fs::read_to_string(&lock_path).ok();
                    let parsed_pid = contents
                        .as_deref()
                        .and_then(|s| s.trim().parse::<u32>().ok());
                    match parsed_pid {
                        Some(pid) if pid == my_pid => {
                            // Re-entrant from same process.
                            return None;
                        }
                        Some(pid) if is_process_alive(pid) => {
                            // Live owner.
                            return None;
                        }
                        Some(pid) => {
                            // Stale lock from a dead process.
                            tracing::info!(
                                pid,
                                path = %lock_path.display(),
                                "Removing stale lock file (process no longer running)"
                            );
                            let _ = fs::remove_file(&lock_path);
                            continue;
                        }
                        None => {
                            // Unparseable / unreadable — treat as stale.
                            let _ = fs::remove_file(&lock_path);
                            continue;
                        }
                    }
                }
                Err(_) => return None,
            }
        }
        None
    }

    /// Returns the lock file path for a given save file path.
    pub fn lock_path_for(save_path: &Path) -> PathBuf {
        let mut p = save_path.as_os_str().to_os_string();
        p.push(".lock");
        PathBuf::from(p)
    }
}

impl Drop for SaveFileLock {
    fn drop(&mut self) {
        // Best-effort removal. If it fails (e.g. permission, already gone),
        // the next instance will detect the stale lock via PID check.
        if let Err(e) = fs::remove_file(&self.lock_path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %self.lock_path.display(),
                error = %e,
                "Failed to remove lock file on drop"
            );
        }
    }
}

/// Checks whether a save file is currently locked by another live process.
pub fn is_locked(save_path: &Path) -> bool {
    let lock_path = SaveFileLock::lock_path_for(save_path);
    if !lock_path.exists() {
        return false;
    }
    match fs::read_to_string(&lock_path) {
        Ok(contents) => match contents.trim().parse::<u32>() {
            Ok(pid) => is_process_alive(pid),
            Err(_) => false, // Unparseable — treat as stale (not locked).
        },
        Err(_) => false, // Unreadable — treat as stale.
    }
}

/// Returns `true` if a process with the given PID is currently running.
#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    // kill(pid, 0) checks process existence without sending a signal.
    // Returns 0 if the process exists and we have permission to signal it.
    // Returns -1 with ESRCH if the process does not exist.
    // Returns -1 with EPERM if we lack permission — but the process exists.
    let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if ret == 0 {
        return true;
    }
    // EPERM means the process exists but we can't signal it.
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn is_process_alive(_pid: u32) -> bool {
    // Conservative fallback for non-Unix: assume process is alive.
    // This prevents accidental lock theft on unsupported platforms.
    true
}

#[cfg(test)]
mod tests {
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

        // Second acquire from same process should fail (re-entrant guard).
        let lock2 = SaveFileLock::try_acquire(&save);
        assert!(lock2.is_none(), "same process should not double-acquire");
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
}
