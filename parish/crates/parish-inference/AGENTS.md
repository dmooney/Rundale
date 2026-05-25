# parish-inference — agent scope

LLM inference queue + provider clients (OpenAI-compat, Anthropic, Ollama, vllm-mlx). Backend-agnostic. See root [`AGENTS.md`](../../../AGENTS.md) and inference gotchas in [`docs/agent/gotchas.md`](../../../docs/agent/gotchas.md).

## Scoped commands

```sh
cargo test -p parish-inference                       # unit (uses simulator)
cargo test -p parish-inference --test '*'            # integration (may hit local provider)
```

## Local gotchas

- **Local-inference defaults are platform-specific.** macOS: vllm-mlx two-slot Qwen (8000 dialogue + 8001 intent), needs ≥16 GB unified memory. Linux/Windows: Ollama on 11434. Below 16 GB on macOS prefer BYOK cloud — small-slot-only fallback scores 2.96/5 Opus-blind. `Provider::recommended_for_platform()` returns the right pick.
- **Always set explicit reqwest timeouts.** Provider hangs leak into the game loop otherwise.
- **`#[serde(default)]` on optional response fields.** Providers omit fields inconsistently between releases.
- **`InferenceClient` trait + LRU cache** is the seam — keep provider-specific code in `openai_client/`, `anthropic_client/`, `client/` (Ollama). Never let provider types leak into `parish-core`.
- **`simulator/` is the deterministic test backend** — use it instead of mocking HTTP.
- **Latency instrumentation** is per-category (intent, dialogue, reaction, sim). Touching it requires updating the eval scaffolding too.

## Module map

`openai_client/`+`anthropic_client/`+`client/` HTTP, `inference_client/` trait+cache+metrics, `rate_limit/`, `setup/` worker wiring, `simulator/` test client, `utf8_stream/` streaming decoder.
