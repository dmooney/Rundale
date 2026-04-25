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
/// # Re-entrant safety
///
/// When the same process calls [`try_acquire`](Self::try_acquire) while already
/// holding a lock on the same path (e.g. `state.save_lock = Some(new_lock)`),
/// the new guard becomes the sole owner and the previous guard's `Drop` is
/// silenced via a shared [`AtomicBool`].  This prevents the old guard's `Drop`
/// from deleting the lock file while the new guard is still alive — the race
/// identified in codex P1.
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
    pub fn try_acquire(save_path: &Path) -> Option<Self> {
        let lock_path = Self::lock_path_for(save_path);
        let my_pid = std::process::id();

        if lock_path.exists() {
            match fs::read_to_string(&lock_path) {
                Ok(contents) => {
                    if let Ok(pid) = contents.trim().parse::<u32>() {
                        if pid == my_pid {
                            // Re-entrant acquire: same process already holds
                            // the lock.  Silence the previous guard and return
                            // a fresh owning guard.
                            return Self::reentrant_acquire(lock_path);
                        }
                        if is_process_alive(pid) {
                            return None;
                        }
                        // Stale lock — remove it and proceed.
                        tracing::info!(
                            pid,
                            path = %lock_path.display(),
                            "Removing stale lock file (process no longer running)"
                        );
                        let _ = fs::remove_file(&lock_path);
                    }
                    // Unparseable content — treat as stale.
                }
                Err(_) => {
                    // Unreadable lock file — treat as stale.
                    let _ = fs::remove_file(&lock_path);
                }
            }
        }

        // Write PID atomically: write to .tmp then rename.
        let tmp_path = lock_path.with_extension("lock.tmp");
        let mut f = fs::File::create(&tmp_path).ok()?;
        write!(f, "{}", my_pid).ok()?;
        f.sync_all().ok()?;
        drop(f);

        if fs::rename(&tmp_path, &lock_path).is_err() {
            let _ = fs::remove_file(&tmp_path);
            return None;
        }

        // Verify we won the race: re-read and confirm our PID is there.
        match fs::read_to_string(&lock_path) {
            Ok(contents) if contents.trim() == my_pid.to_string() => {}
            _ => return None,
        }

        let flag = Arc::new(AtomicBool::new(true));
        LIVE_GUARDS.with_lock(|map| {
            map.insert(lock_path.clone(), Arc::clone(&flag));
        });

        Some(Self {
            lock_path,
            should_delete: flag,
        })
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
    fn test_reentrant_acquire_same_process_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let save = dir.path().join("test.db");
        fs::write(&save, b"").unwrap();

        let lock1 = SaveFileLock::try_acquire(&save);
        assert!(lock1.is_some());

        // Re-entrant acquire from same process now succeeds so callers
        // can swap the guard without losing lock protection.
        let lock2 = SaveFileLock::try_acquire(&save);
        assert!(lock2.is_some(), "same process re-acquire should succeed");

        let lock_path = SaveFileLock::lock_path_for(&save);

        // Drop lock1 (silenced by reentrant_acquire) — the lock file must
        // survive because lock2 is the new active owner.
        drop(lock1);
        assert!(
            lock_path.exists(),
            "lock file must survive dropping the silenced (old) guard"
        );

        // Drop lock2 (the current owner) — now the file should be removed.
        drop(lock2);
        assert!(
            !lock_path.exists(),
            "lock file should be removed when the owning guard drops"
        );
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
}
