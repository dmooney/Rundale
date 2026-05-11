//! HTTP-mocked integration tests for [`HfModelDownloader`].
//!
//! These tests stub the HuggingFace Hub endpoints (manifest +
//! per-file metadata + range download) with `wiremock` so we can
//! exercise the two-pass download flow without network egress. Pure
//! unit tests for the `Progress` adapter + the file allow-list live
//! inline in `src/hf_downloader.rs`.

use std::sync::{Arc, Mutex};

use parish_inference::hf_downloader::HfModelDownloader;
use parish_inference::setup::SetupProgress;
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Captures progress callbacks for assertion.
#[derive(Default, Clone)]
struct RecordingProgress {
    statuses: Arc<Mutex<Vec<String>>>,
    progresses: Arc<Mutex<Vec<(u64, u64)>>>,
    errors: Arc<Mutex<Vec<String>>>,
}

impl SetupProgress for RecordingProgress {
    fn on_status(&self, msg: &str) {
        self.statuses.lock().unwrap().push(msg.to_string());
    }
    fn on_pull_progress(&self, completed: u64, total: u64) {
        self.progresses.lock().unwrap().push((completed, total));
    }
    fn on_error(&self, msg: &str) {
        self.errors.lock().unwrap().push(msg.to_string());
    }
}

#[tokio::test]
async fn download_models_errors_when_allow_list_skips_every_file() {
    // Manifest with only PyTorch + GGUF weights — both excluded by
    // `keep_file`. The downloader must surface a clear error rather
    // than silently succeed with zero bytes downloaded.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/api/models/test/skip-repo/revision/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": "deadbeef0000000000000000000000000000000",
            "siblings": [
                {"rfilename": "pytorch_model.bin"},
                {"rfilename": "model.gguf"},
                {"rfilename": "README.md"},
            ],
        })))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let progress = Arc::new(RecordingProgress::default());
    let p_trait: Arc<dyn SetupProgress> = progress.clone();

    let downloader = HfModelDownloader::new(p_trait)
        .with_cache_dir(tmp.path().to_path_buf())
        .with_endpoint(server.uri());

    let err = downloader
        .download_models(&["test/skip-repo"])
        .await
        .expect_err("manifest with no whitelisted files must error");

    let msg = err.to_string();
    assert!(
        msg.contains("No downloadable files"),
        "expected manifest-empty error, got: {msg}"
    );
    // The pre-flight status fires before the manifest empties out.
    let statuses = progress.statuses.lock().unwrap().clone();
    assert!(
        statuses.contains(&"Preparing model download…".to_string()),
        "expected pre-flight status to be reported, got: {statuses:?}"
    );
    // No progress callbacks should have fired — the manifest never
    // produced a non-zero grand_total.
    assert!(progress.progresses.lock().unwrap().is_empty());
    assert!(progress.errors.lock().unwrap().is_empty());
}

#[tokio::test]
async fn download_models_surfaces_manifest_fetch_failure() {
    // 404 on the manifest endpoint must propagate as a ParishError
    // rather than panicking or hanging. Common when the user typos a
    // repo id or HF retires a model.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/api/models/test/missing/revision/main"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let progress: Arc<dyn SetupProgress> = Arc::new(RecordingProgress::default());

    let downloader = HfModelDownloader::new(progress)
        .with_cache_dir(tmp.path().to_path_buf())
        .with_endpoint(server.uri());

    let err = downloader
        .download_models(&["test/missing"])
        .await
        .expect_err("404 manifest must error");

    let msg = err.to_string();
    assert!(
        msg.contains("hf-hub info"),
        "expected hf-hub info(...) error, got: {msg}"
    );
}

#[tokio::test]
async fn download_models_sums_head_content_length_into_grand_total() {
    // Manifest passes the allow-list. HEAD responses report byte
    // sizes. The downloader's Pass 1 must call HEAD for each kept
    // file and sum the content-length values. Pass 2 is expected to
    // fail (we don't mock the full ranged-GET protocol here); the
    // assertion is that the pre-flight progress tick seeds the bar
    // with the summed total, which the SetupOverlay needs to render
    // the bar's max value.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/api/models/test/sum-repo/revision/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": "1234567890abcdef0000000000000000000000",
            "siblings": [
                {"rfilename": "config.json"},
                {"rfilename": "tokenizer.json"},
                {"rfilename": "pytorch_model.bin"},  // skipped
            ],
        })))
        .mount(&server)
        .await;

    Mock::given(method("HEAD"))
        .and(path_matcher("/test/sum-repo/resolve/main/config.json"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-length", "100"))
        .mount(&server)
        .await;

    Mock::given(method("HEAD"))
        .and(path_matcher("/test/sum-repo/resolve/main/tokenizer.json"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-length", "250"))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let progress = Arc::new(RecordingProgress::default());
    let p_trait: Arc<dyn SetupProgress> = progress.clone();
    let downloader = HfModelDownloader::new(p_trait)
        .with_cache_dir(tmp.path().to_path_buf())
        .with_endpoint(server.uri());

    // Pass 2 will fail (no ranged-GET mocks). The assertion is on
    // what happened during Pass 1.
    let _ = downloader.download_models(&["test/sum-repo"]).await;

    let progresses = progress.progresses.lock().unwrap().clone();
    // First progress tick seeds the bar with the summed total
    // before the first byte arrives. `pytorch_model.bin` is filtered
    // out of the sum because it fails `keep_file`.
    assert!(!progresses.is_empty(), "expected at least the seed tick");
    let (completed, total) = progresses[0];
    assert_eq!(completed, 0);
    assert_eq!(total, 350, "expected 100 + 250 = 350 (bin excluded)");
}
