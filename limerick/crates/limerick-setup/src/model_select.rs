//! Model selection based on available GPU/VRAM hardware.

use super::gpu_detect::{GpuInfo, GpuVendor};

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

/// A model tier with its VRAM threshold (minimum MB) and config.
struct ModelTier {
    threshold_mb: u64,
    model_name: &'static str,
    tier_label: &'static str,
    vram_required_mb: u64,
}

/// Model tiers ordered highest-threshold first.
///
/// Ollama disk sizes (which closely track runtime memory for gemma4 quants):
///   e2b=7.2GB, e4b=9.6GB, 26b=18GB (MoE, 4B active), 31b=20GB (dense).
/// Thresholds sit a few GB above each model's size to leave context headroom.
static MODEL_TIERS: &[ModelTier] = &[
    ModelTier {
        threshold_mb: 25_000,
        model_name: "gemma4:31b",
        tier_label: "Tier 1 — Full quality (dense 31B)",
        vram_required_mb: 22_000,
    },
    ModelTier {
        threshold_mb: 17_000,
        model_name: "gemma4:26b",
        tier_label: "Tier 2 — MoE (26B / 4B active)",
        vram_required_mb: 19_000,
    },
    ModelTier {
        threshold_mb: 11_000,
        model_name: "gemma4:e4b",
        tier_label: "Tier 3 — Edge (4.5B effective)",
        vram_required_mb: 10_500,
    },
];

/// Fallback tier when VRAM is below all thresholds.
static MODEL_TIER_FALLBACK: ModelTier = ModelTier {
    threshold_mb: 0,
    model_name: "gemma4:e2b",
    tier_label: "Tier 4 — Edge minimal (2.3B effective)",
    vram_required_mb: 8_000,
};

/// Selects a gemma4 model given a specific VRAM budget in MB using a table-driven lookup.
pub(super) fn select_model_for_vram(vram_mb: u64) -> ModelConfig {
    let tier = MODEL_TIERS
        .iter()
        .find(|t| vram_mb >= t.threshold_mb)
        .unwrap_or(&MODEL_TIER_FALLBACK);
    ModelConfig {
        model_name: tier.model_name.to_string(),
        tier_label: tier.tier_label.to_string(),
        vram_required_mb: tier.vram_required_mb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
