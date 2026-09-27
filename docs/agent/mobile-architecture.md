# Mobile architecture map

> Status: Decided · Updated: 2026-09-25 · [Product specs](../product-specs/README.md) · [ADR-025](../adr/025-mobile-runtime-on-shared-engine.md) · [Convergence plan](../plans/mobile-engine-convergence.md)

This note maps the mobile reset onto the repository. The two specs in
[product-specs](../product-specs/README.md) remain the source of product requirements.
[ADR-025](../adr/025-mobile-runtime-on-shared-engine.md) records the architecture
decisions. This file is a short implementation orientation and makes no completion claim.

Read the [technical vision](../product-specs/software-technical-vision.md) for the full
proposal and [existing architecture](architecture.md) for crate ownership. Start at
vision sections 5–7 for FFI, requests, and persistence; sections 8–10 for native UI,
bindings, and Endpoints; sections 11–14 for simulation, recovery, and phase evidence.
The [build/test guide](build-test.md) distinguishes available commands from required
mobile verification.

## Mobile direction

The Product & Technical Specification defines a native iPhone experience: SwiftUI owns the compact status header, transcript, and composer; the authoritative Limerick gameplay runtime is embedded on-device; local saves remain authoritative; and production remote inference goes through Limerick Endpoints. Semantic events and read-only projections cross a small Swift/Rust boundary; Rust owns intent, simulation, validation, and commit ordering; an iOS platform adapter owns lifecycle and transport without moving game authority into SwiftUI or the Endpoint.

**The engine is shared, not forked.** Mobile runs the same `limerick-core` turn pipeline, parser (`limerick-input`), mod loader, NPC systems, and `limerick-persistence` save store as desktop. The only mobile-specific Rust is the host boundary: the FFI crate and a host-supplied inference seam. See ADR-025 for the full decision.

## Rules for mobile engine work

- **No mobile-only duplicates.** Do not add a mobile-only turn loop, parser, addressee resolver, world builder, content format, prompt builder, or save store. If the shared engine cannot meet an iOS need, record the missing property and the narrowest adapter in an ADR before writing code.
- **Host-supplied inference.** The engine yields an inference request and the Swift host returns the validated result, a failure, or Stop. Desktop drives the same API with in-process inference.
- **One save system.** Request and transcript records live in the existing `limerick-persistence` database.
  - An unknown transcript event kind never blocks opening a save.
  - Only unreadable authoritative state refuses to open, and then the player gets a clear message, the file is kept, and a new game is offered.
  - Every change to saved data bumps the format version.
- **Content is a mod.** The canonical tiny world (product spec §14) is its own mod with its own ID space, loaded by the normal mod pipeline.
- **Prompts are game data.** Endpoint definitions are source-controlled files in the world's mod. Limerick Endpoints publishes immutable versions from those files, and its database holds published copies, not the source of truth.
- **Keep desktop green.** Changes to shared engine code keep the desktop tests and harness passing. Weakening a shared rule to fit a mobile slice is not allowed.

## Repository shape

- Shared game behavior is composed through limerick-core and its leaf crates under limerick/crates/, including world, input, NPC, inference, persistence, and type layers. limerick-engine is a thin headless/CLI entry point, not a second copy of the engine.
- The desktop frontend is the Svelte 5 application under limerick/apps/ui/, hosted by limerick-tauri; limerick-server provides the Axum HTTP/WebSocket server; limerick-client is a thin HTTP client.
- The SwiftUI app, Swift/Rust boundary, Limerick Endpoints service, and mobile verification tooling were first built on the `ios-port` branch on top of a mobile-only runtime. They are being brought onto `main` in the order set by the [convergence plan](../plans/mobile-engine-convergence.md). `ios-port`'s runtime, save store, and content bundle are not carried over.
- Existing local inference and desktop setup paths include provider, process, and server concerns that the iOS runtime must not inherit. The `desktop` / `mobile` Cargo features separate them, and CI builds the mobile configuration.

## Boundary map

| Reset concern          | Technical vision proposal                                                                                                           | Existing repository analogue                                                               | Work implied by the reset                                                                                                                 |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Presentation           | SwiftUI transcript/composer and a presentation reducer over semantic events                                                         | Svelte play surface and Tauri/web IPC projections                                          | Define a versioned mobile semantic event/read-model contract and build the native shell                                                   |
| Game authority         | One embedded Rust runtime owns mutable world state and the commit lane                                                              | Shared limerick-core plus leaf crates already centralize game behavior across entry points | Add a portable turn API with host-supplied inference to the shared game loop; the FFI stays a thin boundary (ADR-025)                     |
| Persistence            | One local transactional boundary for accepted requests, world state, and committed transcript history                               | Existing limerick-persistence and branch/save lifecycles                                   | Extend the existing database with request and transcript tables and one transactional turn commit; verify iOS file protection and locking |
| Inference              | On-device Rust orchestrates role-specific requests; Limerick Endpoints owns provider execution, credentials, routing, and streaming | Current provider/inference paths serve desktop, server, and local/cloud configurations     | Keep the Endpoint adapter; author Endpoint definitions as game-data files and publish them to Limerick Endpoints                          |
| Mobile services        | Swift-side lifecycle, credential access, native networking, and safe-area/accessibility integration                                 | Current Tauri/Svelte lifecycle and web/server transport surfaces                           | Keep platform glue behind narrow interfaces and preserve fixture/headless testability                                                     |
| Content and simulation | Versioned authored definitions separate from mutable instance state; tiny world first                                               | Existing limerick-world, limerick-npc, mods, and canonical world data                      | Author the canonical tiny world as its own mod with its own IDs; keep the world sheet as a regression oracle                              |

## Endpoint capability

Phase 2 requires authenticated, mobile-safe inference through a real Limerick Endpoint with incremental streaming, a validated terminal result, cancellation, and request correlation. That path exists in the Limerick Endpoints service, which is deployed in the dedicated `limerick-prod` project (Firebase anonymous auth plus App Check). A native simulator suite has exercised it live. Physical-device acceptance remains open. Keep deterministic Endpoint doubles and protocol fixtures as the normal regression oracle. Direct model-provider calls from the iOS app, embedded provider credentials, or a bypassing Rundale game server would violate the product boundary.
