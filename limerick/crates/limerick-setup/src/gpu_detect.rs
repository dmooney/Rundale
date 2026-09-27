//! GPU vendor and VRAM detection for the three supported OS families.
//!
//! - macOS: `sysctl hw.memsize` (Apple Silicon unified memory)
//! - Windows: PowerShell `Win32_VideoController` + registry fallback
//! - Linux: `nvidia-smi` (NVIDIA) or `rocm-smi` (AMD)

use std::process::Command;

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
    fn test_gpu_vendor_equality() {
        assert_eq!(GpuVendor::Nvidia, GpuVendor::Nvidia);
        assert_ne!(GpuVendor::Nvidia, GpuVendor::Amd);
        assert_ne!(GpuVendor::Amd, GpuVendor::CpuOnly);
        assert_ne!(GpuVendor::AppleSilicon, GpuVendor::CpuOnly);
        assert_ne!(GpuVendor::AppleSilicon, GpuVendor::Amd);
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
    fn test_extract_bytes_from_line() {
        assert_eq!(
            extract_bytes_from_line("VRAM Total Memory (B): 17163091968"),
            Some(17163091968)
        );
        assert_eq!(extract_bytes_from_line("no numbers here"), None);
        assert_eq!(extract_bytes_from_line("small: 42"), None); // < 1MB threshold
    }

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

    /// Live smoke test — runs `sysctl` on the host Mac and verifies the
    /// detector reports a plausible unified-memory figure and that the
    /// end-to-end pipeline picks a valid gemma4 tier.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_detect_apple_silicon_live() {
        use crate::model_select::select_model;

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

    // ---- Windows-only tests ----

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
}
