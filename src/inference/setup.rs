//! Ollama bootstrap, GPU detection, and model management.
//!
//! Handles the full Ollama lifecycle: installation detection,
//! auto-install, GPU/VRAM detection, model selection based on
//! available hardware, and automatic model pulling.

use crate::error::ParishError;
use crate::inference::client::{OllamaClient, OllamaProcess};
use serde::Deserialize;
use std::process::Command;
use std::time::Duration;

/// GPU vendor detected on the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    /// NVIDIA GPU (CUDA).
    Nvidia,
    /// AMD GPU (ROCm).
    Amd,
    /// No discrete GPU detected; CPU-only inference.
    CpuOnly,
}

impl std::fmt::Display for GpuVendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuVendor::Nvidia => write!(f, "NVIDIA (CUDA)"),
            GpuVendor::Amd => write!(f, "AMD (ROCm)"),
            GpuVendor::CpuOnly => write!(f, "CPU-only"),
        }
    }
}

/// Information about the detected GPU hardware.
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// The GPU vendor/type.
    pub vendor: GpuVendor,
    /// Total VRAM in megabytes (0 for CPU-only).
    pub vram_total_mb: u64,
    /// Free VRAM in megabytes (0 for CPU-only or unknown).
    pub vram_free_mb: u64,
}

impl std::fmt::Display for GpuInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.vendor {
            GpuVendor::CpuOnly => write!(f, "CPU-only (no discrete GPU detected)"),
            _ => write!(
                f,
                "{} — {}MB VRAM total, ~{}MB free",
                self.vendor, self.vram_total_mb, self.vram_free_mb
            ),
        }
    }
}

/// Configuration for a selected model based on available hardware.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// The Ollama model tag (e.g. "qwen3:14b").
    pub model_name: String,
    /// Human-readable tier label (e.g. "Tier 1 — Full quality").
    pub tier_label: String,
    /// Approximate VRAM required in MB when loaded.
    pub vram_required_mb: u64,
}

impl std::fmt::Display for ModelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, ~{}MB VRAM)",
            self.model_name, self.tier_label, self.vram_required_mb
        )
    }
}

/// The result of the full Ollama setup process.
pub struct OllamaSetup {
    /// The managed Ollama server process (stops on drop if we started it).
    pub process: OllamaProcess,
    /// The configured HTTP client.
    pub client: OllamaClient,
    /// The selected model name.
    pub model_name: String,
    /// Detected GPU information.
    pub gpu_info: GpuInfo,
}

/// Trait for reporting setup progress to the UI layer.
///
/// Implemented differently by TUI and headless modes to show
/// installation, detection, and download progress appropriately.
pub trait SetupProgress {
    /// Reports a status message during setup.
    fn on_status(&self, msg: &str);
    /// Reports model pull progress (bytes downloaded vs total).
    fn on_pull_progress(&self, completed: u64, total: u64);
    /// Reports an error during setup.
    fn on_error(&self, msg: &str);
}

/// A simple progress reporter that prints to stdout.
pub struct StdoutProgress;

impl SetupProgress for StdoutProgress {
    fn on_status(&self, msg: &str) {
        println!("[Parish] {}", msg);
    }

    fn on_pull_progress(&self, completed: u64, total: u64) {
        if total > 0 {
            let pct = (completed as f64 / total as f64) * 100.0;
            print!("\r[Parish] Downloading model: {:.1}%", pct);
            if completed >= total {
                println!();
            }
        }
    }

    fn on_error(&self, msg: &str) {
        eprintln!("[Parish] ERROR: {}", msg);
    }
}

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
    progress.on_status("Ollama not found. Installing via official script...");
    progress.on_status("This may take a few minutes and may request sudo access.");

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

    progress.on_status("Ollama installed successfully.");
    Ok(())
}

/// Detects the GPU vendor and VRAM on the system.
///
/// Tries NVIDIA first (via `nvidia-smi`), then AMD (via `rocm-smi`),
/// and falls back to CPU-only if neither is detected.
pub async fn detect_gpu_info() -> GpuInfo {
    // Try NVIDIA first
    if let Some(info) = detect_nvidia().await {
        return info;
    }

    // Try AMD/ROCm
    if let Some(info) = detect_amd().await {
        return info;
    }

    // Fallback: CPU-only
    GpuInfo {
        vendor: GpuVendor::CpuOnly,
        vram_total_mb: 0,
        vram_free_mb: 0,
    }
}

/// Detects NVIDIA GPU VRAM via `nvidia-smi`.
async fn detect_nvidia() -> Option<GpuInfo> {
    let output = tokio::task::spawn_blocking(|| {
        Command::new("nvidia-smi")
            .args([
                "--query-gpu=memory.total,memory.free",
                "--format=csv,noheader,nounits",
            ])
            .output()
    })
    .await
    .ok()?
    .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 {
        return None;
    }

    let total: u64 = parts[0].parse().ok()?;
    let free: u64 = parts[1].parse().ok()?;

    Some(GpuInfo {
        vendor: GpuVendor::Nvidia,
        vram_total_mb: total,
        vram_free_mb: free,
    })
}

/// Detects AMD GPU VRAM via `rocm-smi`.
async fn detect_amd() -> Option<GpuInfo> {
    let output = tokio::task::spawn_blocking(|| {
        Command::new("rocm-smi")
            .args(["--showmeminfo", "vram"])
            .output()
    })
    .await
    .ok()?
    .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let (mut total_mb, mut used_mb) = (0u64, 0u64);

    for line in stdout.lines() {
        let lower = line.to_lowercase();
        if lower.contains("total")
            && let Some(bytes) = extract_bytes_from_line(line)
        {
            total_mb = bytes / (1024 * 1024);
        } else if lower.contains("used")
            && let Some(bytes) = extract_bytes_from_line(line)
        {
            used_mb = bytes / (1024 * 1024);
        }
    }

    if total_mb == 0 {
        // rocm-smi exists but we couldn't parse VRAM — still AMD
        // Try a simpler fallback: just detect that ROCm is present
        if std::path::Path::new("/opt/rocm").exists() {
            return Some(GpuInfo {
                vendor: GpuVendor::Amd,
                vram_total_mb: 0,
                vram_free_mb: 0,
            });
        }
        return None;
    }

    let free_mb = total_mb.saturating_sub(used_mb);
    Some(GpuInfo {
        vendor: GpuVendor::Amd,
        vram_total_mb: total_mb,
        vram_free_mb: free_mb,
    })
}

/// Extracts a byte count from a rocm-smi output line.
///
/// Looks for a large numeric value on the line (the byte count).
fn extract_bytes_from_line(line: &str) -> Option<u64> {
    line.split_whitespace()
        .filter_map(|token| token.parse::<u64>().ok())
        .find(|&n| n > 1_000_000) // VRAM values are in bytes, so > 1MB
}

/// Selects the best model for the available VRAM.
///
/// Uses conservative thresholds to leave headroom for the OS and
/// other GPU workloads:
/// - 12GB+ VRAM → qwen3:14b (Tier 1, best quality)
/// - 6GB+ VRAM → qwen3:8b (Tier 2, good quality)
/// - 3GB+ VRAM → qwen3:3b (reduced quality)
/// - <3GB or CPU → qwen3:1.5b (minimal, CPU-viable)
///
/// If VRAM is 0 (unknown but GPU detected), assumes 8GB as a
/// conservative default for modern discrete GPUs.
pub fn select_model(gpu_info: &GpuInfo) -> ModelConfig {
    let effective_vram = match gpu_info.vendor {
        GpuVendor::CpuOnly => 0,
        _ => {
            if gpu_info.vram_free_mb > 0 {
                gpu_info.vram_free_mb
            } else if gpu_info.vram_total_mb > 0 {
                // Use 80% of total as estimate of available
                gpu_info.vram_total_mb * 80 / 100
            } else {
                // GPU detected but VRAM unknown — assume 8GB
                8192
            }
        }
    };

    select_model_for_vram(effective_vram)
}

/// Selects a model given a specific VRAM budget in MB.
fn select_model_for_vram(vram_mb: u64) -> ModelConfig {
    if vram_mb >= 12_000 {
        ModelConfig {
            model_name: "qwen3:14b".to_string(),
            tier_label: "Tier 1 — Full quality".to_string(),
            vram_required_mb: 10_000,
        }
    } else if vram_mb >= 6_000 {
        ModelConfig {
            model_name: "qwen3:8b".to_string(),
            tier_label: "Tier 2 — Good quality".to_string(),
            vram_required_mb: 5_500,
        }
    } else if vram_mb >= 3_000 {
        ModelConfig {
            model_name: "qwen3:3b".to_string(),
            tier_label: "Tier 3 — Reduced quality".to_string(),
            vram_required_mb: 2_500,
        }
    } else {
        ModelConfig {
            model_name: "qwen3:1.5b".to_string(),
            tier_label: "Tier 4 — Minimal (CPU-viable)".to_string(),
            vram_required_mb: 1_200,
        }
    }
}

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
    total: u64,
    #[serde(default)]
    completed: u64,
}

/// Checks whether a model is available locally in Ollama.
///
/// Queries the `/api/tags` endpoint and checks if the model name
/// appears in the list of locally available models.
pub async fn is_model_available(
    client: &OllamaClient,
    model_name: &str,
) -> Result<bool, ParishError> {
    let url = format!("{}/api/tags", client.base_url());
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
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
///
/// # Errors
///
/// Returns `ParishError::ModelNotAvailable` if the pull fails.
pub async fn pull_model(
    client: &OllamaClient,
    model_name: &str,
    progress: &dyn SetupProgress,
) -> Result<(), ParishError> {
    progress.on_status(&format!("Pulling model '{}'...", model_name));

    let url = format!("{}/api/pull", client.base_url());
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600)) // Model downloads can take a while
        .build()
        .map_err(|e| ParishError::Setup(format!("failed to build HTTP client: {}", e)))?;

    let resp = http
        .post(&url)
        .json(&serde_json::json!({ "name": model_name }))
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

    // Stream the response line by line (NDJSON)
    let body = resp
        .text()
        .await
        .map_err(|e| ParishError::ModelNotAvailable(format!("pull stream error: {}", e)))?;

    for line in body.lines() {
        if let Ok(progress_line) = serde_json::from_str::<PullProgressLine>(line) {
            if progress_line.total > 0 {
                progress.on_pull_progress(progress_line.completed, progress_line.total);
            } else if !progress_line.status.is_empty() {
                progress.on_status(&format!("  {}", progress_line.status));
            }
        }
    }

    progress.on_status(&format!("Model '{}' ready.", model_name));
    Ok(())
}

/// Ensures a model is available locally, pulling it if necessary.
///
/// Returns `Ok(())` if the model is available (either already present
/// or successfully pulled).
pub async fn ensure_model_available(
    client: &OllamaClient,
    model_name: &str,
    progress: &dyn SetupProgress,
) -> Result<(), ParishError> {
    if is_model_available(client, model_name).await? {
        progress.on_status(&format!("Model '{}' is available locally.", model_name));
        return Ok(());
    }

    pull_model(client, model_name, progress).await
}

/// Runs the full Ollama setup sequence.
///
/// 1. Checks if Ollama is installed; installs if not
/// 2. Starts Ollama server (or connects to existing)
/// 3. Detects GPU vendor and VRAM
/// 4. Selects the best model for available hardware
/// 5. Pulls the model if not already available
///
/// The `model_override` parameter allows skipping auto-selection
/// (e.g. from the `PARISH_MODEL` env var).
///
/// # Errors
///
/// Returns errors if installation fails, Ollama cannot start,
/// or the selected model cannot be pulled.
pub async fn setup_ollama(
    base_url: &str,
    model_override: Option<&str>,
    progress: &dyn SetupProgress,
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
        progress.on_status("Ollama is installed.");
    }

    // Step 2: Start Ollama
    progress.on_status("Starting Ollama server...");
    let process = OllamaProcess::ensure_running(base_url).await?;
    if process.was_started_by_us() {
        progress.on_status("Ollama server started by Parish.");
    } else {
        progress.on_status("Connected to existing Ollama server.");
    }

    // Step 3: Detect GPU
    progress.on_status("Detecting GPU hardware...");
    let gpu_info = detect_gpu_info().await;
    progress.on_status(&format!("GPU: {}", gpu_info));

    // Step 4: Select model
    let model_config = match model_override {
        Some(name) => {
            progress.on_status(&format!("Using model override: {}", name));
            ModelConfig {
                model_name: name.to_string(),
                tier_label: "User override".to_string(),
                vram_required_mb: 0,
            }
        }
        None => {
            let config = select_model(&gpu_info);
            progress.on_status(&format!("Selected model: {}", config));
            config
        }
    };

    // Step 5: Ensure model is available
    let client = OllamaClient::new(base_url);
    ensure_model_available(&client, &model_config.model_name, progress).await?;

    Ok(OllamaSetup {
        process,
        client,
        model_name: model_config.model_name,
        gpu_info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_vendor_display() {
        assert_eq!(GpuVendor::Nvidia.to_string(), "NVIDIA (CUDA)");
        assert_eq!(GpuVendor::Amd.to_string(), "AMD (ROCm)");
        assert_eq!(GpuVendor::CpuOnly.to_string(), "CPU-only");
    }

    #[test]
    fn test_gpu_info_display_cpu_only() {
        let info = GpuInfo {
            vendor: GpuVendor::CpuOnly,
            vram_total_mb: 0,
            vram_free_mb: 0,
        };
        assert!(info.to_string().contains("CPU-only"));
    }

    #[test]
    fn test_gpu_info_display_with_vram() {
        let info = GpuInfo {
            vendor: GpuVendor::Amd,
            vram_total_mb: 16384,
            vram_free_mb: 14000,
        };
        let display = info.to_string();
        assert!(display.contains("AMD"));
        assert!(display.contains("16384"));
        assert!(display.contains("14000"));
    }

    #[test]
    fn test_model_config_display() {
        let config = ModelConfig {
            model_name: "qwen3:14b".to_string(),
            tier_label: "Tier 1 — Full quality".to_string(),
            vram_required_mb: 10_000,
        };
        let display = config.to_string();
        assert!(display.contains("qwen3:14b"));
        assert!(display.contains("Tier 1"));
        assert!(display.contains("10000"));
    }

    #[test]
    fn test_select_model_large_vram() {
        let config = select_model_for_vram(16_000);
        assert_eq!(config.model_name, "qwen3:14b");
        assert!(config.tier_label.contains("Tier 1"));
    }

    #[test]
    fn test_select_model_12gb() {
        let config = select_model_for_vram(12_000);
        assert_eq!(config.model_name, "qwen3:14b");
    }

    #[test]
    fn test_select_model_8gb() {
        let config = select_model_for_vram(8_000);
        assert_eq!(config.model_name, "qwen3:8b");
        assert!(config.tier_label.contains("Tier 2"));
    }

    #[test]
    fn test_select_model_6gb() {
        let config = select_model_for_vram(6_000);
        assert_eq!(config.model_name, "qwen3:8b");
    }

    #[test]
    fn test_select_model_4gb() {
        let config = select_model_for_vram(4_000);
        assert_eq!(config.model_name, "qwen3:3b");
        assert!(config.tier_label.contains("Tier 3"));
    }

    #[test]
    fn test_select_model_3gb() {
        let config = select_model_for_vram(3_000);
        assert_eq!(config.model_name, "qwen3:3b");
    }

    #[test]
    fn test_select_model_2gb() {
        let config = select_model_for_vram(2_000);
        assert_eq!(config.model_name, "qwen3:1.5b");
        assert!(config.tier_label.contains("Tier 4"));
    }

    #[test]
    fn test_select_model_zero_vram() {
        let config = select_model_for_vram(0);
        assert_eq!(config.model_name, "qwen3:1.5b");
    }

    #[test]
    fn test_select_model_cpu_only_gpu_info() {
        let gpu = GpuInfo {
            vendor: GpuVendor::CpuOnly,
            vram_total_mb: 0,
            vram_free_mb: 0,
        };
        let config = select_model(&gpu);
        assert_eq!(config.model_name, "qwen3:1.5b");
    }

    #[test]
    fn test_select_model_amd_16gb() {
        let gpu = GpuInfo {
            vendor: GpuVendor::Amd,
            vram_total_mb: 16384,
            vram_free_mb: 14000,
        };
        let config = select_model(&gpu);
        assert_eq!(config.model_name, "qwen3:14b");
    }

    #[test]
    fn test_select_model_unknown_vram_defaults() {
        // GPU detected but VRAM unknown (e.g. rocm-smi parse failure)
        let gpu = GpuInfo {
            vendor: GpuVendor::Amd,
            vram_total_mb: 0,
            vram_free_mb: 0,
        };
        let config = select_model(&gpu);
        // Should assume 8GB → select 8b model
        assert_eq!(config.model_name, "qwen3:8b");
    }

    #[test]
    fn test_select_model_uses_free_vram_when_available() {
        let gpu = GpuInfo {
            vendor: GpuVendor::Nvidia,
            vram_total_mb: 16384,
            vram_free_mb: 5000, // Only 5GB free
        };
        let config = select_model(&gpu);
        // 5000 < 6000, should pick 3b
        assert_eq!(config.model_name, "qwen3:3b");
    }

    #[test]
    fn test_select_model_uses_total_when_free_unknown() {
        let gpu = GpuInfo {
            vendor: GpuVendor::Nvidia,
            vram_total_mb: 8192,
            vram_free_mb: 0, // Free unknown
        };
        let config = select_model(&gpu);
        // 80% of 8192 = 6553 → should select 8b
        assert_eq!(config.model_name, "qwen3:8b");
    }

    #[test]
    fn test_extract_bytes_from_line() {
        assert_eq!(
            extract_bytes_from_line("VRAM Total Memory (B): 17163091968"),
            Some(17163091968)
        );
        assert_eq!(extract_bytes_from_line("no numbers here"), None);
        assert_eq!(extract_bytes_from_line("small: 42"), None); // < 1MB threshold
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
        let json = r#"{"status": "downloading", "total": 1000000, "completed": 500000}"#;
        let line: PullProgressLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.status, "downloading");
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

    /// Tracks status messages for testing.
    struct TestProgress {
        messages: std::cell::RefCell<Vec<String>>,
    }

    impl TestProgress {
        fn new() -> Self {
            Self {
                messages: std::cell::RefCell::new(Vec::new()),
            }
        }

        fn messages(&self) -> Vec<String> {
            self.messages.borrow().clone()
        }
    }

    impl SetupProgress for TestProgress {
        fn on_status(&self, msg: &str) {
            self.messages.borrow_mut().push(msg.to_string());
        }

        fn on_pull_progress(&self, completed: u64, total: u64) {
            self.messages
                .borrow_mut()
                .push(format!("progress: {}/{}", completed, total));
        }

        fn on_error(&self, msg: &str) {
            self.messages.borrow_mut().push(format!("ERROR: {}", msg));
        }
    }

    #[test]
    fn test_stdout_progress_on_status() {
        // Just verify it doesn't panic
        let progress = StdoutProgress;
        progress.on_status("test message");
    }

    #[test]
    fn test_stdout_progress_on_error() {
        let progress = StdoutProgress;
        progress.on_error("test error");
    }

    #[test]
    fn test_test_progress_tracks_messages() {
        let progress = TestProgress::new();
        progress.on_status("hello");
        progress.on_status("world");
        progress.on_pull_progress(50, 100);
        progress.on_error("oops");

        let msgs = progress.messages();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0], "hello");
        assert_eq!(msgs[1], "world");
        assert_eq!(msgs[2], "progress: 50/100");
        assert_eq!(msgs[3], "ERROR: oops");
    }

    #[test]
    fn test_gpu_vendor_equality() {
        assert_eq!(GpuVendor::Nvidia, GpuVendor::Nvidia);
        assert_ne!(GpuVendor::Nvidia, GpuVendor::Amd);
        assert_ne!(GpuVendor::Amd, GpuVendor::CpuOnly);
    }

    #[test]
    fn test_select_model_boundary_values() {
        // Exactly at boundaries
        let at_12000 = select_model_for_vram(12_000);
        assert_eq!(at_12000.model_name, "qwen3:14b");

        let at_11999 = select_model_for_vram(11_999);
        assert_eq!(at_11999.model_name, "qwen3:8b");

        let at_6000 = select_model_for_vram(6_000);
        assert_eq!(at_6000.model_name, "qwen3:8b");

        let at_5999 = select_model_for_vram(5_999);
        assert_eq!(at_5999.model_name, "qwen3:3b");

        let at_3000 = select_model_for_vram(3_000);
        assert_eq!(at_3000.model_name, "qwen3:3b");

        let at_2999 = select_model_for_vram(2_999);
        assert_eq!(at_2999.model_name, "qwen3:1.5b");
    }
}
