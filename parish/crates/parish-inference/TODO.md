# parish-inference — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-031 | Complexity | P1 | `src/setup.rs:1-3150` | Largest file in the crate. It combines Ollama/vLLM/vLLM-MLX process management, GPU detection for three OS families, model selection, pull/delete progress streaming, setup orchestration, provider-client selection, and ~1,250 lines of tests. Split process management, GPU probing, model download, and provider setup before adding more local-runtime support. |
| TD-032 | Complexity | P2 | `src/lib.rs:1-2157` | Crate root still mixes exports, queue types, bounded logs, await/submit helpers, client aggregation, `AnyClient`, worker spawning, timeout helpers, and ~900 lines of tests. Move queue/log/client-worker concerns into focused modules so `lib.rs` becomes the stable facade. |
| TD-033 | API Shape | P2 | `src/lib.rs:334`, `src/lib.rs:370`, `src/lib.rs:409`, `src/lib.rs:847`, `src/lib.rs:993`, `src/inference_client.rs:376`, `src/openai_client.rs:404`, `src/openai_client.rs:434` | Several public or central constructors/request builders need `#[allow(clippy::too_many_arguments)]`. Introduce typed request/config structs for queue submission, streaming, metrics emission, and OpenAI-compatible request construction so call sites stop depending on long positional parameter lists. |
| TD-034 | Complexity | P2 | `src/anthropic_client.rs:54-543`, `src/openai_client.rs:50-659` | Provider clients still contain request schema structs, request builders, retry/stream logic, SSE parsing, response extraction, structural-tag hardening, and tests in the same module. Split wire types, builders, streaming, and safety helpers per provider to make provider drift easier to review. |
| TD-035 | Stale Comments | P3 | `src/openai_client.rs:22`, `src/openai_client.rs:675` | Historical comments mention previous `.expect()` panic behavior as regression notes. They are useful context, but they read like unfinished cleanup in debt scans. Convert to issue/test references or plain regression comments during the next adjacent edit. |

## In Progress

*(none)*

## Done

| ID | Date | Summary |
|----|------|---------|
| TD-001 | 2026-05-07 | Removed unused `anyhow` dependency from Cargo.toml. |
| TD-002 | 2026-05-07 | Removed unused `tracing-test` dev-dependency from Cargo.toml. |
| TD-005 | 2026-05-07 | Unified `SseResult` enum: moved to `lib.rs` as `pub(crate)` with `Error(String)` variant, removed duplicate definitions from both client modules. |
| TD-006 | 2026-05-07 | Moved `strip_json_fence` to `lib.rs` as `pub(crate)`; `OpenAiClient::generate_json` now strips fences before parsing. |
| TD-007 | 2026-05-07 | `warmup_model_with_config` now uses `build_client_or_fallback` instead of manual reqwest builder (graceful TLS fallback). |
| TD-008 | 2026-05-07 | Added wiremock test `anthropic_generate_json_parses_typed_payload` + `anthropic_generate_json_parses_fenced_payload` for `AnthropicClient::generate_json`. |
| TD-009 | 2026-05-07 | Added wiremock test `anthropic_generate_stream_json_parses_sse_chunks` for `AnthropicClient::generate_stream_json`. |
| TD-010 | 2026-05-07 | Added wiremock test `anthropic_generate_json_retries_on_parse_failure` for the retry-on-parse-failure path. |
| TD-012 | 2026-05-07 | Added wiremock test `anthropic_generate_maps_401_with_structured_error_body` verifying structured Anthropic error payload extraction. |
| TD-013 | 2026-05-07 | Added `valid_multibyte_then_invalid_bytes` test for UTF-8 decoder with mixed valid/invalid bytes. |
| TD-014 | 2026-05-07 | Added `test_inference_clients_reaction_falls_back_to_base` and `test_inference_clients_reaction_uses_override` tests. |
| TD-016 | 2026-05-07 | Updated `lib.rs` module doc to mention Anthropic + Simulator support. |
| TD-017 | 2026-05-07 | Updated `client.rs` module doc to mention `OllamaProcess` lifecycle management. |
| TD-018 | 2026-05-07 | Declared `inference-client-trait` and `inference-response-cache` feature flags in `Cargo.toml` `[features]`. |
| TD-019 | 2026-05-07 | Updated `generate_stream` doc to reference configurable `streaming_timeout_secs`. |
| TD-022 | 2026-05-07 | Refactored `select_model_for_vram` to table-driven lookup with `ModelTier` static slice. |
| TD-011 | 2026-05-07 | Added wiremock-based integration test `test_generate_blocks_when_rate_limiter_exhausted` in `openai_client.rs` verifying `generate()` blocks when rate limiter is exhausted. |
| TD-023 | 2026-05-07 | Moved `OllamaProcess` from `client.rs` to `setup.rs` (logical home for server lifecycle). Re-exported from `client.rs` for backward compatibility. Updated all imports. |
| TD-003 | 2026-05-07 | Extracted `ClientBase` struct from identical field layout/constructors in `OpenAiClient` and `AnthropicClient`. `src/client_base.rs` holds shared fields (`client`, `streaming_client`, `base_url`, `api_key`, `rate_limiter`) plus builder methods (`with_rate_limit`, `maybe_with_rate_limit`, `has_rate_limiter`, `acquire_slot`, `base_url`). Each client now embeds `base: ClientBase`. |
| TD-004 | 2026-05-07 | Extracted shared `read_sse_stream` free function and `OpenAiClient::stream_response` helper from the ~30-line identical streaming loop in `generate_stream` / `generate_stream_json`. |
| TD-020 | 2026-05-07 | Extracted `inference_with_timeout` helper to eliminate triple-repeated timeout+duration+error-construction pattern in `spawn_inference_worker`. Reduced function from ~146 to ~116 lines. |
| TD-021 | 2026-05-07 | Replaced hand-rolled byte-level XML tag parser (`neutralise_structural_tags`, `match_structural_close_at`, `skip_ascii_ws`) with `regex`-based replacement using `LazyLock<Regex>` compiled from `STRUCTURAL_TAGS`. Removed ~68 lines of byte-walking code. |
| TD-015 | 2026-05-08 | Extracted pure helpers `taskkill_args(pid_arg) -> [&str; 4]` and `pid_string(pid)` from `OllamaProcess::stop`. Added 2 cross-platform unit tests (`taskkill_args_are_force_tree_kill_with_pid`, `taskkill_args_handle_u32_max_pid`) that pin the `/F /T /PID <pid>` invariant without needing a Windows host or a Command mock. The `Command::new("taskkill")` invocation itself remains platform-locked but is now a thin shim around tested data. |
| TD-024 | 2026-05-11 | Deleted dead `OllamaClient`, `GenerateRequest`, `GenerateResponse` and their tests from `client.rs` and `tests/http_mock_tests.rs`. |
| TD-025 | 2026-05-11 | Removed unused `inference-client-trait` and `inference-response-cache` feature flags from `Cargo.toml`; updated docs in `inference_client.rs` and `parish-server/src/state.rs`. |
| TD-026 | 2026-05-11 | Removed stale `OllamaClient` doc comment about 30-second timeout (code deleted in TD-024). |
| TD-027 | 2026-05-11 | Updated `README.md` key modules list to include `anthropic_client`, `inference_client`, `utf8_stream` and corrected `client` description. |
| TD-028 | 2026-05-11 | Added four unit tests for `submit_json` async helper in `lib.rs`. |
| TD-029 | 2026-05-11 | Fixed OpenAI trailing-buffer SSE error propagation in `read_sse_stream` to match Anthropic pattern. |
| TD-030 | 2026-05-11 | Tightened `ChatCompletionChunk`, `StreamChoice`, and `Delta` visibility from `pub(crate)` to private in `openai_client.rs`.

## Progress Log

- **2026-05-25**: Refreshed the debt scan against current source. Reopened TD-031 through TD-035 after checking LOC hotspots, clippy allows, and inline TODO/regression comments.
