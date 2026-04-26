//! RAII guard for inference-pause / inference-resume on the world clock.
//!
//! # Problem (#342)
//!
//! Every handler that calls an LLM must bracket the network call with
//! `world.clock.inference_pause()` / `world.clock.inference_resume()`.
//! If a panic or an early `return`/`?` happens between the two calls the
//! clock stays frozen permanently, which stops the game world from advancing.
//!
//! # Solution
//!
//! [`InferencePauseGuard`] calls `inference_pause()` on construction and
//! guarantees `inference_resume()` is called in its [`Drop`] implementation,
//! even if the owning task unwinds.  Callers replace the manual pair with:
//!
//! ```text
//! let _guard = InferencePauseGuard::new(Arc::clone(&state.world)).await;
//! // … await LLM calls …
//! // _guard is dropped here (or on any early-exit path), resuming the clock.
//! ```
//!
//! # Drop implementation
//!
//! `Drop` is synchronous, but the world lock is a `tokio::sync::Mutex`.
//!
//! **Fast path:** `try_lock()` succeeds (the common case — call sites release
//! the world lock before awaiting the LLM).
//!
//! **Slow path:** another task holds the world lock at the exact moment the
//! guard drops.  We use `tokio::task::block_in_place` to block the current
//! thread until the lock is available, then call `inference_resume()`.
//! `block_in_place` moves the blocking work off the Tokio worker thread pool
//! and is safe to call from `Drop` because it does not require an `async`
//! context — it only requires that the current thread belongs to a Tokio
//! multi-thread runtime (the only runtime we use in production).
//!
//! If there is no Tokio runtime at all (e.g. called from a pure synchronous
//! test or an OS thread without a runtime), `blocking_lock` is used as a
//! last resort — safe there because we are not on a Tokio worker thread.
//!
//! Callers must not hold any other `world` lock when the guard is dropped —
//! doing so would deadlock exactly as it would with any synchronous lock
//! acquisition.
//!
//! # Caveat: cancellation safety
//!
//! When a Tokio future is *cancelled* (e.g. the caller's task is aborted),
//! `Drop` is called on the guard's stack frame as the stack unwinds.
//! `blocking_lock` will then try to acquire the world mutex; if no other
//! task holds the lock this succeeds immediately.  If another task holds
//! the lock the thread blocks until it is released.  Because all handlers
//! release the world lock before awaiting the LLM, this is expected to be
//! fast in practice.

use std::sync::Arc;

use parish_world::WorldState;
use tokio::sync::Mutex;

/// RAII guard that holds the inference-pause state of the world clock.
///
/// Created via [`InferencePauseGuard::new`], which pauses the clock.
/// Releases the pause on [`Drop`], even if the owner panics or returns early.
pub struct InferencePauseGuard {
    world: Arc<Mutex<WorldState>>,
}

impl InferencePauseGuard {
    /// Acquires the world lock, calls `inference_pause()`, and releases the
    /// lock.  Returns the guard; dropping it calls `inference_resume()`.
    pub async fn new(world: Arc<Mutex<WorldState>>) -> Self {
        {
            let mut w = world.lock().await;
            w.clock.inference_pause();
        }
        Self { world }
    }
}

impl Drop for InferencePauseGuard {
    fn drop(&mut self) {
        // Fast path: all call sites release the world lock before awaiting
        // the LLM, so `try_lock()` almost always succeeds immediately.
        if let Ok(mut w) = self.world.try_lock() {
            w.clock.inference_resume();
            return;
        }

        // Slow path (rare — another task holds the world lock at the exact
        // moment this guard drops).
        //
        // Previously this used `handle.spawn(...)` whose `JoinHandle` was
        // immediately dropped, making the resume a fire-and-forget that could
        // silently never run if the runtime was shutting down (issue #650).
        //
        // Fix: use `block_in_place` to block the current thread until the
        // lock is available, guaranteeing `inference_resume()` always runs
        // before the guard is fully dropped.
        if tokio::runtime::Handle::try_current().is_ok() {
            let world = Arc::clone(&self.world);
            tokio::task::block_in_place(|| {
                let mut w = world.blocking_lock();
                w.clock.inference_resume();
            });
        } else {
            // No Tokio runtime (e.g. called from a pure synchronous test or
            // an OS thread without a runtime).  Use blocking_lock as a last
            // resort — safe here because we are not on a Tokio worker thread.
            let mut w = self.world.blocking_lock();
            w.clock.inference_resume();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_world::WorldState;

    fn fresh_world() -> Arc<Mutex<WorldState>> {
        Arc::new(Mutex::new(WorldState::default()))
    }

    /// Normal path: guard pauses on creation and resumes on drop.
    #[tokio::test]
    async fn guard_pauses_on_new_and_resumes_on_drop() {
        let world = fresh_world();

        assert!(!world.lock().await.clock.is_inference_paused());

        {
            let _guard = InferencePauseGuard::new(Arc::clone(&world)).await;
            assert!(world.lock().await.clock.is_inference_paused());
        } // guard drops here

        assert!(!world.lock().await.clock.is_inference_paused());
    }

    /// Panic regression for #342: the clock must be resumed even when the
    /// owning task panics after the guard is created.
    ///
    /// We spawn the panicking work in a `spawn_blocking` so the panic is
    /// contained and we can inspect the clock afterwards.
    #[tokio::test]
    async fn guard_resumes_clock_on_panic() {
        let world = fresh_world();
        let world_for_task = Arc::clone(&world);

        // Spawn a blocking task that:
        //  1. Creates the guard (pausing the clock via blocking_lock)
        //  2. Panics
        // The guard's Drop should still run during unwinding.
        //
        // We create the guard in a synchronous context by manually calling
        // the inner lock so we can use it in a std::thread.  The async
        // constructor is tested in the normal-path test above.
        let world_for_thread = Arc::clone(&world_for_task);
        let handle = std::thread::spawn(move || {
            // Pause manually (mirrors what `new` does without needing an async context).
            world_for_thread.blocking_lock().clock.inference_pause();

            // Build a minimal guard directly to test Drop on panic.
            let _guard = InferencePauseGuard {
                world: Arc::clone(&world_for_thread),
            };

            panic!("deliberate panic to test unwind-safety (#342)");
        });

        // The thread panics — that's expected.
        let _ = handle.join(); // ignore the Err(panic payload)

        // The clock must be resumed: Drop ran during unwinding.
        assert!(
            !world.lock().await.clock.is_inference_paused(),
            "clock must not be left paused after a panic in the owning task"
        );
    }

    /// Slow-path regression for #650: clock must be resumed even when another
    /// task holds the world lock at the exact moment the guard is dropped.
    ///
    /// We hold the world lock in one task while dropping the guard in another,
    /// forcing the slow path (`try_lock` fails → `block_in_place` fallback).
    /// The guard must block until the lock is released and then call
    /// `inference_resume()` — the previous `spawn` implementation would have
    /// silently skipped this if the runtime was shutting down.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn slow_path_resumes_clock_when_lock_contended() {
        let world = fresh_world();

        // Pause the clock manually so we can confirm it's resumed.
        world.lock().await.clock.inference_pause();
        assert!(world.lock().await.clock.is_inference_paused());

        // Channels: coordinate the lock-holder and the guard-dropper.
        // `lock_held_tx` signals that Task 1 has acquired the lock.
        // `done_tx` signals that Task 1 is ready to release the lock.
        let (lock_held_tx, lock_held_rx) = tokio::sync::oneshot::channel::<()>();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();

        // Task 1: acquires the world lock, signals that it's held, waits for
        // the go-ahead, then releases it.
        let world_for_holder = Arc::clone(&world);
        let holder = tokio::spawn(async move {
            let _held = world_for_holder.lock().await;
            // Signal: we now hold the lock.
            let _ = lock_held_tx.send(());
            // Wait until the test instructs us to release.
            let _ = done_rx.await;
            // `_held` dropped here → lock released.
        });

        // Build a guard directly (skip the `async new` which would re-pause).
        let guard = InferencePauseGuard {
            world: Arc::clone(&world),
        };

        // Wait until Task 1 holds the lock, then signal it to release soon.
        lock_held_rx.await.expect("holder task should signal");
        // Tell Task 1 to release — but `drop(guard)` below will call
        // `block_in_place` which spins until the lock is actually free.  The
        // release is async so there's a genuine race; block_in_place handles it.
        let _ = done_tx.send(());

        // Drop the guard while Task 1 (likely) still holds the lock → slow path.
        drop(guard);

        holder.await.expect("lock-holder task should not panic");

        // After drop (and after Task 1 released the lock), the clock must be
        // resumed — the slow path must have completed, not been abandoned.
        assert!(
            !world.lock().await.clock.is_inference_paused(),
            "clock must not remain paused after slow-path guard drop (#650)"
        );
    }

    /// Early-return path: guard resumes even when the caller returns before
    /// the natural end of the function (simulated via a nested block).
    #[tokio::test]
    async fn guard_resumes_on_early_return() {
        let world = fresh_world();

        let early_exit_ran = {
            let _guard = InferencePauseGuard::new(Arc::clone(&world)).await;
            assert!(world.lock().await.clock.is_inference_paused());
            // Simulate an early return by breaking out of the block.
            true
        };

        assert!(early_exit_ran);
        assert!(
            !world.lock().await.clock.is_inference_paused(),
            "clock must be resumed after early-exit from the guarded block"
        );
    }
}
