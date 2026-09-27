//! Advisory save-file locking.
//!
//! A save is locked by an exclusive kernel lock (`flock` on Unix, including
//! iOS; `LockFileEx` on Windows) held on `<save_path>.lock` through
//! [`std::fs::File::try_lock`]. The kernel releases the lock when its owner
//! process ends for any reason, including a force-quit or an iOS jetsam kill,
//! so a relaunch never depends on whether a recorded PID is still alive. The
//! file records the owner's PID for diagnostics only. See ADR-026.
//!
//! Earlier builds locked with an owner directory at the same path. A leftover
//! directory whose recorded owner is dead is removed on the next acquisition;
//! a live or unreadable one keeps the save locked.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::Deserialize;

/// Owner record file inside a lock directory written by earlier builds.
const LEGACY_OWNER_FILENAME: &str = "owner.json";
const LEGACY_OWNER_VERSION: u8 = 1;
/// Bounds retries when a released lock file is replaced while opening it.
const MAX_OPEN_ATTEMPTS: usize = 8;

/// Locks held by this process, by lock path. The kernel lock belongs to one
/// open file, so a second acquisition in the same process shares it instead of
/// opening the file again (which would conflict with our own lock).
struct LiveGuards(Mutex<Option<HashMap<PathBuf, LiveGuard>>>);

struct LiveGuard {
    file: Arc<File>,
    holders: usize,
}

impl LiveGuards {
    const fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn lock(&self) -> MutexGuard<'_, Option<HashMap<PathBuf, LiveGuard>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

static LIVE_GUARDS: LiveGuards = LiveGuards::new();

/// Advisory lock on a save file, held until the last guard for it drops.
pub struct SaveFileLock {
    lock_path: PathBuf,
    file: Arc<File>,
}

impl SaveFileLock {
    /// Attempts to acquire the save lock without blocking.
    ///
    /// Returns `None` when another process holds it, when a lock directory
    /// from an earlier build names a live or unreadable owner, or when the
    /// lock file cannot be opened. Acquiring again in the process that holds
    /// the lock succeeds and shares it.
    pub fn try_acquire(save_path: &Path) -> Option<Self> {
        let lock_path = Self::lock_path_for(save_path);
        let mut guards = LIVE_GUARDS.lock();
        let guards = guards.get_or_insert_with(HashMap::new);
        if let Some(live) = guards.get_mut(&lock_path) {
            live.holders += 1;
            return Some(Self {
                lock_path,
                file: Arc::clone(&live.file),
            });
        }

        if !clear_dead_legacy_directory(&lock_path) {
            return None;
        }
        let file = Arc::new(lock_file(&lock_path)?);
        guards.insert(
            lock_path.clone(),
            LiveGuard {
                file: Arc::clone(&file),
                holders: 1,
            },
        );
        Some(Self { lock_path, file })
    }

    /// Returns the lock path for a save file.
    pub fn lock_path_for(save_path: &Path) -> PathBuf {
        let mut path = save_path.as_os_str().to_os_string();
        path.push(".lock");
        PathBuf::from(path)
    }
}

impl Drop for SaveFileLock {
    fn drop(&mut self) {
        let mut guards = LIVE_GUARDS.lock();
        let Some(guards) = guards.as_mut() else {
            return;
        };
        let Some(live) = guards.get_mut(&self.lock_path) else {
            return;
        };
        live.holders -= 1;
        if live.holders > 0 {
            return;
        }
        guards.remove(&self.lock_path);

        // Remove the file while still holding its lock, and only if the path
        // still names our file, so a successor's lock file is never removed.
        // A contender that opened our file before the removal locks an
        // unlinked file, notices, and retries on a fresh one.
        if same_file(&self.file, &self.lock_path)
            && let Err(error) = fs::remove_file(&self.lock_path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %self.lock_path.display(),
                %error,
                "Failed to remove save-lock file on release"
            );
        }
        // The kernel lock is released when the last handle to the file closes.
    }
}

/// Opens `lock_path` and takes the exclusive kernel lock on it.
fn lock_file(lock_path: &Path) -> Option<File> {
    for _ in 0..MAX_OPEN_ATTEMPTS {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .ok()?;
        if file.try_lock().is_err() {
            return None;
        }
        // The previous owner may have released and removed this file between
        // our open and our lock. Its path now names nothing or a newer file,
        // so retry on whatever the path names now.
        if !same_file(&file, lock_path) {
            continue;
        }
        record_owner_pid(&mut file);
        return Some(file);
    }
    None
}

/// Records this process's PID in the lock file, for diagnostics and so older
/// builds (which read the PID) keep treating a held save as locked.
fn record_owner_pid(file: &mut File) {
    let written = file
        .set_len(0)
        .and_then(|()| file.rewind())
        .and_then(|()| file.write_all(std::process::id().to_string().as_bytes()));
    if let Err(error) = written {
        tracing::debug!(%error, "Could not record the save-lock owner PID");
    }
}

#[cfg(unix)]
fn same_file(file: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (file.metadata(), fs::metadata(path)) {
        (Ok(open), Ok(named)) => open.dev() == named.dev() && open.ino() == named.ino(),
        _ => false,
    }
}

/// Windows cannot create a file at a path whose previous file is pending
/// deletion while any handle to it is open, so the path always names the file
/// that was opened.
#[cfg(not(unix))]
fn same_file(_file: &File, path: &Path) -> bool {
    path.is_file()
}

#[derive(Deserialize)]
struct LegacyOwnerRecord {
    version: u8,
    pid: u32,
}

enum LegacyDirectory {
    /// No lock directory at the path.
    Absent,
    /// A lock directory whose recorded owner process is gone.
    DeadOwner,
    /// A lock directory with a live, unreadable, or incomplete owner.
    Held,
}

fn observe_legacy_directory(lock_path: &Path) -> LegacyDirectory {
    match fs::symlink_metadata(lock_path) {
        Ok(metadata) if metadata.is_dir() => {}
        _ => return LegacyDirectory::Absent,
    }
    let owner = fs::read(lock_path.join(LEGACY_OWNER_FILENAME))
        .ok()
        .and_then(|body| serde_json::from_slice::<LegacyOwnerRecord>(&body).ok());
    match owner {
        Some(owner)
            if owner.version == LEGACY_OWNER_VERSION
                && owner.pid > 0
                && !is_process_alive(owner.pid) =>
        {
            LegacyDirectory::DeadOwner
        }
        _ => LegacyDirectory::Held,
    }
}

/// Removes a lock directory left by an earlier build whose owner is dead.
/// Returns `false` when such a directory still holds the save.
fn clear_dead_legacy_directory(lock_path: &Path) -> bool {
    match observe_legacy_directory(lock_path) {
        LegacyDirectory::Absent => true,
        LegacyDirectory::Held => false,
        LegacyDirectory::DeadOwner => {
            tracing::info!(
                path = %lock_path.display(),
                "Removing save-lock directory left by a dead owner"
            );
            match fs::remove_dir_all(lock_path) {
                Ok(()) => true,
                Err(error) => error.kind() == std::io::ErrorKind::NotFound,
            }
        }
    }
}

/// Checks whether a save file is currently locked.
///
/// Probes the kernel lock with a shared lock that is released at once. A
/// contender acquiring at the same instant can be refused once; it is never
/// granted a lock another process holds. Unreadable state reports locked.
pub fn is_locked(save_path: &Path) -> bool {
    let lock_path = SaveFileLock::lock_path_for(save_path);
    if LIVE_GUARDS
        .lock()
        .as_ref()
        .is_some_and(|guards| guards.contains_key(&lock_path))
    {
        return true;
    }
    match fs::symlink_metadata(&lock_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
        Ok(metadata) if metadata.is_dir() => {
            return matches!(observe_legacy_directory(&lock_path), LegacyDirectory::Held);
        }
        Ok(_) => {}
    }
    match File::open(&lock_path) {
        Ok(file) => file.try_lock_shared().is_err(),
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    let result = unsafe { libc::kill(pid, 0) };
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
