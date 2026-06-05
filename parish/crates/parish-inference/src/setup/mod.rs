//! Ollama bootstrap, GPU detection, and model management.
//!
//! Handles the full Ollama lifecycle: installation detection,
//! auto-install, GPU/VRAM detection, model selection based on
//! available hardware, and automatic model pulling.
//!
//! # Submodule layout
//!
//! | Submodule | Responsibility |
//! |-----------|----------------|
//! | [`progress`] | [`SetupProgress`] trait + [`StdoutProgress`] impl |
//! | [`gpu_detect`] | GPU vendor / VRAM detection (macOS/Windows/Linux) |
//! | [`model_select`] | VRAM-based model tier selection |
//! | [`process`] | Managed child-process handles (Ollama, vllm-mlx, vllm) |
//! | [`orchestration`] | Full setup sequence + unified provider entry point |

pub mod gpu_detect;
pub mod model_select;
pub mod orchestration;
pub mod process;
pub mod progress;

// ── Public re-exports (preserve the pre-split `setup::*` paths) ─────────────

pub use gpu_detect::{GpuInfo, GpuVendor, detect_gpu_info};
pub use model_select::{ModelConfig, select_model};
pub use orchestration::{
    OllamaSetup, build_gpu_env, check_ollama_installed, ensure_model_available,
    ensure_model_available_with_config, install_ollama, is_model_available,
    is_model_available_with_config, pull_model, pull_model_with_config, setup_ollama,
    setup_ollama_with_config, setup_provider_client,
};
pub use process::{
    OllamaProcess, RuntimeProcesses, VllmMlxProcess, VllmMlxSlot, VllmProcess, VllmSlot,
};
pub use progress::{SetupProgress, StdoutProgress};
