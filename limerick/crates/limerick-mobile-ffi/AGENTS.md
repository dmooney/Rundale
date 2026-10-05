# limerick-mobile-ffi — agent scope

The callback-free C boundary between the iPhone app and the shared engine: a
static library hosting `limerick_core::turn::TurnEngine` with host-supplied
inference. [README.md](README.md) documents the ABI, operations, and inference
protocol; keep it current with any boundary change. Mobile rules:
[mobile-architecture.md](../../../docs/agent/mobile-architecture.md).

## Commands

```sh
cargo test -p limerick-mobile-ffi                                  # C entry points vs the canonical world
cargo check -p limerick-core --no-default-features --features mobile  # portable surface (CI: rust-mobile-build)
bash mobile/scripts/build-rust-mobile.sh                           # device + simulator xcframework
just ios-sim-save-lock                                             # save-lock tests on the iOS Simulator (macOS)
```

## Traps

- **Thin boundary only.** No mobile-only turn loop, parser, prompt builder, or
  save store here; change the shared engine instead
  ([ADR-025](../../../docs/adr/025-mobile-runtime-on-shared-engine.md)).
- **Two copies of the header.** `include/limerick_mobile_ffi.h` is vendored
  into `mobile/RundaleBridge` beside its module map; change both together. A
  unit test fails if they drift.
- **Wire shapes are a contract with Swift.** Responses project onto the
  `RundaleKit` `SemanticEvent` contract, and numeric identities serialize as
  `{"rawValue": n}`. A field rename breaks the app silently at decode time;
  change the Swift side in the same PR.
- **Portable features only.** This crate links `limerick-core` with
  `default-features = false, features = ["mobile"]`, which compiles out setup,
  the Ollama/vLLM client, the editor, chronicle, and diagnostics. Do not add a
  dependency that pulls desktop features back in.
- **Iterate `HashMap`s in sorted order before they reach prompts, journals, or
  expected strings.** `WorldGraph::location_ids()` and `NpcManager::npcs_at`
  order changes per process; unsorted output made request-body differentials
  and transcript tests flaky (#2041).
- **Endpoint definitions are mod files.** `pending_endpoint` reports the slug,
  version, and `stream` flag from `mods/rundale/endpoints/`; see
  [mods/rundale/AGENTS.md](../../../mods/rundale/AGENTS.md) before changing one.
- **An ignored step leaves the awaited call pending.** `settle` keeps
  `Session::pending` when the engine answers `TurnStatus::Ignored` (a stale
  call, attempt, or revision): the engine still waits on the same call.
  Clearing it made `pending_endpoint` answer `null` while the engine waited,
  and dropped the real answer from the bug report's exchange log (#2022).
