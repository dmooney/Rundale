Verdict: sufficient
Technical debt: clear

This PR hardens a flaky test in `parish-inference` (issue #981). No gameplay
behaviour, IPC handlers, or runtime-shipping paths were changed.

Root cause confirmed: `tracing::subscriber::set_default` is thread-local; the
multi-threaded tokio executor could poll the future on a different thread after
an `.await`, losing the subscriber before the `info!()` call in `emit_metrics`
fired. Under tarpaulin's MIR instrumentation this race was observable.

Two changes together eliminate the flake without altering production semantics:

- `#[tokio::test(flavor = "current_thread")]` prevents cross-thread task
  migration, keeping the thread-local subscriber visible for the whole test.
- Replacing the `MakeWriter` buffer with a `tracing::Layer` collector removes
  the fmt→write→flush pipeline that added write-timing sensitivity.

Evidence: 262 `parish-inference` lib tests pass (including the target test),
0 failures.
