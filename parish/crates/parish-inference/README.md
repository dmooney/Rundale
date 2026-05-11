# parish-inference

LLM inference queue and provider clients for Parish.

## Purpose

`parish-inference` handles prompt execution against OpenAI-compatible backends
(Ollama, LM Studio, OpenRouter, and similar providers), including priority
lanes, optional streaming token output, and request logging.

## Key modules

- `openai_client` — HTTP client for OpenAI-compatible APIs.
- `anthropic_client` — HTTP client for Anthropic's Messages API.
- `client` — `OllamaProcess` lifecycle management.
- `inference_client` — `InferenceClient` trait, LRU cache, and metrics.
- `rate_limit` — request throttling helpers.
- `setup` — worker wiring and queue construction.
- `simulator` — deterministic/local simulation client for tests.
- `utf8_stream` — incremental UTF-8 decoder for streaming responses.

## Notes

- Keep provider-specific behavior isolated to this crate.
- Shared request/response types are consumed by `parish-core` and other crates.
