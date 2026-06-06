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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Advisory lock backed by a `.lock` sidecar file.
///
/// On creation, writes the current PID to `<save_path>.lock`.
/// On drop, removes the lock file (best-effort) **only** when this guard
/// is the last live owner (refcount reaches zero).
///
/// # Re-entrant safety (codex P1 — round 2)
///
/// When the same process calls [`try_acquire`](Self::try_acquire) while already
/// holding a lock on the same path, a shared [`AtomicUsize`] refcount is
/// incremented rather than silencing the previous guard.  Every guard holds an
/// `Arc` to the same counter.  The lock file is deleted only when the **last**
/// guard's `Drop` decrements the counter to zero.  This correctly handles both:
///
/// * **Replacement pattern** — `state.save_lock = Some(new_lock)` drops the old
///   guard but keeps the new guard alive; refcount stays ≥ 1 so the file is not
///   removed until the new guard itself drops.
/// * **Transient pattern** — `let _ = try_acquire(…)` immediately drops the
///   returned guard, but the original caller's guard still holds a reference so
///   the refcount is still ≥ 1 and the file is preserved.
pub struct SaveFileLock {
    lock_path: PathBuf,
    /// Shared refcount for all live guards on the same `lock_path`.
    /// File deletion happens only when the last `Drop` decrements to zero.
    refcount: Arc<AtomicUsize>,
}

// ---------------------------------------------------------------------------
// Process-global live-guard registry
// ---------------------------------------------------------------------------
//
// Maps `lock_path → refcount` for every owned `SaveFileLock` in this process.
// Multiple reentrant guards for the same path share the same `Arc<AtomicUsize>`.
//
// `std::sync::Mutex` (not `tokio::sync::Mutex`) so the registry is usable
// from sync `Drop` without an async runtime.

struct LiveGuardRegistry(Mutex<Option<HashMap<PathBuf, Arc<AtomicUsize>>>>);

impl LiveGuardRegistry {
    const fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn with_lock<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut HashMap<PathBuf, Arc<AtomicUsize>>) -> R,
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
    /// If the current process already holds the lock (same PID), a new
    /// guard sharing the same refcount is returned.  The lock file is
    /// removed only when **all** live guards for the same path have dropped.
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
                        let refcount = Arc::new(AtomicUsize::new(1));
                        LIVE_GUARDS.with_lock(|map| {
                            map.insert(lock_path.clone(), Arc::clone(&refcount));
                        });
                        return Some(Self {
                            lock_path,
                            refcount,
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
                            // the lock.  Bump the shared refcount and return a
                            // new guard pointing at the same Arc.  This fixes
                            // codex P1 (both replacement and transient patterns).
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
    /// PID.  Bumps the shared refcount and returns a new guard holding the
    /// same `Arc<AtomicUsize>`.
    ///
    /// # Codex P1 — refcount approach
    ///
    /// Unlike the prior "silence prior guard" approach, this never modifies
    /// any existing guard.  Each guard independently decrements the counter
    /// on drop; only the guard that decrements it to zero deletes the file.
    /// Transient reentrant guards (`let _ = try_acquire(…)`) just bump-then-
    /// decrement — the counter never falls to zero while any other guard
    /// is alive.
    fn reentrant_acquire(lock_path: PathBuf) -> Option<Self> {
        LIVE_GUARDS.with_lock(|map| {
            if let Some(refcount) = map.get(&lock_path) {
                // Bump the refcount while we hold the registry lock so
                // no concurrent Drop can race us to zero.
                refcount.fetch_add(1, Ordering::AcqRel);
                Some(Self {
                    lock_path,
                    refcount: Arc::clone(refcount),
                })
            } else {
                // No entry in the registry — this shouldn't happen if
                // the PID in the file is ours, but be conservative.
                None
            }
        })
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
        // Decrement our share of the refcount.  If we were the last holder
        // (previous value was 1, now 0), remove the registry entry and
        // delete the lock file.  Use AcqRel so the decrement pairs with
        // the Acquire load used for the zero-check, and so that every
        // preceding store from all threads is visible before we delete.
        let prev = self.refcount.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            // We are the last guard — clean up.
            LIVE_GUARDS.with_lock(|map| {
                // Only remove our entry if it still points to our Arc
                // (a future first-acquire for a new session may have already
                // replaced it after we hit zero but before we took the lock —
                // unlikely but defensive).
                if map
                    .get(&self.lock_path)
                    .is_some_and(|rc| Arc::ptr_eq(rc, &self.refcount))
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
        // If prev > 1: other guards still alive, do nothing.
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
mod tests;
