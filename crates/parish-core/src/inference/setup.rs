//! Ollama bootstrap, GPU detection, and model management.
//!
//! Handles the full Ollama lifecycle: installation detection,
//! auto-install, GPU/VRAM detection, model selection based on
//! available hardware, and automatic model pulling.

use crate::config::InferenceConfig;
use crate::error::ParishError;
use crate::inference::client::OllamaProcess;
use crate::inference::openai_client::OpenAiClient;
use serde::Deserialize;
use std::process::Command;
use std::time::Duration;

/// GPU vendor detected on the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    /// NVIDIA GPU (CUDA).
    Nvidia,
    /// AMD GPU (ROCm on Linux, DirectX/Vulkan on Windows).
    Amd,
    /// No discrete GPU detected; CPU-only inference.
    CpuOnly,
}

impl std::fmt::Display for GpuVendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuVendor::Nvidia => write!(f, "NVIDIA (CUDA)"),
            GpuVendor::Amd => write!(f, "AMD"),
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
    /// The configured OpenAI-compatible HTTP client.
    pub client: OpenAiClient,
    /// The selected model name.
    pub model_name: String,
    /// Detected GPU information.
    pub gpu_info: GpuInfo,
}

/// Trait for reporting setup progress to the UI layer.
///
/// Implemented differently by headless and other modes to show
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
            print!("\r[Parish] The tale is {:.1}% arrived...", pct);
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

/// Detects the GPU vendor and VRAM on the system.
///
/// Tries platform-specific detection first (Windows via PowerShell,
/// Linux via `nvidia-smi` / `rocm-smi`), then falls back to CPU-only.
pub async fn detect_gpu_info() -> GpuInfo {
    // On Windows, use PowerShell/WMI for GPU detection
    #[cfg(target_os = "windows")]
    {
        if let Some(info) = detect_windows_gpu().await {
            return info;
        }
    }

    // Try NVIDIA (works on both Linux and Windows with CUDA drivers)
    if let Some(info) = detect_nvidia().await {
        return info;
    }

    // Try AMD/ROCm (Linux)
    #[cfg(not(target_os = "windows"))]
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

/// Detects GPU on Windows via PowerShell WMI queries.
///
/// Uses `Get-CimInstance Win32_VideoController` for the GPU name, and
/// falls back to the registry `HardwareInformation.qwMemorySize` for
/// accurate VRAM on cards with >4GB (the WMI `AdapterRAM` field is
/// a 32-bit integer that overflows for modern GPUs).
#[cfg(target_os = "windows")]
async fn detect_windows_gpu() -> Option<GpuInfo> {
    // Query GPU name and AdapterRAM via PowerShell
    let output = tokio::task::spawn_blocking(|| {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance -ClassName Win32_VideoController | Select-Object Name, AdapterRAM | ConvertTo-Json -Compress",
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
    let gpu_entries = parse_windows_gpu_json(&stdout)?;

    // Find the first discrete GPU (skip Microsoft Basic Display Adapter, etc.)
    let (name, adapter_ram_bytes) = gpu_entries
        .iter()
        .find(|(name, _)| is_discrete_gpu(name))?
        .clone();

    let vendor = if name.to_lowercase().contains("nvidia") {
        GpuVendor::Nvidia
    } else if name.to_lowercase().contains("amd") || name.to_lowercase().contains("radeon") {
        GpuVendor::Amd
    } else {
        // Unknown discrete GPU — still better than CPU-only
        GpuVendor::Amd
    };

    // WMI AdapterRAM is uint32, overflows at 4GB. Try registry for real VRAM.
    let vram_mb = if adapter_ram_bytes >= 4_000_000_000 || adapter_ram_bytes == 0 {
        // AdapterRAM overflowed or missing — query registry
        detect_windows_vram_from_registry().await.unwrap_or(0)
    } else {
        adapter_ram_bytes / (1024 * 1024)
    };

    Some(GpuInfo {
        vendor,
        vram_total_mb: vram_mb,
        vram_free_mb: 0, // Windows WMI doesn't report free VRAM
    })
}

/// Parses the JSON output from `Get-CimInstance Win32_VideoController`.
///
/// Returns a list of (Name, AdapterRAM) tuples. Handles both single-object
/// JSON (one GPU) and array JSON (multiple GPUs).
#[cfg(target_os = "windows")]
fn parse_windows_gpu_json(json_str: &str) -> Option<Vec<(String, u64)>> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Try as array first, then single object
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        let entries: Vec<(String, u64)> = arr
            .iter()
            .filter_map(|v| {
                let name = v.get("Name")?.as_str()?.to_string();
                let ram = v.get("AdapterRAM").and_then(|r| r.as_u64()).unwrap_or(0);
                Some((name, ram))
            })
            .collect();
        if entries.is_empty() {
            return None;
        }
        return Some(entries);
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let name = v.get("Name")?.as_str()?.to_string();
        let ram = v.get("AdapterRAM").and_then(|r| r.as_u64()).unwrap_or(0);
        return Some(vec![(name, ram)]);
    }

    None
}

/// Returns true if the GPU name looks like a discrete GPU (not an
/// integrated or virtual adapter).
#[cfg(target_os = "windows")]
fn is_discrete_gpu(name: &str) -> bool {
    let lower = name.to_lowercase();
    // Skip known virtual/integrated adapters
    if lower.contains("microsoft basic")
        || lower.contains("remote desktop")
        || lower.contains("virtual")
    {
        return false;
    }
    // Positive match for known discrete GPU vendors
    lower.contains("nvidia")
        || lower.contains("radeon")
        || lower.contains("amd")
        || lower.contains("geforce")
        || lower.contains("quadro")
        || lower.contains("arc") // Intel Arc
}

/// Queries the Windows registry for accurate VRAM (64-bit value).
///
/// Reads `HardwareInformation.qwMemorySize` from the display adapter
/// registry keys, which correctly reports VRAM for cards >4GB.
/// Uses property access instead of `ForEach-Object`/`$_` to avoid
/// escaping issues when invoked via `std::process::Command`.
#[cfg(target_os = "windows")]
async fn detect_windows_vram_from_registry() -> Option<u64> {
    let output = tokio::task::spawn_blocking(|| {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                r#"(Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' -Name 'HardwareInformation.qwMemorySize' -ErrorAction SilentlyContinue).'HardwareInformation.qwMemorySize'"#,
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
    // Take the largest value (in case of multiple GPUs, pick the biggest)
    let max_bytes: u64 = stdout
        .lines()
        .filter_map(|line| line.trim().parse::<u64>().ok())
        .max()?;

    if max_bytes == 0 {
        return None;
    }

    Some(max_bytes / (1024 * 1024))
}

/// Detects AMD GPU VRAM via `rocm-smi` (Linux only).
#[cfg(not(target_os = "windows"))]
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
#[cfg(not(target_os = "windows"))]
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
/// - 3GB+ VRAM → qwen3:4b (reduced quality)
/// - <3GB or CPU → qwen3:1.7b (minimal, CPU-viable)
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
            model_name: "qwen3:4b".to_string(),
            tier_label: "Tier 3 — Reduced quality".to_string(),
            vram_required_mb: 2_800,
        }
    } else {
        ModelConfig {
            model_name: "qwen3:1.7b".to_string(),
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

    progress.on_status(&format!(
        "The storyteller has '{}' in hand. Grand so.",
        model_name
    ));
    Ok(())
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
    if is_model_available_with_config(base_url, model_name, config).await? {
        progress.on_status(&format!(
            "The storyteller already has '{}' in hand.",
            model_name
        ));
        return Ok(());
    }

    pull_model_with_config(base_url, model_name, progress, config).await
}

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

    // Require a discrete GPU — refuse to run on CPU-only
    if gpu_info.vendor == GpuVendor::CpuOnly {
        return Err(ParishError::Setup(
            "No discrete GPU detected. Parish requires a dedicated GPU (NVIDIA or AMD) \
             for local inference. Please ensure your GPU drivers are installed and \
             the GPU is recognized by your system."
                .to_string(),
        ));
    }

    // Step 3: Build GPU env vars and start Ollama
    let gpu_env = build_gpu_env(&gpu_info);
    if gpu_env.is_some() {
        progress.on_status("Stoking the Vulkan fires...");
    }

    progress.on_status("Lighting the fire in the storyteller's cottage...");
    let process = OllamaProcess::ensure_running(base_url, gpu_env.as_deref()).await?;
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

    // Create an OpenAI-compatible client pointing at Ollama's /v1/ endpoint
    let client = OpenAiClient::new_with_config(base_url, None, config);

    Ok(OllamaSetup {
        process,
        client,
        model_name: model_config.model_name,
        gpu_info,
    })
}

/// Sends a trivial generate request to force Ollama to load the model into VRAM.
///
/// Without this, Ollama defers model loading until the first real request,
/// causing a long delay on the player's first interaction. The warmup prompt
/// is minimal so the response completes quickly once the model is loaded.
///
/// Uses a dedicated HTTP client with a 5-minute timeout since the first
/// model load (moving weights from disk to VRAM) can be slow.
#[allow(dead_code)] // Kept as default-timeout wrapper for external callers
async fn warmup_model(
    base_url: &str,
    model_name: &str,
    progress: &dyn SetupProgress,
) -> Result<(), ParishError> {
    warmup_model_with_config(base_url, model_name, progress, &InferenceConfig::default()).await
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

    // Build a one-off client with a generous timeout for model loading
    let warmup_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.model_loading_timeout_secs))
        .build()
        .map_err(|e| ParishError::Setup(format!("failed to build warmup client: {}", e)))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_vendor_display() {
        assert_eq!(GpuVendor::Nvidia.to_string(), "NVIDIA (CUDA)");
        assert_eq!(GpuVendor::Amd.to_string(), "AMD");
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
        assert_eq!(config.model_name, "qwen3:4b");
        assert!(config.tier_label.contains("Tier 3"));
    }

    #[test]
    fn test_select_model_3gb() {
        let config = select_model_for_vram(3_000);
        assert_eq!(config.model_name, "qwen3:4b");
    }

    #[test]
    fn test_select_model_2gb() {
        let config = select_model_for_vram(2_000);
        assert_eq!(config.model_name, "qwen3:1.7b");
        assert!(config.tier_label.contains("Tier 4"));
    }

    #[test]
    fn test_select_model_zero_vram() {
        let config = select_model_for_vram(0);
        assert_eq!(config.model_name, "qwen3:1.7b");
    }

    #[test]
    fn test_select_model_cpu_only_gpu_info() {
        let gpu = GpuInfo {
            vendor: GpuVendor::CpuOnly,
            vram_total_mb: 0,
            vram_free_mb: 0,
        };
        let config = select_model(&gpu);
        assert_eq!(config.model_name, "qwen3:1.7b");
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
        assert_eq!(config.model_name, "qwen3:4b");
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

    #[cfg(not(target_os = "windows"))]
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
        assert_eq!(at_5999.model_name, "qwen3:4b");

        let at_3000 = select_model_for_vram(3_000);
        assert_eq!(at_3000.model_name, "qwen3:4b");

        let at_2999 = select_model_for_vram(2_999);
        assert_eq!(at_2999.model_name, "qwen3:1.7b");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_windows_gpu_json_single_gpu() {
        let json = r#"{"Name":"AMD Radeon RX 9070","AdapterRAM":4293918720}"#;
        let result = parse_windows_gpu_json(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "AMD Radeon RX 9070");
        assert_eq!(result[0].1, 4293918720);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_windows_gpu_json_multiple_gpus() {
        let json = r#"[{"Name":"AMD Radeon RX 9070","AdapterRAM":4293918720},{"Name":"Microsoft Basic Display Adapter","AdapterRAM":0}]"#;
        let result = parse_windows_gpu_json(json).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "AMD Radeon RX 9070");
        assert_eq!(result[1].0, "Microsoft Basic Display Adapter");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_windows_gpu_json_empty() {
        assert!(parse_windows_gpu_json("").is_none());
        assert!(parse_windows_gpu_json("   ").is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_is_discrete_gpu() {
        assert!(is_discrete_gpu("AMD Radeon RX 9070"));
        assert!(is_discrete_gpu("NVIDIA GeForce RTX 4090"));
        assert!(is_discrete_gpu("Intel Arc A770"));
        assert!(!is_discrete_gpu("Microsoft Basic Display Adapter"));
        assert!(!is_discrete_gpu("Microsoft Remote Display Adapter"));
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
}
