# parish-setup

Local-inference setup & bootstrap for Parish.

## Purpose

`parish-setup` owns the **provider-bootstrap** half of local inference: the
full local-provider lifecycle that turns a configured provider into a live,
ready-to-use client. It was split out of `parish-inference` (which keeps the
**scheduling** half — queue, worker, priority lanes, timeout, validation, file
logging) so the bootstrap surface can evolve independently.

The crate is backend-agnostic. It depends on
[`parish-providers`](../parish-providers/) for the `AnyClient` factory
(`build_client`), `OpenAiClient`, the `InferenceRateLimiter`, and the hardened
`build_client_or_fallback` reqwest builder, plus `parish-config` /
`parish-types` for config and error types. It does **not** depend on
`parish-inference`; instead `parish-inference` re-exports this crate at its
former `parish_inference::setup::*` path
(`pub use parish_setup as setup;`), so no downstream consumer changes an
import.

## Key modules

- `progress.rs` — `SetupProgress` trait + `StdoutProgress` impl: the callback
  surface for streaming setup progress to a CLI or GUI front-end.
- `gpu_detect.rs` — GPU vendor / VRAM detection across macOS (Apple Silicon),
  Windows, and Linux.
- `model_select.rs` — VRAM-based model tier selection (`ModelConfig`,
  `select_model`).
- `process.rs` — managed child-process handles for Ollama, vllm-mlx, and vllm
  (`OllamaProcess`, `VllmMlxProcess`, `VllmProcess`, `RuntimeProcesses`), each
  probing the server endpoint before spawning.
- `orchestration.rs` — the full setup sequence (install / start / pull /
  warmup) and the unified `setup_provider_client` entry point that resolves a
  `ProviderConfig` into `(AnyClient, RuntimeProcesses)`.

## Scoped commands

```sh
cargo test -p parish-setup       # unit tests (wiremock-backed Ollama probes)
```

## Local gotchas

- **Local-inference backends are platform-specific, but quality qualification
  is separate.** macOS can run the vllm-mlx two-slot Qwen profile (8000
  dialogue + 8001 intent); Linux/Windows can run Ollama/vLLM. Consult
  `parish_config::local_dialogue` before describing any exact profile as
  production-qualified. The registry is currently empty.
- **Always set explicit reqwest timeouts.** Provider hangs leak into the game
  loop otherwise; the reachability/warmup probes here build their own clients.
- **No `parish-inference` dependency.** The bootstrap code reaches the client
  factory and rate limiter through `parish-providers` directly, keeping the
  dependency edge one-directional (`parish-inference` → `parish-setup`, never
  the reverse) so the architecture-fitness test stays green.
