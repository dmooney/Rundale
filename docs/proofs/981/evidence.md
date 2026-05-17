Evidence type: gameplay transcript
Date: 2026-05-17
Branch: claude/fix-issue-981-gs9a8

## Issue

#981 — `metered_client_emits_tracing_event_on_success` flakes under
`cargo tarpaulin` because `tracing::subscriber::set_default` is thread-local
and the default multi-threaded tokio executor can poll the future on a
different OS thread after an `.await`, losing the subscriber before the
`info!()` call in `emit_metrics` fires.

## Changes

`parish/crates/parish-inference/src/inference_client.rs` (test section only):

1. Changed `#[tokio::test]` to `#[tokio::test(flavor = "current_thread")]`
   to pin all `.await` continuations to the same OS thread.

2. Replaced the `tracing_subscriber::fmt()` + `MakeWriter` buffer chain with
   a direct `tracing::Layer` collector (`EventCollector`) whose `on_event`
   fires synchronously at dispatch time — no fmt → write → flush timing gap.

`parish/Cargo.toml`: Added `"registry"` feature to `tracing-subscriber`
(required for `tracing_subscriber::registry()`).

## Test Run — target test

Command:

```sh
cargo test -p parish-inference inference_client::tests::metered_client_emits_tracing_event_on_success
```

Result:

```
test inference_client::tests::metered_client_emits_tracing_event_on_success ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 267 filtered out; finished in 0.00s
```

## Test Run — full suite

Command:

```sh
cargo test -p parish-inference --lib
```

Result:

```
test result: ok. 262 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 4.45s
```

## Behaviour Impact

This is a test-only change. No production code paths were modified. The
`emit_metrics` function and `MeteredInferenceClient` implementation are
unchanged. The fix ensures the test correctly captures the tracing event it
has always been intended to assert — it does not change what the production
code emits.
