# ADR-026: One Save Lock, Held by the Kernel

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-09-27). Resolves the save-lock item of Mobile Phase 2 in the
[mobile engine convergence plan](../plans/mobile-engine-convergence.md) and
[ADR-025](025-mobile-runtime-on-shared-engine.md) (issue #2039).

## Context

ADR-025 keeps one save system for desktop and iPhone, so it keeps one save lock.
The plan said to check the existing lock under the iOS sandbox and to adopt a
kernel lock only if the directory/PID lock proved inadequate there.

The existing `SaveFileLock` (`limerick-persistence/src/lock/`) created a
`<save>.lock` directory holding an `owner.json` record with the owner's PID. A
contender decided whether the lock was stale by asking whether that PID was alive
(`kill(pid, 0)` on Unix, where `EPERM` counts as alive).

An iOS app does not exit normally. The system suspends it in the background and
kills it with SIGKILL when it needs memory (jetsam) or when the player swipes it
away. No destructor runs, so the lock record is always left behind. The next launch
is a new process. Its PID can be one the dead owner's record already names: it may
belong to an unrelated process, to a process the sandbox does not let the app
signal, or to the relaunched app itself.

### Evidence

`limerick-persistence/tests/save_lock_processes.rs` tests the lock with real
processes. The test binary starts itself again as a child that holds the lock,
tries it, or idles. SIGKILL models a force-quit or jetsam kill, and SIGSTOP models
suspension. PID reuse is modelled by rewriting the PID in the lock the killed owner
left behind: to a live unrelated process, to PID 1 (which answers `kill(pid, 0)`
with `EPERM`, as other apps' processes do in the iOS sandbox), or to the relaunched
process's own PID.

`limerick/scripts/ios-sim-save-lock.sh` (`just ios-sim-save-lock`) builds these
tests and the lock unit tests for `aarch64-apple-ios-sim`. It runs them with
`xcrun simctl spawn` on a new iPhone 17 simulator (iOS 26.5, Xcode 26.6), with the
saves inside Safari's sandbox data container under
`Library/Application Support/Rundale/`, where the iOS app keeps its save.

Directory/PID lock on the simulator, 2026-09-27 (lock code as at `e3c1b8de0`):

| Scenario                                                     | Result                |
| ------------------------------------------------------------ | --------------------- |
| Acquire and release across processes                         | pass                  |
| Contending process refused until the holder exits            | pass                  |
| Owner suspended (SIGSTOP): relaunch refused                  | pass                  |
| Relaunch after force-quit, recorded PID free                 | pass                  |
| Relaunch after force-quit, recorded PID names a live process | **fail: save locked** |
| Relaunch after force-quit, recorded PID answers `EPERM`      | **fail: save locked** |
| Relaunch receives the killed owner's own PID                 | **fail: save locked** |

In each failure the lockout is permanent. Nothing ever removes the record, so the
player can't open their save again until that PID is free. A process killed between
creating the directory and publishing `owner.json` leaves a directory that the
lock treats as locked forever, by design. On iOS, where any instant can be the
moment of a kill, this is the same kind of lockout. The lock is inadequate on iOS.

## Decision

Keep one lock, `SaveFileLock`, and hold it with the kernel. `try_acquire` opens
`<save>.lock` and takes an exclusive, non-blocking lock with
`std::fs::File::try_lock` (`flock` on macOS, iOS and Linux; `LockFileEx` on
Windows). The kernel releases the lock when the owning process ends for any
reason, so a stale lock can't exist and no PID is ever consulted to decide
ownership.

- The file records the owner's PID only for diagnostics. It also keeps older builds,
  which read that PID, treating a held save as locked.
- Acquisitions in the same process share one open file through a registry, because a
  second open file's lock would conflict with the first.
- On Unix (including iOS) the last guard removes the file, but only if the path
  still names its file (same device and inode). A contender that locked a file
  just removed notices and retries on the new file. std has no stable file
  identity on Windows, so there the file is never removed and the lock on that
  one file is the whole protocol.
- `is_locked` probes with a shared lock that it drops at once. A contender acquiring
  at that same instant can be refused once; it is never granted a held lock.
- A lock directory left by an earlier build is removed when its recorded owner is
  dead. A live, unreadable, or incomplete one keeps the save locked, as before. This
  affects only desktop saves locked by an older build; no iOS build has shipped
  with the directory lock.

The same simulator run with the kernel lock passes all eight lifecycle tests and
the ten lock unit tests. The transcript is in the PR for #2039.

## Consequences

- A relaunch after a force-quit, jetsam kill, or crash always gets its save back,
  on every platform, whatever PIDs the system has reused.
- A backgrounded or suspended app keeps its lock, because the process and its open
  file still exist.
- iOS terminates a suspended app that holds a file lock in a container shared with
  other processes (an App Group), with the exception code `0xdead10cc`. The save,
  and its lock, must stay in the app's own data container
  (`Library/Application Support`). A save in an App Group container would need the
  lock released on entering the background.
- Not verified here: a physical iPhone, the real app sandbox's signal policy, real
  suspension, and jetsam. `simctl spawn` runs a process inside the simulator's
  runtime and writes into an app's data container, but it is not a sandboxed
  app launch. The kernel lock doesn't depend on signal permissions or on how a
  process ends, which is why the simulator result is expected to hold on a device.
  Re-run `just ios-sim-save-lock` after changing the lock, and exercise force-quit
  and relaunch on a device once the app from #2043 exists.
