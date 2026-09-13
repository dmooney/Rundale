# Native Rundale client

This checkout contains the first three phases of the
[mobile reset](../docs/product-specs/product-technical-spec.md). Phase 2 added
the embedded Parish runtime, local persistence, Swift FFI bridge, and deployed
Endpoint path. Phase 3 replaces the one-room slice with the canonical
three-location, three-NPC Kilteevan world, authoritative travel and presence,
explicit schedule movement, and resumable clarification. Physical-iPhone
acceptance remains a separate gate.

## Build and run

The initial development baseline is Xcode 26.6 (Swift 6.3.3), XcodeGen 2.45.4,
and an iOS 17 deployment target. The deployment target is provisional until the
physical-device acceptance matrix establishes the supported baseline.

From the repository root:

```sh
bash mobile/scripts/build-rust-mobile.sh
xcodegen generate --spec mobile/project.yml
open mobile/Rundale.xcodeproj
./verify --phase 1
./verify --phase 2
./verify --phase 3
```

Running `./verify` (or `./verify --phase all`) runs all three implemented phases and
records later phases as non-blocking unavailable work. The report is written to
`mobile/.verification/`.

The Rust package contains arm64 device and Apple silicon simulator slices.
The build script requires Rust 1.98.0 and its `aarch64-apple-ios` and
`aarch64-apple-ios-sim` targets. Run it again after Rust changes.

For remote authentication, supply the ignored Firebase configuration described
in [phase2-auth.md](phase2-auth.md) before generating the Xcode project.
Configure `RUNDALE_ENDPOINT_BASE_URL` with the verified Parish Endpoints service
origin; no production origin is baked into the app. The client appends the
pinned `/v1/endpoints/{organization}/{slug}/versions/{version}/stream` route.
`RUNDALE_ENDPOINT_ORGANIZATION`, `RUNDALE_ENDPOINT_SLUG`, and
`RUNDALE_ENDPOINT_VERSION` override the versioned deployment identity. The
mobile request carries short-lived Firebase Auth and App Check credentials
directly to Parish Endpoints; provider credentials and shared Endpoint keys
never enter the app. See the [Endpoint handoff](endpoint/phase2-handoff.md) for
the SSE contract, deployed revision, live evidence, and remaining device gate.

Choose the Rundale scheme and an iPhone simulator in Xcode. For a physical
iPhone, choose your development signing team in the generated project and run
on the connected device. Signing credentials are local configuration.

The generated Xcode project and build/test results are ignored. The XcodeGen
specification and Swift sources are the reproducible inputs. The application
launches directly into the transcript; only the compact status header,
transcript, and native composer are persistent gameplay regions. Normal
launches use the embedded Rust runtime; `--ui-tests` without `--phase2` or
`--phase3`, or an
explicit `--fixture=...` argument, selects the deterministic Phase 1 adapter.

## Ownership

- `RundaleKit` defines the semantic presentation contract and deterministic
  presentation adapter, with independent unit tests.
- `Rundale` owns SwiftUI rendering, native input, accessibility, and app lifecycle.
- `RundaleUITests` exercises the application through native UI automation.
- `RundaleBridge` owns the actor-isolated Swift boundary for the embedded Parish
  session and the C ABI module map.
- `parish-mobile-ffi` and the `mobile` feature of `parish-core` own the portable
  runtime path; local persistence remains authoritative.
- `scripts` supplies the phase verification runner and its regression tests.

The renderer consumes semantic presentation state. Game rules, request identity,
validation, travel, presence, schedules, clarification, and committed state
remain in the embedded Parish runtime rather than in SwiftUI views. The
[canonical world sheet](content/canonical-world.md) is the independent Phase 3
acceptance oracle; `content/phase3-tiny-world.json` is the machine-readable
authority.

The transcript uses one isolated UIKit collection scroller with SwiftUI-hosted
rows. Phase 1 simulator checks showed that a lazy SwiftUI stack could report
incomplete bottom and row geometry while it materialized a long history; proxy
scroll requests then landed before the newest entry, moved a passage during
streaming, or failed to restore its logical row. The native boundary makes the
real content offset and drag state authoritative while preserving the SwiftUI
row design, Dynamic Type, VoiceOver labels, and command-recall action.

## Acceptance

The [Phase 1 test plan](../docs/test-plans/phase-1-test-cases.md),
[Phase 2 test plan](../docs/test-plans/phase-2-test-cases.md), Milestone 3
acceptance criteria, and full product
checklist all apply. Automated success does not establish physical-device
acceptance. Record live Endpoint and device results separately in
[acceptance.md](acceptance.md).
Use the [demo walkthrough](demo.md) to present the running client at delivery.

The verification entry point distinguishes automated failures and unavailable
infrastructure from human/device gates. It must not report a missing simulator
as a passing UI suite. The verifier reports live Endpoint and physical-device
gates separately from deterministic automated suites.
