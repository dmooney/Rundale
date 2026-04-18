# parish-inference

LLM inference queue and provider clients for Parish.

## Purpose

`parish-inference` handles prompt execution against OpenAI-compatible backends
(Ollama, LM Studio, OpenRouter, and similar providers), including priority
lanes, optional streaming token output, and request logging.

## Key modules

- `openai_client` — HTTP client for OpenAI-compatible APIs.
- `client` — trait and polymorphic client interfaces.
- `rate_limit` — request throttling helpers.
- `setup` — worker wiring and queue construction.
- `simulator` — deterministic/local simulation client for tests.

## Notes

- Keep provider-specific behavior isolated to this crate.
- Shared request/response types are consumed by `parish-core` and other crates.
