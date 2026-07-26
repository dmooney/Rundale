//! Advisory save-file locking.
//!
//! New locks use an atomically-created `<save_path>.lock` directory containing
//! a fully-written owner record. The directory is the exclusion primitive:
//! while its owner record is being published, peers conservatively treat it as
//! locked. Plain PID lock files from older Parish versions remain readable and
//! a parseable dead owner is migrated safely on the next acquisition.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

const OWNER_FILENAME: &str = "owner.json";
const OWNER_VERSION: u8 = 1;
static OWNER_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OwnerRecord {
    version: u8,
    pid: u32,
    token: String,
}

impl OwnerRecord {
    fn new(pid: u32) -> Self {
        let counter = OWNER_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            version: OWNER_VERSION,
            pid,
            token: format!("{pid}-{counter}"),
        }
    }

    fn is_valid(&self) -> bool {
        self.version == OWNER_VERSION && self.pid > 0 && !self.token.trim().is_empty()
    }
}

#[derive(Clone)]
struct LiveGuardEntry {
    owner: OwnerRecord,
    refcount: Arc<AtomicUsize>,
}

struct LiveGuardRegistry(Mutex<Option<HashMap<PathBuf, LiveGuardEntry>>>);

impl LiveGuardRegistry {
    const fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn with_lock<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut HashMap<PathBuf, LiveGuardEntry>) -> R,
    {
        let mut guard = self.0.lock().expect("LiveGuardRegistry mutex poisoned");
        f(guard.get_or_insert_with(HashMap::new))
    }
}

static LIVE_GUARDS: LiveGuardRegistry = LiveGuardRegistry::new();

/// Advisory lock backed by a `.lock` owner directory.
pub struct SaveFileLock {
    lock_path: PathBuf,
    owner: OwnerRecord,
    refcount: Arc<AtomicUsize>,
}

enum LockObservation {
    Missing,
    Owner(OwnerRecord),
    LegacyPid(u32),
    Invalid,
}

impl SaveFileLock {
    /// Attempts to acquire the save lock.
    ///
    /// Only a parseable owner whose process is known dead is eligible for
    /// cleanup. Missing, unreadable, incomplete, or malformed owner state is
    /// treated as locked so a peer can never steal a just-published lock.
    pub fn try_acquire(save_path: &Path) -> Option<Self> {
        Self::try_acquire_with(save_path, std::process::id(), is_process_alive)
    }

    fn try_acquire_with(
        save_path: &Path,
        my_pid: u32,
        is_alive: impl Fn(u32) -> bool + Copy,
    ) -> Option<Self> {
        let lock_path = Self::lock_path_for(save_path);
        if let Some(guard) = Self::reentrant_acquire(&lock_path, my_pid) {
            return Some(guard);
        }

        let cleanup_path = Self::cleanup_path_for(&lock_path);
        if path_exists_or_unreadable(&cleanup_path) {
            return None;
        }

        match fs::create_dir(&lock_path) {
            Ok(()) => return Self::publish_owner(lock_path, OwnerRecord::new(my_pid)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }

        match observe_lock(&lock_path) {
            LockObservation::Owner(owner) if owner.pid == my_pid => {
                Self::reentrant_acquire_matching(&lock_path, &owner)
            }
            LockObservation::Owner(owner) if is_alive(owner.pid) => None,
            LockObservation::LegacyPid(pid) if is_alive(pid) => None,
            LockObservation::Owner(_) | LockObservation::LegacyPid(_) => {
                Self::replace_stale(lock_path, cleanup_path, my_pid, is_alive)
            }
            LockObservation::Missing => Self::try_acquire_with(save_path, my_pid, is_alive),
            LockObservation::Invalid => None,
        }
    }

    fn replace_stale(
        lock_path: PathBuf,
        cleanup_path: PathBuf,
        my_pid: u32,
        is_alive: impl Fn(u32) -> bool + Copy,
    ) -> Option<Self> {
        let cleanup = StaleCleanupGuard::try_acquire(cleanup_path)?;

        // Re-read only after winning the cleanup mutex. Another contender may
        // already have replaced the stale owner while this caller waited.
        match observe_lock(&lock_path) {
            LockObservation::Owner(owner) if owner.pid == my_pid => {
                return Self::reentrant_acquire_matching(&lock_path, &owner);
            }
            LockObservation::Owner(owner) if is_alive(owner.pid) => return None,
            LockObservation::LegacyPid(pid) if is_alive(pid) => return None,
            LockObservation::Invalid => return None,
            LockObservation::Owner(owner) => {
                tracing::info!(
                    pid = owner.pid,
                    path = %lock_path.display(),
                    "Replacing stale save-lock owner directory"
                );
                if fs::remove_dir_all(&lock_path).is_err() {
                    return None;
                }
            }
            LockObservation::LegacyPid(pid) => {
                tracing::info!(
                    pid,
                    path = %lock_path.display(),
                    "Replacing stale legacy save-lock file"
                );
                if fs::remove_file(&lock_path).is_err() {
                    return None;
                }
            }
            LockObservation::Missing => {}
        }

        let result = match fs::create_dir(&lock_path) {
            Ok(()) => Self::publish_owner(lock_path, OwnerRecord::new(my_pid)),
            Err(_) => None,
        };
        drop(cleanup);
        result
    }

    fn publish_owner(lock_path: PathBuf, owner: OwnerRecord) -> Option<Self> {
        let temp_path = lock_path.join(format!(".{OWNER_FILENAME}.tmp"));
        let owner_path = lock_path.join(OWNER_FILENAME);
        let body = serde_json::to_vec(&owner).ok()?;
        let publish_result = (|| -> std::io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)?;
            file.write_all(&body)?;
            file.sync_all()?;
            fs::rename(&temp_path, &owner_path)?;
            #[cfg(unix)]
            fs::File::open(&lock_path)?.sync_all()?;
            Ok(())
        })();
        if publish_result.is_err() {
            let _ = fs::remove_file(&temp_path);
            let _ = fs::remove_dir_all(&lock_path);
            return None;
        }

        let refcount = Arc::new(AtomicUsize::new(1));
        LIVE_GUARDS.with_lock(|map| {
            map.insert(
                lock_path.clone(),
                LiveGuardEntry {
                    owner: owner.clone(),
                    refcount: Arc::clone(&refcount),
                },
            );
        });
        Some(Self {
            lock_path,
            owner,
            refcount,
        })
    }

    fn reentrant_acquire(lock_path: &Path, my_pid: u32) -> Option<Self> {
        LIVE_GUARDS.with_lock(|map| {
            let entry = map.get(lock_path)?;
            (entry.owner.pid == my_pid
                && matches!(
                    observe_lock(lock_path),
                    LockObservation::Owner(ref owner) if owner == &entry.owner
                ))
            .then(|| {
                entry.refcount.fetch_add(1, Ordering::AcqRel);
                Self {
                    lock_path: lock_path.to_path_buf(),
                    owner: entry.owner.clone(),
                    refcount: Arc::clone(&entry.refcount),
                }
            })
        })
    }

    fn reentrant_acquire_matching(lock_path: &Path, owner: &OwnerRecord) -> Option<Self> {
        LIVE_GUARDS.with_lock(|map| {
            let entry = map.get(lock_path)?;
            (&entry.owner == owner).then(|| {
                entry.refcount.fetch_add(1, Ordering::AcqRel);
                Self {
                    lock_path: lock_path.to_path_buf(),
                    owner: entry.owner.clone(),
                    refcount: Arc::clone(&entry.refcount),
                }
            })
        })
    }

    /// Returns the lock path for a save file.
    pub fn lock_path_for(save_path: &Path) -> PathBuf {
        let mut path = save_path.as_os_str().to_os_string();
        path.push(".lock");
        PathBuf::from(path)
    }

    fn cleanup_path_for(lock_path: &Path) -> PathBuf {
        let mut path = lock_path.as_os_str().to_os_string();
        path.push(".cleanup");
        PathBuf::from(path)
    }
}

impl Drop for SaveFileLock {
    fn drop(&mut self) {
        if self.refcount.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }

        LIVE_GUARDS.with_lock(|map| {
            if map.get(&self.lock_path).is_some_and(|entry| {
                entry.owner == self.owner && Arc::ptr_eq(&entry.refcount, &self.refcount)
            }) {
                map.remove(&self.lock_path);
            }
        });

        // Never delete a successor's lock if the on-disk owner changed.
        if matches!(
            observe_lock(&self.lock_path),
            LockObservation::Owner(ref owner) if owner == &self.owner
        ) && let Err(error) = fs::remove_dir_all(&self.lock_path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %self.lock_path.display(),
                %error,
                "Failed to remove save-lock owner directory on drop"
            );
        }
    }
}

struct StaleCleanupGuard {
    path: PathBuf,
}

impl StaleCleanupGuard {
    fn try_acquire(path: PathBuf) -> Option<Self> {
        fs::create_dir(&path).ok().map(|()| Self { path })
    }
}

impl Drop for StaleCleanupGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

fn path_exists_or_unreadable(path: &Path) -> bool {
    path.try_exists().unwrap_or(true)
}

fn observe_lock(lock_path: &Path) -> LockObservation {
    let metadata = match fs::symlink_metadata(lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LockObservation::Missing;
        }
        Err(_) => return LockObservation::Invalid,
    };

    if metadata.is_dir() {
        let body = match fs::read(lock_path.join(OWNER_FILENAME)) {
            Ok(body) => body,
            Err(_) => return LockObservation::Invalid,
        };
        return match serde_json::from_slice::<OwnerRecord>(&body) {
            Ok(owner) if owner.is_valid() => LockObservation::Owner(owner),
            _ => LockObservation::Invalid,
        };
    }

    if metadata.is_file() {
        return match fs::read_to_string(lock_path)
            .ok()
            .and_then(|body| body.trim().parse::<u32>().ok())
        {
            Some(pid) if pid > 0 => LockObservation::LegacyPid(pid),
            _ => LockObservation::Invalid,
        };
    }

    LockObservation::Invalid
}

/// Checks whether a save file is currently locked.
///
/// Invalid or unreadable state is conservatively locked. A parseable stale PID
/// is reported unlocked so discovery UIs may identify it as reclaimable.
pub fn is_locked(save_path: &Path) -> bool {
    match observe_lock(&SaveFileLock::lock_path_for(save_path)) {
        LockObservation::Missing => false,
        LockObservation::Owner(owner) => is_process_alive(owner.pid),
        LockObservation::LegacyPid(pid) => is_process_alive(pid),
        LockObservation::Invalid => true,
    }
}

#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::ffi::c_void;

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
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
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut exit_code = 0;
        let ok = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        ok != 0 && exit_code == STILL_ACTIVE
    }
}

#[cfg(not(any(unix, windows)))]
fn is_process_alive(_pid: u32) -> bool {
    true
}

#[cfg(test)]
mod tests;
