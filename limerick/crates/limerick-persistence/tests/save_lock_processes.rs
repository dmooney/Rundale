//! Cross-process save-lock behaviour across an app's lifecycle.
//!
//! Every scenario uses real processes: the test binary re-executes itself as a
//! child (`lock_probe_child`), which holds, tries, or idles according to
//! `LIMERICK_LOCK_PROBE`. The same binary runs on the desktop and on the iOS
//! Simulator (`limerick/scripts/ios-sim-save-lock.sh`), where
//! `LIMERICK_LOCK_TEST_ROOT` points into an app's sandbox data container.
//!
//! The iOS lifecycle cases are a force-quit or jetsam kill (SIGKILL with the
//! lock held) and a relaunch that gets a new PID, a PID now used by an
//! unrelated process, or the killed owner's own PID.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};

use limerick_persistence::SaveFileLock;
use limerick_persistence::lock::is_locked;

const MODE_ENV: &str = "LIMERICK_LOCK_PROBE";
const SAVE_ENV: &str = "LIMERICK_LOCK_PROBE_SAVE";
const ROOT_ENV: &str = "LIMERICK_LOCK_TEST_ROOT";

/// Child entry point. A no-op unless the parent set `LIMERICK_LOCK_PROBE`.
#[test]
fn lock_probe_child() {
    let Ok(mode) = std::env::var(MODE_ENV) else {
        return;
    };
    let save = PathBuf::from(std::env::var(SAVE_ENV).expect("probe save path"));
    let mut out = std::io::stdout();
    match mode.as_str() {
        "hold" => {
            let guard = SaveFileLock::try_acquire(&save);
            writeln!(out, "PROBE {} pid={}", verdict(&guard), std::process::id()).unwrap();
            out.flush().unwrap();
            // Hold until the parent closes stdin (a normal exit) or kills us.
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            drop(guard);
            writeln!(out, "PROBE RELEASED").unwrap();
        }
        "try" => {
            let guard = SaveFileLock::try_acquire(&save);
            writeln!(out, "PROBE {} pid={}", verdict(&guard), std::process::id()).unwrap();
        }
        "try-as-recorded-owner" => {
            // A relaunch that receives the killed owner's PID.
            repoint_recorded_pid(&save, std::process::id());
            let guard = SaveFileLock::try_acquire(&save);
            writeln!(out, "PROBE {} pid={}", verdict(&guard), std::process::id()).unwrap();
        }
        "idle" => {
            writeln!(out, "PROBE READY pid={}", std::process::id()).unwrap();
            out.flush().unwrap();
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
        }
        other => panic!("unknown probe mode {other}"),
    }
    out.flush().unwrap();
}

fn verdict(guard: &Option<SaveFileLock>) -> &'static str {
    if guard.is_some() {
        "ACQUIRED"
    } else {
        "REFUSED"
    }
}

struct Probe {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    pid: u32,
}

impl Probe {
    fn spawn(mode: &str, save: &Path) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lock_probe_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(MODE_ENV, mode)
            .env(SAVE_ENV, save)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn probe process");
        let pid = child.id();
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            pid,
        }
    }

    /// Returns the next `PROBE ...` line, skipping libtest's own output.
    fn next_event(&mut self) -> String {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.stdout.read_line(&mut line).unwrap();
            assert!(read > 0, "probe {} exited without reporting", self.pid);
            // libtest may print `test lock_probe_child ... ` on the same line.
            if let Some(start) = line.find("PROBE ") {
                let event = line[start + "PROBE ".len()..].trim();
                return event.split(" pid=").next().unwrap().to_string();
            }
        }
    }

    /// A normal exit: the probe drops its guard and returns.
    fn finish(mut self) -> String {
        drop(self.stdin.take());
        let event = self.next_event();
        assert!(self.child.wait().unwrap().success());
        event
    }

    /// A force-quit or jetsam kill: SIGKILL, no destructors run.
    fn kill(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

/// Runs one short-lived probe to completion and returns its verdict.
fn try_in_new_process(mode: &str, save: &Path) -> String {
    let mut probe = Probe::spawn(mode, save);
    let event = probe.next_event();
    drop(probe.stdin.take());
    assert!(probe.child.wait().unwrap().success());
    event
}

/// Rewrites the PID a leftover lock records, whatever its on-disk form, to
/// model PID reuse after the recorded owner died.
fn repoint_recorded_pid(save: &Path, pid: u32) {
    let lock_path = SaveFileLock::lock_path_for(save);
    if lock_path.is_dir() {
        let owner_path = lock_path.join("owner.json");
        let mut owner: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&owner_path).unwrap()).unwrap();
        owner["pid"] = pid.into();
        std::fs::write(owner_path, serde_json::to_vec(&owner).unwrap()).unwrap();
    } else {
        assert!(lock_path.is_file(), "a killed owner leaves its lock behind");
        std::fs::write(&lock_path, pid.to_string()).unwrap();
    }
}

fn describe(save: &Path) -> String {
    let lock_path = SaveFileLock::lock_path_for(save);
    if lock_path.is_dir() {
        let owner = std::fs::read_to_string(lock_path.join("owner.json")).unwrap_or_default();
        format!("directory, owner.json={owner}")
    } else if lock_path.is_file() {
        let body = std::fs::read_to_string(&lock_path).unwrap_or_default();
        format!("file, contents={:?}", body.trim())
    } else {
        "absent".to_string()
    }
}

/// Serialises scenarios. Pipes are not created close-on-exec atomically on
/// Apple platforms, so a probe spawned by one test can inherit another test's
/// stdin pipe and keep it open; one scenario at a time avoids that.
static SERIAL: Mutex<()> = Mutex::new(());

/// A save path inside `LIMERICK_LOCK_TEST_ROOT` (the Simulator app container)
/// when set, otherwise a temporary directory.
fn save_path(name: &str) -> (MutexGuard<'static, ()>, Option<tempfile::TempDir>, PathBuf) {
    let serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let (temp, dir) = match std::env::var_os(ROOT_ENV) {
        Some(root) => (None, PathBuf::from(root).join(name)),
        None => {
            let temp = tempfile::tempdir().unwrap();
            let dir = temp.path().join(name);
            (Some(temp), dir)
        }
    };
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let save = dir.join("rundale.db");
    std::fs::write(&save, b"").unwrap();
    eprintln!("[{name}] save: {}", save.display());
    (serial, temp, save)
}

#[test]
fn acquire_and_release_across_processes() {
    let (_serial, _temp, save) = save_path("acquire_release");
    let holder = Probe::spawn("hold", &save);
    let mut holder = holder;
    assert_eq!(holder.next_event(), "ACQUIRED");
    assert!(is_locked(&save));
    eprintln!("[acquire_release] while held: {}", describe(&save));
    assert_eq!(holder.finish(), "RELEASED");
    assert!(!is_locked(&save));
    assert_eq!(try_in_new_process("try", &save), "ACQUIRED");
    eprintln!("[acquire_release] after release: {}", describe(&save));
}

#[test]
fn contending_process_is_refused_until_the_holder_exits() {
    let (_serial, _temp, save) = save_path("contention");
    let mut holder = Probe::spawn("hold", &save);
    assert_eq!(holder.next_event(), "ACQUIRED");
    assert_eq!(try_in_new_process("try", &save), "REFUSED");
    assert_eq!(try_in_new_process("try", &save), "REFUSED");
    assert_eq!(holder.finish(), "RELEASED");
    assert_eq!(try_in_new_process("try", &save), "ACQUIRED");
}

#[test]
fn relaunch_after_force_quit_acquires() {
    let (_serial, _temp, save) = save_path("force_quit");
    let mut holder = Probe::spawn("hold", &save);
    assert_eq!(holder.next_event(), "ACQUIRED");
    holder.kill();
    eprintln!("[force_quit] after SIGKILL: {}", describe(&save));
    assert!(!is_locked(&save), "a killed owner holds nothing");
    assert_eq!(try_in_new_process("try", &save), "ACQUIRED");
}

#[test]
fn relaunch_acquires_when_killed_owner_pid_now_names_an_unrelated_process() {
    let (_serial, _temp, save) = save_path("pid_reuse_unrelated");
    let mut holder = Probe::spawn("hold", &save);
    assert_eq!(holder.next_event(), "ACQUIRED");
    holder.kill();

    let mut unrelated = Probe::spawn("idle", &save);
    assert_eq!(unrelated.next_event(), "READY");
    repoint_recorded_pid(&save, unrelated.pid);
    eprintln!(
        "[pid_reuse_unrelated] record now names live pid {}: {}",
        unrelated.pid,
        describe(&save)
    );
    let verdict = try_in_new_process("try", &save);
    unrelated.kill();
    assert_eq!(
        verdict, "ACQUIRED",
        "a reused PID must not lock the save out"
    );
}

#[test]
fn relaunch_acquires_when_killed_owner_pid_names_a_process_it_cannot_signal() {
    // Inside the iOS app sandbox, other processes' PIDs answer kill(pid, 0)
    // with EPERM. PID 1 gives that answer to any unprivileged process.
    let (_serial, _temp, save) = save_path("pid_reuse_eperm");
    let mut holder = Probe::spawn("hold", &save);
    assert_eq!(holder.next_event(), "ACQUIRED");
    holder.kill();
    repoint_recorded_pid(&save, 1);
    eprintln!(
        "[pid_reuse_eperm] record now names pid 1: {}",
        describe(&save)
    );
    assert_eq!(try_in_new_process("try", &save), "ACQUIRED");
}

#[test]
fn relaunch_acquires_when_it_receives_the_killed_owner_pid() {
    let (_serial, _temp, save) = save_path("pid_reuse_own");
    let mut holder = Probe::spawn("hold", &save);
    assert_eq!(holder.next_event(), "ACQUIRED");
    holder.kill();
    assert_eq!(
        try_in_new_process("try-as-recorded-owner", &save),
        "ACQUIRED"
    );
}

#[test]
fn live_owner_is_never_displaced_by_a_relaunch() {
    // Background and suspension keep the process, so its lock must hold.
    // SIGSTOP models suspension: the owner exists but runs no code.
    let (_serial, _temp, save) = save_path("live_owner");
    let mut holder = Probe::spawn("hold", &save);
    assert_eq!(holder.next_event(), "ACQUIRED");
    assert_eq!(try_in_new_process("try", &save), "REFUSED");
    let pid = libc::pid_t::try_from(holder.pid).unwrap();
    assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
    let suspended = try_in_new_process("try", &save);
    let still_locked = is_locked(&save);
    assert_eq!(unsafe { libc::kill(pid, libc::SIGCONT) }, 0);
    assert_eq!(suspended, "REFUSED", "a suspended owner keeps its lock");
    assert!(still_locked);
    assert_eq!(holder.finish(), "RELEASED");
}
