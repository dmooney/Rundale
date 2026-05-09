# parish-inference — Technical Debt

## Open

| ID | Date | Summary |
|----|------|---------|
| TD-024 | 2026-05-09 | `OllamaClient` + `GenerateRequest` + `GenerateResponse` in `src/client.rs:22-265` are dead production code: `rg` confirms zero call sites outside `parish-inference` (only the crate's own unit tests + `tests/http_mock_tests.rs:35-242` reference it). The only re-export the codebase still consumes is `pub use crate::setup::OllamaProcess` at `src/client.rs:14`. Delete `OllamaClient`/`GenerateRequest`/`GenerateResponse` and their unit tests; collapse `client.rs` to the single `pub use` line, or move that re-export into `lib.rs` and delete the file (also drop the `OllamaClient` test block in `tests/http_mock_tests.rs:23-275`). |
| TD-025 | 2026-05-09 | Feature flags `inference-client-trait` and `inference-response-cache` are declared in `Cargo.toml:27-28` and documented in `src/inference_client.rs:30-36` as "fall back to direct AnyClient call-site path" / "wrapper not constructed", but `rg 'cfg\s*\(\s*feature\s*='` returns zero hits across the entire crate — neither flag actually gates code. Toggling them is silently a no-op, violating CLAUDE.md non-negotiable rule #6. Either add real `#[cfg(feature = "...")]` gates around `build_inference_client_stack` in `src/inference_client.rs:494-508` (and the `CachingInferenceClient` branch on L501-505) and around the trait re-exports in `src/lib.rs:43-47`, or delete the flag declarations and update the inference_client.rs module doc table. |
| TD-026 | 2026-05-09 | Stale doc comment in `src/client.rs:18-19`: "Wraps `reqwest::Client` with a configurable base URL and 30-second default timeout." The struct now sources its timeouts from `InferenceConfig::timeout_secs` / `streaming_timeout_secs` (see `new_with_config` at `src/client.rs:73-92`), so the literal "30-second" value is wrong and misleading. Fix the doc to match `InferenceConfig`, or — if TD-024 lands first — delete the file. |
| TD-027 | 2026-05-09 | Stale README in `parish/crates/parish-inference/README.md:13-17`: the `Key modules` list omits `anthropic_client`, `inference_client`, `client_base`, `utf8_stream`, and mis-describes `setup` as "worker wiring and queue construction" when the file is actually 2418 lines of Ollama bootstrap, GPU detection, model selection, pull/warmup, and the `OllamaProcess` lifecycle (queue/worker wiring lives in `src/lib.rs`). CLAUDE.md non-negotiable rule #7 requires keeping README current. |
| TD-028 | 2026-05-09 | Missing test for the public async helper `submit_json` in `src/lib.rs:378-408`. It is called from production code (`parish/crates/parish-npc/src/ticks.rs:605` and `:949` for Tier 2/3 batch inference) so a regression here breaks NPC simulation, but the only `submit_json` reference in this crate is the definition itself — no `#[tokio::test]` exercises the queue → response → JSON-deserialize path, the `error.is_some()` branch, or the `serde_json::from_str` failure branch. Add at least three tests using a fake worker that drains `interactive_rx`/`background_rx`/`batch_rx` and replies on `response_tx`. |
| TD-029 | 2026-05-09 | `read_sse_stream` in `src/openai_client.rs:421-425` silently swallows `SseResult::Error` returned from the post-loop trailing-buffer flush — `process_sse_line(remaining, token_tx, &mut accumulated);` ignores its return value. The Anthropic streaming loop at `src/anthropic_client.rs:438-444` *does* propagate the same error variant (`if let SseResult::Error(msg) = process_sse_line(...) { return Err(...) }`), so the two providers diverge: an OpenAI provider that emits a final error-bearing `data:` chunk without a trailing newline would have its error dropped on the floor and the partial body returned as `Ok(...)`. Match the Anthropic pattern: pattern-match the result and return `Err(ParishError::Inference(msg))` on the error variant. |
| TD-030 | 2026-05-09 | `pub(crate)` SSE chunk types `ChatCompletionChunk` / `StreamChoice` / `Delta` in `src/openai_client.rs:106-125` are only ever used within their own module (`rg ChatCompletionChunk` shows them only in `src/openai_client.rs`). Their fields are also marked `pub(crate)`. Tighten visibility to private (`struct ChatCompletionChunk { ... }` with private fields) so the SSE wire format does not leak across the crate's module boundary as a load-bearing internal API. |

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
