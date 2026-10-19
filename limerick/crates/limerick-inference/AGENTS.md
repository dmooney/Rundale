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
- **`desktop` (default) / `mobile` features.** `desktop` owns `setup` (limerick-setup), `client` (Ollama/vllm process handles), and `hf_downloader` (hf-hub). The workspace dependency sets `default-features = false`, so a member that needs those modules enables `limerick-inference/desktop` (limerick-core's `desktop` feature does). `limerick-npc` and `limerick-input` must stay on the portable surface.
- **Latency instrumentation is per-category** (intent, dialogue, reaction, sim). Changes require updating the eval scaffolding.
- **`limerick-inference` is split into transport (`limerick-providers`) + scheduling (`limerick-inference`).** The provider HTTP clients (`openai_client/`, `anthropic_client/`), `simulator.rs`, `mock_client.rs`, `any_client.rs` (`AnyClient`, `build_client`, `TOKEN_CHANNEL_CAPACITY`, `StreamStats`), `rate_limit.rs` (`InferenceRateLimiter` + `governor`), `client_base.rs`, and `utf8_stream.rs` live in `limerick-providers`; queue/worker/timeout/validate/file_log/setup/hf_downloader/Ollama-`client.rs` stay in `limerick-inference`. `limerick-inference` re-exports EVERY moved symbol (incl. the whole `any_client`/`openai_client`/`simulator`/… modules) at the unchanged `limerick_inference::*` path, so downstream (`limerick-core`'s `pub use limerick_inference as inference`, npc, input, tauri, engine) needs zero import changes. Gotchas when touching this: (1) `limerick-inference` STILL needs a direct `reqwest` dep (json feature) — `validate.rs`/`hf_downloader.rs`/`setup/` make their own HTTP calls, only the `stream` feature + streaming clients are exclusive to `limerick-providers`; (2) `build_client_or_fallback` (openai_client) and `StreamStats` (any_client) are `pub` (not `pub(crate)`) precisely because `limerick-inference::setup`/`worker` construct them cross-crate — don't re-narrow them; (3) `limerick-providers` must never depend on `limerick-inference` (cycle) and is in the `BACKEND_AGNOSTIC` arch-fitness list. The `--script` headless harness (`GameTestHarness::build_with_mod`) does NOT enable the simulator fallback and reports provider as `ollama`; drive deterministic NPC dialogue proofs with `/stub <name>: <text>` (→ `npc_response` with real text), not by expecting a `provider:simulator` line.

## Module map

`queue.rs` priority lanes + request/response, `worker.rs` dispatch loop, `timeout.rs` submit/await, `validate.rs` provider reachability/auth probes, `client.rs` Ollama REST + setup re-exports, `hf_downloader.rs` model download, `file_log.rs`+`logs.rs` inference logging, `secret_scrub.rs` redaction. Transport (`openai_client/`, `anthropic_client/`, `AnyClient`, `simulator`, `mock_client`, `rate_limit`, `client_base`, `utf8_stream`) lives in [`limerick-providers`](../limerick-providers/), re-exported from `lib.rs`.
