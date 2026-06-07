//! Opt-in demonstration of the CI flaky-test detector (#1176).
//!
//! The CI test job runs `cargo nextest run --profile ci`, which sets
//! `retries = 1` (see `parish/.config/nextest.toml`). A test that **fails on
//! its first attempt and passes on the retry** is reported by nextest as
//! `FLAKY` rather than hard-failing the run — that is the nondeterminism
//! detector the issue asks for.
//!
//! This test is the proof that the detector fires. It is **disabled by
//! default**: it only does anything when `PARISH_FLAKY_DEMO=1` is set, so a
//! normal `cargo test` / `cargo nextest run` never sees a spurious failure. CI
//! does not set that variable for the gating job either — the flaky demo is run
//! in a dedicated, non-gating step (and in the proof bundle) so the FLAKY
//! classification can be observed without putting an intentionally-failing test
//! on the merge path.
//!
//! Mechanism: nextest runs each attempt in a fresh process. To make attempt 1
//! fail and attempt 2 pass, the first run records a marker file; the presence
//! of that marker on the second run flips the assertion to success. The marker
//! lives under a caller-provided directory (`PARISH_FLAKY_DEMO_DIR`, falling
//! back to the system temp dir) so repeated invocations start clean.

use std::path::PathBuf;

fn marker_path() -> PathBuf {
    let dir = std::env::var("PARISH_FLAKY_DEMO_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    dir.join("parish_flaky_demo_attempt.marker")
}

#[test]
fn intentionally_flaky_passes_on_retry() {
    // Inert unless explicitly opted in, so this never destabilises the real
    // suite.
    if std::env::var("PARISH_FLAKY_DEMO").as_deref() != Ok("1") {
        return;
    }

    let marker = marker_path();
    if marker.exists() {
        // Second attempt (the nextest retry): clean up and pass. nextest now
        // classifies this test as FLAKY (failed once, passed on retry).
        let _ = std::fs::remove_file(&marker);
        return;
    }

    // First attempt: drop the marker and fail, triggering the retry.
    std::fs::write(&marker, b"attempted").expect("write flaky marker");
    panic!("intentional first-attempt failure to exercise the flaky-test detector (#1176)");
}
