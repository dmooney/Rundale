# parish-inference — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-003 | Duplication | P2 | `src/openai_client.rs:47-59`, `src/anthropic_client.rs:51-62` | Both client structs share identical field layout, constructors, rate-limiter builder methods. Extract shared base struct or builder trait. |
| TD-004 | Duplication | P2 | `src/openai_client.rs` | `generate_stream` and `generate_stream_json` share ~30 lines of identical streaming-loop boilerplate. |
| TD-011 | Weak Tests | P2 | `src/rate_limit.rs` | No integration test verifies `generate()` blocks when limiter exhausted. Unit tests cover mechanism. |
| TD-015 | Weak Tests | P3 | `src/client.rs:362-370` | `OllamaProcess::stop` Windows `taskkill` has zero coverage. Needs Command mock abstraction. |
| TD-020 | Complexity | P2 | `src/lib.rs:642-787` | `spawn_inference_worker` ~145 lines with repeated timeout+error-construction logic. |
| TD-021 | Complexity | P2 | `src/anthropic_client.rs:335-396` | Hand-rolled byte-level XML tag parser. |
| TD-023 | Duplication | P2 | `src/client.rs:268-388` | `OllamaProcess` belongs in `setup.rs`, not `client.rs`. |

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
| TD-022 | 2026-05-07 | Refactored `select_model_for_vram` to table-driven lookup with `ModelTier` static slice.
