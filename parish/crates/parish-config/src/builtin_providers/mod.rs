//! Builtin LLM providers — the five hardcoded into the engine.
//!
//! These ship with `parish-config` and are always available regardless of
//! what's under `mods/`. Three categories:
//!   - `simulator` — offline mock; no network, no model.
//!   - `ollama` / `vllm` / `vllm_mlx` — local-inference providers the engine
//!     auto-spawns or downloads models for.
//!   - `custom` — generic OpenAI-compat escape hatch; no fixed identity.
//!
//! All other providers (anthropic, openai, openrouter, ...) live under
//! `mods/<id>/providers/<id>.toml` and are registered into `ProviderRegistry`
//! at startup via `register_mod_providers`.

pub static SIMULATOR_TOML: &str = include_str!("simulator.toml");
pub static OLLAMA_TOML: &str = include_str!("ollama.toml");
pub static VLLM_TOML: &str = include_str!("vllm.toml");
pub static VLLM_MLX_TOML: &str = include_str!("vllm_mlx.toml");
pub static CUSTOM_TOML: &str = include_str!("custom.toml");

/// All builtin TOMLs as raw strings. Parsed into `ProviderMod` by the
/// registry on first access.
pub static ALL: &[&str] = &[
    SIMULATOR_TOML,
    OLLAMA_TOML,
    VLLM_TOML,
    VLLM_MLX_TOML,
    CUSTOM_TOML,
];

/// IDs of the five builtins, in the same order as `ALL`. Used by callers
/// that want to distinguish builtin vs mod-loaded providers.
pub const BUILTIN_IDS: &[&str] = &["simulator", "ollama", "vllm", "vllmmlx", "custom"];
