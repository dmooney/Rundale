//! Transactional storage for the portable/mobile Parish runtime.
//!
//! Mobile saves deliberately use a small schema separate from the legacy
//! branching desktop save schema.  The store owns one SQLite connection and a
//! kernel-backed save lock, so a mobile runtime has one local authority for its
//! current domain projection, logical requests, and durable semantic events.
//!
//! The store does not persist incremental model tokens.  Callers persist the
//! acceptance, retry, terminal, and completed semantic boundaries as event
//! records, then use [`MobileStore::commit`] for one generation-guarded
//! transaction.  A `None` domain update means that the previous current
//! projection is retained; this is useful for the acceptance boundary.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io;
#[cfg(test)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde_json::Value;

use crate::IntoParishDbError as _;
use parish_types::ParishError;

/// The first schema owned by the mobile runtime.
pub const MOBILE_FORMAT_VERSION: i64 = 1;

/// Maximum number of durable events returned by one history query.
///
/// The caller may request a smaller page.  A larger request is clamped rather
/// than allowed to turn a history read into an unbounded load.
pub const MAX_EVENT_PAGE_SIZE: usize = 256;

const METADATA_TABLE: &str = "mobile_metadata";
const STATE_TABLE: &str = "mobile_state";
const REQUESTS_TABLE: &str = "mobile_requests";
const EVENTS_TABLE: &str = "mobile_events";

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Current portable-save projection and identity metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct MobileSave {
    /// The last committed domain projection, if one has been written.
    pub current_domain: Option<Value>,
    /// Monotonic local generation used for compare-and-swap commits.
    pub generation: u64,
    /// Stable identity for this local save/session.
    pub session_id: String,
}

/// A logical request record persisted for restore and deduplication.
#[derive(Clone, Debug, PartialEq)]
pub struct MobileRequest {
    pub logical_id: String,
    pub json: Value,
}

/// Input to the request upsert portion of [`MobileStore::commit`].
pub type MobileRequestUpsert = MobileRequest;

impl MobileRequest {
    pub fn new(logical_id: impl Into<String>, json: Value) -> Self {
        Self {
            logical_id: logical_id.into(),
            json,
        }
    }
}

/// A new durable semantic event to insert idempotently.
#[derive(Clone, Debug, PartialEq)]
pub struct MobileEventInput {
    pub event_id: String,
    pub json: Value,
}

impl MobileEventInput {
    pub fn new(event_id: impl Into<String>, json: Value) -> Self {
        Self {
            event_id: event_id.into(),
            json,
        }
    }
}

/// A durable semantic event with its SQLite-assigned monotonic cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct MobileEvent {
    pub event_id: String,
    pub sequence: u64,
    pub json: Value,
}

/// Result of one atomic generation-guarded commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobileCommit {
    pub generation: u64,
    /// Sequence for each event input, in input order.  An idempotent duplicate
    /// returns the sequence of the existing event.
    pub event_sequences: Vec<u64>,
}

/// Kernel-backed lifetime lock for a mobile save or its bootstrap sidecar.
///
/// The legacy [`crate::SaveFileLock`] is deliberately not used here.  A
/// mobile process may be killed between filesystem operations, so an owner
/// directory and PID record can leave a save permanently unopenable.  The
/// operating system releases this exclusive lock when the owning file handle
/// dies.  For a completed save this handle is opened on the database inode,
/// which also covers hard-link aliases of the same file.
struct KernelSaveLock {
    file: File,
    #[cfg(unix)]
    inode_key: Option<InodeKey>,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct InodeKey {
    device: u64,
    inode: u64,
}

#[cfg(unix)]
static MOBILE_INODE_GUARDS: OnceLock<Mutex<HashSet<InodeKey>>> = OnceLock::new();

impl KernelSaveLock {
    fn acquire_existing(path: &Path) -> Result<Self, ParishError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| lock_error(path, error))?;
        acquire_database_lock(path, file)
    }

    fn acquire_sidecar(path: &Path) -> Result<Self, ParishError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| lock_error(path, error))?;
        file.try_lock_exclusive()
            .map_err(|error| lock_attempt_error(path, error))?;
        Ok(Self {
            file,
            #[cfg(unix)]
            inode_key: None,
        })
    }

    fn sync_all(&self) -> Result<(), ParishError> {
        self.file.sync_all().map_err(ParishError::Io)
    }
}

fn acquire_database_lock(path: &Path, file: File) -> Result<KernelSaveLock, ParishError> {
    #[cfg(unix)]
    let inode_key = inode_key(&file).map_err(|error| lock_error(path, error))?;
    #[cfg(unix)]
    {
        let guards = MOBILE_INODE_GUARDS.get_or_init(|| Mutex::new(HashSet::new()));
        let mut guards = guards.lock().expect("mobile inode guard mutex poisoned");
        if guards.contains(&inode_key) {
            return Err(database_error(format!(
                "mobile save is locked: {}",
                path.display()
            )));
        }
        lock_database_file(&file).map_err(|error| lock_attempt_error(path, error))?;
        guards.insert(inode_key);
    }
    #[cfg(not(unix))]
    lock_database_file(&file).map_err(|error| lock_attempt_error(path, error))?;
    Ok(KernelSaveLock {
        file,
        #[cfg(unix)]
        inode_key: Some(inode_key),
    })
}

#[cfg(unix)]
fn inode_key(file: &File) -> io::Result<InodeKey> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(InodeKey {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(unix)]
fn inode_key_for_path(path: &Path) -> io::Result<InodeKey> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path)?;
    Ok(InodeKey {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(unix)]
fn lock_database_file(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    // SQLite uses its own record-lock ranges. Reserve one byte far beyond
    // those ranges and bind it to this open file description, so closing an
    // unrelated SQLite descriptor cannot release the lifetime guard. Linux
    // and Apple platforms both expose F_OFD_SETLK; the latter covers macOS,
    // iOS, and the simulator. The lock is inode-scoped, so hard links contend.
    let mut lock = libc::flock {
        l_type: libc::F_WRLCK as libc::c_short,
        l_whence: libc::SEEK_SET as libc::c_short,
        l_start: 0x7fff_ffff_ffff,
        l_len: 1,
        l_pid: 0,
    };
    let result = unsafe { libc::fcntl(file.as_raw_fd(), database_lock_command(), &mut lock) };
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(unix))]
fn lock_database_file(file: &File) -> io::Result<()> {
    file.try_lock_exclusive()
}

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
fn database_lock_command() -> libc::c_int {
    libc::F_OFD_SETLK
}

#[cfg(all(unix, not(any(target_os = "linux", target_vendor = "apple"))))]
fn database_lock_command() -> libc::c_int {
    libc::F_SETLK
}

impl Drop for KernelSaveLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(inode_key) = self.inode_key.take()
            && let Some(guards) = MOBILE_INODE_GUARDS.get()
        {
            guards
                .lock()
                .expect("mobile inode guard mutex poisoned")
                .remove(&inode_key);
        }
    }
}

fn lock_error(path: &Path, error: io::Error) -> ParishError {
    database_error(format!(
        "cannot acquire mobile save lock {}: {error}",
        path.display()
    ))
}

fn lock_attempt_error(path: &Path, error: io::Error) -> ParishError {
    if error.kind() == io::ErrorKind::WouldBlock {
        database_error(format!("mobile save is locked: {}", path.display()))
    } else {
        lock_error(path, error)
    }
}

fn bootstrap_lock_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".mobile-bootstrap.lock");
    PathBuf::from(value)
}

fn bootstrap_temp_path(path: &Path) -> PathBuf {
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("mobile-save");
    path.with_file_name(format!(
        ".{filename}.mobile-bootstrap-{}-{counter}-{nanos}.tmp",
        std::process::id()
    ))
}

/// Remove abandoned bootstrap files for this save name.
///
/// The pattern is intentionally narrow: only hidden files emitted by this
/// module for this exact destination basename are considered. The bootstrap
/// sidecar is held by the caller, so another cooperating bootstrap for this
/// destination cannot be active while this scan runs.
fn cleanup_bootstrap_temps(path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let prefix = format!(".{filename}.mobile-bootstrap-");
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let candidate = entry.path();
        let Some(name) = candidate.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(suffix) = name
            .strip_prefix(&prefix)
            .and_then(|name| name.strip_suffix(".tmp"))
        else {
            continue;
        };
        if suffix.split('-').count() != 3
            || suffix.split('-').any(|part| {
                part.is_empty() || !part.chars().all(|character| character.is_ascii_digit())
            })
        {
            continue;
        }
        let _ = fs::remove_file(candidate);
    }
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<(), ParishError> {
    let parent = path.parent().ok_or_else(|| {
        database_error(format!(
            "mobile save path has no parent for directory sync: {}",
            path.display()
        ))
    })?;
    let directory = File::open(parent).map_err(|error| {
        database_error(format!(
            "cannot open mobile save parent for directory sync {}: {error}",
            parent.display()
        ))
    })?;
    directory.sync_all().map_err(|error| {
        database_error(format!(
            "cannot sync mobile save parent {}: {error}",
            parent.display()
        ))
    })
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<(), ParishError> {
    Ok(())
}

#[cfg(test)]
fn pause_for_bootstrap_interruption_test() {
    if std::env::var_os("PARISH_MOBILE_BOOTSTRAP_PAUSE").is_some() {
        println!("MOBILE_BOOTSTRAP_READY");
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Resolve a mobile save to a regular-file target and report whether it exists.
///
/// Existing symlinks are resolved to their target.  Missing saves use a
/// canonical parent, so a symlinked parent cannot create a second bootstrap
/// sidecar for the same destination.  Hard-link aliases are covered later by
/// [`KernelSaveLock`], which locks the file inode rather than its pathname.
fn resolve_mobile_path(path: &Path) -> Result<(PathBuf, bool), ParishError> {
    if !path.is_absolute() {
        return Err(config_error("mobile save path must be absolute"));
    }

    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_symlink() && !metadata.is_file() {
                return Err(config_error(format!(
                    "mobile save path is not a regular file: {}",
                    path.display()
                )));
            }
            let target = fs::canonicalize(path).map_err(|error| {
                database_error(format!(
                    "cannot resolve mobile save path {}: {error}",
                    path.display()
                ))
            })?;
            let target_metadata = fs::metadata(&target).map_err(|error| {
                database_error(format!(
                    "cannot inspect mobile save path {}: {error}",
                    target.display()
                ))
            })?;
            if !target_metadata.is_file() {
                return Err(config_error(format!(
                    "mobile save path is not a regular file: {}",
                    target.display()
                )));
            }
            Ok((target, true))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| {
                config_error(format!(
                    "mobile save path has no parent: {}",
                    path.display()
                ))
            })?;
            let filename = path.file_name().ok_or_else(|| {
                config_error(format!(
                    "mobile save path has no filename: {}",
                    path.display()
                ))
            })?;
            let parent = fs::canonicalize(parent).map_err(|error| {
                database_error(format!(
                    "cannot resolve mobile save parent {}: {error}",
                    parent.display()
                ))
            })?;
            let parent_metadata = fs::metadata(&parent).map_err(|error| {
                database_error(format!(
                    "cannot inspect mobile save parent {}: {error}",
                    parent.display()
                ))
            })?;
            if !parent_metadata.is_dir() {
                return Err(config_error(format!(
                    "mobile save parent is not a directory: {}",
                    parent.display()
                )));
            }
            Ok((parent.join(filename), false))
        }
        Err(error) => Err(database_error(format!(
            "cannot inspect mobile save path {}: {error}",
            path.display()
        ))),
    }
}

/// A locked, open portable-save database.
pub struct MobileStore {
    path: PathBuf,
    connection: Connection,
    // The guard must outlive the connection.  Dropping it allows another
    // runtime to open the save after this store is closed.
    // This is the database inode lock after bootstrap.  It must outlive the
    // SQLite connection and is separate from SQLite's own file locking.
    _lock: KernelSaveLock,
    content_version: String,
    content_fingerprint: String,
}

impl std::fmt::Debug for MobileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MobileStore")
            .field("path", &self.path)
            .field("content_version", &self.content_version)
            .field("content_fingerprint", &self.content_fingerprint)
            .finish_non_exhaustive()
    }
}

impl MobileStore {
    /// Open or initialize a mobile save at an explicit path.
    ///
    /// A stable bootstrap sidecar serializes cooperating opens. Existing files
    /// are inspected read-only before writable setup and then held by a
    /// kernel lock on the database inode; a legacy desktop database, corrupt
    /// database, unsupported format, or content mismatch is returned as an
    /// error and is never replaced with a fresh save.
    pub fn open(
        path: &Path,
        content_version: &str,
        content_fingerprint: &str,
    ) -> Result<Self, ParishError> {
        if content_version.is_empty() {
            return Err(config_error("mobile content version must not be empty"));
        }
        if content_fingerprint.is_empty() {
            return Err(config_error("mobile content fingerprint must not be empty"));
        }

        let (canonical_path, exists) = resolve_mobile_path(path)?;

        // A same-process alias can be rejected before opening its bootstrap
        // sidecar.  Hard links have distinct sidecar pathnames, so acquiring
        // one here and discovering the inode guard only inside
        // `open_existing` leaves a short-lived descriptor that a concurrently
        // forked child can inherit until exec.  That window is enough for a
        // sibling test or host child to make the alias appear spuriously
        // locked after the owning store is dropped.
        #[cfg(unix)]
        if exists && inode_is_guarded(&canonical_path)? {
            return Err(database_error(format!(
                "mobile save is locked: {}",
                path.display()
            )));
        }

        let bootstrap_lock =
            KernelSaveLock::acquire_sidecar(&bootstrap_lock_path(&canonical_path))?;

        // A peer may have created the file while the caller was resolving the
        // path.  Re-resolve while the bootstrap sidecar is held, then acquire
        // the database inode before releasing that sidecar.
        let (canonical_path, exists) = resolve_mobile_path(&canonical_path)?;
        let result = if exists {
            Self::open_existing(&canonical_path, content_version, content_fingerprint)
        } else {
            cleanup_bootstrap_temps(&canonical_path);
            Self::bootstrap_new(&canonical_path, content_version, content_fingerprint)
        };
        drop(bootstrap_lock);
        result
    }

    fn open_existing(
        path: &Path,
        content_version: &str,
        content_fingerprint: &str,
    ) -> Result<Self, ParishError> {
        // Acquire the inode guard before any SQLite connection. Hard-link
        // aliases do not share the sidecar pathname, so the inode guard is
        // the authority that closes that alias race during inspection too.
        let lock = KernelSaveLock::acquire_existing(path)?;
        inspect_existing(path, content_version, content_fingerprint)?;
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .db_err()
            .map_err(|error| database_error(format!("cannot open mobile save: {error}")))?;
        ensure_lock_matches_path(&lock, path)?;
        configure_connection(&connection)?;
        Ok(Self {
            path: path.to_path_buf(),
            connection,
            _lock: lock,
            content_version: content_version.to_owned(),
            content_fingerprint: content_fingerprint.to_owned(),
        })
    }

    fn bootstrap_new(
        path: &Path,
        content_version: &str,
        content_fingerprint: &str,
    ) -> Result<Self, ParishError> {
        let temp_path = bootstrap_temp_path(path);
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| {
                database_error(format!(
                    "cannot create mobile bootstrap {}: {error}",
                    temp_path.display()
                ))
            })?;
        #[cfg(test)]
        pause_for_bootstrap_interruption_test();
        let result = (|| {
            let connection =
                Connection::open_with_flags(&temp_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
                    .db_err()
                    .map_err(|error| {
                        database_error(format!("cannot create mobile bootstrap: {error}"))
                    })?;
            configure_bootstrap_connection(&connection)?;
            initialize_schema(&connection, content_version, content_fingerprint)?;
            close_connection(connection)?;
            let lock = KernelSaveLock::acquire_existing(&temp_path)?;
            lock.sync_all()?;

            // hard_link is an atomic no-clobber publication: it fails when a
            // peer has created the destination.  The lock file descriptor still
            // points at this inode after the temporary pathname is removed.
            fs::hard_link(&temp_path, path).map_err(|error| {
                database_error(format!(
                    "cannot publish mobile save {}: {error}",
                    path.display()
                ))
            })?;
            sync_parent_directory(path)?;
            fs::remove_file(&temp_path).map_err(|error| {
                database_error(format!(
                    "cannot remove mobile bootstrap temporary file {}: {error}",
                    temp_path.display()
                ))
            })?;
            sync_parent_directory(path)?;

            let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
                .db_err()
                .map_err(|error| database_error(format!("cannot open mobile save: {error}")))?;
            configure_connection(&connection)?;
            Ok((connection, lock))
        })();

        match result {
            Ok((connection, lock)) => Ok(Self {
                path: path.to_path_buf(),
                connection,
                _lock: lock,
                content_version: content_version.to_owned(),
                content_fingerprint: content_fingerprint.to_owned(),
            }),
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                Err(error)
            }
        }
    }

    /// Explicit path used by this store.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Content identity checked when this store was opened.
    pub fn content_version(&self) -> &str {
        &self.content_version
    }

    /// Content fingerprint checked when this store was opened.
    pub fn content_fingerprint(&self) -> &str {
        &self.content_fingerprint
    }

    /// Read the current projection, generation, and session identity.
    pub fn load(&self) -> Result<MobileSave, ParishError> {
        let (generation, session_id): (i64, String) = self
            .connection
            .query_row(
                "SELECT generation, session_id FROM mobile_metadata WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .db_err()?;
        let generation = u64::try_from(generation)
            .map_err(|_| database_error("mobile save has a negative generation"))?;
        if session_id.is_empty() {
            return Err(database_error("mobile save has an empty session id"));
        }

        let domain_json: Option<String> = self
            .connection
            .query_row(
                "SELECT domain_json FROM mobile_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .db_err()?;
        let current_domain = domain_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?;

        Ok(MobileSave {
            current_domain,
            generation,
            session_id,
        })
    }

    /// Restore all logical request records in stable key order.
    ///
    /// Request records are the deduplication ledger and are intentionally
    /// separate from the bounded event-history API.
    pub fn load_requests(&self) -> Result<Vec<MobileRequest>, ParishError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT logical_id, request_json
                 FROM mobile_requests
                 ORDER BY logical_id ASC",
            )
            .db_err()?;
        let rows = statement
            .query_map([], |row| {
                let logical_id: String = row.get(0)?;
                let json: String = row.get(1)?;
                Ok((logical_id, json))
            })
            .db_err()?;

        rows.map(|row| {
            let (logical_id, json) = row.db_err()?;
            Ok(MobileRequest {
                logical_id,
                json: serde_json::from_str(&json)?,
            })
        })
        .collect()
    }

    /// Read a bounded page of durable events strictly after `after_sequence`.
    pub fn read_events(
        &self,
        after_sequence: u64,
        limit: usize,
    ) -> Result<Vec<MobileEvent>, ParishError> {
        let after_sequence = i64::try_from(after_sequence)
            .map_err(|_| config_error("event cursor exceeds SQLite integer range"))?;
        let limit = limit.min(MAX_EVENT_PAGE_SIZE);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit).expect("MAX_EVENT_PAGE_SIZE fits SQLite integer");
        let mut statement = self
            .connection
            .prepare(
                "SELECT event_id, sequence, event_json
                 FROM mobile_events
                 WHERE sequence > ?1
                 ORDER BY sequence ASC
                 LIMIT ?2",
            )
            .db_err()?;
        let rows = statement
            .query_map(params![after_sequence, limit], |row| {
                let event_id: String = row.get(0)?;
                let sequence: i64 = row.get(1)?;
                let json: String = row.get(2)?;
                Ok((event_id, sequence, json))
            })
            .db_err()?;

        rows.map(|row| {
            let (event_id, sequence, json) = row.db_err()?;
            let sequence = u64::try_from(sequence)
                .map_err(|_| database_error("mobile event has a negative sequence"))?;
            Ok(MobileEvent {
                event_id,
                sequence,
                json: serde_json::from_str(&json)?,
            })
        })
        .collect()
    }

    /// Atomically update state, upsert logical requests, and append events.
    ///
    /// The metadata generation is compared and incremented in the same
    /// transaction as every other write.  A stale caller, duplicate event
    /// with different JSON, or any SQLite failure rolls back the whole batch.
    pub fn commit(
        &self,
        expected_generation: u64,
        next_domain_json: Option<Value>,
        request_upserts: &[MobileRequestUpsert],
        events: &[MobileEventInput],
    ) -> Result<MobileCommit, ParishError> {
        let expected_generation = i64::try_from(expected_generation)
            .map_err(|_| config_error("generation exceeds SQLite integer range"))?;
        if expected_generation == i64::MAX {
            return Err(config_error("mobile generation is exhausted"));
        }
        validate_request_inputs(request_upserts)?;
        validate_event_inputs(events)?;

        let transaction = self.connection.unchecked_transaction().db_err()?;
        let changed = transaction
            .execute(
                "UPDATE mobile_metadata
                 SET generation = generation + 1
                 WHERE id = 1 AND generation = ?1",
                [expected_generation],
            )
            .db_err()?;
        if changed != 1 {
            let actual: Option<i64> = transaction
                .query_row(
                    "SELECT generation FROM mobile_metadata WHERE id = 1",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .db_err()?;
            return Err(database_error(format!(
                "stale mobile generation: expected {}, found {}",
                expected_generation,
                actual.map_or_else(|| "missing".to_owned(), |value| value.to_string())
            )));
        }

        if let Some(domain) = next_domain_json {
            let json = serde_json::to_string(&domain)?;
            transaction
                .execute(
                    "INSERT INTO mobile_state (id, domain_json) VALUES (1, ?1)
                     ON CONFLICT(id) DO UPDATE SET domain_json = excluded.domain_json",
                    [&json],
                )
                .db_err()?;
        }

        for request in request_upserts {
            let json = serde_json::to_string(&request.json)?;
            transaction
                .execute(
                    "INSERT INTO mobile_requests (logical_id, request_json)
                     VALUES (?1, ?2)
                     ON CONFLICT(logical_id) DO UPDATE SET request_json = excluded.request_json",
                    params![request.logical_id, json],
                )
                .db_err()?;
        }

        let mut event_sequences = Vec::with_capacity(events.len());
        for event in events {
            event_sequences.push(upsert_event(&transaction, event)?);
        }

        transaction.commit().db_err()?;
        Ok(MobileCommit {
            generation: u64::try_from(expected_generation + 1)
                .expect("validated generation is non-negative"),
            event_sequences,
        })
    }
}

fn ensure_lock_matches_path(lock: &KernelSaveLock, path: &Path) -> Result<(), ParishError> {
    #[cfg(unix)]
    {
        let expected = inode_key(&lock.file).map_err(|error| lock_error(path, error))?;
        let actual = inode_key_for_path(path).map_err(|error| lock_error(path, error))?;
        if expected != actual {
            return Err(database_error(format!(
                "mobile save path changed while opening: {}",
                path.display()
            )));
        }
    }
    #[cfg(not(unix))]
    let _ = (lock, path);
    Ok(())
}

#[cfg(unix)]
fn inode_is_guarded(path: &Path) -> Result<bool, ParishError> {
    let inode_key = inode_key_for_path(path).map_err(|error| lock_error(path, error))?;
    let guards = MOBILE_INODE_GUARDS.get_or_init(|| Mutex::new(HashSet::new()));
    Ok(guards
        .lock()
        .expect("mobile inode guard mutex poisoned")
        .contains(&inode_key))
}

fn validate_request_inputs(requests: &[MobileRequestUpsert]) -> Result<(), ParishError> {
    for request in requests {
        if request.logical_id.trim().is_empty() {
            return Err(config_error("logical request id must not be empty"));
        }
    }
    Ok(())
}

fn validate_event_inputs(events: &[MobileEventInput]) -> Result<(), ParishError> {
    for event in events {
        if event.event_id.trim().is_empty() {
            return Err(config_error("durable event id must not be empty"));
        }
    }
    Ok(())
}

fn upsert_event(
    transaction: &Transaction<'_>,
    event: &MobileEventInput,
) -> Result<u64, ParishError> {
    let existing: Option<(i64, String)> = transaction
        .query_row(
            "SELECT sequence, event_json FROM mobile_events WHERE event_id = ?1",
            [&event.event_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .db_err()?;
    if let Some((sequence, existing_json)) = existing {
        let existing_value: Value = serde_json::from_str(&existing_json)?;
        if existing_value != event.json {
            return Err(database_error(format!(
                "durable event id {:?} already has different JSON",
                event.event_id
            )));
        }
        return u64::try_from(sequence)
            .map_err(|_| database_error("mobile event has a negative sequence"));
    }

    let json = serde_json::to_string(&event.json)?;
    transaction
        .execute(
            "INSERT INTO mobile_events (event_id, event_json) VALUES (?1, ?2)",
            params![event.event_id, json],
        )
        .db_err()?;
    let sequence: i64 = transaction
        .query_row(
            "SELECT sequence FROM mobile_events WHERE event_id = ?1",
            [&event.event_id],
            |row| row.get(0),
        )
        .db_err()?;
    u64::try_from(sequence).map_err(|_| database_error("mobile event has a negative sequence"))
}

fn inspect_existing(
    path: &Path,
    content_version: &str,
    content_fingerprint: &str,
) -> Result<(), ParishError> {
    if !path.exists() {
        return Err(database_error(format!(
            "mobile save disappeared before inspection: {}",
            path.display()
        )));
    }

    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .db_err()
        .map_err(|error| database_error(format!("cannot inspect mobile save: {error}")))?;
    let has_mobile_metadata = table_exists(&connection, METADATA_TABLE)?;
    let has_any_user_table = has_any_user_table(&connection)?;
    if !has_mobile_metadata {
        if !has_any_user_table {
            return Err(database_error(
                "existing SQLite database has no mobile schema; refusing implicit reinitialization",
            ));
        }
        return Err(database_error(
            "existing database is not a mobile save; legacy desktop saves are not imported",
        ));
    }

    validate_quick_check(&connection)?;
    validate_schema_shape(&connection)?;
    let user_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .db_err()?;
    if user_version != MOBILE_FORMAT_VERSION {
        return Err(database_error(format!(
            "unsupported mobile SQLite user_version {}; supported version is {}",
            user_version, MOBILE_FORMAT_VERSION
        )));
    }

    let metadata: Option<(i64, String, String, i64, String)> = connection
        .query_row(
            "SELECT format_version, content_version, content_fingerprint,
                    generation, session_id
             FROM mobile_metadata WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .db_err()?;
    let Some((format_version, saved_content_version, saved_fingerprint, generation, session_id)) =
        metadata
    else {
        return Err(database_error(
            "mobile metadata singleton row is missing; save is invalid",
        ));
    };
    if format_version != MOBILE_FORMAT_VERSION {
        return Err(database_error(format!(
            "unsupported mobile format {}; supported version is {}",
            format_version, MOBILE_FORMAT_VERSION
        )));
    }
    if saved_content_version != content_version || saved_fingerprint != content_fingerprint {
        return Err(database_error(format!(
            "mobile content mismatch: save uses version {:?} fingerprint {:?}, requested version {:?} fingerprint {:?}",
            saved_content_version, saved_fingerprint, content_version, content_fingerprint
        )));
    }
    if generation < 0 || session_id.is_empty() {
        return Err(database_error(
            "mobile metadata has invalid generation or session id",
        ));
    }
    validate_stored_json(&connection)?;
    Ok(())
}

fn validate_quick_check(connection: &Connection) -> Result<(), ParishError> {
    let result: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .db_err()?;
    if result.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(database_error(format!(
            "mobile SQLite quick_check failed: {result}"
        )))
    }
}

fn configure_connection(connection: &Connection) -> Result<(), ParishError> {
    // FULL makes the local save durable at transaction boundaries.  The
    // sidecar lock keeps this single-authority store from needing concurrent
    // writers, while WAL still gives readers a consistent view.
    connection
        .execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|error| database_error(format!("cannot enable mobile WAL: {error}")))?;
    connection
        .execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")
        .map_err(|error| database_error(format!("cannot configure mobile SQLite: {error}")))
}

fn configure_bootstrap_connection(connection: &Connection) -> Result<(), ParishError> {
    connection
        .execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA synchronous=FULL;
             PRAGMA foreign_keys=ON;",
        )
        .map_err(|error| database_error(format!("cannot configure mobile bootstrap: {error}")))
}

fn close_connection(connection: Connection) -> Result<(), ParishError> {
    connection
        .close()
        .map_err(|(_, error)| database_error(format!("cannot close mobile bootstrap: {error}")))
}

fn initialize_schema(
    connection: &Connection,
    content_version: &str,
    content_fingerprint: &str,
) -> Result<(), ParishError> {
    let transaction = connection.unchecked_transaction().db_err()?;
    transaction
        .execute_batch(
            "CREATE TABLE mobile_metadata (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                format_version INTEGER NOT NULL,
                content_version TEXT NOT NULL,
                content_fingerprint TEXT NOT NULL,
                generation INTEGER NOT NULL CHECK (generation >= 0),
                session_id TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE mobile_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                domain_json TEXT NOT NULL
            );
            CREATE TABLE mobile_requests (
                logical_id TEXT PRIMARY KEY,
                request_json TEXT NOT NULL
            );
            CREATE TABLE mobile_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                event_json TEXT NOT NULL
            );
            CREATE INDEX mobile_events_sequence_idx
                ON mobile_events(sequence);",
        )
        .db_err()?;
    let session_id = new_session_id();
    transaction
        .execute(
            "INSERT INTO mobile_metadata
                (id, format_version, content_version, content_fingerprint,
                 generation, session_id, created_at)
             VALUES (1, ?1, ?2, ?3, 0, ?4, ?5)",
            params![
                MOBILE_FORMAT_VERSION,
                content_version,
                content_fingerprint,
                session_id,
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .db_err()?;
    transaction
        .execute_batch("PRAGMA user_version = 1;")
        .db_err()?;
    transaction.commit().db_err()
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, ParishError> {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
            )",
            [table],
            |row| row.get(0),
        )
        .db_err()
}

fn has_any_user_table(connection: &Connection) -> Result<bool, ParishError> {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
            )",
            [],
            |row| row.get(0),
        )
        .db_err()
}

fn validate_schema_shape(connection: &Connection) -> Result<(), ParishError> {
    let expected = [
        (
            METADATA_TABLE,
            [
                ("id", "INTEGER", false, 1),
                ("format_version", "INTEGER", true, 0),
                ("content_version", "TEXT", true, 0),
                ("content_fingerprint", "TEXT", true, 0),
                ("generation", "INTEGER", true, 0),
                ("session_id", "TEXT", true, 0),
                ("created_at", "TEXT", true, 0),
            ]
            .as_slice(),
        ),
        (
            STATE_TABLE,
            [
                ("id", "INTEGER", false, 1),
                ("domain_json", "TEXT", true, 0),
            ]
            .as_slice(),
        ),
        (
            REQUESTS_TABLE,
            [
                ("logical_id", "TEXT", false, 1),
                ("request_json", "TEXT", true, 0),
            ]
            .as_slice(),
        ),
        (
            EVENTS_TABLE,
            [
                ("sequence", "INTEGER", false, 1),
                ("event_id", "TEXT", true, 0),
                ("event_json", "TEXT", true, 0),
            ]
            .as_slice(),
        ),
    ];
    for (table, expected_columns) in expected {
        if !table_exists(connection, table)? {
            return Err(database_error(format!(
                "mobile save is missing required table {table:?}"
            )));
        }
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .db_err()?;
        let columns = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                    row.get::<_, i64>(5)?,
                ))
            })
            .db_err()?;
        let actual: Vec<(String, String, bool, i64)> =
            columns.collect::<Result<_, _>>().db_err()?;
        if actual.len() != expected_columns.len()
            || actual.iter().zip(expected_columns).any(
                |(
                    (name, declared_type, not_null, primary_key),
                    (expected_name, expected_type, expected_not_null, expected_primary_key),
                )| {
                    name != expected_name
                        || !declared_type.eq_ignore_ascii_case(expected_type)
                        || *not_null != *expected_not_null
                        || *primary_key != *expected_primary_key
                },
            )
        {
            return Err(database_error(format!(
                "mobile save table {table:?} has an invalid schema"
            )));
        }
    }

    validate_table_sql(
        connection,
        METADATA_TABLE,
        &["check(id=1)", "check(generation>=0)"],
    )?;
    validate_table_sql(connection, STATE_TABLE, &["check(id=1)"])?;
    validate_table_sql(connection, EVENTS_TABLE, &["autoincrement"])?;
    validate_unique_index(connection, EVENTS_TABLE, &["event_id"])?;
    validate_index(
        connection,
        EVENTS_TABLE,
        "mobile_events_sequence_idx",
        false,
        &["sequence"],
    )?;
    Ok(())
}

fn validate_table_sql(
    connection: &Connection,
    table: &str,
    required_fragments: &[&str],
) -> Result<(), ParishError> {
    let sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .db_err()?;
    let normalized: String = sql
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase();
    if required_fragments
        .iter()
        .any(|fragment| !normalized.contains(fragment))
    {
        return Err(database_error(format!(
            "mobile save table {table:?} has invalid constraints"
        )));
    }
    Ok(())
}

fn validate_index(
    connection: &Connection,
    table: &str,
    expected_name: &str,
    expected_unique: bool,
    expected_columns: &[&str],
) -> Result<(), ParishError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA index_list({table})"))
        .db_err()?;
    let indexes = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(4)? != 0,
            ))
        })
        .db_err()?;
    let Some((name, unique, partial)) = indexes
        .collect::<Result<Vec<_>, _>>()
        .db_err()?
        .into_iter()
        .find(|(name, _, _)| name == expected_name)
    else {
        return Err(database_error(format!(
            "mobile save is missing required index {expected_name:?}"
        )));
    };
    if unique != expected_unique || partial {
        return Err(database_error(format!(
            "mobile index {expected_name:?} has invalid uniqueness or partial definition"
        )));
    }
    let mut statement = connection
        .prepare(&format!("PRAGMA index_info({expected_name})"))
        .db_err()?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(2))
        .db_err()?;
    let actual = columns.collect::<Result<Vec<_>, _>>().db_err()?;
    if actual != expected_columns {
        return Err(database_error(format!(
            "mobile index {expected_name:?} has invalid columns"
        )));
    }
    let _ = name;
    Ok(())
}

fn validate_unique_index(
    connection: &Connection,
    table: &str,
    expected_columns: &[&str],
) -> Result<(), ParishError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA index_list({table})"))
        .db_err()?;
    let indexes = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(4)? != 0,
            ))
        })
        .db_err()?;
    for (name, unique, partial) in indexes.collect::<Result<Vec<_>, _>>().db_err()? {
        if !unique || partial {
            continue;
        }
        let mut statement = connection
            .prepare(&format!("PRAGMA index_info({name})"))
            .db_err()?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(2))
            .db_err()?;
        let actual = columns.collect::<Result<Vec<_>, _>>().db_err()?;
        if actual == expected_columns {
            return Ok(());
        }
    }
    Err(database_error(format!(
        "mobile table {table:?} is missing a unique constraint for {:?}",
        expected_columns
    )))
}

fn validate_stored_json(connection: &Connection) -> Result<(), ParishError> {
    let mut state = connection
        .prepare("SELECT domain_json FROM mobile_state")
        .db_err()?;
    let values = state
        .query_map([], |row| row.get::<_, String>(0))
        .db_err()?;
    for value in values {
        let value = value.db_err()?;
        serde_json::from_str::<Value>(&value).map_err(|error| {
            database_error(format!(
                "mobile save contains invalid JSON in {STATE_TABLE}.domain_json: {error}"
            ))
        })?;
    }

    let mut requests = connection
        .prepare("SELECT logical_id, request_json FROM mobile_requests")
        .db_err()?;
    let rows = requests
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .db_err()?;
    for row in rows {
        let (logical_id, json) = row.db_err()?;
        if logical_id.trim().is_empty() {
            return Err(database_error(
                "mobile save contains an empty logical request id",
            ));
        }
        serde_json::from_str::<Value>(&json).map_err(|error| {
            database_error(format!(
                "mobile save contains invalid JSON in {REQUESTS_TABLE}.request_json: {error}"
            ))
        })?;
    }

    let mut events = connection
        .prepare("SELECT sequence, event_id, event_json FROM mobile_events")
        .db_err()?;
    let rows = events
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .db_err()?;
    for row in rows {
        let (sequence, event_id, json) = row.db_err()?;
        if sequence <= 0 {
            return Err(database_error(format!(
                "mobile save contains invalid event sequence {sequence}"
            )));
        }
        if event_id.trim().is_empty() {
            return Err(database_error(
                "mobile save contains an empty durable event id",
            ));
        }
        serde_json::from_str::<Value>(&json).map_err(|error| {
            database_error(format!(
                "mobile save contains invalid JSON in {EVENTS_TABLE}.event_json: {error}"
            ))
        })?;
    }
    Ok(())
}

fn new_session_id() -> String {
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("mobile-{}-{nanos}-{counter}", std::process::id())
}

fn config_error(message: impl Into<String>) -> ParishError {
    ParishError::Config(message.into())
}

fn database_error(message: impl Into<String>) -> ParishError {
    ParishError::Database(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use tempfile::TempDir;

    fn test_store() -> (TempDir, MobileStore) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let store = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        (directory, store)
    }

    fn event(id: &str, kind: &str) -> MobileEventInput {
        MobileEventInput::new(id, serde_json::json!({ "kind": kind }))
    }

    fn wait_for_child_marker(child: &mut std::process::Child, marker: &str) {
        let stdout = child.stdout.as_mut().expect("child stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line).expect("child output");
            assert!(bytes > 0, "child exited before printing {marker}");
            if line.contains(marker) {
                return;
            }
        }
    }

    #[test]
    fn initializes_independent_schema_and_empty_load() {
        let (directory, store) = test_store();
        let save = store.load().unwrap();
        assert_eq!(save.generation, 0);
        assert!(save.current_domain.is_none());
        assert!(save.session_id.starts_with("mobile-"));
        assert!(store.load_requests().unwrap().is_empty());
        assert!(store.read_events(0, 10).unwrap().is_empty());

        let connection = Connection::open(directory.path().join("mobile.db")).unwrap();
        let user_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(user_version, MOBILE_FORMAT_VERSION);
        for table in [METADATA_TABLE, STATE_TABLE, REQUESTS_TABLE, EVENTS_TABLE] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing table {table}");
        }
        assert!(
            connection
                .query_row::<i64, _, _>("SELECT COUNT(*) FROM branches", [], |row| row.get(0))
                .is_err()
        );
    }

    #[test]
    fn commit_reopens_and_restores_state_requests_and_events() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let store = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        let commit = store
            .commit(
                0,
                Some(serde_json::json!({ "location": "crossroads" })),
                &[MobileRequest::new(
                    "request-1",
                    serde_json::json!({ "text": "/look", "status": "accepted" }),
                )],
                &[event("event-1", "accepted"), event("event-2", "completed")],
            )
            .unwrap();
        assert_eq!(commit.generation, 1);
        assert_eq!(commit.event_sequences, vec![1, 2]);
        drop(store);

        let reopened = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        assert_eq!(
            reopened.load().unwrap().current_domain,
            Some(serde_json::json!({ "location": "crossroads" }))
        );
        assert_eq!(reopened.load().unwrap().generation, 1);
        assert_eq!(reopened.load_requests().unwrap().len(), 1);
        assert_eq!(reopened.read_events(0, 10).unwrap().len(), 2);
    }

    #[test]
    fn duplicate_events_are_idempotent_but_conflicting_payload_is_rejected() {
        let (_directory, store) = test_store();
        let first = store
            .commit(0, None, &[], &[event("event-1", "accepted")])
            .unwrap();
        let duplicate = store
            .commit(1, None, &[], &[event("event-1", "accepted")])
            .unwrap();
        assert_eq!(first.event_sequences, duplicate.event_sequences);
        assert_eq!(store.read_events(0, 10).unwrap().len(), 1);

        let conflicting = store.commit(
            2,
            None,
            &[],
            &[MobileEventInput::new(
                "event-1",
                serde_json::json!({ "kind": "terminal" }),
            )],
        );
        assert!(conflicting.is_err());
        assert_eq!(store.load().unwrap().generation, 2);
        assert_eq!(store.read_events(0, 10).unwrap().len(), 1);
    }

    #[test]
    fn stale_generation_has_no_side_effects() {
        let (_directory, store) = test_store();
        store
            .commit(
                0,
                None,
                &[MobileRequest::new("one", serde_json::json!(1))],
                &[],
            )
            .unwrap();
        let error = store
            .commit(
                0,
                Some(serde_json::json!({ "should": "rollback" })),
                &[MobileRequest::new("two", serde_json::json!(2))],
                &[event("event-1", "accepted")],
            )
            .unwrap_err();
        assert!(error.to_string().contains("stale mobile generation"));
        let save = store.load().unwrap();
        assert_eq!(save.generation, 1);
        assert!(save.current_domain.is_none());
        assert_eq!(store.load_requests().unwrap().len(), 1);
        assert!(store.read_events(0, 10).unwrap().is_empty());
    }

    #[test]
    fn sqlite_failure_rolls_back_generation_state_request_and_event_together() {
        let (_directory, store) = test_store();
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_mobile_events
                 BEFORE INSERT ON mobile_events
                 BEGIN SELECT RAISE(ABORT, 'injected mobile event failure'); END;",
            )
            .unwrap();
        let error = store
            .commit(
                0,
                Some(serde_json::json!({ "location": "must-not-commit" })),
                &[MobileRequest::new(
                    "request-1",
                    serde_json::json!({ "accepted": true }),
                )],
                &[event("event-1", "completed")],
            )
            .unwrap_err();
        assert!(error.to_string().contains("injected mobile event failure"));
        let save = store.load().unwrap();
        assert_eq!(save.generation, 0);
        assert!(save.current_domain.is_none());
        assert!(store.load_requests().unwrap().is_empty());
        assert!(store.read_events(0, 10).unwrap().is_empty());
    }

    #[test]
    fn content_and_schema_mismatch_preserve_existing_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let store = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        let session_id = store.load().unwrap().session_id;
        drop(store);

        let error = MobileStore::open(&path, "phase2", "fingerprint-b").unwrap_err();
        assert!(
            error.to_string().contains("content mismatch"),
            "unexpected content mismatch error: {error}"
        );
        let reopened = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        assert_eq!(reopened.load().unwrap().session_id, session_id);
        drop(reopened);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute("UPDATE mobile_metadata SET format_version = 2", [])
            .unwrap();
        let error = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("unsupported mobile format"));
        let raw_format: i64 = connection
            .query_row("SELECT format_version FROM mobile_metadata", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(raw_format, 2);
    }

    #[test]
    fn legacy_desktop_database_is_rejected_without_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE branches (id INTEGER PRIMARY KEY, name TEXT);
                 INSERT INTO branches (id, name) VALUES (1, 'main');",
            )
            .unwrap();
        drop(connection);
        let error = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("legacy desktop"));
        let connection = Connection::open(&path).unwrap();
        let branch_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM branches", [], |row| row.get(0))
            .unwrap();
        assert_eq!(branch_count, 1);
        let mobile_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'mobile_metadata'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mobile_count, 0);
    }

    #[test]
    fn existing_empty_file_is_not_treated_as_a_fresh_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("truncated.db");
        std::fs::File::create(&path).unwrap();
        let error = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("mobile") || error.to_string().contains("SQLite"));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    }

    #[test]
    fn save_path_must_be_absolute_and_regular() {
        let relative =
            MobileStore::open(Path::new("mobile.db"), "phase2", "fingerprint-a").unwrap_err();
        assert!(relative.to_string().contains("absolute"));

        #[cfg(unix)]
        {
            let special =
                MobileStore::open(Path::new("/dev/null"), "phase2", "fingerprint-a").unwrap_err();
            assert!(
                special.to_string().contains("regular") || special.to_string().contains("save")
            );
        }
    }

    #[test]
    fn invalid_stored_json_is_rejected_before_writable_open() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let store = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                "INSERT INTO mobile_requests (logical_id, request_json) VALUES ('bad', 'not-json')",
                [],
            )
            .unwrap();
        let generation: i64 = connection
            .query_row(
                "SELECT generation FROM mobile_metadata WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);

        let error = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("invalid JSON"));
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT generation FROM mobile_metadata WHERE id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            generation
        );
    }

    #[test]
    fn malformed_schema_types_and_index_semantics_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("malformed.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE mobile_metadata (
                    id TEXT PRIMARY KEY CHECK (id = 1),
                    format_version INTEGER NOT NULL,
                    content_version TEXT NOT NULL,
                    content_fingerprint TEXT NOT NULL,
                    generation INTEGER NOT NULL CHECK (generation >= 0),
                    session_id TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
                CREATE TABLE mobile_state (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    domain_json TEXT NOT NULL
                );
                CREATE TABLE mobile_requests (
                    logical_id TEXT PRIMARY KEY,
                    request_json TEXT NOT NULL
                );
                CREATE TABLE mobile_events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL UNIQUE,
                    event_json TEXT NOT NULL
                );
                CREATE UNIQUE INDEX mobile_events_sequence_idx ON mobile_events(sequence);
                PRAGMA user_version = 1;
                INSERT INTO mobile_metadata
                    (id, format_version, content_version, content_fingerprint,
                     generation, session_id, created_at)
                VALUES ('1', 1, 'phase2', 'fingerprint-a', 0, 'session', 'now');",
            )
            .unwrap();
        drop(connection);

        let error = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("invalid schema"));
    }

    #[test]
    fn partial_event_id_unique_index_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("partial-index.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE mobile_metadata (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    format_version INTEGER NOT NULL,
                    content_version TEXT NOT NULL,
                    content_fingerprint TEXT NOT NULL,
                    generation INTEGER NOT NULL CHECK (generation >= 0),
                    session_id TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
                CREATE TABLE mobile_state (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    domain_json TEXT NOT NULL
                );
                CREATE TABLE mobile_requests (
                    logical_id TEXT PRIMARY KEY,
                    request_json TEXT NOT NULL
                );
                CREATE TABLE mobile_events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL,
                    event_json TEXT NOT NULL
                );
                CREATE UNIQUE INDEX mobile_events_event_id_partial
                    ON mobile_events(event_id) WHERE event_id LIKE 'x%';
                CREATE INDEX mobile_events_sequence_idx
                    ON mobile_events(sequence);
                PRAGMA user_version = 1;
                INSERT INTO mobile_metadata
                    (id, format_version, content_version, content_fingerprint,
                     generation, session_id, created_at)
                VALUES (1, 1, 'phase2', 'fingerprint-a', 0, 'session', 'now');",
            )
            .unwrap();
        drop(connection);

        let error = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("unique constraint"));
    }

    #[test]
    fn invalid_stored_identifiers_and_sequences_are_rejected() {
        let request_directory = tempfile::tempdir().unwrap();
        let request_path = request_directory.path().join("empty-request-id.db");
        let request_store = MobileStore::open(&request_path, "phase2", "fingerprint-a").unwrap();
        drop(request_store);
        let connection = Connection::open(&request_path).unwrap();
        connection
            .execute(
                "INSERT INTO mobile_requests (logical_id, request_json)
                 VALUES ('   ', '{}')",
                [],
            )
            .unwrap();
        drop(connection);
        let error = MobileStore::open(&request_path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("logical request id"));

        let event_directory = tempfile::tempdir().unwrap();
        let event_path = event_directory.path().join("negative-event-sequence.db");
        let event_store = MobileStore::open(&event_path, "phase2", "fingerprint-a").unwrap();
        drop(event_store);
        let connection = Connection::open(&event_path).unwrap();
        connection
            .execute(
                "INSERT INTO mobile_events (sequence, event_id, event_json)
                 VALUES (-1, 'event-1', '{}')",
                [],
            )
            .unwrap();
        drop(connection);
        let error = MobileStore::open(&event_path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("event sequence"));

        let connection = Connection::open(&event_path).unwrap();
        connection.execute("DELETE FROM mobile_events", []).unwrap();
        connection
            .execute(
                "INSERT INTO mobile_events (sequence, event_id, event_json)
                 VALUES (1, '   ', '{}')",
                [],
            )
            .unwrap();
        drop(connection);
        let error = MobileStore::open(&event_path, "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("durable event id"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_and_hardlink_aliases_cannot_open_while_inode_is_owned() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let symlink = directory.path().join("alias.db");
        let hardlink = directory.path().join("hardlink.db");
        let store = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        std::os::unix::fs::symlink(&path, &symlink).unwrap();
        std::fs::hard_link(&path, &hardlink).unwrap();

        let symlink_error = MobileStore::open(&symlink, "phase2", "fingerprint-a").unwrap_err();
        assert!(symlink_error.to_string().contains("locked"));
        let hardlink_error = MobileStore::open(&hardlink, "phase2", "fingerprint-a").unwrap_err();
        assert!(hardlink_error.to_string().contains("locked"));
        drop(store);

        let reopened = MobileStore::open(&hardlink, "phase2", "fingerprint-a").unwrap();
        drop(reopened);
    }

    #[test]
    fn kernel_lock_releases_after_process_death() {
        if let Some(path) = std::env::var_os("PARISH_MOBILE_LOCK_CHILD") {
            let _store = MobileStore::open(Path::new(&path), "phase2", "fingerprint-a").unwrap();
            println!("MOBILE_LOCK_CHILD_READY");
            std::io::stdout().flush().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(60));
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mobile::tests::kernel_lock_releases_after_process_death",
                "--nocapture",
            ])
            .env("PARISH_MOBILE_LOCK_CHILD", &path)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        wait_for_child_marker(&mut child, "MOBILE_LOCK_CHILD_READY");

        let error = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap_err();
        assert!(
            error.to_string().contains("lock"),
            "unexpected lock error: {error}"
        );
        child.kill().unwrap();
        child.wait().unwrap();

        let reopened = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        assert_eq!(reopened.load().unwrap().generation, 0);
    }

    #[cfg(unix)]
    #[test]
    fn inode_lock_survives_existing_delete_to_wal_transition() {
        if let Some(path) = std::env::var_os("PARISH_MOBILE_WAL_LOCK_CHILD") {
            let _store = MobileStore::open(Path::new(&path), "phase2", "fingerprint-a").unwrap();
            println!("MOBILE_WAL_LOCK_CHILD_READY");
            std::io::stdout().flush().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(60));
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let store = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        drop(store);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=DELETE;")
            .unwrap();
        drop(connection);

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mobile::tests::inode_lock_survives_existing_delete_to_wal_transition",
                "--nocapture",
            ])
            .env("PARISH_MOBILE_WAL_LOCK_CHILD", &path)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        wait_for_child_marker(&mut child, "MOBILE_WAL_LOCK_CHILD_READY");

        let error = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap_err();
        assert!(
            error.to_string().contains("lock"),
            "unexpected lock error: {error}"
        );
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn inode_lock_survives_unrelated_sqlite_close() {
        if let Some(path) = std::env::var_os("PARISH_MOBILE_UNRELATED_CLOSE_CHILD") {
            let _store = MobileStore::open(Path::new(&path), "phase2", "fingerprint-a").unwrap();

            // POSIX record locks are process-scoped on Darwin: closing this
            // unrelated SQLite descriptor used to release the lifetime guard
            // held by `_store`, even though its own file remained open.
            let connection = Connection::open(&path).unwrap();
            connection
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap();
            drop(connection);

            println!("MOBILE_UNRELATED_CLOSE_CHILD_READY");
            std::io::stdout().flush().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(60));
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let hardlink = directory.path().join("hardlink.db");
        let store = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        drop(store);
        std::fs::hard_link(&path, &hardlink).unwrap();

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mobile::tests::inode_lock_survives_unrelated_sqlite_close",
                "--nocapture",
            ])
            .env("PARISH_MOBILE_UNRELATED_CLOSE_CHILD", &path)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        wait_for_child_marker(&mut child, "MOBILE_UNRELATED_CLOSE_CHILD_READY");

        let error = MobileStore::open(&hardlink, "phase2", "fingerprint-a").unwrap_err();
        assert!(
            error.to_string().contains("lock"),
            "unexpected lock error: {error}"
        );
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn interrupted_bootstrap_leaves_no_final_file_and_can_retry() {
        if let Some(path) = std::env::var_os("PARISH_MOBILE_BOOTSTRAP_CHILD") {
            let _ = MobileStore::open(Path::new(&path), "phase2", "fingerprint-a");
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mobile.db");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mobile::tests::interrupted_bootstrap_leaves_no_final_file_and_can_retry",
                "--nocapture",
            ])
            .env("PARISH_MOBILE_BOOTSTRAP_CHILD", &path)
            .env("PARISH_MOBILE_BOOTSTRAP_PAUSE", "1")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        wait_for_child_marker(&mut child, "MOBILE_BOOTSTRAP_READY");
        assert!(!path.exists());
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!path.exists());
        let abandoned_temps = || {
            std::fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .contains("mobile-bootstrap-")
                })
                .count()
        };
        assert!(abandoned_temps() > 0);

        let store = MobileStore::open(&path, "phase2", "fingerprint-a").unwrap();
        assert_eq!(store.load().unwrap().generation, 0);
        assert_eq!(abandoned_temps(), 0);
    }

    #[test]
    fn history_pages_are_bounded_and_cursor_ordered() {
        let (_directory, store) = test_store();
        let events: Vec<_> = (0..(MAX_EVENT_PAGE_SIZE + 10))
            .map(|index| event(&format!("event-{index}"), "terminal"))
            .collect();
        store.commit(0, None, &[], &events).unwrap();
        let first = store.read_events(0, usize::MAX).unwrap();
        assert_eq!(first.len(), MAX_EVENT_PAGE_SIZE);
        assert_eq!(first.first().unwrap().sequence, 1);
        let second = store
            .read_events(first.last().unwrap().sequence, MAX_EVENT_PAGE_SIZE)
            .unwrap();
        assert_eq!(second.len(), 10);
        assert_eq!(second.first().unwrap().sequence, first.len() as u64 + 1);
    }

    #[test]
    fn lock_is_held_for_store_lifetime() {
        let (_directory, store) = test_store();
        let error = MobileStore::open(store.path(), "phase2", "fingerprint-a").unwrap_err();
        assert!(error.to_string().contains("lock"));
    }
}
