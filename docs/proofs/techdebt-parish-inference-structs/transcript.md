# Techdebt: parish-inference struct/streaming dedup (TD-003 + TD-004)

## What was changed

Two items from the `parish-inference/TODO.md` tech-debt backlog:

### TD-003 — Extracted `ClientBase` shared struct

**Before:** `OpenAiClient` and `AnthropicClient` each declared 5 identical fields
(`client`, `streaming_client`, `base_url`, `api_key`, `rate_limiter`) with
identical constructors (`new`, `new_with_config`) and identical builder methods
(`with_rate_limit`, `maybe_with_rate_limit`, `has_rate_limiter`, `base_url`,
`acquire_slot`).

**After:** Created `src/client_base.rs` with a `ClientBase` struct holding all 5
shared fields and their builder methods. Both client structs now embed
`base: ClientBase` and delegate the common methods. The `new_with_config`
constructor on each client is a one-liner that calls `ClientBase::new`.

Files changed:
- `parish/crates/parish-inference/src/client_base.rs` — new module
- `parish/crates/parish-inference/src/lib.rs` — register `client_base` module
- `parish/crates/parish-inference/src/openai_client.rs` — embed `ClientBase`,
  delegate constructors/builder methods, update internal field accesses
- `parish/crates/parish-inference/src/anthropic_client.rs` — same pattern,
  removed now-unused `use std::time::Duration`

### TD-004 — Extracted streaming-loop helper

**Before:** `OpenAiClient::generate_stream` and `generate_stream_json` shared
~30 lines of identical streaming-loop boilerplate (HTTP POST, SSE parsing,
token forwarding, accumulated text assembly). Only the `json_mode` boolean
differed.

**After:** Extracted the loop into a free function `read_sse_stream` and a
method `OpenAiClient::stream_response`. Both `generate_stream` and
`generate_stream_json` now call `self.stream_response(body, token_tx).await`,
delegating the HTTP request + SSE parsing to the shared path.

File changed:
- `parish/crates/parish-inference/src/openai_client.rs` — added
  `read_sse_stream` free function and `stream_response` method, slimmed
  `generate_stream`/`generate_stream_json` to ~3 lines each

## Commands run

```sh
cargo clippy -p parish-inference --all-targets -- -D warnings   # clean
cargo test -p parish-inference                                   # 251 passed
cargo fmt -p parish-inference                                    # clean
```

## Test results

- 215 unit tests passed, 7 ignored (live Ollama/Anthropic tests)
- 36 integration tests (HTTP mock) passed, 0 ignored
- 0 doc test
- 0 failures

## Files changed

- `parish/crates/parish-inference/src/client_base.rs` (new)
- `parish/crates/parish-inference/src/lib.rs`
- `parish/crates/parish-inference/src/openai_client.rs`
- `parish/crates/parish-inference/src/anthropic_client.rs`
- `parish/crates/parish-inference/TODO.md`
