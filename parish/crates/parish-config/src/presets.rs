//! Recommended model presets per provider, indexed by inference category.
//!
//! Each entry in [`PresetModels`] is a model id chosen as a sensible default
//! for that role, e.g. Anthropic's preset uses Opus for player-facing
//! dialogue, Sonnet for background simulation and arrival reactions, and
//! Haiku for low-latency intent parsing.
//!
//! Local providers reference Ollama/HuggingFace-style tags
//! (`qwen3:32b`, `qwen3:14b`, `qwen3:4b`) sized to match the
//! flagship/mid/small tier mapping. `Custom` and `Simulator` declare no
//! preset — `Custom` because the endpoint shape is unknown, `Simulator`
//! because it ignores the model name entirely.

use crate::provider::{InferenceCategory, Provider};

/// Recommended model id per [`InferenceCategory`], in canonical
/// [`InferenceCategory::ALL`] order: `[Dialogue, Simulation, Intent, Reaction]`.
///
/// `None` in any slot means "no preset available for this provider/role".
pub type PresetModels = [Option<&'static str>; 4];

impl InferenceCategory {
    /// Array index matching [`InferenceCategory::ALL`] order.
    pub fn idx(self) -> usize {
        match self {
            InferenceCategory::Dialogue => 0,
            InferenceCategory::Simulation => 1,
            InferenceCategory::Intent => 2,
            InferenceCategory::Reaction => 3,
        }
    }
}

impl Provider {
    /// Returns the recommended model id for each [`InferenceCategory`].
    ///
    /// `Custom` and `Simulator` return `[None; 4]`: `Custom` is opaque
    /// (the user must know their own endpoint's model ids) and `Simulator`
    /// runs offline without a real model.
    pub fn preset_models(&self) -> PresetModels {
        // Tier mapping (matches the Anthropic example — see crate docs):
        //   Dialogue  → flagship / opus-tier   (highest quality reasoning)
        //   Simulation→ mid-tier / sonnet-tier (balanced quality/throughput)
        //   Intent    → small  / haiku-tier    (cheap, low-latency JSON)
        //   Reaction  → mid-tier / sonnet-tier (same as simulation)
        //
        // All cloud IDs were verified against each provider's docs in
        // April 2026. Dated IDs are used where the `-latest` alias points
        // at a stale version (notably Mistral, where `mistral-large-latest`
        // still resolves to the 2024-era model).
        match self {
            Provider::Anthropic => [
                Some("claude-opus-4-7"),
                Some("claude-sonnet-4-6"),
                Some("claude-haiku-4-5"),
                Some("claude-sonnet-4-6"),
            ],
            // OpenAI: GPT-5.5 is the current flagship (Apr 2026); the
            // 5.4 mini/nano variants superseded the original 5-mini/nano
            // in March 2026.
            Provider::OpenAi => [
                Some("gpt-5.5"),
                Some("gpt-5.4-mini"),
                Some("gpt-5.4-nano"),
                Some("gpt-5.4-mini"),
            ],
            // Google: 2.5 Pro flagship → Flash mid → Flash-Lite small.
            // (Gemini 3 not yet generally available; 2.5 family is current.)
            Provider::Google => [
                Some("gemini-2.5-pro"),
                Some("gemini-2.5-flash"),
                Some("gemini-2.5-flash-lite"),
                Some("gemini-2.5-flash"),
            ],
            // Groq: GPT-OSS 120B is the largest open-weight flagship hosted
            // (replaced Llama-4 Maverick in Feb 2026); Llama 3.3 70B for
            // the mid tier; Llama 3.1 8B Instant for haiku-tier.
            Provider::Groq => [
                Some("openai/gpt-oss-120b"),
                Some("llama-3.3-70b-versatile"),
                Some("llama-3.1-8b-instant"),
                Some("llama-3.3-70b-versatile"),
            ],
            // xAI: Grok 4.20 reasoning for top quality, Grok 4.20
            // non-reasoning for the balanced tier, Grok 4.1 Fast for the
            // cheap/fast tier.
            Provider::Xai => [
                Some("grok-4.20-reasoning"),
                Some("grok-4.20-non-reasoning"),
                Some("grok-4.1-fast-non-reasoning"),
                Some("grok-4.20-non-reasoning"),
            ],
            // Mistral: Large 3 (2512) flagship, Medium 3.1 (2508) mid,
            // Ministral 3 3B (2512) small. Dated IDs because
            // `mistral-large-latest` still resolves to the 2024-02 build.
            Provider::Mistral => [
                Some("mistral-large-2512"),
                Some("mistral-medium-2508"),
                Some("ministral-3-3b-2512"),
                Some("mistral-medium-2508"),
            ],
            // DeepSeek: V4 Pro flagship (1.6T params), V4 Flash mid (284B);
            // the older `deepseek-chat` / `deepseek-reasoner` aliases are
            // scheduled for deprecation 2026-07-24.
            Provider::DeepSeek => [
                Some("deepseek-v4-pro"),
                Some("deepseek-v4-flash"),
                Some("deepseek-v4-flash"),
                Some("deepseek-v4-flash"),
            ],
            // Together: Qwen 3.5 397B-A17B for the flagship tier (the
            // 405B Llama is no longer on serverless); Llama 3.3 70B
            // Turbo for mid, Llama 3.1 8B Turbo for small.
            Provider::Together => [
                Some("Qwen/Qwen3.5-397B-A17B"),
                Some("meta-llama/Llama-3.3-70B-Instruct-Turbo"),
                Some("meta-llama/Llama-3.1-8B-Instruct-Turbo"),
                Some("meta-llama/Llama-3.3-70B-Instruct-Turbo"),
            ],
            // NVIDIA NIM: Nemotron 3 family — NVIDIA's own Mamba-Transformer
            // hybrid MoE models, purpose-tuned for this endpoint with 1M
            // context and first-class tool calling. Per NVIDIA's docs, the
            // Super 120B-A12B variant meets or beats DeepSeek-R1 on
            // reasoning at much higher throughput. The Nano 30B-A3B (3B
            // active params) is the balanced MoE for JSON simulation, and
            // Nemotron Nano 9B v2 is the dedicated low-latency model.
            // Users wanting alternatives (deepseek-ai/deepseek-v4-pro,
            // meta/llama-3.1-405b-instruct, etc.) can override per
            // category via PARISH_DIALOGUE_MODEL etc.
            Provider::NvidiaNim => [
                Some("nvidia/nemotron-3-super-120b-a12b"),
                Some("nvidia/nemotron-3-nano-30b-a3b"),
                Some("nvidia/nvidia-nemotron-nano-9b-v2"),
                Some("nvidia/nemotron-3-nano-30b-a3b"),
            ],
            // OpenRouter: cross-provider IDs mirror the Anthropic preset.
            Provider::OpenRouter => [
                Some("anthropic/claude-opus-4-7"),
                Some("anthropic/claude-sonnet-4-6"),
                Some("anthropic/claude-haiku-4-5"),
                Some("anthropic/claude-sonnet-4-6"),
            ],
            // Local providers (Ollama / LM Studio / vLLM): qwen3 32B is
            // the largest size that still fits modern consumer hardware;
            // 14B is the balanced mid-tier; 4B is the fast tier.
            Provider::Ollama => [
                Some("qwen3:32b"),
                Some("qwen3:14b"),
                Some("qwen3:4b"),
                Some("qwen3:14b"),
            ],
            Provider::LmStudio => [
                Some("qwen3:32b"),
                Some("qwen3:14b"),
                Some("qwen3:4b"),
                Some("qwen3:14b"),
            ],
            Provider::Vllm => [
                Some("Qwen/Qwen3-32B"),
                Some("Qwen/Qwen3-14B"),
                Some("Qwen/Qwen3-4B"),
                Some("Qwen/Qwen3-14B"),
            ],
            // WebGPU and other local providers have no cloud models.
            Provider::Custom | Provider::WebGpu | Provider::Simulator => [None, None, None, None],
        }
    }

    /// Returns the recommended model id for a single [`InferenceCategory`],
    /// or `None` if no preset is available for that role.
    pub fn preset_model(&self, cat: InferenceCategory) -> Option<&'static str> {
        self.preset_models()[cat.idx()]
    }

    /// Returns true if this provider declares any preset models.
    pub fn has_preset(&self) -> bool {
        self.preset_models().iter().any(Option::is_some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_category_idx_matches_all_order() {
        for (i, cat) in InferenceCategory::ALL.iter().enumerate() {
            assert_eq!(cat.idx(), i, "idx() must match position in ALL");
        }
    }

    #[test]
    fn cloud_providers_have_complete_presets() {
        for provider in [
            Provider::Anthropic,
            Provider::OpenAi,
            Provider::Google,
            Provider::Groq,
            Provider::Xai,
            Provider::Mistral,
            Provider::DeepSeek,
            Provider::Together,
            Provider::NvidiaNim,
            Provider::OpenRouter,
        ] {
            let presets = provider.preset_models();
            for (i, slot) in presets.iter().enumerate() {
                let model =
                    slot.unwrap_or_else(|| panic!("{:?} missing preset for slot {}", provider, i));
                assert!(
                    !model.is_empty(),
                    "{:?} has empty preset for slot {}",
                    provider,
                    i
                );
            }
            assert!(provider.has_preset());
        }
    }

    #[test]
    fn local_providers_have_complete_presets() {
        for provider in [Provider::Ollama, Provider::LmStudio, Provider::Vllm] {
            let presets = provider.preset_models();
            for (i, slot) in presets.iter().enumerate() {
                let model =
                    slot.unwrap_or_else(|| panic!("{:?} missing preset for slot {}", provider, i));
                assert!(!model.is_empty());
            }
            assert!(provider.has_preset());
        }
    }

    #[test]
    fn custom_and_simulator_have_no_presets() {
        assert_eq!(Provider::Custom.preset_models(), [None, None, None, None]);
        assert_eq!(
            Provider::Simulator.preset_models(),
            [None, None, None, None]
        );
        assert!(!Provider::Custom.has_preset());
        assert!(!Provider::Simulator.has_preset());
    }

    #[test]
    fn anthropic_preset_matches_user_intent() {
        let p = Provider::Anthropic;
        assert_eq!(
            p.preset_model(InferenceCategory::Dialogue),
            Some("claude-opus-4-7")
        );
        assert_eq!(
            p.preset_model(InferenceCategory::Simulation),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            p.preset_model(InferenceCategory::Intent),
            Some("claude-haiku-4-5")
        );
        assert_eq!(
            p.preset_model(InferenceCategory::Reaction),
            Some("claude-sonnet-4-6")
        );
    }

    #[test]
    fn nvidia_nim_preset_matches_user_intent() {
        let p = Provider::NvidiaNim;
        assert_eq!(
            p.preset_model(InferenceCategory::Dialogue),
            Some("nvidia/nemotron-3-super-120b-a12b")
        );
        assert_eq!(
            p.preset_model(InferenceCategory::Simulation),
            Some("nvidia/nemotron-3-nano-30b-a3b")
        );
        assert_eq!(
            p.preset_model(InferenceCategory::Intent),
            Some("nvidia/nvidia-nemotron-nano-9b-v2")
        );
        assert_eq!(
            p.preset_model(InferenceCategory::Reaction),
            Some("nvidia/nemotron-3-nano-30b-a3b")
        );
    }

    #[test]
    fn preset_model_indexes_correctly() {
        let p = Provider::Ollama;
        assert_eq!(
            p.preset_model(InferenceCategory::Dialogue),
            Some("qwen3:32b")
        );
        assert_eq!(p.preset_model(InferenceCategory::Intent), Some("qwen3:4b"));
    }
}
