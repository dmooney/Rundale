# Mobile architecture map

> Status: Orientation note · Updated: 2026-09-09 · [Product specs](../product-specs/README.md)

This note maps the proposed mobile reset onto the repository that exists today. The two imported specs in [product-specs](../product-specs/README.md) remain the source of product requirements; this file is a short implementation orientation and makes no completion claim.

Read the [technical vision](../product-specs/software-technical-vision.md) for the full
proposal and [existing architecture](architecture.md) for crate ownership. Start at
vision sections 5–7 for FFI, requests, and persistence; sections 8–10 for native UI,
bindings, and Endpoints; sections 11–14 for simulation, recovery, and phase evidence.
The [build/test guide](build-test.md) distinguishes available commands from required
future mobile verification.

## Proposed mobile direction

The Product & Technical Specification defines a native iPhone experience: SwiftUI owns the compact status header, transcript, and composer; the authoritative Limerick gameplay runtime is embedded on-device; local saves remain authoritative; and production remote inference goes through Limerick Endpoints. The Software Technical Vision turns that into proposed boundaries: semantic events and read-only projections cross a small Swift/Rust interface, Rust owns intent, simulation, validation, and commit ordering, and an iOS platform adapter owns lifecycle and transport mechanics without moving game authority into SwiftUI or the Endpoint.

## Existing repository shape

- Shared game behavior is composed through limerick-core and its leaf crates under limerick/crates/, including world, input, NPC, inference, persistence, and type layers. limerick-engine is a thin headless/CLI entry point, not a second copy of the engine.
- The current player-facing frontend is the Svelte 5 application under limerick/apps/ui/. limerick-tauri hosts the desktop application; limerick-server provides the Axum HTTP/WebSocket server; limerick-client is a thin HTTP client. Existing ADRs and design notes describe those desktop/web modes.
- The native SwiftUI client under [mobile/](../../mobile/README.md) now includes the Phase 1 presentation package and Phase 2 embedded Rust/SQLite vertical slice plus the deployed Endpoint adapter. This remains no evidence of physical-device acceptance.
- Existing local inference and desktop setup paths include provider/process/server concerns that the proposed iOS runtime must not inherit accidentally. Reuse is a repository-audit decision, not an assumption made from this map.

## Boundary map

| Reset concern          | Technical vision proposal                                                                                                           | Existing repository analogue                                                               | Work implied by the reset                                                                      |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| Presentation           | SwiftUI transcript/composer and a presentation reducer over semantic events                                                         | Svelte play surface and Tauri/web IPC projections                                          | Define a versioned mobile semantic event/read-model contract and build the native shell        |
| Game authority         | One embedded Rust runtime owns mutable world state and the commit lane                                                              | Shared limerick-core plus leaf crates already centralize game behavior across entry points | Audit portable dependencies and expose a small Swift-facing boundary without duplicating rules |
| Persistence            | One local transactional boundary for accepted requests, world state, and committed transcript history                               | Existing limerick-persistence and branch/save lifecycles                                   | Verify iOS storage/file-protection behavior, migrations, draft recovery, and request atomicity |
| Inference              | On-device Rust orchestrates role-specific requests; Limerick Endpoints owns provider execution, credentials, routing, and streaming | Current provider/inference paths serve desktop, server, and local/cloud configurations     | Add the mobile Endpoint adapter and prove the exact request/result/cancellation contract       |
| Mobile services        | Swift-side lifecycle, credential access, native networking, and safe-area/accessibility integration                                 | Current Tauri/Svelte lifecycle and web/server transport surfaces                           | Keep platform glue behind narrow interfaces and preserve fixture/headless testability          |
| Content and simulation | Versioned authored definitions separate from mutable instance state; tiny world first                                               | Existing limerick-world, limerick-npc, mods, and canonical world data                      | Reconcile the three-location/three-NPC fixture and retain it as a regression oracle            |

## Interpretation on mobile

Free-form player input reaches the shared interpretation flow the desktop
client already uses: `limerick_input::parse_intent_local` first, then — only
for input it does not recognise — an inferred intent, then the selected action.
The prompt, payload shape, and post-response validation live once in
`limerick_input::intent_contract`; the desktop provider client and the mobile
Limerick Endpoints adapter are two transports over that one contract, not two
sets of rules (#1993).

On mobile the inferred step is the `intent` Endpoint role, published as
[`rundale-intent-v1.json`](../../mobile/endpoint/rundale-intent-v1.json). One
logical request carries both stages on one attempt: interpretation first, then
dialogue generation only when the resolved action calls for it. Deterministic
commands stay entirely offline. `limerick-core` selects the role and executes
the action; Swift transports the request and never interprets, retargets, or
acts on a payload. A dialogue reply is therefore not evidence that a player's
action was interpreted or executed — the authoritative state change is.

## Phase 2 Endpoint capability

The real Limerick Endpoint path for authenticated mobile-safe inference with
incremental streaming lives in the self-contained `endpoints/` workspace. Its
versioned Firebase Auth/App Check boundary, pinned Google SSE contract, durable
Stop accounting, and native-app simulator path have deployed evidence in the
[Phase 2 handoff](../../mobile/endpoint/phase2-handoff.md). Deterministic Endpoint
doubles and the shared protocol fixture remain the normal regression oracle.
Physical-iPhone App Attest and device acceptance remain open. Direct provider
calls from the iOS app, embedded provider credentials, or a bypassing Rundale
game server would violate the product boundary.
