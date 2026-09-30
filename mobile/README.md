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

Swift lint/format/package/Xcode quality gates are #2103
([docs/agent/swift-quality-gates.md](../docs/agent/swift-quality-gates.md)).
Phase verification tooling (`./verify`, release, UI recording) remains #2045.
Physical-device and TestFlight gates remain #2046.

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

## Quality gates (macOS)

```sh
bash mobile/scripts/install-swift-tools.sh   # pinned SwiftLint + SwiftFormat
just swift-quality                           # lint, format check, three package suites
just swift-quality-xcode                     # + FFI, Xcode build, app-unit, deterministic UI
```

Reports land under `mobile/.verification/swift-quality/` with separate
`passed` / `failed` / `skipped` / `unavailable` counts. Live Endpoint,
performance, and soak suites are opt-in and never implied by the fast lane.
Physical-device evidence is recorded in [acceptance.md](acceptance.md).

Compiler policy: Swift 6, complete strict concurrency, warnings as errors
(see `project.yml`). Coverage collection is enabled; the regression baseline in
`coverage-baseline.json` stays `unmeasured` until a real macOS Xcode run fills
it in — do not invent a percentage.

On Linux, `just swift-quality` exits non-zero with `unavailable` required gates
rather than reporting a pass. GitHub-hosted macOS Actions are not required for
this gate; use a local Mac, or later a self-hosted runner via `swift-ci.yml`.

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
- `scripts`: Rust FFI build, Swift quality runner, and tool install helpers.
