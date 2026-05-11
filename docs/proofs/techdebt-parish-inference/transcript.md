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

## Residual Items
- TD-003, TD-004, TD-011, TD-015, TD-020, TD-021, TD-023: Left open for follow-up (risky refactors or require external abstractions).
