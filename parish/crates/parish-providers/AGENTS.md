# parish-providers — agent scope

LLM provider transport for Parish: concrete HTTP clients for OpenAI-compatible and Anthropic Messages API endpoints, offline bigram-Markov simulator, deterministic scriptable mock, unified `AnyClient` dispatch enum, `InferenceClients` per-category routing, and outbound GCRA rate limiting. Backend-agnostic leaf crate — extracted from `parish-inference` in #1392; `reqwest` and `governor` are confined here so no other shared crate pulls an HTTP stack. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-providers                   # unit + integration tests (rate limiter, AnyClient, simulator JSON routing)
cargo test -p parish-providers -- --nocapture    # with stdout for debugging
```

## Gotchas

- **`build_client` over direct construction.** Always call `build_client(provider, base_url, api_key, inference_config)` rather than constructing `OpenAiClient` or `AnthropicClient` directly — `Provider::Anthropic` must reach `AnthropicClient` (endpoint `/v1/messages`); routing it through `OpenAiClient` silently returns 404 (`/v1/chat/completions` not found on Anthropic).
- **GitHub Models needs a custom completions path.** `build_client` special-cases `provider.id() == "github_models"` to strip the `/v1` prefix from the path — other providers include it automatically.
- **Anthropic auth and schema differ completely.** `AnthropicClient` sends `x-api-key` + `anthropic-version`, passes `system` as a top-level field, requires `max_tokens`, and uses named SSE events (`content_block_delta`, `message_stop`). None of this is compatible with the OpenAI wire format.
- **Simulator must stream JSON when the prompt asks for it.** `generate_stream_with_format` on `AnyClient::Simulator` inspects `response_format`, the user prompt, and the system prompt for JSON markers before dispatching to the Markov path; falling through to plain text causes parse failures in every Tier 2/3 and intent-parser call (regression covered by `simulator_streams_json_when_format_or_prompt_requests_it`).
- **`TOKEN_CHANNEL_CAPACITY` (1 024) is deliberate back-pressure.** The bounded channel throttles HTTP reads from providers when the consumer is slow; do not raise it without measuring the slow-consumer OOM risk.
- **`reqwest` is isolated here.** No other backend-agnostic crate should add `reqwest` as a dependency — the architecture-fitness test enforces that leaf crates remain free of runtime-specific HTTP stacks.
- **`InferenceRateLimiter::new(0, _)` returns `None`.** A zero `per_minute` quota disables the limiter silently; `from_config(None)` also returns `None`. A zero burst is promoted to 1.

## Module map

`lib.rs` — crate root, re-exports, `SseResult` enum, `strip_json_fence` helper; `any_client.rs` — `AnyClient` dispatch enum, `InferenceClients` per-category routing (dialogue/simulation/intent/reaction), `build_client` factory, `StreamStats`, `TOKEN_CHANNEL_CAPACITY`; `openai_client/` — `OpenAiClient` + `GenerateParams` / `ResponseFormat` / `JsonSchemaSpec` (split into `mod.rs` + `sse.rs` + `wire.rs`); `anthropic_client/` — `AnthropicClient` for the native Messages API (split into `mod.rs` + `sse.rs` + `wire.rs` + `json_isolation.rs`); `simulator.rs` — offline bigram-Markov `SimulatorClient` with embedded Irish-village corpus, JSON fallback path; `mock_client.rs` — deterministic `MockClient` / `MockMatcher` for tests; `rate_limit.rs` — GCRA `InferenceRateLimiter` wrapping `governor`; `client_base.rs` (crate-private) — shared `ClientBase` HTTP state (two `reqwest::Client`s, URL, key, limiter) composed by both real clients; `utf8_stream.rs` (crate-private) — incremental UTF-8 decoder for SSE byte streams.
