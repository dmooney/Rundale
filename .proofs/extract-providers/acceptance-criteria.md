# Acceptance Criteria: extract-providers

## Task

Split `parish-inference` along a transport/scheduling boundary. The four
provider-transport modules — `anthropic_client/` (~1 494 lines),
`openai_client/` (~1 204 lines), `simulator.rs` (~538 lines), and
`any_client.rs` (~798 lines) — plus the shared `rate_limit.rs` (~187 lines)
move into a new workspace crate `parish-providers`. `parish-inference` keeps
queue/worker/priority-lanes/file_log/validate and adds a dependency on
`parish-providers`, re-exporting every moved public symbol at its former
`parish_inference::*` path. No downstream consumer (`parish-core`,
`parish-npc`, `parish-input`, `parish-tauri`, `parish-engine`) changes a
single import. This is a behaviour-preserving refactor; the game output on
the simulator provider must be bit-for-bit identical before and after.

Note: `client_base.rs`, `utf8_stream.rs`, `secret_scrub.rs`, `logs.rs`,
`hf_downloader.rs`, `mock_client.rs`, `timeout.rs`, and `setup/` remain in
`parish-inference` (or move with the clients — `client_base.rs` and
`utf8_stream.rs` must follow the HTTP clients into `parish-providers` since
they are used exclusively by `openai_client/` and `anthropic_client/`).
`mock_client.rs` depends on `simulator::intent_json_for`; it must also move
into `parish-providers` to avoid a cross-crate private-item reference, or
its `intent_json_for` dependency must be made `pub` and re-exported.

---

## Criteria

### 1. New crate `parish-providers` exists in the workspace

- `parish/crates/parish-providers/Cargo.toml` is present and the crate is
  listed under `[workspace] members` in `parish/Cargo.toml`.
- The following modules exist inside `parish-providers`:
  `anthropic_client`, `openai_client`, `simulator`, `any_client`,
  `rate_limit` (plus `client_base` and `utf8_stream` which `openai_client`
  and `anthropic_client` depend on), and `mock_client`.
- Observable via: `cargo metadata --manifest-path parish/Cargo.toml --no-deps
| jq '.packages[].name'` contains `"parish-providers"`.

### 2. `parish-inference` depends on `parish-providers`; no cycle

- `parish/crates/parish-inference/Cargo.toml` lists
  `parish-providers = { workspace = true }` under `[dependencies]`.
- `parish-providers/Cargo.toml` does **not** list `parish-inference` in any
  dependency section — the dependency direction is strictly
  `parish-inference → parish-providers`, never the reverse.
- Observable via: `cargo tree -p parish-providers | grep parish-inference`
  returns empty; `cargo tree -p parish-inference | grep parish-providers`
  returns one hit.

### 3. Moved modules are absent from `parish-inference`; no duplication

- The following source paths **do not exist** after the refactor:
  `parish/crates/parish-inference/src/anthropic_client/`,
  `parish/crates/parish-inference/src/openai_client/`,
  `parish/crates/parish-inference/src/simulator.rs`,
  `parish/crates/parish-inference/src/any_client.rs`,
  `parish/crates/parish-inference/src/rate_limit.rs`,
  `parish/crates/parish-inference/src/client_base.rs`,
  `parish/crates/parish-inference/src/utf8_stream.rs`.
- No type or function is defined in both crates simultaneously — the
  architecture-fitness orphan-source test (`no_orphaned_source_files`) passes,
  confirming no leftover stale files.
- Observable via: `just check` passes, including `no_orphaned_source_files`.

### 4. Re-exports preserve all downstream import paths

- `parish-inference`'s `lib.rs` re-exports every public symbol that moved:
  - `pub use parish_providers::AnthropicClient;`
  - `pub use parish_providers::rate_limit::InferenceRateLimiter;`
  - `pub use parish_providers::{AnyClient, InferenceClients, TOKEN_CHANNEL_CAPACITY, build_client};`
  - `pub use parish_providers::openai_client::{GenerateParams, JsonSchemaSpec, ResponseFormat};`
  - `pub use parish_providers::mock_client::{MockClient, MockMatcher};`
  - `pub use parish_providers::simulator;` (for `simulator::CORPUS` used in
    `parish-npc::quality`, and `simulator::SimulatorClient` used in
    `parish-core::game_session`).
- Zero changes are required in any of the following files (confirmed by a
  clean `git diff --name-only` restricted to those crate trees after the
  refactor):
  - `parish/crates/parish-core/src/**`
  - `parish/crates/parish-npc/src/**`
  - `parish/crates/parish-input/src/**`
  - `parish/crates/parish-tauri/src/**`
  - `parish/crates/parish-engine/src/**`
- Observable via: `just check` green with no import-path errors in downstream
  crates.

### 5. `reqwest` containment rule updated, not silenced

- `parish-providers/Cargo.toml` carries
  `reqwest = { workspace = true, features = ["json", "stream"] }` (the
  HTTP clients require both features).
- `parish-inference/Cargo.toml` no longer lists `reqwest` as a direct
  dependency (the transitive path through `parish-providers` is sufficient).
- The architecture-fitness test `backend_agnostic_crates_do_not_pull_runtime_deps`
  does not forbid `reqwest`; it blocks `tauri`/`axum`/`tower*`/`wry`/`tao`.
  However, `docs/agent/architecture.md` (and the inline CLAUDE.md note in
  `parish-inference`) states that `reqwest` is "contained to parish-inference".
  After this refactor the correct statement is: "`reqwest` is contained to
  `parish-providers` (and entry-point/tool crates that legitimately need HTTP)."
  Update `docs/agent/architecture.md`, the `parish-inference/CLAUDE.md`
  summary, and any inline comment that says "reqwest contained to
  parish-inference" to name `parish-providers` instead. Do NOT add a
  `#[allow]` or silently ignore the mismatch.
- Observable via: `grep -r 'reqwest.*parish-inference\|parish-inference.*reqwest'
docs/ parish/crates/parish-inference/` returns no stale references after
  the docs update.

### 6. `governor` moves with `rate_limit.rs`

- `parish-providers/Cargo.toml` declares `governor = { workspace = true }`.
- `parish-inference/Cargo.toml` no longer lists `governor` (rate limiting is
  entirely in `parish-providers`).
- Observable via: `cargo tree -p parish-inference --no-dedupe | grep governor`
  shows it only as a transitive dependency (via `parish-providers`), not a
  direct one.

### 7. `just check` green — all existing tests pass unmodified

- `cargo test -p parish-inference` passes with no test changes.
- `cargo test -p parish-providers` passes (the moved tests travel with their
  modules).
- `cargo test -p parish-core` passes, including
  `architecture_fitness::backend_agnostic_crates_do_not_pull_runtime_deps`
  and `architecture_fitness::no_orphaned_source_files`.
- The `BACKEND_AGNOSTIC` constant in
  `parish/crates/parish-core/tests/architecture_fitness.rs` is updated to
  include `"parish-providers"` (it must not pull `tauri`/`axum`/`tower*`/`wry`/`tao`
  either).
- Observable via: `just check` exits 0.

### 8. Three priority lanes still drain in order

- The existing queue unit tests in `parish-inference/src/queue.rs` — specifically
  `test_priority_lanes_route_correctly` and
  `test_priority_lanes_batch_yields_to_interactive_when_queued` — pass
  unmodified (no test source changes required).
- The existing worker tests in `parish-inference/src/worker.rs` —
  `test_spawn_inference_worker_abort_stops_task`,
  `test_streaming_request_records_ttft_and_token_count`,
  `test_cancellation_fires_mid_stream_yields_error`, and
  `test_worker_timeout_sends_error_and_continues` — pass unmodified.
- Observable via: `cargo test -p parish-inference -- queue worker` exits 0.

### 9. Simulator provider: fixture output identical before and after

- Running `cargo run --manifest-path parish/Cargo.toml -p parish-engine --
--script parish/testing/fixtures/play_extract-providers.txt` on the
  simulator provider before and after the refactor produces structurally
  identical output (same action kinds, NPC dialogue present, no inference
  errors in the log).
- Observable via: the live transcript in `evidence.md` shows NPC dialogue
  turns that contain `"kind":"talked"` or `"outcome":"talked"` lines with
  non-empty `"text"` fields — proving the simulator path through
  `parish-providers` is wired end-to-end, not bypassed.

### 10. Rate limiting still configured per category from `parish.toml`

- The `[engine.inference.rate_limits.<category>]` TOML sections (parsed by
  `parish_config::RateLimitConfig`) continue to be applied to the correct
  `AnyClient` variant via `parish_core::ipc::config::InferenceConfig::install_rate_limits`.
- The seam is: `parish_config::RateLimitConfig` → `InferenceRateLimiter::from_config`
  → `AnyClient::OpenAi(c).maybe_with_rate_limit` / `AnyClient::Anthropic(c).maybe_with_rate_limit`.
  All three types (`InferenceRateLimiter`, `AnyClient`, `maybe_with_rate_limit`) now
  live in `parish-providers`; `parish-core/src/ipc/config.rs` imports them via
  the re-exported `parish_inference::InferenceRateLimiter` path (which now
  re-exports from `parish-providers`). No source change in `parish-core` is
  required.
- Observable via: `cargo test -p parish-core -- install_rate_limits
resolve_category_client` passes — these tests assert that a limiter attached
  via `install_rate_limits` is reflected in `client.has_rate_limiter()`.

### 11. Documentation updated

- `docs/agent/architecture.md` — the `parish-inference` row is updated to
  say it delegates HTTP transport to `parish-providers`; a new `parish-providers`
  row is added describing the four client modules and rate limiter.
- `docs/agent/codebase-map.md` — the "17 crates" count and the crate table
  are updated to reflect the new 18th crate.
- `CLAUDE.md` / `AGENTS.md` crate-count references ("17 crates") updated to 18.
- `parish-inference/CLAUDE.md` updated to remove the `rate_limit/` from its
  module map and note the `parish-providers` dependency.
- `just notices` passes (dependencies moved between `Cargo.toml`s; third-party
  notices must be regenerated).
- Observable via: `just notices` exits 0 and `git diff --name-only` shows
  `THIRD_PARTY_NOTICES.md` (or equivalent) touched.

### 12. Live proof

- The evidence file must declare `Evidence type: live gameplay transcript`.
- The transcript must be produced by
  `cargo run --manifest-path parish/Cargo.toml -p parish-engine --
--script parish/testing/fixtures/play_extract-providers.txt`
  (or `parish --script ...` against a live server, or `mcp__parish__*`).
- The evidence must map each of the 11 criteria above to the specific
  transcript line(s) or `cargo test` output that prove it.

---

## Verification script

Run:

```
cargo run --manifest-path parish/Cargo.toml -p parish-engine \
  --script parish/testing/fixtures/play_extract-providers.txt
```

Expected signals in output:

- `"provider":"simulator"` in the status block — confirms the simulator path
  routes through `parish-providers`.
- At least one `"kind":"talked"` turn with a non-empty `"text"` field — proves
  simulator dialogue is live, not a stub.
- No `"error"` field in any inference log entry — proves the rate-limit and
  worker wiring is intact.
- `/provider` output lists `simulator` as the current provider — confirms the
  BYOK/setup path still resolves the simulator variant via the re-exported
  `build_client` factory.
