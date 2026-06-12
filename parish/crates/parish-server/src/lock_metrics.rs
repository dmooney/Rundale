//! Lock-contention metrics for the [`crate::state::AppState`] mutexes.
//!
//! #1366 §2 prerequisite: before any finer-grained lock splitting we need
//! evidence of actual contention. [`MeteredMutex`] wraps `tokio::sync::Mutex`
//! and records, per lock, how often it is taken, how long acquirers waited,
//! and how many waits crossed the slow threshold — cheap atomics on the
//! acquisition path, no behavioural change.
//!
//! The wrapper derefs to the inner `Mutex` so the shared
//! `parish_core::game_loop` APIs (`GameLoopContext`, `NewGameParams`), which
//! borrow fields as `&Mutex<T>`, keep compiling unchanged; those borrows
//! bypass the metering, which is fine — the server's own handler paths are
//! the ones the lock-ordering invariant worries about, and they call the
//! inherent [`MeteredMutex::lock`] below.

use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, MutexGuard};

/// Wait time above which an acquisition is logged at WARN and counted as a
/// slow wait. Single-player sessions should effectively never cross this;
/// when they do, the named lock is a splitting candidate.
pub const SLOW_WAIT: Duration = Duration::from_millis(50);

/// Wait time above which an acquisition is logged at DEBUG — visible noise
/// before it becomes a WARN-level problem.
const NOTABLE_WAIT: Duration = Duration::from_millis(5);

/// Point-in-time counters for one metered lock.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LockMetricsSnapshot {
    /// Lock name — matches the corresponding `LOCK_ORDER` node where one exists.
    pub name: &'static str,
    /// Total successful `lock()` acquisitions.
    pub acquisitions: u64,
    /// Cumulative time spent waiting to acquire, in microseconds.
    pub total_wait_micros: u64,
    /// Longest single wait, in microseconds.
    pub max_wait_micros: u64,
    /// Waits that crossed [`SLOW_WAIT`].
    pub slow_waits: u64,
    /// Failed `try_lock()` attempts — each one is a contention event.
    pub try_lock_failures: u64,
}

/// A `tokio::sync::Mutex` that records wait-time metrics on acquisition.
pub struct MeteredMutex<T> {
    name: &'static str,
    inner: Mutex<T>,
    acquisitions: AtomicU64,
    total_wait_micros: AtomicU64,
    max_wait_micros: AtomicU64,
    slow_waits: AtomicU64,
    try_lock_failures: AtomicU64,
}

impl<T> MeteredMutex<T> {
    /// Creates a metered mutex. `name` should match the lock's
    /// `crate::state::LOCK_ORDER` node so warnings and snapshots line up
    /// with the documented ordering chain.
    pub fn new(name: &'static str, value: T) -> Self {
        Self {
            name,
            inner: Mutex::new(value),
            acquisitions: AtomicU64::new(0),
            total_wait_micros: AtomicU64::new(0),
            max_wait_micros: AtomicU64::new(0),
            slow_waits: AtomicU64::new(0),
            try_lock_failures: AtomicU64::new(0),
        }
    }

    /// Locks the inner mutex, recording how long the acquisition waited.
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        let start = Instant::now();
        let guard = self.inner.lock().await;
        let waited = start.elapsed();

        let micros = u64::try_from(waited.as_micros()).unwrap_or(u64::MAX);
        self.acquisitions.fetch_add(1, Ordering::Relaxed);
        self.total_wait_micros.fetch_add(micros, Ordering::Relaxed);
        self.max_wait_micros.fetch_max(micros, Ordering::Relaxed);

        if waited >= SLOW_WAIT {
            self.slow_waits.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                lock = self.name,
                wait_ms = waited.as_millis() as u64,
                "slow AppState lock acquisition — contention on this lock"
            );
        } else if waited >= NOTABLE_WAIT {
            tracing::debug!(
                lock = self.name,
                wait_ms = waited.as_millis() as u64,
                "notable AppState lock wait"
            );
        }

        guard
    }

    /// Attempts to lock without waiting; a failure is counted as contention.
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, tokio::sync::TryLockError> {
        match self.inner.try_lock() {
            Ok(guard) => {
                self.acquisitions.fetch_add(1, Ordering::Relaxed);
                Ok(guard)
            }
            Err(e) => {
                self.try_lock_failures.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    /// Returns the current counters for this lock.
    pub fn metrics(&self) -> LockMetricsSnapshot {
        LockMetricsSnapshot {
            name: self.name,
            acquisitions: self.acquisitions.load(Ordering::Relaxed),
            total_wait_micros: self.total_wait_micros.load(Ordering::Relaxed),
            max_wait_micros: self.max_wait_micros.load(Ordering::Relaxed),
            slow_waits: self.slow_waits.load(Ordering::Relaxed),
            try_lock_failures: self.try_lock_failures.load(Ordering::Relaxed),
        }
    }
}

/// Deref to the inner mutex so `&MeteredMutex<T>` coerces to `&Mutex<T>`
/// where the shared core APIs expect a plain mutex borrow.
impl<T> Deref for MeteredMutex<T> {
    type Target = Mutex<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn lock_metrics_count_acquisitions() {
        let m = MeteredMutex::new("test", 0u32);
        for _ in 0..3 {
            let mut g = m.lock().await;
            *g += 1;
        }
        let snap = m.metrics();
        assert_eq!(snap.name, "test");
        assert_eq!(snap.acquisitions, 3);
        assert_eq!(snap.slow_waits, 0);
    }

    #[tokio::test]
    async fn contended_lock_records_wait_time() {
        let m = Arc::new(MeteredMutex::new("contended", ()));
        let guard = m.lock().await;

        let contender = {
            let m = Arc::clone(&m);
            tokio::spawn(async move {
                let _g = m.lock().await;
            })
        };
        // Hold the lock long enough for the contender to be measurably blocked.
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(guard);
        contender.await.unwrap();

        let snap = m.metrics();
        assert_eq!(snap.acquisitions, 2);
        assert!(
            snap.max_wait_micros >= 10_000,
            "contender should have waited ≥10ms, recorded {}µs",
            snap.max_wait_micros
        );
        assert!(snap.total_wait_micros >= snap.max_wait_micros);
    }

    #[tokio::test]
    async fn try_lock_failure_is_counted() {
        let m = MeteredMutex::new("try", ());
        let _held = m.lock().await;
        assert!(m.try_lock().is_err());
        assert_eq!(m.metrics().try_lock_failures, 1);
    }

    #[tokio::test]
    async fn deref_exposes_inner_mutex() {
        let m = MeteredMutex::new("deref", 7u32);
        // The shared-core borrow shape: a plain `&Mutex<T>`.
        let inner: &Mutex<u32> = &m;
        assert_eq!(*inner.lock().await, 7);
        // Locks through the deref'd reference are deliberately unmetered.
        assert_eq!(m.metrics().acquisitions, 0);
    }
}
