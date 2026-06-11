//! Provider setup orchestration — Ollama install/start/pull/warmup and the
//! unified [`setup_provider_client`] entry point.

use parish_config::{InferenceConfig, ProviderConfig};
use parish_providers::AnyClient;
use parish_providers::openai_client::{OpenAiClient, build_client_or_fallback};
use parish_types::ParishError;
use reqwest::StatusCode;
use serde::Deserialize;
use std::process::Command;
use std::time::Duration;

use super::gpu_detect::{GpuInfo, GpuVendor, detect_gpu_info};
use super::model_select::{ModelConfig, select_model};
use super::process::{
    OllamaProcess, RuntimeProcesses, VllmMlxProcess, VllmMlxSlot, VllmProcess, VllmSlot,
};
use super::progress::SetupProgress;

// ── URL helpers ─────────────────────────────────────────────────────────────

/// Parses the port out of an OpenAI-compat base_url like
/// `http://localhost:8000/v1`. Returns `None` if no explicit port.
pub(super) fn port_from_base_url(base_url: &str) -> Option<u16> {
    let url = url_from_str(base_url)?;
    url.port()
}

/// Lightweight URL parser using `reqwest::Url` (already a workspace dep).
fn url_from_str(s: &str) -> Option<reqwest::Url> {
    reqwest::Url::parse(s).ok()
}

// ── GPU env vars ─────────────────────────────────────────────────────────────

/// Builds GPU-specific environment variables for the Ollama process.
///
/// On Windows with an AMD GPU, returns `OLLAMA_VULKAN=1` to enable
/// experimental Vulkan acceleration (required for RDNA 4 / unsupported
/// AMD GPUs where ROCm is not available). For NVIDIA or Linux AMD,
/// Ollama auto-detects CUDA/ROCm so no extra env vars are needed.
pub fn build_gpu_env(gpu_info: &GpuInfo) -> Option<Vec<(String, String)>> {
    #[cfg(target_os = "windows")]
    if gpu_info.vendor == GpuVendor::Amd {
        return Some(vec![("OLLAMA_VULKAN".to_string(), "1".to_string())]);
    }

    // Suppress unused variable warning on non-Windows
    let _ = gpu_info;

    None
}

// ── Ollama check / install ───────────────────────────────────────────────────

/// Checks whether the `ollama` binary is available on the system PATH.
pub fn check_ollama_installed() -> bool {
    Command::new("ollama")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Attempts to install Ollama using the official install script.
///
/// Downloads and runs `https://ollama.com/install.sh`. This script
/// auto-detects the GPU vendor and installs the appropriate version
/// (CUDA for NVIDIA, ROCm for AMD, CPU fallback).
///
/// # Errors
///
/// Returns `ParishError::Setup` if the install script fails or
/// if `curl` is not available.
pub async fn install_ollama(progress: &dyn SetupProgress) -> Result<(), ParishError> {
    progress.on_status("The parish storyteller hasn't arrived yet. Sending word...");
    progress.on_status("This may take a few minutes. Put the kettle on.");

    let status = tokio::task::spawn_blocking(|| {
        Command::new("sh")
            .arg("-c")
            .arg("curl -fsSL https://ollama.com/install.sh | sh")
            .status()
    })
    .await
    .map_err(|e| ParishError::Setup(format!("install task panicked: {}", e)))?
    .map_err(|e| ParishError::Setup(format!("failed to run install script: {}", e)))?;

    if !status.success() {
        return Err(ParishError::Setup(
            "Ollama install script failed. Please install manually: https://ollama.com/download"
                .to_string(),
        ));
    }

    progress.on_status("Grand — the storyteller has arrived.");
    Ok(())
}

// ── Ollama model availability + pull ────────────────────────────────────────

/// Response from Ollama's `/api/tags` endpoint.
#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

/// A single model entry from `/api/tags`.
#[derive(Deserialize)]
struct TagModel {
    #[serde(default)]
    name: String,
}

/// Response line from Ollama's streaming `/api/pull` endpoint.
#[derive(Deserialize)]
struct PullProgressLine {
    #[serde(default)]
    status: String,
    #[serde(default)]
    digest: String,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    completed: u64,
}

#[derive(Default)]
struct PullArtifactProgress {
    completed: u64,
    total: u64,
}

#[derive(Default)]
struct PullProgressTracker {
    /// Ollama reports each manifest/layer separately; the UI needs the aggregate model pull.
    artifacts: std::collections::BTreeMap<String, PullArtifactProgress>,
}

impl PullProgressTracker {
    fn record(&mut self, line: &PullProgressLine) -> Option<(u64, u64)> {
        if line.total == 0 {
            return None;
        }

        let key = line.download_key();
        let artifact = self.artifacts.entry(key).or_default();
        artifact.total = artifact.total.max(line.total);
        artifact.completed = artifact.completed.max(line.completed.min(artifact.total));

        Some(self.aggregate())
    }

    fn aggregate(&self) -> (u64, u64) {
        self.artifacts
            .values()
            .fold((0_u64, 0_u64), |(completed, total), artifact| {
                (
                    completed.saturating_add(artifact.completed.min(artifact.total)),
                    total.saturating_add(artifact.total),
                )
            })
    }
}

impl PullProgressLine {
    fn download_key(&self) -> String {
        let digest = self.digest.trim();
        if !digest.is_empty() {
            return format!("digest:{digest}");
        }

        let status = self.status.trim();
        if !status.is_empty() {
            return format!("status:{status}");
        }

        "download".to_string()
    }
}

/// Checks whether a model is available locally in Ollama.
///
/// Queries the `/api/tags` endpoint and checks if the model name
/// appears in the list of locally available models. Uses the default
/// reachability timeout (10s).
pub async fn is_model_available(base_url: &str, model_name: &str) -> Result<bool, ParishError> {
    is_model_available_with_config(base_url, model_name, &InferenceConfig::default()).await
}

/// Checks whether a model is available locally in Ollama, with configurable timeout.
///
/// Uses `config.reachability_timeout_secs` for the HTTP request timeout.
pub async fn is_model_available_with_config(
    base_url: &str,
    model_name: &str,
    config: &InferenceConfig,
) -> Result<bool, ParishError> {
    let url = format!("{}/api/tags", base_url);
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.reachability_timeout_secs))
        .build()
        .map_err(|e| ParishError::Setup(format!("failed to build HTTP client: {}", e)))?;

    let resp = http
        .get(&url)
        .send()
        .await
        .map_err(|e| ParishError::Setup(format!("failed to query models: {}", e)))?;

    let tags: TagsResponse = resp
        .json()
        .await
        .map_err(|e| ParishError::Setup(format!("failed to parse model list: {}", e)))?;

    // Check both exact match and with :latest suffix
    let available = tags.models.iter().any(|m| {
        m.name == model_name
            || m.name == format!("{}:latest", model_name)
            || model_name == format!("{}:latest", m.name)
    });

    Ok(available)
}

/// Pulls (downloads) a model from the Ollama registry.
///
/// Streams progress from the `/api/pull` endpoint and reports it
/// via the `SetupProgress` trait. Blocks until the pull is complete.
/// Uses the default model download timeout (3600s).
///
/// # Errors
///
/// Returns `ParishError::ModelNotAvailable` if the pull fails.
pub async fn pull_model(
    base_url: &str,
    model_name: &str,
    progress: &dyn SetupProgress,
) -> Result<(), ParishError> {
    pull_model_with_config(base_url, model_name, progress, &InferenceConfig::default()).await
}

/// Pulls (downloads) a model from the Ollama registry, with configurable timeout.
///
/// Uses `config.model_download_timeout_secs` for the HTTP request timeout.
///
/// # Errors
///
/// Returns `ParishError::ModelNotAvailable` if the pull fails.
pub async fn pull_model_with_config(
    base_url: &str,
    model_name: &str,
    progress: &dyn SetupProgress,
    config: &InferenceConfig,
) -> Result<(), ParishError> {
    progress.on_status(&format!(
        "Fetching the storyteller's book of tales ('{}')...",
        model_name
    ));

    let url = format!("{}/api/pull", base_url);
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.model_download_timeout_secs))
        .build()
        .map_err(|e| ParishError::Setup(format!("failed to build HTTP client: {}", e)))?;

    let resp = http
        .post(&url)
        .json(&serde_json::json!({ "model": model_name }))
        .send()
        .await
        .map_err(|e| {
            ParishError::ModelNotAvailable(format!(
                "failed to start pull for '{}': {}",
                model_name, e
            ))
        })?;

    if !resp.status().is_success() {
        return Err(ParishError::ModelNotAvailable(format!(
            "Ollama returned {} when pulling '{}'",
            resp.status(),
            model_name
        )));
    }

    stream_pull_progress(resp, progress).await?;

    progress.on_status(&format!(
        "The storyteller has '{}' in hand. Grand so.",
        model_name
    ));
    Ok(())
}

/// Deletes a locally available Ollama model so the next pull fetches it anew.
async fn delete_model_with_config(
    base_url: &str,
    model_name: &str,
    progress: &dyn SetupProgress,
    config: &InferenceConfig,
) -> Result<(), ParishError> {
    progress.on_status(&format!(
        "Clearing the local copy of '{}' before fetching it again...",
        model_name
    ));

    let url = format!("{}/api/delete", base_url);
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.model_download_timeout_secs))
        .build()
        .map_err(|e| ParishError::Setup(format!("failed to build HTTP client: {}", e)))?;

    let resp = http
        .delete(&url)
        .json(&serde_json::json!({ "model": model_name }))
        .send()
        .await
        .map_err(|e| {
            ParishError::Setup(format!(
                "failed to delete local Ollama model '{}': {}",
                model_name, e
            ))
        })?;

    if resp.status() == StatusCode::NOT_FOUND {
        progress.on_status(&format!(
            "No local copy of '{}' was present. Fetching it now...",
            model_name
        ));
        return Ok(());
    }

    if !resp.status().is_success() {
        return Err(ParishError::Setup(format!(
            "Ollama returned {} when deleting local model '{}'",
            resp.status(),
            model_name
        )));
    }

    progress.on_status(&format!(
        "Local copy of '{}' removed. Fetching a fresh copy...",
        model_name
    ));
    Ok(())
}

async fn stream_pull_progress(
    mut resp: reqwest::Response,
    progress: &dyn SetupProgress,
) -> Result<(), ParishError> {
    let mut pending = Vec::new();
    let mut tracker = PullProgressTracker::default();

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| ParishError::ModelNotAvailable(format!("pull stream error: {}", e)))?
    {
        pending.extend_from_slice(&chunk);

        while let Some(newline_index) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=newline_index).collect();
            report_pull_progress_line(&line, progress, &mut tracker);
        }
    }

    if !pending.is_empty() {
        report_pull_progress_line(&pending, progress, &mut tracker);
    }

    Ok(())
}

fn report_pull_progress_line(
    line: &[u8],
    progress: &dyn SetupProgress,
    tracker: &mut PullProgressTracker,
) {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);

    if line.is_empty() {
        return;
    }

    let Ok(line) = std::str::from_utf8(line) else {
        return;
    };

    if let Ok(progress_line) = serde_json::from_str::<PullProgressLine>(line) {
        if let Some((completed, total)) = tracker.record(&progress_line) {
            progress.on_pull_progress(completed, total);
        } else if !progress_line.status.is_empty() {
            progress.on_status(&format!("  {}", progress_line.status));
        }
    }
}

/// Ensures a model is available locally, pulling it if necessary.
///
/// Returns `Ok(())` if the model is available (either already present
/// or successfully pulled). Uses default timeouts.
pub async fn ensure_model_available(
    base_url: &str,
    model_name: &str,
    progress: &dyn SetupProgress,
) -> Result<(), ParishError> {
    ensure_model_available_with_config(base_url, model_name, progress, &InferenceConfig::default())
        .await
}

/// Ensures a model is available locally, pulling it if necessary, with configurable timeouts.
///
/// Uses `config.reachability_timeout_secs` for checking availability and
/// `config.model_download_timeout_secs` for pulling.
pub async fn ensure_model_available_with_config(
    base_url: &str,
    model_name: &str,
    progress: &dyn SetupProgress,
    config: &InferenceConfig,
) -> Result<(), ParishError> {
    let available = is_model_available_with_config(base_url, model_name, config).await?;
    if force_model_redownload_enabled(config) {
        progress.on_status(&format!("Forcing a fresh download of '{}'.", model_name));
        if available {
            delete_model_with_config(base_url, model_name, progress, config).await?;
        } else {
            progress.on_status(&format!(
                "No local copy of '{}' was present. Fetching it now...",
                model_name
            ));
        }
        return pull_model_with_config(base_url, model_name, progress, config).await;
    }

    if available {
        progress.on_status(&format!(
            "The storyteller already has '{}' in hand.",
            model_name
        ));
        return Ok(());
    }

    pull_model_with_config(base_url, model_name, progress, config).await
}

fn force_model_redownload_enabled(config: &InferenceConfig) -> bool {
    config.force_model_redownload || env_flag_enabled("PARISH_OLLAMA_FORCE_REDOWNLOAD")
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| parse_env_flag(&value))
}

fn parse_env_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

// ── Ollama full setup sequence ────────────────────────────────────────────────

/// The result of the full Ollama setup process.
pub struct OllamaSetup {
    /// The managed Ollama server process (stops on drop if we started it).
    pub process: OllamaProcess,
    /// The configured OpenAI-compatible HTTP client.
    pub client: OpenAiClient,
    /// The selected model name.
    pub model_name: String,
    /// Detected GPU information.
    pub gpu_info: GpuInfo,
}

/// Runs the full Ollama setup sequence with default timeouts.
///
/// See [`setup_ollama_with_config`] for details.
pub async fn setup_ollama(
    base_url: &str,
    model_override: Option<&str>,
    progress: &dyn SetupProgress,
) -> Result<OllamaSetup, ParishError> {
    setup_ollama_with_config(
        base_url,
        model_override,
        progress,
        &InferenceConfig::default(),
    )
    .await
}

/// Runs the full Ollama setup sequence with configurable timeouts.
///
/// 1. Checks if Ollama is installed; installs if not
/// 2. Detects GPU vendor and VRAM — **fails if no discrete GPU found**
/// 3. Starts Ollama server with GPU env vars (e.g. `OLLAMA_VULKAN=1` for AMD on Windows)
/// 4. Selects the best model for available hardware
/// 5. Pulls the model if not already available
///
/// The `model_override` parameter allows skipping auto-selection
/// (e.g. from the `PARISH_MODEL` env var).
///
/// # Errors
///
/// Returns `ParishError::Setup` if no discrete GPU is detected,
/// installation fails, Ollama cannot start, or the selected model
/// cannot be pulled.
pub async fn setup_ollama_with_config(
    base_url: &str,
    model_override: Option<&str>,
    progress: &dyn SetupProgress,
    config: &InferenceConfig,
) -> Result<OllamaSetup, ParishError> {
    // Step 1: Check/install Ollama
    if !check_ollama_installed() {
        install_ollama(progress).await?;
        if !check_ollama_installed() {
            return Err(ParishError::Setup(
                "Ollama installation completed but binary not found on PATH. \
                 Try restarting your shell or adding it to PATH manually."
                    .to_string(),
            ));
        }
    } else {
        progress.on_status("The storyteller's tools are at hand.");
    }

    // Step 2: Detect GPU (before starting Ollama so we can pass GPU env vars)
    progress.on_status("Taking stock of what we have to work with...");
    let gpu_info = detect_gpu_info().await;
    progress.on_status(&format!("Hardware: {}", gpu_info));

    // Require GPU acceleration — refuse to run on CPU-only.
    // Apple Silicon counts: Metal acceleration is automatic via Ollama.
    if gpu_info.vendor == GpuVendor::CpuOnly {
        return Err(ParishError::Setup(
            "No GPU acceleration available. Parish requires a dedicated GPU (NVIDIA or AMD) \
             or Apple Silicon for local inference. Please ensure your GPU drivers are installed \
             and the GPU is recognized by your system."
                .to_string(),
        ));
    }

    // Step 3: Build GPU env vars and start Ollama
    let gpu_env = build_gpu_env(&gpu_info);
    if gpu_env.is_some() {
        progress.on_status("Stoking the Vulkan fires...");
    }

    progress.on_status("Lighting the fire in the storyteller's cottage...");
    let process: OllamaProcess =
        OllamaProcess::ensure_running(base_url, gpu_env.as_deref()).await?;
    if process.was_started_by_us() {
        progress.on_status("The hearth is lit. The storyteller is settling in.");
    } else {
        progress.on_status("The storyteller was already here. Grand so.");
    }

    // Step 4: Select model
    let model_config = match model_override {
        Some(name) => {
            progress.on_status(&format!("The storyteller will use '{}' tonight.", name));
            ModelConfig {
                model_name: name.to_string(),
                tier_label: "User override".to_string(),
                vram_required_mb: 0,
            }
        }
        None => {
            let selected = select_model(&gpu_info);
            progress.on_status(&format!("Chosen tale: {}", selected));
            selected
        }
    };

    // Step 5: Ensure model is available (uses Ollama native /api/tags + /api/pull)
    ensure_model_available_with_config(base_url, &model_config.model_name, progress, config)
        .await?;

    // Step 6: Warm up the model (uses Ollama native /api/generate)
    warmup_model_with_config(base_url, &model_config.model_name, progress, config).await?;

    // Create an OpenAI-compatible client pointing at Ollama's /v1/ endpoint.
    // Attach the configured base rate limiter so all calls that fall through
    // to the base provider (no per-category override) are throttled together.
    let base_limiter =
        parish_providers::rate_limit::InferenceRateLimiter::from_config(config.rate_limits.default);
    let client =
        OpenAiClient::new_with_config(base_url, None, config).maybe_with_rate_limit(base_limiter);

    Ok(OllamaSetup {
        process,
        client,
        model_name: model_config.model_name,
        gpu_info,
    })
}

/// Sends a trivial generate request to force Ollama to load the model into VRAM,
/// with configurable timeout.
///
/// Uses `config.model_loading_timeout_secs` for the HTTP request timeout.
async fn warmup_model_with_config(
    base_url: &str,
    model_name: &str,
    progress: &dyn SetupProgress,
    config: &InferenceConfig,
) -> Result<(), ParishError> {
    progress.on_status("The storyteller is gathering their thoughts...");

    // Build a client with a generous timeout for model loading
    let warmup_client = build_client_or_fallback(
        Duration::from_secs(config.model_loading_timeout_secs),
        "warmup",
    );

    let url = format!("{}/api/generate", base_url);
    let body = serde_json::json!({
        "model": model_name,
        "prompt": "Hi",
        "stream": false,
    });

    match warmup_client.post(&url).json(&body).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                progress.on_status("The storyteller is ready. The parish awaits.");
                Ok(())
            } else {
                let status = resp.status();
                let msg = format!(
                    "Warmup request returned HTTP {}: model '{}' may not be loaded",
                    status, model_name
                );
                progress.on_error(&msg);
                Err(ParishError::Setup(msg))
            }
        }
        Err(e) => {
            let msg = format!(
                "Failed to load model '{}' into GPU memory: {}",
                model_name, e
            );
            progress.on_error(&msg);
            Err(ParishError::Setup(msg))
        }
    }
}

// ── Unified provider client setup ────────────────────────────────────────────

/// Builds an inference client for the resolved [`ProviderConfig`], running
/// the full Ollama setup sequence (install, auto-start, GPU detection,
/// model pull, warmup) when the provider is `"ollama"`, or auto-spawning
/// vllm-mlx / vllm slots when the provider is `"vllmmlx"` / `"vllm"`.
///
/// This is the single entry point shared by all runtime modes (CLI, Tauri,
/// web server) so they stay in lock-step — CLAUDE.md rule #2 (mode parity).
/// Callers are responsible for keeping the returned [`RuntimeProcesses`]
/// alive for the lifetime of the app so spawned children are stopped on
/// exit.
///
/// `extra_vllm_mlx_slots` lists additional vllm-mlx slots beyond the base
/// provider — used for the two-slot Apple Silicon loadout. Pass an empty
/// slice when only the base slot is needed.
///
/// `extra_vllm_slots` lists additional vllm slots beyond the base provider
/// — used for the two-slot Linux/Windows loadout. Pass an empty slice when
/// only the base slot is needed.
///
/// # Errors
///
/// - `"ollama"`: bubbles up whatever `setup_ollama_with_config`
///   returns (no GPU, install failure, pull failure, …).
/// - `"vllmmlx"` / `"vllm"`: returns [`ParishError::Inference`] if any
///   slot fails to spawn or become reachable within 60s.
/// - Other providers: returns [`ParishError::Config`] if no model is set,
///   since non-Ollama / non-vllm backends have no auto-detect fallback.
pub async fn setup_provider_client(
    config: &ProviderConfig,
    extra_vllm_mlx_slots: &[VllmMlxSlot],
    extra_vllm_slots: &[VllmSlot],
    inference_config: &InferenceConfig,
    progress: &dyn SetupProgress,
) -> Result<(AnyClient, String, RuntimeProcesses), ParishError> {
    match config.provider.id() {
        "simulator" => Ok((
            AnyClient::simulator(),
            "simulator".to_string(),
            RuntimeProcesses::none(),
        )),
        "ollama" => {
            let setup = setup_ollama_with_config(
                &config.base_url,
                config.model.as_deref(),
                progress,
                inference_config,
            )
            .await?;
            let client = AnyClient::open_ai(setup.client);
            let vllm_mlx = VllmMlxProcess::ensure_slots(extra_vllm_mlx_slots).await?;
            let vllm = VllmProcess::ensure_slots(extra_vllm_slots).await?;
            Ok((
                client,
                setup.model_name,
                RuntimeProcesses {
                    ollama: setup.process,
                    vllm_mlx,
                    vllm,
                },
            ))
        }
        "vllmmlx" => {
            let model = config.model.clone().ok_or_else(|| {
                ParishError::Config(
                    "vllmmlx provider requires a model name. Set --model or PARISH_MODEL."
                        .to_string(),
                )
            })?;
            let mut all_slots: Vec<VllmMlxSlot> =
                Vec::with_capacity(1 + extra_vllm_mlx_slots.len());
            all_slots.push(VllmMlxSlot {
                base_url: config.base_url.clone(),
                model: model.clone(),
            });
            all_slots.extend(extra_vllm_mlx_slots.iter().cloned());
            let vllm_mlx = VllmMlxProcess::ensure_slots(&all_slots).await?;
            let vllm = VllmProcess::ensure_slots(extra_vllm_slots).await?;
            let client = parish_providers::build_client(
                &config.provider,
                &config.base_url,
                config.api_key.as_deref(),
                inference_config,
            );
            Ok((
                client,
                model,
                RuntimeProcesses {
                    ollama: OllamaProcess::none(),
                    vllm_mlx,
                    vllm,
                },
            ))
        }
        "vllm" => {
            let model = config.model.clone().ok_or_else(|| {
                ParishError::Config(
                    "vllm provider requires a model name. Set --model or PARISH_MODEL.".to_string(),
                )
            })?;
            let mut all_slots: Vec<VllmSlot> = Vec::with_capacity(1 + extra_vllm_slots.len());
            all_slots.push(VllmSlot {
                base_url: config.base_url.clone(),
                model: model.clone(),
            });
            all_slots.extend(extra_vllm_slots.iter().cloned());
            let vllm_mlx = VllmMlxProcess::ensure_slots(extra_vllm_mlx_slots).await?;
            let vllm = VllmProcess::ensure_slots(&all_slots).await?;
            let client = parish_providers::build_client(
                &config.provider,
                &config.base_url,
                config.api_key.as_deref(),
                inference_config,
            );
            Ok((
                client,
                model,
                RuntimeProcesses {
                    ollama: OllamaProcess::none(),
                    vllm_mlx,
                    vllm,
                },
            ))
        }
        _ => {
            let model = config.model.clone().ok_or_else(|| {
                ParishError::Config(format!(
                    "{} provider requires a model name. Set --model or PARISH_MODEL.",
                    config.provider.id()
                ))
            })?;
            let client = parish_providers::build_client(
                &config.provider,
                &config.base_url,
                config.api_key.as_deref(),
                inference_config,
            );
            let vllm_mlx = VllmMlxProcess::ensure_slots(extra_vllm_mlx_slots).await?;
            let vllm = VllmProcess::ensure_slots(extra_vllm_slots).await?;
            Ok((
                client,
                model,
                RuntimeProcesses {
                    ollama: OllamaProcess::none(),
                    vllm_mlx,
                    vllm,
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::progress::StdoutProgress;
    use super::*;

    #[test]
    fn test_port_from_base_url() {
        assert_eq!(port_from_base_url("http://localhost:8000/v1"), Some(8000));
        assert_eq!(port_from_base_url("http://localhost:11434"), Some(11434));
        assert_eq!(port_from_base_url("http://127.0.0.1:8001/v1"), Some(8001));
        assert_eq!(port_from_base_url("http://localhost/v1"), None);
        assert_eq!(port_from_base_url("not-a-url"), None);
    }

    #[test]
    fn test_parse_env_flag_truthy_values() {
        for value in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(parse_env_flag(value), "{value:?} should be truthy");
        }

        for value in ["", "0", "false", "no", "off", "maybe"] {
            assert!(!parse_env_flag(value), "{value:?} should be falsey");
        }
    }

    #[test]
    fn test_force_model_redownload_enabled_from_config() {
        let cfg = InferenceConfig {
            force_model_redownload: true,
            ..Default::default()
        };

        assert!(force_model_redownload_enabled(&cfg));
    }

    #[test]
    fn test_tags_response_deserialize() {
        let json = r#"{"models": [{"name": "qwen3:14b"}, {"name": "llama3:8b"}]}"#;
        let resp: TagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 2);
        assert_eq!(resp.models[0].name, "qwen3:14b");
    }

    #[test]
    fn test_tags_response_empty() {
        let json = r#"{"models": []}"#;
        let resp: TagsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.models.is_empty());
    }

    #[test]
    fn test_tags_response_missing_field() {
        let json = r#"{}"#;
        let resp: TagsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.models.is_empty());
    }

    #[test]
    fn test_pull_progress_line_deserialize() {
        let json = r#"{"status": "downloading", "digest": "sha256:abc", "total": 1000000, "completed": 500000}"#;
        let line: PullProgressLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.status, "downloading");
        assert_eq!(line.digest, "sha256:abc");
        assert_eq!(line.total, 1_000_000);
        assert_eq!(line.completed, 500_000);
    }

    #[test]
    fn test_pull_progress_line_status_only() {
        let json = r#"{"status": "verifying sha256 digest"}"#;
        let line: PullProgressLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.status, "verifying sha256 digest");
        assert_eq!(line.total, 0);
        assert_eq!(line.completed, 0);
    }

    #[test]
    fn test_pull_progress_aggregates_multiple_ollama_artifacts() {
        use super::super::progress::tests::TestProgress;
        let progress = TestProgress::new();
        let mut tracker = PullProgressTracker::default();

        report_pull_progress_line(br#"{"status":"pulling manifest"}"#, &progress, &mut tracker);
        report_pull_progress_line(
            br#"{"status":"pulling blob","digest":"sha256:large","total":1000,"completed":400}"#,
            &progress,
            &mut tracker,
        );
        report_pull_progress_line(
            br#"{"status":"pulling blob","digest":"sha256:large","total":1000,"completed":1000}"#,
            &progress,
            &mut tracker,
        );
        report_pull_progress_line(
            br#"{"status":"pulling blob","digest":"sha256:tiny","total":488,"completed":488}"#,
            &progress,
            &mut tracker,
        );

        let msgs = progress.messages();
        assert!(msgs.iter().any(|m| m == "  pulling manifest"));
        assert!(msgs.iter().any(|m| m == "progress: 400/1000"));
        assert!(msgs.iter().any(|m| m == "progress: 1000/1000"));
        assert!(msgs.iter().any(|m| m == "progress: 1488/1488"));
        assert!(!msgs.iter().any(|m| m == "progress: 488/488"));
    }

    #[tokio::test]
    async fn test_setup_provider_client_simulator_skips_runtime_spawn() {
        let cfg = ProviderConfig {
            provider: parish_config::Provider::simulator(),
            base_url: String::new(),
            api_key: None,
            model: None,
        };
        let inf = InferenceConfig::default();
        let progress = StdoutProgress;
        let result = setup_provider_client(&cfg, &[], &[], &inf, &progress).await;
        match result {
            Ok((_client, model, procs)) => {
                assert_eq!(model, "simulator");
                assert!(procs.vllm_mlx.is_empty());
                assert!(procs.vllm.is_empty());
            }
            Err(e) => panic!("simulator setup must succeed, got: {e}"),
        }
    }

    #[tokio::test]
    async fn test_setup_provider_client_vllm_requires_model() {
        let cfg = ProviderConfig {
            provider: parish_config::Provider::from_str_loose("vllm").unwrap(),
            base_url: "http://localhost:8000".to_string(),
            api_key: None,
            model: None,
        };
        let inf = InferenceConfig::default();
        let progress = StdoutProgress;
        match setup_provider_client(&cfg, &[], &[], &inf, &progress).await {
            Ok(_) => panic!("vllm provider without a model must error"),
            Err(e) => {
                let msg = format!("{e}");
                assert!(
                    msg.contains("vllm provider requires a model name"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_setup_provider_client_vllmmlx_requires_model() {
        let cfg = ProviderConfig {
            provider: parish_config::Provider::vllmmlx(),
            base_url: "http://localhost:8000".to_string(),
            api_key: None,
            model: None,
        };
        let inf = InferenceConfig::default();
        let progress = StdoutProgress;
        match setup_provider_client(&cfg, &[], &[], &inf, &progress).await {
            Ok(_) => panic!("vllmmlx provider without a model must error"),
            Err(e) => {
                let msg = format!("{e}");
                assert!(
                    msg.contains("vllmmlx provider requires a model name"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_setup_provider_client_cloud_fallthrough_arm() {
        // Cloud providers hit the `_ =>` arm: build_client (no network) +
        // ensure_slots on empty slot slices (also no network). Both new
        // `let vllm = VllmProcess::ensure_slots(...)` lines are exercised.
        let cfg = ProviderConfig {
            provider: parish_config::Provider::from_id("openrouter")
                .expect("openrouter provider mod must be loaded"),
            base_url: "http://localhost:9999".to_string(),
            api_key: Some("test-key".to_string()),
            model: Some("openrouter/auto".to_string()),
        };
        let inf = InferenceConfig::default();
        let progress = StdoutProgress;
        match setup_provider_client(&cfg, &[], &[], &inf, &progress).await {
            Ok((_client, model, procs)) => {
                assert_eq!(model, "openrouter/auto");
                assert!(procs.vllm_mlx.is_empty());
                assert!(procs.vllm.is_empty());
            }
            Err(e) => panic!("cloud fallthrough must succeed without network, got: {e}"),
        }
    }

    #[tokio::test]
    async fn test_setup_provider_client_cloud_requires_model() {
        let cfg = ProviderConfig {
            provider: parish_config::Provider::from_id("openrouter")
                .expect("openrouter provider mod must be loaded"),
            base_url: "http://localhost:9999".to_string(),
            api_key: Some("test-key".to_string()),
            model: None,
        };
        let inf = InferenceConfig::default();
        let progress = StdoutProgress;
        match setup_provider_client(&cfg, &[], &[], &inf, &progress).await {
            Ok(_) => panic!("openrouter without a model must error"),
            Err(e) => {
                let msg = format!("{e}");
                assert!(
                    msg.contains("openrouter provider requires a model name"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    #[test]
    fn test_build_gpu_env_cpu_only_returns_none() {
        let gpu = GpuInfo {
            vendor: GpuVendor::CpuOnly,
            vram_total_mb: 0,
            vram_free_mb: 0,
        };
        let env = build_gpu_env(&gpu);
        assert!(env.is_none());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_build_gpu_env_amd_linux_returns_none() {
        // On Linux, Ollama auto-detects ROCm — no extra env needed
        let gpu = GpuInfo {
            vendor: GpuVendor::Amd,
            vram_total_mb: 16384,
            vram_free_mb: 0,
        };
        let env = build_gpu_env(&gpu);
        assert!(env.is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_build_gpu_env_amd_windows_sets_vulkan() {
        let gpu = GpuInfo {
            vendor: GpuVendor::Amd,
            vram_total_mb: 16384,
            vram_free_mb: 0,
        };
        let env = build_gpu_env(&gpu);
        assert!(env.is_some());
        let vars = env.unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "OLLAMA_VULKAN");
        assert_eq!(vars[0].1, "1");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_build_gpu_env_nvidia_windows_returns_none() {
        let gpu = GpuInfo {
            vendor: GpuVendor::Nvidia,
            vram_total_mb: 8192,
            vram_free_mb: 7000,
        };
        let env = build_gpu_env(&gpu);
        assert!(env.is_none());
    }

    // ---- HTTP mock tests for is_model_available / pull_model ----

    #[tokio::test]
    async fn test_is_model_available_exact_match() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "models": [
                        {"name": "qwen3:14b"},
                        {"name": "llama3:8b"}
                    ]
                })),
            )
            .mount(&server)
            .await;

        let result = is_model_available(&server.uri(), "qwen3:14b")
            .await
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_is_model_available_latest_suffix_match() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "models": [ {"name": "qwen3:latest"} ]
                })),
            )
            .mount(&server)
            .await;

        // Query for "qwen3" should match "qwen3:latest"
        let result = is_model_available(&server.uri(), "qwen3").await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_is_model_available_query_with_latest_matches_bare() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "models": [ {"name": "qwen3"} ]
                })),
            )
            .mount(&server)
            .await;

        // Query for "qwen3:latest" should match bare "qwen3"
        let result = is_model_available(&server.uri(), "qwen3:latest")
            .await
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_is_model_available_missing_model() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "models": [ {"name": "llama3:8b"} ]
                })),
            )
            .mount(&server)
            .await;

        let result = is_model_available(&server.uri(), "qwen3:14b")
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_is_model_available_empty_model_list() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "models": [] })),
            )
            .mount(&server)
            .await;

        let result = is_model_available(&server.uri(), "qwen3:14b")
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_is_model_available_malformed_json_errors() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let result = is_model_available(&server.uri(), "qwen3:14b").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ParishError::Setup(msg) => assert!(msg.contains("parse model list")),
            other => panic!("expected Setup error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_pull_model_success_reports_progress() {
        use super::super::progress::tests::TestProgress;
        let server = wiremock::MockServer::start().await;
        // Ollama returns NDJSON progress lines
        let body = "\
{\"status\":\"pulling manifest\"}
{\"status\":\"downloading\",\"total\":1000000,\"completed\":250000}
{\"status\":\"downloading\",\"total\":1000000,\"completed\":1000000}
{\"status\":\"success\"}
";
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/pull"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "model": "qwen3:14b"
            })))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let progress = TestProgress::new();
        pull_model(&server.uri(), "qwen3:14b", &progress)
            .await
            .expect("pull should succeed");

        let msgs = progress.messages();
        // At least: pre-status, progress entries, and final status
        assert!(msgs.iter().any(|m| m.contains("Fetching")));
        assert!(msgs.iter().any(|m| m.contains("250000/1000000")));
        assert!(msgs.iter().any(|m| m.contains("1000000/1000000")));
        assert!(msgs.iter().any(|m| m.contains("hand")));
    }

    #[tokio::test]
    async fn test_pull_model_reports_streamed_progress_before_response_finishes() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        struct ChannelProgress {
            tx: tokio::sync::mpsc::UnboundedSender<String>,
        }

        impl SetupProgress for ChannelProgress {
            fn on_status(&self, msg: &str) {
                let _ = self.tx.send(format!("status: {}", msg));
            }

            fn on_pull_progress(&self, completed: u64, total: u64) {
                let _ = self.tx.send(format!("progress: {}/{}", completed, total));
            }

            fn on_error(&self, msg: &str) {
                let _ = self.tx.send(format!("error: {}", msg));
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let (continue_tx, continue_rx) = tokio::sync::oneshot::channel::<()>();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buf = [0_u8; 1024];
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    return;
                }
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }

            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            socket
                .write_all(br#"{"status":"downloading","total":1000000,"completed":250000}"#)
                .await
                .unwrap();
            socket.write_all(b"\n").await.unwrap();
            socket.flush().await.unwrap();

            let _ = continue_rx.await;

            socket
                .write_all(br#"{"status":"downloading","total":1000000,"completed":1000000}"#)
                .await
                .unwrap();
            socket.write_all(b"\n").await.unwrap();
            socket.flush().await.unwrap();
        });

        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = ChannelProgress { tx: event_tx };
        let pull = tokio::spawn(async move { pull_model(&base_url, "qwen3:14b", &progress).await });

        let first_progress = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let event = event_rx.recv().await.expect("progress channel closed");
                if event == "progress: 250000/1000000" {
                    break event;
                }
            }
        })
        .await
        .expect("first pull progress should arrive before response finishes");

        assert_eq!(first_progress, "progress: 250000/1000000");
        continue_tx.send(()).unwrap();
        pull.await.unwrap().expect("pull should finish");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_pull_model_maps_404_to_model_not_available() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/pull"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;

        use super::super::progress::tests::TestProgress;
        let progress = TestProgress::new();
        let result = pull_model(&server.uri(), "does-not-exist", &progress).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ParishError::ModelNotAvailable(msg) => {
                assert!(msg.contains("404"));
                assert!(msg.contains("does-not-exist"));
            }
            other => panic!("expected ModelNotAvailable, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_pull_model_status_only_lines_do_not_emit_progress() {
        use super::super::progress::tests::TestProgress;
        let server = wiremock::MockServer::start().await;
        // Only status lines, no total/completed
        let body = "\
{\"status\":\"pulling manifest\"}
{\"status\":\"verifying sha256 digest\"}
{\"status\":\"success\"}
";
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/pull"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "model": "qwen3:14b"
            })))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let progress = TestProgress::new();
        pull_model(&server.uri(), "qwen3:14b", &progress)
            .await
            .expect("pull should succeed");

        let msgs = progress.messages();
        // No "progress: N/M" entries expected since total == 0
        assert!(!msgs.iter().any(|m| m.starts_with("progress:")));
        // But status relays should be present
        assert!(msgs.iter().any(|m| m.contains("pulling manifest")));
        assert!(msgs.iter().any(|m| m.contains("verifying sha256 digest")));
    }

    #[tokio::test]
    async fn test_ensure_model_available_skips_pull_when_present() {
        use super::super::progress::tests::TestProgress;
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "models": [ {"name": "qwen3:14b"} ] })),
            )
            .mount(&server)
            .await;
        // NB: no mock for /api/pull — if ensure_model_available attempted to pull,
        // the request would 404 from wiremock and the test would fail.

        let progress = TestProgress::new();
        ensure_model_available(&server.uri(), "qwen3:14b", &progress)
            .await
            .expect("should short-circuit on present model");

        let msgs = progress.messages();
        assert!(msgs.iter().any(|m| m.contains("already has")));
    }

    #[tokio::test]
    async fn test_ensure_model_available_force_redownload_deletes_then_pulls_when_present() {
        use super::super::progress::tests::TestProgress;
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "models": [ {"name": "qwen3:14b"} ] })),
            )
            .expect(1)
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("DELETE"))
            .and(wiremock::matchers::path("/api/delete"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "model": "qwen3:14b"
            })))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/pull"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "model": "qwen3:14b"
            })))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                "{\"status\":\"downloading\",\"total\":1000000,\"completed\":500000}\n\
                 {\"status\":\"downloading\",\"total\":1000000,\"completed\":1000000}\n",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let config = InferenceConfig {
            force_model_redownload: true,
            ..Default::default()
        };
        let progress = TestProgress::new();
        ensure_model_available_with_config(&server.uri(), "qwen3:14b", &progress, &config)
            .await
            .expect("force redownload should delete then pull");

        let msgs = progress.messages();
        assert!(msgs.iter().any(|m| m.contains("Forcing a fresh download")));
        assert!(msgs.iter().any(|m| m.contains("Clearing the local copy")));
        assert!(msgs.iter().any(|m| m.contains("Local copy")));
        assert!(msgs.iter().any(|m| m.contains("500000/1000000")));
        assert!(msgs.iter().any(|m| m.contains("1000000/1000000")));
    }

    #[tokio::test]
    async fn test_ensure_model_available_force_redownload_tolerates_missing_delete() {
        use super::super::progress::tests::TestProgress;
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/tags"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "models": [ {"name": "qwen3:14b"} ] })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("DELETE"))
            .and(wiremock::matchers::path("/api/delete"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "model": "qwen3:14b"
            })))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/pull"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                "{\"status\":\"downloading\",\"total\":1000000,\"completed\":1000000}\n",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let config = InferenceConfig {
            force_model_redownload: true,
            ..Default::default()
        };
        let progress = TestProgress::new();
        ensure_model_available_with_config(&server.uri(), "qwen3:14b", &progress, &config)
            .await
            .expect("force redownload should tolerate a missing local model");

        let msgs = progress.messages();
        assert!(msgs.iter().any(|m| m.contains("No local copy")));
        assert!(msgs.iter().any(|m| m.contains("1000000/1000000")));
    }
}
