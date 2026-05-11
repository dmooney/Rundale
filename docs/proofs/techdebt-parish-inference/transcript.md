Evidence type: gameplay transcript

## Changes Summary

Resolved 17 items from `parish/crates/parish-inference/TODO.md`:

### Config/Cargo
- TD-001: Removed unused `anyhow` dependency.
- TD-002: Removed unused `tracing-test` dev-dependency.
- TD-018: Declared `inference-client-trait` and `inference-response-cache` feature flags in `Cargo.toml`.

### Duplication Removed
- TD-005: Unified `SseResult` enum into `lib.rs` with `Error(String)` variant.
- TD-006: Moved `strip_json_fence` to `lib.rs`; OpenAI's `generate_json` now strips fences.
- TD-007: `warmup_model_with_config` uses `build_client_or_fallback` instead of manual reqwest builder.

### Weak Tests Added
- TD-008: Wiremock tests for `AnthropicClient::generate_json` (typed payload + fenced payload).
- TD-009: Wiremock test for `AnthropicClient::generate_stream_json`.
- TD-010: Wiremock test for retry-on-parse-failure path with `expect(2)`.
- TD-012: Wiremock test for structured Anthropic error body extraction.
- TD-013: UTF-8 decoder test for valid multibyte followed by invalid bytes.
- TD-014: `reaction_client()` fallback and override tests.

### Stale Docs Fixed
- TD-016: Updated `lib.rs` module doc for Anthropic + Simulator.
- TD-017: Updated `client.rs` module doc for `OllamaProcess`.
- TD-019: Updated `generate_stream` timeout doc to reference config.

### Complexity Reduced
- TD-022: `select_model_for_vram` refactored to table-driven lookup with `ModelTier` static slice.

## Test Output

```
cargo test -p parish-inference: 214 passed (unit) + 36 passed (integration) + 0 doc-tests
cargo clippy -p parish-inference -- -D warnings: clean
cargo fmt --check: clean
```

## Phase 2 — TD-024 through TD-030

### Dead Code Removal
- TD-024: Deleted dead `OllamaClient`, `GenerateRequest`, `GenerateResponse` and their tests from `client.rs` and `tests/http_mock_tests.rs`. Collapsed `client.rs` to the single `pub use crate::setup::OllamaProcess` re-export.
- TD-026: Removed stale `OllamaClient` doc comment about 30-second timeout (code deleted in TD-024).

### Config Cleanup
- TD-025: Removed unused `inference-client-trait` and `inference-response-cache` feature flags from `Cargo.toml`; updated module docs in `inference_client.rs` and `parish-server/src/state.rs`.

### Documentation
- TD-027: Updated `README.md` key modules list to include `anthropic_client`, `inference_client`, `utf8_stream` and corrected `setup` description.

### Weak Tests Added
- TD-028: Added four unit tests for `submit_json` async helper in `lib.rs` (success path, error path, JSON parse failure, missing response channel).

### Bug Fix
- TD-029: Fixed OpenAI trailing-buffer SSE error propagation in `read_sse_stream` to match Anthropic pattern — `SseResult::Error` now returns `Err(ParishError::Inference(msg))` instead of being silently dropped.

### Visibility Hardening
- TD-030: Tightened `ChatCompletionChunk`, `StreamChoice`, and `Delta` visibility from `pub(crate)` to private in `openai_client.rs`.

## Test Output

```
cargo test -p parish-inference: 233 passed (unit) + 6 ignored + 0 doc-tests
cargo clippy -p parish-inference --all-targets -- -D warnings: clean
cargo fmt --check: clean
```

## Residual Items
- All 30 TODO items resolved. No remaining open debt in parish-inference.
