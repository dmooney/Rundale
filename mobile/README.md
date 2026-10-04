# Native Rundale client

The SwiftUI iPhone app for Rundale. It embeds the shared Limerick engine
through the `limerick-mobile-ffi` static library; there is no mobile-only
runtime ([ADR-025](../docs/adr/025-mobile-runtime-on-shared-engine.md)). The
[convergence plan](../docs/plans/mobile-engine-convergence.md) sets the order
in which the mobile work lands on `main`.

## Status

The app plays the canonical world on the shared engine:

- A normal launch opens the save through the FFI boundary, which runs the
  shared turn API (`TurnEngine`): the same parser, intent inference, travel,
  dialogue, and save database as desktop. The world is `mods/rundale`, bundled
  as game data. The app bundles no world content of its own.
- Every model call goes to the Limerick Endpoint the engine names:
  `rundale-intent` for input the local parser does not recognise, then
  `rundale-dialogue` for the reply. Dialogue streams into the transcript as
  provisional text and the committed line replaces it. Stop, a failure, or a
  relaunch leave no state effects; the request can be retried.
- Slash commands answer locally, with no Endpoint call. Commands lists the
  advertised four (`/look`, `/people`, `/exits`, `/help`). Typing `/` offers
  every command, including `/wait`, `/pause`, `/resume`, `/debug`, and
  `/flags`, then each next word from the engine's registry: after `/debug`
  it offers the views, and after `/debug memory` everyone in the world. Names match
  without case or fadas, on the phone and in the engine.
- `--fixture=<name>` (or `--ui-tests` without `--phase2`) selects the
  deterministic presentation fixtures in `RundaleKit`, which exercise the UI
  without the engine.

Background simulation that needs inference (NPC reactions, banter, tier 2–4)
waits for #2025. The Phase 2–4 engine-mode UI suites still assert `ios-port` fixture text
and are brought onto the canonical world with the `just mobile-verify` gates (#2046).
Physical-device and TestFlight gates are #2046.

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

## Verify and release

```sh
just mobile-verify --phase 1   # or 2, 3, 4; no flag (or `all`) runs them all
just mobile-build              # unsigned Release build
just testflight-update         # verify, sign, and upload an internal beta
```

`just mobile-verify` writes its JSON, JUnit, and summary reports under
`mobile/.verification/` and reuses suites that passed with identical inputs.
The [scripts README](scripts/README.md) covers its gates and options, recording
one UI test, and frame analysis of a streamed reply. The
[TestFlight runbook](testflight.md) covers signing, upload, and export compliance.

## Layout

- `Rundale`: SwiftUI rendering, native input, accessibility, and app lifecycle.
- `RundaleKit`: the semantic presentation contract, reducer, and deterministic
  presentation fixtures, with unit tests (`swift test --package-path mobile/RundaleKit`).
- `RundaleBridge`: the actor-isolated Swift owner of the C boundary and its
  module map.
- `LimerickEndpointKit`: the Endpoint streaming client (renamed in #2007).
- `endpoint`: the Endpoint contract files and SSE fixtures that the
  `endpoints/` service tests read.
- `RundaleTests` and `RundaleUITests`: app unit and UI tests.
  `RundaleEngineLifecycleTests` runs the RundaleKit request-lifecycle tests
  against the real engine through the FFI. `RundaleLiveEndpointUITests` is an
  opt-in live suite against a deployed Limerick Endpoints service; see
  [the endpoint README](endpoint/README.md#live-endpoint-suite).
