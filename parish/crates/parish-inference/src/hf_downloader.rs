//! HuggingFace Hub model downloader for the vllm-mlx onboarding flow.
//!
//! Wraps the [`hf-hub`](https://docs.rs/hf-hub) crate to pre-fetch all files
//! for one or more model repos into a local cache before `vllm-mlx serve`
//! starts. Reports byte-level progress through the existing
//! [`SetupProgress`] callback surface so the Tauri `SetupOverlay` (or any
//! other front-end implementing the trait) can render a download bar.
//!
//! ### Why a separate downloader instead of letting vllm-mlx do it
//!
//! `vllm-mlx serve` calls `huggingface_hub.snapshot_download()` itself on
//! startup, but that path is silent to Rust — progress only lands in
//! tqdm-rendered stderr lines and is fragile to parse. By pre-fetching from
//! Rust we get:
//!
//! - First-class progress events bound to the existing setup-overlay UI.
//! - Cache laid out exactly the way `huggingface_hub` expects (because
//!   `hf-hub` matches Python's cache format), so vllm-mlx finds the files
//!   without re-downloading.
//! - Spawn `vllm-mlx serve` with `HF_HUB_OFFLINE=1`, eliminating any
//!   surprise network calls during gameplay.
//!
//! ### Cache layout
//!
//! `hf-hub` writes the canonical HuggingFace cache tree:
//!
//! ```text
//! <cache_dir>/
//! └── models--<org>--<name>/
//!     ├── blobs/<sha256>                              ← actual bytes
//!     ├── snapshots/<commit-sha>/<rfilename> -> ../../blobs/<sha256>
//!     └── refs/main                                   ← contains commit-sha
//! ```
//!
//! vllm-mlx's downstream `mlx_lm.load(repo_id)` resolves files via the
//! same layout, so as long as we set `HF_HOME` (or rely on its default)
//! to the same root, no re-download fires.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parish_types::ParishError;
use reqwest::header::CONTENT_LENGTH;

use crate::setup::SetupProgress;

/// Downloads model files from HuggingFace Hub with progress reporting.
///
/// Construct with [`HfModelDownloader::new`], optionally override the
/// cache directory with [`HfModelDownloader::with_cache_dir`], then call
/// [`HfModelDownloader::download_models`] with one or more repo ids.
pub struct HfModelDownloader {
    progress: Arc<dyn SetupProgress>,
    cache_dir: Option<PathBuf>,
    endpoint: Option<String>,
    http: reqwest::Client,
}

impl HfModelDownloader {
    /// Creates a downloader that uses the default HuggingFace cache
    /// directory (`$HF_HOME/hub` or `~/.cache/huggingface/hub`).
    pub fn new(progress: Arc<dyn SetupProgress>) -> Self {
        Self {
            progress,
            cache_dir: None,
            endpoint: None,
            http: reqwest::Client::builder()
                .user_agent("parish-inference/hf-downloader")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Pins the cache to a specific directory. Must match the `HF_HOME`
    /// env var passed to `vllm-mlx serve` later so the downloaded files
    /// are visible to it.
    pub fn with_cache_dir(mut self, cache_dir: PathBuf) -> Self {
        self.cache_dir = Some(cache_dir);
        self
    }

    /// Overrides the HuggingFace Hub endpoint. Production uses the
    /// default (`https://huggingface.co`). Tests point this at a
    /// wiremock server so the two-pass manifest + download flow can
    /// be exercised without network egress.
    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Downloads all relevant files for each repo in `repos`.
    ///
    /// Runs in two passes:
    /// 1. **Manifest** — `repo.info()` lists every file in the repo, then
    ///    a `HEAD` against each file's resolve URL produces a
    ///    `content-length` to build a grand total in bytes. Files that
    ///    don't match a 4-bit MLX checkpoint (e.g. `*.bin`, `*.gguf`,
    ///    `*.onnx`) are filtered out so we don't fetch hundreds of MB of
    ///    unused weights.
    /// 2. **Download** — for each surviving file,
    ///    [`hf_hub::api::tokio::ApiRepo::download_with_progress`] streams
    ///    bytes into the cache and emits per-chunk callbacks that this
    ///    adapter forwards to [`SetupProgress::on_pull_progress`].
    pub async fn download_models(&self, repos: &[&str]) -> Result<(), ParishError> {
        let mut builder = hf_hub::api::tokio::ApiBuilder::from_env().with_progress(false);
        if let Some(dir) = &self.cache_dir {
            builder = builder.with_cache_dir(dir.clone());
        }
        if let Some(endpoint) = &self.endpoint {
            builder = builder.with_endpoint(endpoint.clone());
        }
        let api = builder
            .build()
            .map_err(|e| ParishError::Inference(format!("hf-hub Api build: {e}")))?;

        // Pass 1: manifest + size discovery.
        self.progress.on_status("Preparing model download…");
        let mut plan: Vec<(String, String)> = Vec::new();
        let mut grand_total: u64 = 0;
        for &repo in repos {
            let api_repo = api.model(repo.to_string());
            let info = api_repo
                .info()
                .await
                .map_err(|e| ParishError::Inference(format!("hf-hub info({repo}): {e}")))?;

            for sib in info.siblings.iter().filter(|s| keep_file(&s.rfilename)) {
                let url = api_repo.url(&sib.rfilename);
                let head = self
                    .http
                    .head(&url)
                    .send()
                    .await
                    .map_err(|e| ParishError::Inference(format!("HEAD {url}: {e}")))?;
                if !head.status().is_success() {
                    // Skip files HF reports but won't serve (rare; e.g. LFS
                    // pointers without an uploaded blob).
                    continue;
                }
                let size = head
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                grand_total = grand_total.saturating_add(size);
                plan.push((repo.to_string(), sib.rfilename.clone()));
            }
        }

        if plan.is_empty() {
            return Err(ParishError::Inference(format!(
                "No downloadable files in repos: {:?} (allow-list may be too tight)",
                repos
            )));
        }

        // Seed the progress bar so the UI shows total bytes before the
        // first chunk arrives.
        self.progress.on_pull_progress(0, grand_total);

        // Pass 2: per-file download with progress.
        let running = Arc::new(AtomicU64::new(0));
        for (repo, filename) in plan {
            let api_repo = api.model(repo.clone());
            let adapter = DownloadProgressAdapter {
                setup: Arc::clone(&self.progress),
                running_completed: Arc::clone(&running),
                grand_total,
            };
            api_repo
                .download_with_progress(&filename, adapter)
                .await
                .map_err(|e| {
                    ParishError::Inference(format!("hf-hub download {repo} {filename}: {e}"))
                })?;
        }

        // Final tick so any UI listening for "total == completed" can
        // mark the phase done.
        self.progress.on_pull_progress(grand_total, grand_total);

        Ok(())
    }
}

/// File allow-list. Keep everything `mlx_lm` needs to load a 4-bit MLX
/// checkpoint; drop large alternative-format weights so we don't fetch
/// hundreds of MB the runtime never reads.
fn keep_file(rfilename: &str) -> bool {
    // Drop known alternative-format weights.
    const SKIP_EXT: &[&str] = &[
        ".bin",   // PyTorch original
        ".pt",    // PyTorch
        ".pth",   // PyTorch
        ".ckpt",  // TF/legacy
        ".gguf",  // llama.cpp
        ".gguf2", // future llama.cpp
        ".onnx", ".tflite", ".h5", // Keras
        ".msgpack", ".flax", // Flax
    ];

    if let Some(ext_start) = rfilename.rfind('.') {
        let ext = &rfilename[ext_start..];
        if SKIP_EXT.iter().any(|s| s.eq_ignore_ascii_case(ext)) {
            return false;
        }
    }

    // Skip hidden/.gitattributes/README assets — vllm-mlx ignores them.
    // Keep everything else (safetensors, config.json, tokenizer*, etc.).
    !rfilename.starts_with('.') && rfilename != "README.md" && rfilename != "LICENSE"
}

/// Adapter implementing [`hf_hub::api::tokio::Progress`] that forwards
/// byte updates into our [`SetupProgress`] trait.
///
/// Cloned by `hf-hub` between files (the trait requires `Clone`). Internal
/// state lives behind `Arc` so clones share the running counter and the
/// grand total.
#[derive(Clone)]
struct DownloadProgressAdapter {
    setup: Arc<dyn SetupProgress>,
    running_completed: Arc<AtomicU64>,
    grand_total: u64,
}

impl hf_hub::api::tokio::Progress for DownloadProgressAdapter {
    async fn init(&mut self, _size: usize, filename: &str) {
        self.setup.on_status(&format!("Downloading {filename}"));
    }

    async fn update(&mut self, size: usize) {
        let prev = self
            .running_completed
            .fetch_add(size as u64, Ordering::Relaxed);
        let now = prev.saturating_add(size as u64);
        self.setup.on_pull_progress(now, self.grand_total);
    }

    async fn finish(&mut self) {
        // No-op: the next file's `init` will report progress; the final
        // call in `download_models` clamps to grand_total.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The `Progress` trait must be in scope so the adapter's `init` /
    // `update` / `finish` methods resolve via trait dispatch.
    use hf_hub::api::tokio::Progress;

    #[test]
    fn keep_file_filters_alternative_weights() {
        assert!(keep_file("config.json"));
        assert!(keep_file("tokenizer.json"));
        assert!(keep_file("tokenizer_config.json"));
        assert!(keep_file("model-00001-of-00002.safetensors"));
        assert!(keep_file("special_tokens_map.json"));

        assert!(!keep_file("pytorch_model.bin"));
        assert!(!keep_file("model.gguf"));
        assert!(!keep_file("model.onnx"));
        assert!(!keep_file("model.pth"));
        assert!(!keep_file("model.h5"));
        assert!(!keep_file(".gitattributes"));
        assert!(!keep_file("README.md"));
        assert!(!keep_file("LICENSE"));
    }

    #[test]
    fn keep_file_is_case_insensitive_on_extensions() {
        assert!(!keep_file("MODEL.BIN"));
        assert!(!keep_file("model.GGUF"));
    }

    /// Captures every `SetupProgress` callback in shared state so tests
    /// can assert ordering/values.
    #[derive(Default, Clone)]
    struct RecordingProgress {
        statuses: Arc<std::sync::Mutex<Vec<String>>>,
        progresses: Arc<std::sync::Mutex<Vec<(u64, u64)>>>,
        errors: Arc<std::sync::Mutex<Vec<String>>>,
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
    async fn adapter_forwards_running_total_across_files() {
        // Simulates hf-hub's per-file calls: init → update*N → finish,
        // repeated for a second file. Adapter must report a monotonic
        // running sum against grand_total.
        let progress = Arc::new(RecordingProgress::default());
        let p_trait: Arc<dyn SetupProgress> = progress.clone();
        let mut adapter = DownloadProgressAdapter {
            setup: p_trait,
            running_completed: Arc::new(AtomicU64::new(0)),
            grand_total: 1000,
        };

        // File 1: 400 bytes in two chunks.
        adapter.init(400, "config.json").await;
        adapter.update(200).await;
        adapter.update(200).await;
        adapter.finish().await;

        // File 2: 600 bytes in one chunk.
        adapter.init(600, "model.safetensors").await;
        adapter.update(600).await;
        adapter.finish().await;

        let progresses = progress.progresses.lock().unwrap().clone();
        let statuses = progress.statuses.lock().unwrap().clone();

        assert_eq!(progresses, vec![(200, 1000), (400, 1000), (1000, 1000)]);
        assert_eq!(
            statuses,
            vec![
                "Downloading config.json".to_string(),
                "Downloading model.safetensors".to_string(),
            ]
        );
        assert!(progress.errors.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn adapter_clone_shares_running_counter() {
        // hf-hub clones the adapter; clones must observe shared state so
        // the running total stays monotonic across files.
        let progress = Arc::new(RecordingProgress::default());
        let p_trait: Arc<dyn SetupProgress> = progress.clone();
        let mut a = DownloadProgressAdapter {
            setup: p_trait,
            running_completed: Arc::new(AtomicU64::new(0)),
            grand_total: 100,
        };
        a.update(40).await;
        let mut b = a.clone();
        b.update(60).await;

        let progresses = progress.progresses.lock().unwrap().clone();
        assert_eq!(progresses, vec![(40, 100), (100, 100)]);
    }
}
