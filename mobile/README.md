# Native Rundale client

The SwiftUI iPhone app for Rundale. It embeds the shared Limerick engine
through the `limerick-mobile-ffi` static library; there is no mobile-only
runtime ([ADR-025](../docs/adr/025-mobile-runtime-on-shared-engine.md)). The
[convergence plan](../docs/plans/mobile-engine-convergence.md) sets the order
in which the mobile work lands on `main`.

## Status

The app builds and launches on the iOS Simulator. It does not play yet:

- A normal launch opens the engine session, and the boundary answers
  `not_wired` until #2044 connects it to the shared `TurnEngine`. The app shows
  "Not yet connected to the game engine (#2044)." above the composer.
- The world comes from the normal mod pipeline once the boundary is wired. The
  canonical tiny world is authored as its own mod in Mobile Phase 3 (#2040).
  The app bundles no world content of its own.
- `--fixture=<name>` (or `--ui-tests` without `--phase2`) selects the
  deterministic presentation fixtures in `RundaleKit`, which exercise the UI
  without the engine.

Verification tooling (`./verify`, release, UI recording) is #2045. Physical-device
and TestFlight gates are #2046.

## Build and run

Requirements: Xcode 26.6 or later, XcodeGen, and the Rust toolchain from
`rust-toolchain.toml` with the `aarch64-apple-ios` and `aarch64-apple-ios-sim`
targets.

From the repository root:

```sh
bash mobile/scripts/build-rust-mobile.sh
xcodegen generate --spec mobile/project.yml
xcodebuild -project mobile/Rundale.xcodeproj -scheme Rundale \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' build
```

Run the Rust script again after engine changes. The generated Xcode project and
build products are ignored; `project.yml`, the Swift sources, and the pinned
`Package.resolved` are the inputs.

Remote inference uses Limerick Endpoints with Firebase Auth and App Check. The
Firebase configuration file is supplied privately and is ignored.
`RUNDALE_ENDPOINT_BASE_URL` configures the Endpoints origin; no production
origin is baked into the app.

## Layout

- `Rundale`: SwiftUI rendering, native input, accessibility, and app lifecycle.
- `RundaleKit`: the semantic presentation contract, reducer, and deterministic
  presentation fixtures, with unit tests (`swift test --package-path mobile/RundaleKit`).
- `RundaleBridge`: the actor-isolated Swift owner of the C boundary and its
  module map.
- `LimerickEndpointKit`: the Endpoint streaming client (renamed in #2007).
- `endpoint`: the Endpoint contract files and SSE fixtures that the
  `endpoints/` service tests read.
- `RundaleTests` and `RundaleUITests`: app unit and UI tests. The engine-mode
  suites need #2044 to pass.
