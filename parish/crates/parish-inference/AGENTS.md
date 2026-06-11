# parish-inference — agent scope

LLM inference **scheduling**: queue, priority lanes, worker, timeout, provider validation, file logging, and bootstrap/setup. The **transport** half — the provider HTTP clients (OpenAI-compat, Anthropic), simulator/mock backends, `AnyClient` dispatch, and outbound rate limiter — lives in the sibling [`parish-providers`](../parish-providers/) crate; this crate depends on it and re-exports every moved symbol at its former `parish_inference::*` path (so downstream consumers are unchanged). Backend-agnostic. See root [`AGENTS.md`](../../../AGENTS.md) and inference gotchas in [`docs/agent/gotchas.md`](../../../docs/agent/gotchas.md).

## Scoped commands

```sh
cargo test -p parish-inference                       # unit (uses simulator)
cargo test -p parish-inference --test '*'            # integration (may hit local provider)
```

## Local gotchas

- **Local-inference defaults are platform-specific.** macOS: vllm-mlx two-slot Qwen (8000 dialogue + 8001 intent), needs ≥16 GB unified memory. Linux/Windows: Ollama on 11434. Below 16 GB on macOS prefer BYOK cloud — small-slot-only fallback scores 2.96/5 Opus-blind. `Provider::recommended_for_platform()` returns the right pick.
- **Always set explicit reqwest timeouts.** Provider hangs leak into the game loop otherwise.
- **`#[serde(default)]` on optional response fields.** Providers omit fields inconsistently between releases.
- **Provider-specific transport code lives in `parish-providers`** (`openai_client/`, `anthropic_client/`, `simulator.rs`, `mock_client.rs`, `any_client.rs`, `rate_limit.rs`). Never let provider types leak into `parish-core`; `parish-inference` re-exports them under the unchanged `parish_inference::*` paths.
- **The simulator (`parish_providers::simulator`) is the deterministic test backend** — use it instead of mocking HTTP. The Ollama REST client (`client.rs`) and reachability/auth probes (`validate.rs`) stay here and keep their own `reqwest` (json feature) calls.
- **Latency instrumentation** is per-category (intent, dialogue, reaction, sim). Touching it requires updating the eval scaffolding too.

## Module map

`queue.rs` priority lanes + request/response, `worker.rs` dispatch loop, `timeout.rs` submit/await, `validate.rs` provider reachability/auth probes, `client.rs` Ollama REST + setup re-exports, `setup/` worker wiring + bootstrap, `hf_downloader.rs` model download, `file_log.rs`+`logs.rs` inference logging, `secret_scrub.rs` redaction. Transport (HTTP clients, `AnyClient`, `simulator`, `mock_client`, `rate_limit`, `client_base`, `utf8_stream`) now lives in [`parish-providers`](../parish-providers/) and is re-exported from `lib.rs`.
