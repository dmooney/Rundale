# parish-inference — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Config/Cargo | P2 | `Cargo.toml:16` | `anyhow` dependency declared but never imported or used anywhere in this crate (verified via grep: zero matches for `use anyhow` or `anyhow::`). |
| TD-002 | Config/Cargo | P2 | `Cargo.toml:26` | `tracing-test` dev-dependency declared but never imported or used in any test module (zero matches for `tracing_test` or `traced_test`). |
| TD-003 | Duplication | P2 | `src/openai_client.rs:47-59`, `src/anthropic_client.rs:51-62` | Both client structs share identical field layout (`client`, `streaming_client`, `base_url`, `api_key`, `rate_limiter`), identical constructors (`new`, `new_with_config`), identical rate-limiter builder methods (`with_rate_limit`, `maybe_with_rate_limit`, `has_rate_limiter`), and identical `acquire_slot`/`base_url` accessors. Only auth header format and request/response schema differ. Extract a shared base struct or builder trait. |
| TD-004 | Duplication | P2 | `src/openai_client.rs:281-312`, `src/openai_client.rs:346-370` | `generate_stream` and `generate_stream_json` contain ~30 lines of identical streaming-loop boilerplate (UTF-8 decoder init, chunk reading, line buffering, newline splitting, `process_sse_line` dispatch, trailing flush). The only difference is `build_request(…, json_mode: false)` vs `build_request(…, json_mode: true)`. |
| TD-005 | Duplication | P2 | `src/openai_client.rs:476-480`, `src/anthropic_client.rs:536-543` | `SseResult` enum is defined independently in both client modules: OpenAI version has 2 variants (`Continue`, `Done`), Anthropic version has 3 (`Continue`, `Done`, `Error(String)`). Merge into a single shared definition with an `Error` variant. |
| TD-006 | Duplication | P2 | `src/openai_client.rs:378-396`, `src/anthropic_client.rs:246-283` | `strip_json_fence` exists only in `anthropic_client.rs`; `OpenAiClient::generate_json` does not strip Markdown code fences (` ```json...``` `). OpenAI-compatible providers also occasionally wrap JSON in fences, so this protection gap can cause parse failures. |
| TD-007 | Duplication | P2 | `src/setup.rs:1126-1129` | `warmup_model_with_config` manually builds a `reqwest::Client` via `Client::builder().timeout(…)` instead of using `crate::openai_client::build_client_or_fallback`. If TLS backend initialization fails, this path panics or errors unconditionally while all other client construction paths degrade gracefully (issue #98). |
| TD-008 | Weak Tests | P1 | `tests/http_mock_tests.rs` (missing), `src/anthropic_client.rs:246-283` | No wiremock test for `AnthropicClient::generate_json`. The method has its own XML-isolated system prompt assembly, JSON fence stripping, and a retry-on-parse-failure path — zero of this is exercised by an HTTP-mocked integration test. |
| TD-009 | Weak Tests | P1 | `tests/http_mock_tests.rs` (missing), `src/anthropic_client.rs:445-464` | No wiremock test for `AnthropicClient::generate_stream_json`. This method delegates to `generate_stream` passing `isolate_system_for_json` output — the XML-isolation contract on the streaming path has unit tests but no end-to-end SSE integration test. |
| TD-010 | Weak Tests | P1 | `src/anthropic_client.rs:263-282` | No unit or integration test for the `generate_json` retry-on-parse-failure path. The fallback to `temperature = 0.3` and the `ParishError::InferenceJsonParseFailed` error variant are unreachable by any existing test. |
| TD-011 | Weak Tests | P2 | `src/rate_limit.rs:64-66`, `src/openai_client.rs:241` | Rate limiter `acquire()` is only tested in isolation. No integration test verifies that `generate()` calls actually block when the limiter is exhausted (e.g. with a mock HTTP server and a low quota). |
| TD-012 | Weak Tests | P2 | `src/anthropic_client.rs:179-200`, `tests/http_mock_tests.rs:643-661` | `send_request` on non-2xx response reads the error body and extracts `{"error":{"message":"…"}}`. The wiremock test for 401 only checks the error message contains "401" — it does not verify extraction of the structured Anthropic error payload from the response body. |
| TD-013 | Weak Tests | P3 | `src/utf8_stream.rs:60-67` | No test for a chunk containing a valid multi-byte character followed by genuinely invalid bytes (e.g. `[0xC3, 0xA9, 0xFF, 0x80]`). The loop branches on `error_len()` being `Some` but this combo is never exercised. |
| TD-014 | Weak Tests | P3 | `src/lib.rs:439-441`, `src/lib.rs:1046-1113` | `InferenceClients::reaction_client()` has no test. Tests exist for `dialogue_client`, `simulation_client`, and `intent_client` override + fallback paths, but the `Reaction` category is untested. |
| TD-015 | Weak Tests | P3 | `src/client.rs:362-370` | `OllamaProcess::stop` Windows-specific `taskkill` logic is cfg-gated and has zero test coverage on any platform. A mock abstraction around `Command` would be needed. |
| TD-016 | Stale Docs | P3 | `src/lib.rs:1-5` | Module doc says "LLM inference pipeline for OpenAI-compatible providers" — this crate now also handles Anthropic's native Messages API and the offline Simulator. Doc is stale. |
| TD-017 | Stale Docs | P3 | `src/client.rs:1-2` | Module doc says "HTTP client for the Ollama REST API at localhost:11434" — this module also defines `OllamaProcess` for server lifecycle management. Doc is incomplete. |
| TD-018 | Config/Cargo | P2 | `src/inference_client.rs:31-37`, `Cargo.toml` | Feature flags `inference-client-trait` and `inference-response-cache` are documented in module-level docs with behavior descriptions and default values, but neither flag is declared in `Cargo.toml` `[features]`. |
| TD-019 | Stale Docs | P3 | `src/openai_client.rs:252-257` | `generate_stream` doc says "Uses a 5-minute timeout" but the timeout is sourced from `InferenceConfig::streaming_timeout_secs` (configurable, default 300s). Doc is stale. |
| TD-020 | Complexity | P2 | `src/lib.rs:642-787` | `spawn_inference_worker` is ~145 lines with a deeply nested `match (token_tx, json_mode)` containing 3 arms each with identical `tokio::time::timeout` wrapping near-identical error formatting. The timeout+error construction logic is repeated 3 times. |
| TD-021 | Complexity | P2 | `src/anthropic_client.rs:335-396` | `neutralise_structural_tags` and `match_structural_close_at` form a hand-rolled byte-level XML tag parser (~60 lines) with early returns, loops over `STRUCTURAL_TAGS`, inline `strip_ascii_ws` calls, and `eq_ignore_ascii_case` iterator chains. Hard to audit and easy to introduce off-by-one errors. |
| TD-022 | Complexity | P2 | `src/setup.rs:578-604` | `select_model_for_vram` is a 4-level deep `if/else if/else` chain with hardcoded magic numbers (25_000, 17_000, 11_000) and inline model config construction. Refactor to a table-driven lookup. |
| TD-023 | Duplication | P2 | `src/client.rs:268-388` | `OllamaProcess` (server lifecycle management) is defined in `client.rs` (the Ollama REST HTTP client module). It belongs in `setup.rs` alongside the other Ollama bootstrap logic (`check_ollama_installed`, `install_ollama`, `setup_ollama`). |

## In Progress

*(none)*

## Done

*(none)*
