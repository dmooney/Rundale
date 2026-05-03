//! Ollama bootstrap, GPU detection, and model management.
//!
//! Handles the full Ollama lifecycle: installation detection,
//! auto-install, GPU/VRAM detection, model selection based on
//! available hardware, and automatic model pulling.

use crate::AnyClient;
use crate::client::OllamaProcess;
use crate::openai_client::OpenAiClient;
use parish_config::{InferenceConfig, Provider, ProviderConfig};
use parish_types::ParishError;
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
    /// Apple Silicon (M-series) with unified memory; Metal acceleration via Ollama.
    AppleSilicon,
    /// No discrete GPU detected; CPU-only inference.
    CpuOnly,
}

impl std::fmt::Display for GpuVendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuVendor::Nvidia => write!(f, "NVIDIA (CUDA)"),
            GpuVendor::Amd => write!(f, "AMD"),
            GpuVendor::AppleSilicon => write!(f, "Apple Silicon (Metal)"),
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
            GpuVendor::AppleSilicon => write!(
                f,
                "{} — {}MB unified memory, ~{}MB available",
                self.vendor, self.vram_total_mb, self.vram_free_mb
            ),
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
    /// The Ollama model tag (e.g. "gemma4:e4b").
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
pub trait SetupProgress: Send + Sync {
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
/// Tries platform-specific detection first (macOS via `sysctl`, Windows via
/// PowerShell, Linux via `nvidia-smi` / `rocm-smi`), then falls back to CPU-only.
pub async fn detect_gpu_info() -> GpuInfo {
    // On macOS, every supported machine is Apple Silicon with unified memory.
    // Metal acceleration is automatic via Ollama; no discrete GPU check needed.
    #[cfg(target_os = "macos")]
    {
        if let Some(info) = detect_apple_silicon().await {
            return info;
        }
    }

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
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
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

/// Detects Apple Silicon unified memory via `sysctl hw.memsize`.
///
/// Unified memory is shared with the OS, so we report ~70% as "available"
/// to leave headroom for the system, the game, and other apps. This feeds
/// `select_model_for_vram`, which picks the largest gemma4 tier that fits.
#[cfg(target_os = "macos")]
async fn detect_apple_silicon() -> Option<GpuInfo> {
    let output =
        tokio::task::spawn_blocking(|| Command::new("sysctl").args(["-n", "hw.memsize"]).output())
            .await
            .ok()?
            .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let bytes: u64 = stdout.trim().parse().ok()?;
    if bytes == 0 {
        return None;
    }

    let total_mb = bytes / (1024 * 1024);
    // Reserve ~30% for OS + app; the rest is what a model can realistically use.
    let available_mb = total_mb * 70 / 100;

    Some(GpuInfo {
        vendor: GpuVendor::AppleSilicon,
        vram_total_mb: total_mb,
        vram_free_mb: available_mb,
    })
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
    parse_nvidia_smi_output(&stdout)
}

/// Parses the first line of `nvidia-smi --query-gpu=memory.total,memory.free
/// --format=csv,noheader,nounits` output into a `GpuInfo`.
///
/// Expected format: `"<total>, <free>"` (one GPU per line, values in MiB).
/// Returns `None` if the output is empty, has fewer than two comma-separated
/// fields, or either field fails to parse as `u64`.
pub(crate) fn parse_nvidia_smi_output(stdout: &str) -> Option<GpuInfo> {
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
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
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
    if let Some(info) = parse_rocm_smi_output(&stdout) {
        return Some(info);
    }

    // rocm-smi exists but we couldn't parse VRAM — still AMD.
    // Fall back to detecting ROCm's presence on disk.
    if std::path::Path::new("/opt/rocm").exists() {
        return Some(GpuInfo {
            vendor: GpuVendor::Amd,
            vram_total_mb: 0,
            vram_free_mb: 0,
        });
    }
    None
}

/// Parses `rocm-smi --showmeminfo vram` output into a `GpuInfo`.
///
/// Scans each line for "total" / "used" keywords (case-insensitive) and
/// extracts the byte count. VRAM bytes are converted to MiB. Returns
/// `None` if the total VRAM line is missing or unparseable.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn parse_rocm_smi_output(stdout: &str) -> Option<GpuInfo> {
    let (mut total_mb, mut used_mb) = (0u64, 0u64);

    // NB: real `rocm-smi --showmeminfo vram` output labels the used-memory
    // line as "VRAM Total Used Memory (B): ...", which also contains the
    // substring "total". The `used` check must run first so the used line
    // does not clobber the total line.
    for line in stdout.lines() {
        let lower = line.to_lowercase();
        if lower.contains("used")
            && let Some(bytes) = extract_bytes_from_line(line)
        {
            used_mb = bytes / (1024 * 1024);
        } else if lower.contains("total")
            && let Some(bytes) = extract_bytes_from_line(line)
        {
            total_mb = bytes / (1024 * 1024);
        }
    }

    if total_mb == 0 {
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
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn extract_bytes_from_line(line: &str) -> Option<u64> {
    line.split_whitespace()
        .filter_map(|token| token.parse::<u64>().ok())
        .find(|&n| n > 1_000_000) // VRAM values are in bytes, so > 1MB
}

/// Selects the best model for the available VRAM / unified memory.
///
/// Uses conservative thresholds to leave headroom for the OS and
/// other GPU workloads:
/// - 25GB+ → gemma4:31b (Tier 1, dense, best quality)
/// - 17GB+ → gemma4:26b (Tier 2, MoE — 4B active, fast)
/// - 11GB+ → gemma4:e4b (Tier 3, edge, 4.5B effective)
/// - <11GB → gemma4:e2b (Tier 4, edge, 2.3B effective)
///
/// On Apple Silicon `vram_free_mb` is pre-scaled to ~70% of unified memory,
/// so the same thresholds apply uniformly.
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

/// Selects a gemma4 model given a specific VRAM budget in MB.
///
/// Ollama disk sizes (which closely track runtime memory for gemma4 quants):
///   e2b=7.2GB, e4b=9.6GB, 26b=18GB (MoE, 4B active), 31b=20GB (dense).
/// Thresholds sit a few GB above each model's size to leave context headroom.
fn select_model_for_vram(vram_mb: u64) -> ModelConfig {
    if vram_mb >= 25_000 {
        ModelConfig {
            model_name: "gemma4:31b".to_string(),
            tier_label: "Tier 1 — Full quality (dense 31B)".to_string(),
            vram_required_mb: 22_000,
        }
    } else if vram_mb >= 17_000 {
        ModelConfig {
            model_name: "gemma4:26b".to_string(),
            tier_label: "Tier 2 — MoE (26B / 4B active)".to_string(),
            vram_required_mb: 19_000,
        }
    } else if vram_mb >= 11_000 {
        ModelConfig {
            model_name: "gemma4:e4b".to_string(),
            tier_label: "Tier 3 — Edge (4.5B effective)".to_string(),
            vram_required_mb: 10_500,
        }
    } else {
        ModelConfig {
            model_name: "gemma4:e2b".to_string(),
            tier_label: "Tier 4 — Edge minimal (2.3B effective)".to_string(),
            vram_required_mb: 8_000,
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
        crate::rate_limit::InferenceRateLimiter::from_config(config.rate_limits.default);
    let client =
        OpenAiClient::new_with_config(base_url, None, config).maybe_with_rate_limit(base_limiter);

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

/// Builds an inference client for the resolved [`ProviderConfig`], running
/// the full Ollama setup sequence (install, auto-start, GPU detection,
/// model pull, warmup) when the provider is [`Provider::Ollama`].
///
/// This is the single entry point shared by all runtime modes (CLI, Tauri,
/// web server) so they stay in lock-step — CLAUDE.md rule #2 (mode parity).
/// Callers are responsible for keeping the returned [`OllamaProcess`] alive
/// for the lifetime of the app so the child `ollama serve` is stopped on
/// exit.
///
/// # Errors
///
/// - [`Provider::Ollama`]: bubbles up whatever `setup_ollama_with_config`
///   returns (no GPU, install failure, pull failure, …).
/// - Other providers: returns [`ParishError::Config`] if no model is set,
///   since non-Ollama backends have no auto-detect fallback.
pub async fn setup_provider_client(
    config: &ProviderConfig,
    inference_config: &InferenceConfig,
    progress: &dyn SetupProgress,
) -> Result<(AnyClient, String, OllamaProcess), ParishError> {
    match config.provider {
        Provider::Simulator => Ok((
            AnyClient::simulator(),
            "simulator".to_string(),
            OllamaProcess::none(),
        )),
        Provider::Ollama => {
            let setup = setup_ollama_with_config(
                &config.base_url,
                config.model.as_deref(),
                progress,
                inference_config,
            )
            .await?;
            let client = AnyClient::open_ai(setup.client);
            Ok((client, setup.model_name, setup.process))
        }
        _ => {
            let model = config.model.clone().ok_or_else(|| {
                ParishError::Config(format!(
                    "{:?} provider requires a model name. Set --model or PARISH_MODEL.",
                    config.provider
                ))
            })?;
            let client = crate::build_client(
                &config.provider,
                &config.base_url,
                config.api_key.as_deref(),
                inference_config,
            );
            Ok((client, model, OllamaProcess::none()))
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
        assert_eq!(GpuVendor::AppleSilicon.to_string(), "Apple Silicon (Metal)");
        assert_eq!(GpuVendor::CpuOnly.to_string(), "CPU-only");
    }

    #[test]
    fn test_gpu_info_display_apple_silicon() {
        let info = GpuInfo {
            vendor: GpuVendor::AppleSilicon,
            vram_total_mb: 32768,
            vram_free_mb: 22937,
        };
        let display = info.to_string();
        assert!(display.contains("Apple Silicon"));
        assert!(display.contains("32768"));
        assert!(display.contains("unified memory"));
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
            model_name: "gemma4:e4b".to_string(),
            tier_label: "Tier 3 — Edge (4.5B effective)".to_string(),
            vram_required_mb: 10_500,
        };
        let display = config.to_string();
        assert!(display.contains("gemma4:e4b"));
        assert!(display.contains("Tier 3"));
        assert!(display.contains("10500"));
    }

    #[test]
    fn test_select_model_huge_vram_picks_31b() {
        let config = select_model_for_vram(40_000);
        assert_eq!(config.model_name, "gemma4:31b");
        assert!(config.tier_label.contains("Tier 1"));
    }

    #[test]
    fn test_select_model_24gb_picks_26b_moe() {
        let config = select_model_for_vram(24_000);
        assert_eq!(config.model_name, "gemma4:26b");
        assert!(config.tier_label.contains("Tier 2"));
    }

    #[test]
    fn test_select_model_16gb_picks_e4b() {
        let config = select_model_for_vram(16_000);
        assert_eq!(config.model_name, "gemma4:e4b");
        assert!(config.tier_label.contains("Tier 3"));
    }

    #[test]
    fn test_select_model_12gb_picks_e4b() {
        let config = select_model_for_vram(12_000);
        assert_eq!(config.model_name, "gemma4:e4b");
    }

    #[test]
    fn test_select_model_8gb_picks_e2b() {
        let config = select_model_for_vram(8_000);
        assert_eq!(config.model_name, "gemma4:e2b");
        assert!(config.tier_label.contains("Tier 4"));
    }

    #[test]
    fn test_select_model_zero_vram_picks_e2b() {
        let config = select_model_for_vram(0);
        assert_eq!(config.model_name, "gemma4:e2b");
    }

    #[test]
    fn test_select_model_cpu_only_gpu_info() {
        let gpu = GpuInfo {
            vendor: GpuVendor::CpuOnly,
            vram_total_mb: 0,
            vram_free_mb: 0,
        };
        let config = select_model(&gpu);
        assert_eq!(config.model_name, "gemma4:e2b");
    }

    #[test]
    fn test_select_model_amd_24gb() {
        let gpu = GpuInfo {
            vendor: GpuVendor::Amd,
            vram_total_mb: 24_576,
            vram_free_mb: 22_000,
        };
        let config = select_model(&gpu);
        assert_eq!(config.model_name, "gemma4:26b");
    }

    #[test]
    fn test_select_model_apple_silicon_32gb() {
        // Apple Silicon with 32 GB unified memory; detector pre-scales
        // vram_free_mb to ~70% (≈22 GB), which falls in the Tier 2 range.
        let gpu = GpuInfo {
            vendor: GpuVendor::AppleSilicon,
            vram_total_mb: 32_768,
            vram_free_mb: 22_937,
        };
        let config = select_model(&gpu);
        assert_eq!(config.model_name, "gemma4:26b");
    }

    #[test]
    fn test_select_model_apple_silicon_16gb() {
        // 16 GB Mac → ~11 GB scaled → Tier 3 edge model.
        let gpu = GpuInfo {
            vendor: GpuVendor::AppleSilicon,
            vram_total_mb: 16_384,
            vram_free_mb: 11_468,
        };
        let config = select_model(&gpu);
        assert_eq!(config.model_name, "gemma4:e4b");
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
        // Unknown VRAM assumes 8 GB → below 11 GB threshold → e2b
        assert_eq!(config.model_name, "gemma4:e2b");
    }

    #[test]
    fn test_select_model_uses_free_vram_when_available() {
        let gpu = GpuInfo {
            vendor: GpuVendor::Nvidia,
            vram_total_mb: 24_000,
            vram_free_mb: 12_000, // Half in use
        };
        let config = select_model(&gpu);
        // Free VRAM (12 GB) wins over total; 12 GB ≥ 11 GB → e4b
        assert_eq!(config.model_name, "gemma4:e4b");
    }

    #[test]
    fn test_select_model_uses_total_when_free_unknown() {
        let gpu = GpuInfo {
            vendor: GpuVendor::Nvidia,
            vram_total_mb: 16_384,
            vram_free_mb: 0, // Free unknown
        };
        let config = select_model(&gpu);
        // 80% of 16384 ≈ 13_107 → Tier 3 (e4b)
        assert_eq!(config.model_name, "gemma4:e4b");
    }

    /// Live smoke test — runs `sysctl` on the host Mac and verifies the
    /// detector reports a plausible unified-memory figure and that the
    /// end-to-end pipeline picks a valid gemma4 tier.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_detect_apple_silicon_live() {
        let info = detect_apple_silicon()
            .await
            .expect("sysctl hw.memsize should succeed on macOS");
        assert_eq!(info.vendor, GpuVendor::AppleSilicon);
        // Any Mac running this codebase has more than 4 GB of RAM.
        assert!(
            info.vram_total_mb >= 4_096,
            "reported total memory implausibly low: {} MB",
            info.vram_total_mb
        );
        // ~70% scaling: free must be less than total but more than half.
        assert!(info.vram_free_mb < info.vram_total_mb);
        assert!(info.vram_free_mb > info.vram_total_mb / 2);

        let picked = select_model(&info);
        let valid_tags = ["gemma4:31b", "gemma4:26b", "gemma4:e4b", "gemma4:e2b"];
        assert!(
            valid_tags.contains(&picked.model_name.as_str()),
            "picked unknown model: {}",
            picked.model_name
        );
        eprintln!(
            "[live] {}MB total, {}MB available → {}",
            info.vram_total_mb, info.vram_free_mb, picked
        );
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
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
        messages: std::sync::Mutex<Vec<String>>,
    }

    impl TestProgress {
        fn new() -> Self {
            Self {
                messages: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn messages(&self) -> Vec<String> {
            self.messages.lock().unwrap().clone()
        }
    }

    impl SetupProgress for TestProgress {
        fn on_status(&self, msg: &str) {
            self.messages.lock().unwrap().push(msg.to_string());
        }

        fn on_pull_progress(&self, completed: u64, total: u64) {
            self.messages
                .lock()
                .unwrap()
                .push(format!("progress: {}/{}", completed, total));
        }

        fn on_error(&self, msg: &str) {
            self.messages
                .lock()
                .unwrap()
                .push(format!("ERROR: {}", msg));
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
        assert_ne!(GpuVendor::AppleSilicon, GpuVendor::CpuOnly);
        assert_ne!(GpuVendor::AppleSilicon, GpuVendor::Amd);
    }

    #[test]
    fn test_select_model_boundary_values() {
        // Exactly at tier boundaries: 25_000 / 17_000 / 11_000.
        let at_25000 = select_model_for_vram(25_000);
        assert_eq!(at_25000.model_name, "gemma4:31b");

        let at_24999 = select_model_for_vram(24_999);
        assert_eq!(at_24999.model_name, "gemma4:26b");

        let at_17000 = select_model_for_vram(17_000);
        assert_eq!(at_17000.model_name, "gemma4:26b");

        let at_16999 = select_model_for_vram(16_999);
        assert_eq!(at_16999.model_name, "gemma4:e4b");

        let at_11000 = select_model_for_vram(11_000);
        assert_eq!(at_11000.model_name, "gemma4:e4b");

        let at_10999 = select_model_for_vram(10_999);
        assert_eq!(at_10999.model_name, "gemma4:e2b");
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

    // ---- nvidia-smi parser tests ----

    #[test]
    fn test_parse_nvidia_smi_output_success() {
        // Actual format from `nvidia-smi --query-gpu=memory.total,memory.free --format=csv,noheader,nounits`
        let stdout = "24564, 23811\n";
        let info = parse_nvidia_smi_output(stdout).expect("parser should succeed");
        assert_eq!(info.vendor, GpuVendor::Nvidia);
        assert_eq!(info.vram_total_mb, 24564);
        assert_eq!(info.vram_free_mb, 23811);
    }

    #[test]
    fn test_parse_nvidia_smi_output_first_line_only() {
        // Multi-GPU systems return one line per GPU; we only read the first
        let stdout = "16384, 14000\n8192, 7000\n";
        let info = parse_nvidia_smi_output(stdout).expect("parser should succeed");
        assert_eq!(info.vram_total_mb, 16384);
        assert_eq!(info.vram_free_mb, 14000);
    }

    #[test]
    fn test_parse_nvidia_smi_output_empty() {
        assert!(parse_nvidia_smi_output("").is_none());
    }

    #[test]
    fn test_parse_nvidia_smi_output_malformed() {
        // Missing comma separator
        assert!(parse_nvidia_smi_output("24564 23811").is_none());
    }

    #[test]
    fn test_parse_nvidia_smi_output_non_numeric() {
        // Non-numeric where numbers expected
        assert!(parse_nvidia_smi_output("unknown, data").is_none());
    }

    #[test]
    fn test_parse_nvidia_smi_output_extra_whitespace() {
        // Trimming handles leading/trailing whitespace in the fields
        let stdout = "  24564  ,  23811  \n";
        let info = parse_nvidia_smi_output(stdout).expect("parser should succeed");
        assert_eq!(info.vram_total_mb, 24564);
        assert_eq!(info.vram_free_mb, 23811);
    }

    // ---- rocm-smi parser tests ----

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn test_parse_rocm_smi_output_total_and_used() {
        // Simplified rocm-smi --showmeminfo vram style output
        let stdout = "\
GPU[0]  : VRAM Total Memory (B): 17163091968
GPU[0]  : VRAM Total Used Memory (B): 3221225472
";
        let info = parse_rocm_smi_output(stdout).expect("parser should succeed");
        assert_eq!(info.vendor, GpuVendor::Amd);
        // 17163091968 / (1024*1024) ≈ 16368
        assert_eq!(info.vram_total_mb, 16368);
        // used = 3221225472 / (1024*1024) = 3072, so free = 16368 - 3072 = 13296
        assert_eq!(info.vram_free_mb, 13296);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn test_parse_rocm_smi_output_total_only() {
        // If used line is missing, free == total
        let stdout = "GPU[0]  : VRAM Total Memory (B): 17163091968\n";
        let info = parse_rocm_smi_output(stdout).expect("parser should succeed");
        assert_eq!(info.vram_total_mb, 16368);
        assert_eq!(info.vram_free_mb, 16368);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn test_parse_rocm_smi_output_missing_total_returns_none() {
        // Without a total line, we cannot determine VRAM at all
        let stdout = "GPU[0]  : VRAM Total Used Memory (B): 3221225472\n";
        assert!(parse_rocm_smi_output(stdout).is_none());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn test_parse_rocm_smi_output_empty() {
        assert!(parse_rocm_smi_output("").is_none());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn test_parse_rocm_smi_output_used_greater_than_total_saturates() {
        // Defensive: if rocm-smi reported inconsistent numbers, we saturate to 0
        // rather than panicking.
        let stdout = "\
GPU[0]  : VRAM Total Memory (B): 1048576000
GPU[0]  : VRAM Total Used Memory (B): 2097152000
";
        let info = parse_rocm_smi_output(stdout).expect("parser should succeed");
        assert_eq!(info.vram_free_mb, 0);
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
    async fn test_pull_model_maps_404_to_model_not_available() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/pull"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;

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
        let server = wiremock::MockServer::start().await;
        // Only status lines, no total/completed
        let body = "\
{\"status\":\"pulling manifest\"}
{\"status\":\"verifying sha256 digest\"}
{\"status\":\"success\"}
";
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/pull"))
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
}
