# Acceptance Criteria: extract-setup

## Task

Extract `parish/crates/parish-inference/src/setup/` (~3,298 lines across six
files: `mod.rs`, `gpu_detect.rs`, `model_select.rs`, `process.rs`,
`orchestration.rs`, `progress.rs`) into a new workspace crate `parish-setup`.
The extraction is a behavior-preserving refactor: the crate boundary moves but
no observable behavior changes. Consumers are `parish-engine/src/main.rs` (CLI
provider bootstrap) and `parish-tauri/src/{lib,commands.rs}` (BYOK flow,
`TauriProgress` impl, `start_local_inference_setup`). After the move
`parish-inference` re-exports the old symbols at the same paths so consumers
compile without any import changes. The engine still boots, loads a configured
provider, and runs a normal play session identically to before.

## Dependency direction

`parish-setup` depends on `parish-inference` (for `AnyClient`, `OpenAiClient`,
`build_client`, `InferenceRateLimiter`, `openai_client::build_client_or_fallback`)
and on `parish-config`, `parish-types`. `parish-inference` gains a
**dev-only or re-export** dependency on `parish-setup` — specifically, a thin
`pub use parish_setup::setup as setup;` shim in `lib.rs` behind no additional
runtime dep. No cycles are allowed: the architecture-fitness test must stay
green.

**Re-export shim shape:** `parish-inference` adds `parish-setup` to
`[dependencies]` in its `Cargo.toml` and adds `pub use parish_setup as setup;`
(or `pub use parish_setup::*;` selectively) in `lib.rs`, removing its own
`pub mod setup;` declaration. This is strictly cleaner than asking consumers to
adopt a direct `parish-setup` dep because the entry-point crates reach setup
via `parish_core::inference::setup::*` (Tauri, server) and
`parish_engine::inference::setup::*` (CLI) — both routes go through
`parish-core`'s `pub use parish_inference as inference;` re-export. The
re-export shim ensures those paths remain valid without touching any consumer.

## Criteria

1. **New crate in workspace** — `parish-setup` appears in `parish/Cargo.toml`
   `[workspace.members]` and `[workspace.dependencies]`. Observable via:
   `grep 'parish-setup' parish/Cargo.toml` returns both the `members` and
   `[workspace.dependencies]` lines.

2. **Crate directory present with correct structure** — `parish/crates/parish-setup/`
   exists and contains `Cargo.toml` + `src/` with the six extracted modules
   (`mod.rs`, `gpu_detect.rs`, `model_select.rs`, `process.rs`,
   `orchestration.rs`, `progress.rs`). Observable via:
   `ls parish/crates/parish-setup/src/` lists those six files.

3. **`setup/` removed from `parish-inference`** — `parish/crates/parish-inference/src/setup/`
   no longer exists on disk. The directory and all six files are gone.
   Observable via: `ls parish/crates/parish-inference/src/setup/` exits non-zero.

4. **No duplication** — no copy of the setup logic remains inside
   `parish-inference/src/`. Observable via:
   `grep -r 'fn setup_provider_client' parish/crates/parish-inference/src/` returns
   zero results (the function lives only in `parish-setup`).

5. **Re-export shim keeps old paths compiling** — `parish-inference/src/lib.rs`
   contains `pub use parish_setup` (or equivalent selective re-export), replacing
   the old `pub mod setup;` declaration. Consumers reach `parish_inference::setup::*`
   via the same paths as before. Observable via: the crate compiles and
   `grep 'pub use parish_setup' parish/crates/parish-inference/src/lib.rs` hits.

6. **No consumer import changes** — `parish-engine/src/main.rs` still imports
   `parish_engine::inference::setup::{self, StdoutProgress}` unchanged.
   `parish-tauri/src/lib.rs` still imports via `parish_core::inference::setup::*`
   unchanged. Observable via: those two files are unmodified (or modified only
   for unrelated reasons) and `just check` compiles them without error.

7. **Direction-of-dependency: no cycle** — `parish-setup` depends on
   `parish-inference`, not the reverse (except via the re-export shim).
   `parish-inference/Cargo.toml [dependencies]` lists `parish-setup`.
   `parish-setup/Cargo.toml [dependencies]` must NOT list `parish-inference`
   — the setup modules call `crate::AnyClient`, `crate::openai_client`, etc.,
   which after the move become `parish_inference::AnyClient`,
   `parish_inference::openai_client`, etc. Observable via:
   `grep 'parish-inference' parish/crates/parish-setup/Cargo.toml` is absent;
   `grep 'parish-setup' parish/crates/parish-inference/Cargo.toml` is present.

   NOTE: This is the key coupling surprise — `orchestration.rs` already calls
   `crate::AnyClient`, `crate::build_client`, `crate::rate_limit::InferenceRateLimiter`,
   and `crate::openai_client::build_client_or_fallback`. These are all in
   `parish-inference`, so `parish-setup` must depend on `parish-inference`
   (for the client factory and rate limiter) and `parish-inference` re-exports
   `parish-setup` back out. This is a real forward-reference inversion: the
   setup code depends on the very crate it is being extracted from.

8. **Architecture fitness tests updated and passing** — `BACKEND_AGNOSTIC` in
   `parish-core/tests/architecture_fitness.rs` includes `parish-setup` if
   `parish-setup` is backend-agnostic (it should be — it calls only `reqwest`,
   `tokio`, `std::process::Command`, `parish_config`, `parish_types`,
   `parish_inference`). No fitness assertion is silenced or removed; at most
   `parish-setup` is added to the `BACKEND_AGNOSTIC` list. Observable via:
   `cargo test -p parish-core --test architecture_fitness` passes.

9. **`just check` green** — `cargo fmt`, `cargo clippy`, and `cargo test`
   (all workspace members) pass with zero errors. Observable via:
   `just check` exits 0. This is the primary integration signal.

10. **Behavior parity: fixture output identical** — running
    `parish/testing/fixtures/play_extract-setup.txt` before and after the
    refactor produces identical non-LLM output lines (i.e. the structured
    `/status`, `/time`, `look`, and `/debug provider` responses are unchanged).
    Observable via: the fixture transcript contains the startup banner and
    `"provider": "simulator"` in the `/debug provider` output.

11. **Live entry point: setup-status surface still works** — after the refactor
    the engine boots with the simulator provider and `setup_provider_client`
    still returns `("simulator", RuntimeProcesses::none())`. The harness
    `/status` output shows the engine is in the `Lobby` or `active` scene
    (i.e. bootstrap succeeded). Observable via: the fixture transcript contains
    a JSON line with `"scene":` or `"location":` set to a non-error value.

12. **Docs updated** — `docs/agent/architecture.md` workspace table shows
    **17** crates (was 16 before this task; note: the table already says 16 but
    `codebase-map.md` says 17; after adding `parish-setup` the count becomes 18),
    `parish-setup` has a row describing its role. `docs/agent/codebase-map.md`
    Parish Crates table includes `parish/crates/parish-setup/`. `parish-inference`
    row in both tables is updated to drop "setup" from its module list and add a
    note that setup is re-exported from `parish-setup`. Observable via: both
    files contain `parish-setup`.

13. **`just notices` run if deps changed** — if `parish-setup/Cargo.toml`
    introduces any new third-party dep not previously in the workspace (unlikely
    — all its deps already exist), `just notices` is re-run to update
    `THIRD_PARTY_NOTICES.md`. Observable via: `just notices` exits 0. If no
    new external dep is added (expected case), this criterion is vacuously met.

## Verification script

Run:

```
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- \
  --script parish/testing/fixtures/play_extract-setup.txt
```

Expected signals in output:

- JSON line containing `"provider"` key with value `"simulator"` (from
  `/debug provider`), confirming `setup_provider_client` bootstrapped
  correctly via the refactored crate boundary.
- JSON line containing `"scene"` or `"location"` key with a non-null,
  non-error value, confirming the engine reached a playable state.
- JSON line from `/status` showing `"ok": true` (or equivalent engine-healthy
  field), confirming no startup error.
- Absence of any `"error":` top-level JSON line referencing `setup`,
  `parish_setup`, or `parish_inference::setup`.
