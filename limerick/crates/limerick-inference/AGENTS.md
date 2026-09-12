# limerick-inference — agent scope

LLM inference **scheduling** only: queue, priority lanes, worker, timeout, provider validation, and file logging. Bootstrap/setup (GPU detect, model select, Ollama/vllm process management) lives in [`limerick-setup`](../limerick-setup/), re-exported here as `limerick_inference::setup`. Transport (provider HTTP clients, simulator/mock, `AnyClient`, rate limiter) lives in [`limerick-providers`](../limerick-providers/) — every moved symbol is re-exported at its former `limerick_inference::*` path. Backend-agnostic. See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/gotchas.md`](../../../docs/agent/gotchas.md).

## Scoped commands

```sh
cargo test -p limerick-inference                       # unit (uses simulator)
cargo test -p limerick-inference --test '*'            # integration (may hit local provider)
```

## Local gotchas

- **Platform backend selection is not a quality claim.** `Provider::recommended_for_platform()` selects a runnable local backend. Check `limerick_config::local_dialogue` before calling an exact dialogue profile qualified; the registry is currently empty, so setup recommends BYOK and labels local choices experimental.
- **Always set explicit reqwest timeouts.** Provider hangs leak into the game loop.
- **`#[serde(default)]` on optional response fields.** Providers omit fields inconsistently between releases.
- **Transport code belongs in `limerick-providers`.** Never let provider types leak into `limerick-core`; re-exports keep the `limerick_inference::*` paths stable.
- **Use the simulator for tests.** `limerick_providers::simulator` is the deterministic test backend — prefer it over mocking HTTP. Ollama REST client (`client.rs`) and auth probes (`validate.rs`) stay in this crate.
- **Latency instrumentation is per-category** (intent, dialogue, reaction, sim). Changes require updating the eval scaffolding.

## Module map

`queue.rs` priority lanes + request/response, `worker.rs` dispatch loop, `timeout.rs` submit/await, `validate.rs` provider reachability/auth probes, `client.rs` Ollama REST + setup re-exports, `hf_downloader.rs` model download, `file_log.rs`+`logs.rs` inference logging, `secret_scrub.rs` redaction. Transport (`openai_client/`, `anthropic_client/`, `AnyClient`, `simulator`, `mock_client`, `rate_limit`, `client_base`, `utf8_stream`) lives in [`limerick-providers`](../limerick-providers/), re-exported from `lib.rs`.
