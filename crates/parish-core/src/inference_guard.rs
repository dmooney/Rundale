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
//! We use [`tokio::sync::Mutex::blocking_lock`] in `Drop` to acquire the
//! lock from a synchronous context.  This is safe inside a Tokio runtime
//! because `blocking_lock` only blocks the *current* thread (not the entire
//! runtime) and is specifically designed for this use-case.  Callers must
//! not hold any other `world` lock when the guard is dropped — doing so
//! would deadlock exactly as it would with any synchronous lock acquisition.
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
        // moment this guard drops): spawn a task that waits for the lock and
        // then resumes the clock.  We capture the Tokio runtime handle so
        // this works even during stack unwinding from a panicking task.
        let world = Arc::clone(&self.world);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let mut w = world.lock().await;
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
