# Tech Debt Phase 1: parish-inference TD-011 + TD-023

## Changes

### TD-023: Move OllamaProcess from client.rs to setup.rs

**Rationale:** `OllamaProcess` manages an Ollama subprocess lifecycle (start/stop)
and is setup orchestration, not API client logic. Moved to `setup.rs` where it
logically belongs alongside `OllamaSetup`, `setup_ollama_with_config`, etc.

**Files changed:**
- `parish/crates/parish-inference/src/setup.rs`:
  - Removed `use crate::client::OllamaProcess;` import
  - Added `Child` to `std::process` import
  - Added `OllamaProcess` struct, `impl OllamaProcess`, and `impl Drop for OllamaProcess`
    (verbatim from `client.rs`) before `OllamaSetup`
- `parish/crates/parish-inference/src/client.rs`:
  - Updated module doc: "HTTP client for the Ollama REST API and OllamaProcess server lifecycle"
    -> "HTTP client for the Ollama REST API" with cross-reference to `crate::setup`
  - Removed `OllamaProcess` struct, impl, Drop impl (~120 lines)
  - Removed unused `std::process::{Child, Command}` import
  - Added `pub use crate::setup::OllamaProcess;` for backward compatibility

**API compatibility:** The `pub use` re-export ensures all existing paths
(`parish_core::inference::client::OllamaProcess`) continue to work. Verified by
building `parish`, `parish-core`, `parish-server`, and `parish-tauri`.

### TD-011: Integration test for rate-limited generate()

**Rationale:** Added a wiremock-based integration test that verifies the full
`OpenAiClient::generate()` call path blocks when the rate limiter is exhausted.
The existing `test_acquire_slot_blocks_when_limiter_exhausted` only tested the
private `acquire_slot()` method.

**Files changed:**
- `parish/crates/parish-inference/src/openai_client.rs`:
  - Added `test_generate_blocks_when_rate_limiter_exhausted` tokio test
  - Uses wiremock to mock `/v1/chat/completions`
  - Rate limiter: 600/min, burst 1
  - First `generate()` call consumes burst and succeeds via mock
  - Second `generate()` call must wait >50ms for rate-limit refill
  - Asserts second call elapsed > first call elapsed

## Commands run

```sh
cargo test -p parish-inference       # 251 passed, 7 ignored
cargo clippy -p parish-inference --all-targets -- -D warnings  # clean
cargo fmt -p parish-inference        # no changes
cargo check -p parish -p parish-server -p parish-tauri -p parish-core  # all compile
```

## Test results

All 251 tests pass (215 unit + 36 integration), 0 failures, 7 ignored (live Ollama tests).
