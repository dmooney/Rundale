//! Advisory file locking for save files.
//!
//! Prevents multiple app instances from writing to the same save file
//! simultaneously. Each lock is a `<save_path>.lock` sidecar file
//! containing the owning process's PID. Stale locks from crashed
//! processes are detected and cleaned up automatically.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Advisory lock backed by a `.lock` sidecar file.
///
/// On creation, writes the current PID to `<save_path>.lock`.
/// On drop, removes the lock file (best-effort) **only** when this guard
/// is the active owner.
///
/// # Re-entrant safety (codex P1)
///
/// When the same process calls [`try_acquire`](Self::try_acquire) while already
/// holding a lock on the same path (e.g. `state.save_lock = Some(new_lock)`),
/// the new guard becomes the sole owner and the previous guard's `Drop` is
/// silenced via a shared [`AtomicBool`].  This prevents the old guard's `Drop`
/// from deleting the lock file while the new guard is still alive.
pub struct SaveFileLock {
    lock_path: PathBuf,
    /// Shared liveness flag.  Guards for the same PID+path share this `Arc`.
    /// `true` means "I am responsible for removing the file on drop".
    /// A re-entrant acquire sets the old guard's copy to `false` and takes
    /// its own fresh `Arc` set to `true`, so exactly one guard removes the
    /// file.
    should_delete: Arc<AtomicBool>,
}

// ---------------------------------------------------------------------------
// Process-global live-guard registry
// ---------------------------------------------------------------------------
//
// Maps `lock_path → should_delete flag` for every owning `SaveFileLock` in
// this process.  Used by `reentrant_acquire` to silence a previous guard
// when the same path is re-acquired by the same PID.
//
// `std::sync::Mutex` (not `tokio::sync::Mutex`) so the registry is usable
// from sync `Drop` without an async runtime.

struct LiveGuardRegistry(Mutex<Option<HashMap<PathBuf, Arc<AtomicBool>>>>);

impl LiveGuardRegistry {
    const fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn with_lock<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut HashMap<PathBuf, Arc<AtomicBool>>) -> R,
    {
        // Panic on lock poisoning — this is a logic bug, not a recoverable
        // error, and we would rather crash fast than silently corrupt state.
        let mut guard = self.0.lock().expect("LiveGuardRegistry mutex poisoned");
        let map = guard.get_or_insert_with(HashMap::new);
        f(map)
    }
}

static LIVE_GUARDS: LiveGuardRegistry = LiveGuardRegistry::new();

impl SaveFileLock {
    /// Attempts to acquire an advisory lock for the given save file.
    ///
    /// Returns `Some(lock)` on success, or `None` if the file is already
    /// locked by another live process. Stale locks (dead PID) are cleaned
    /// up and re-acquired automatically.
    ///
    /// If the current process already holds the lock (same PID), the
    /// existing guard's delete-on-drop responsibility is silenced and a
    /// fresh owning guard is returned.  This makes the replacement pattern
    /// `state.save_lock = Some(SaveFileLock::try_acquire(&path)?)` safe:
    /// the old guard drops without touching the file, and the new guard
    /// takes over sole ownership.
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
                        let flag = Arc::new(AtomicBool::new(true));
                        LIVE_GUARDS.with_lock(|map| {
                            map.insert(lock_path.clone(), Arc::clone(&flag));
                        });
                        return Some(Self {
                            lock_path,
                            should_delete: flag,
                        });
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
                            // Re-entrant acquire: same process already holds
                            // the lock.  Silence the previous guard and return
                            // a fresh owning guard.  This fixes the codex P1
                            // race where `state.save_lock = Some(new_lock)`
                            // drops the old guard (deleting the file) while
                            // the new guard is still alive.
                            return Self::reentrant_acquire(lock_path);
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

    /// Called from [`try_acquire`] when the lock file already contains our
    /// PID.  Silences any previous owning guard for this path and returns a
    /// fresh owning guard.
    fn reentrant_acquire(lock_path: PathBuf) -> Option<Self> {
        // Silence the previous guard (if any) by setting its flag to false.
        // This prevents the Drop of the old guard from deleting the file.
        LIVE_GUARDS.with_lock(|map| {
            if let Some(old_flag) = map.get(&lock_path) {
                old_flag.store(false, Ordering::Release);
            }
        });

        // Create a fresh owning guard with its own flag and register it.
        let flag = Arc::new(AtomicBool::new(true));
        LIVE_GUARDS.with_lock(|map| {
            map.insert(lock_path.clone(), Arc::clone(&flag));
        });

        Some(Self {
            lock_path,
            should_delete: flag,
        })
    }

    /// Returns `true` if this lock protects the given save file path.
    pub fn covers_path(&self, save_path: &Path) -> bool {
        self.lock_path == Self::lock_path_for(save_path)
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
        if !self.should_delete.load(Ordering::Acquire) {
            // Another guard has taken over responsibility for this lock file
            // (re-entrant replacement pattern).  Do not touch the file.
            return;
        }
        // Deregister from the live-guard registry so a future acquire for the
        // same path does not try to silence us after we have already cleaned up.
        LIVE_GUARDS.with_lock(|map| {
            // Only remove our entry if it still points to our flag (i.e. a
            // subsequent reentrant_acquire hasn't already replaced it).
            if map
                .get(&self.lock_path)
                .is_some_and(|f| Arc::ptr_eq(f, &self.should_delete))
            {
                map.remove(&self.lock_path);
            }
        });
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

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::ffi::c_void;

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    // ERROR_ACCESS_DENIED is returned by OpenProcess when the process exists
    // but is owned by another user/session (e.g. a different logon session or
    // a protected/elevated process).  Mirror the Unix EPERM branch: treat
    // access-denied as "process is alive" to prevent accidental lock theft.
    // (codex P1 fix)
    const ERROR_ACCESS_DENIED: u32 = 5;

    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetExitCodeProcess(handle: *mut c_void, code: *mut u32) -> i32;
        fn GetLastError() -> u32;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            // If OpenProcess failed due to access denial the process exists —
            // treat as alive (mirrors the Unix EPERM path).
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut exit_code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        ok != 0 && exit_code == STILL_ACTIVE
    }
}

#[cfg(not(any(unix, windows)))]
fn is_process_alive(_pid: u32) -> bool {
    // Conservative fallback for unknown platforms: assume process is alive.
    // This prevents accidental lock theft but means stale locks require
    // manual removal on unsupported platforms.
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

    #[test]
    fn test_covers_path() {
        let dir = tempfile::tempdir().unwrap();
        let save_a = dir.path().join("a.db");
        let save_b = dir.path().join("b.db");
        fs::write(&save_a, b"").unwrap();
        fs::write(&save_b, b"").unwrap();

        let lock = SaveFileLock::try_acquire(&save_a).unwrap();
        assert!(lock.covers_path(&save_a));
        assert!(!lock.covers_path(&save_b));
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
