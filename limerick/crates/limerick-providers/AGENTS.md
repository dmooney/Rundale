# limerick-providers — agent scope

LLM provider transport for Limerick: concrete HTTP clients for OpenAI-compatible and Anthropic Messages API endpoints, offline bigram-Markov simulator, deterministic scriptable mock, unified `AnyClient` dispatch enum, `InferenceClients` per-category routing, and outbound GCRA rate limiting. Backend-agnostic leaf crate — extracted from `limerick-inference` in #1392; `reqwest` and `governor` are confined here so no other shared crate pulls an HTTP stack. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p limerick-providers                   # unit + integration tests (rate limiter, AnyClient, simulator JSON routing)
cargo test -p limerick-providers -- --nocapture    # with stdout for debugging
```

## Gotchas

- **`build_client` over direct construction.** Always call `build_client(provider, base_url, api_key, inference_config)` rather than constructing `OpenAiClient` or `AnthropicClient` directly — `Provider::Anthropic` must reach `AnthropicClient` (endpoint `/v1/messages`); routing it through `OpenAiClient` silently returns 404 (`/v1/chat/completions` not found on Anthropic).
- **GitHub Models needs a custom completions path.** `build_client` special-cases `provider.id() == "github_models"` to strip the `/v1` prefix from the path — other providers include it automatically.
- **Anthropic auth and schema differ completely.** `AnthropicClient` sends `x-api-key` + `anthropic-version`, passes `system` as a top-level field, requires `max_tokens`, and uses named SSE events (`content_block_delta`, `message_stop`). None of this is compatible with the OpenAI wire format.
- **Simulator must stream JSON when the prompt asks for it.** `generate_stream_with_format` on `AnyClient::Simulator` inspects `response_format`, the user prompt, and the system prompt for JSON markers before dispatching to the Markov path; falling through to plain text causes parse failures in every Tier 2/3 and intent-parser call (regression covered by `simulator_streams_json_when_format_or_prompt_requests_it`).
- **`TOKEN_CHANNEL_CAPACITY` (1 024) is deliberate back-pressure.** The bounded channel throttles HTTP reads from providers when the consumer is slow; do not raise it without measuring the slow-consumer OOM risk.
- **`reqwest` is isolated here.** No other backend-agnostic crate should add `reqwest` as a dependency — the architecture-fitness test enforces that leaf crates remain free of runtime-specific HTTP stacks.
- **`InferenceRateLimiter::new(0, _)` returns `None`.** A zero `per_minute` quota disables the limiter silently; `from_config(None)` also returns `None`. A zero burst is promoted to 1.
- **Reasoning controls are provider-native, not universally OpenAI-compatible.** OpenRouter expects `reasoning.effort`; first-party DeepSeek V4 expects `thinking.type` plus top-level `reasoning_effort` (`high`/`max`); Google's OpenAI shim expects top-level `reasoning_effort`, and Gemini 3.6/3.5 Lite require deprecated sampling fields such as `temperature` to be omitted.
- **Cloud-provider HTTP 429/5xx are retried with capped backoff in `limerick-providers/src/retry.rs` (#1366 §3).** All four send sites (OpenAI/Anthropic, non-streaming + pre-stream) route through `send_with_retry`; never add a retry mid-SSE-stream, and don't re-acquire the GCRA limiter slot per attempt. Other 4xx are never retried.
- **Gemini `reasoning.effort=low` is not a small fixed token budget.** Gemini 3.5 Flash Lite used 733–1,009 hidden reasoning tokens on ordinary dialogue calls; with `max_tokens=768`, OpenRouter correctly returned `finish_reason=length` / native `MAX_TOKENS`. Give mandatory-reasoning profiles measured completion headroom and reject non-`stop` stream termination.
- **OpenRouter streaming usage is the cost source of truth.** The final SSE usage object includes the actual routed-provider charge, which can differ from the catalog floor (especially for `:nitro`); preserve it instead of substituting a static model-price table.
- **Native Gemini Standard tier is requested by omitting `service_tier`, not by sending `"standard"`.** Keep Standard as the checked-in default; an explicit Priority override must remain observable and must not silently retry in another tier.
- **Gemini v1 Interactions streaming ends with `data: [DONE]` after `interaction.completed`.** Treat the sentinel as SSE framing rather than malformed JSON, but never accept it in place of the required terminal interaction event; mock both shapes and retain live-wire proof.
- **Gemini 3.7 native thinking has two configuration seams.** OpenAI-compatible `reasoning_effort` lives in dialogue generation config, but Google's native Interactions client consumes the resolved `InferenceProfile.thinking_level`. `gemini-3.7-flash` rejects `minimal` (only Low/Medium/High are accepted), so qualification must write a native user-profile override and production must clamp that model's capability floor to Low.

## Module map

`any_client.rs` holds `AnyClient`, `build_client`, and per-category routing; each provider client has its own module; list `src/` for the rest.
