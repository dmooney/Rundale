# parish-providers

LLM provider transport clients and outbound rate limiting for Parish.

## Purpose

`parish-providers` owns the **transport** half of the inference pipeline: the
concrete HTTP clients for each provider, the offline test backends, the unified
dispatch enum, and the outbound rate limiter. It was split out of
`parish-inference` (which keeps the **scheduling** half — queue, worker,
priority lanes, timeout, validation, file logging) so the transport surface can
evolve independently. `parish-inference` depends on this crate and re-exports
every public symbol below at its former `parish_inference::*` path, so no
downstream consumer changes an import.

## Key modules

- `openai_client/` — OpenAI-compatible chat-completions client (Ollama,
  LM Studio, OpenRouter, vllm, …) with SSE streaming. Owns the public
  parameter types `GenerateParams`, `ResponseFormat`, `JsonSchemaSpec` and the
  hardened `build_client_or_fallback` reqwest builder.
- `anthropic_client/` — native Anthropic Messages API client (`/v1/messages`).
- `simulator.rs` — built-in offline "GPT-0" bigram-Markov text generator for
  tests and air-gapped runs. No network, no GPU.
- `mock_client.rs` — scriptable deterministic mock (no socket) for pinning the
  inference seam in tests.
- `any_client.rs` — `AnyClient` dispatch enum, `InferenceClients` routing, the
  `build_client` factory, and `TOKEN_CHANNEL_CAPACITY`.
- `rate_limit.rs` — `InferenceRateLimiter` (GCRA token bucket via `governor`)
  gating outbound requests per provider client.
- `client_base.rs` / `utf8_stream.rs` — shared HTTP client state and the
  incremental UTF-8 stream decoder used by both HTTP clients.

## Notes

- Backend-agnostic: must not depend on `tauri`/`axum`/`tower*`/`wry`/`tao`
  (enforced by `parish-core`'s `architecture_fitness` test).
- `reqwest` (json + stream features) and `governor` are contained here; the
  streaming HTTP stack lives nowhere else in the backend-agnostic crates.
- Must never depend on `parish-inference` — the dependency direction is strictly
  `parish-inference → parish-providers`.
