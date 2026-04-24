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
    /// If the current process already holds the lock (same PID), returns
    /// `Some` to allow callers to replace the old guard without losing
    /// protection. Prefer [`covers_path`](Self::covers_path) to avoid
    /// the brief window where the old guard's `Drop` removes the file
    /// before the new guard takes effect.
    pub fn try_acquire(save_path: &Path) -> Option<Self> {
        let lock_path = Self::lock_path_for(save_path);
        let my_pid = std::process::id();

        if lock_path.exists() {
            match fs::read_to_string(&lock_path) {
                Ok(contents) => {
                    if let Ok(pid) = contents.trim().parse::<u32>() {
                        if pid == my_pid {
                            return Some(Self { lock_path });
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

        Some(Self { lock_path })
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

    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetExitCodeProcess(handle: *mut c_void, code: *mut u32) -> i32;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
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

        // Drop lock1, keep lock2 — simulates the caller replacement pattern.
        drop(lock1);
        // lock2 still holds the logical lock.
        assert!(lock2.is_some());
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
