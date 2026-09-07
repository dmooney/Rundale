# Native Rundale client

This checkout contains the Phase 1 native client and the Phase 2 embedded Parish
vertical slice from the [mobile reset](../docs/product-specs/product-technical-spec.md).
Phase 2 adds the portable Rust runtime, local persistence, the Swift FFI bridge,
and the one-location, one-NPC gameplay fixture. Remote Endpoint delivery and
physical-iPhone acceptance remain separate evidence gates.

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
```

Running `./verify` (or `./verify --phase all`) runs both implemented phases and
records later phases as non-blocking unavailable work. The report is written to
`mobile/.verification/`.

The Rust package contains arm64 device and Apple silicon simulator slices.
The build script requires Rust 1.98.0 and its `aarch64-apple-ios` and
`aarch64-apple-ios-sim` targets. Run it again after Rust changes.

For remote authentication, supply the ignored Firebase configuration described
in [phase2-auth.md](phase2-auth.md) before generating the Xcode project.

Choose the Rundale scheme and an iPhone simulator in Xcode. For a physical
iPhone, choose your development signing team in the generated project and run
on the connected device. Signing credentials are local configuration.

The generated Xcode project and build/test results are ignored. The XcodeGen
specification and Swift sources are the reproducible inputs. The application
launches directly into the transcript; only the compact status header,
transcript, and native composer are persistent gameplay regions. Normal
launches use the Phase 2 Rust runtime; `--ui-tests` without `--phase2`, or an
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
validation, and committed state remain in the embedded Parish runtime rather than
in SwiftUI views. Phase 2 intentionally stays at one location and one NPC;
fixture dialogue and completion names are not the canonical three-location world
required in Milestone 3.

The transcript uses one isolated UIKit collection scroller with SwiftUI-hosted
rows. Phase 1 simulator checks showed that a lazy SwiftUI stack could report
incomplete bottom and row geometry while it materialized a long history; proxy
scroll requests then landed before the newest entry, moved a passage during
streaming, or failed to restore its logical row. The native boundary makes the
real content offset and drag state authoritative while preserving the SwiftUI
row design, Dynamic Type, VoiceOver labels, and command-recall action.

## Acceptance

The [Phase 1 test plan](../docs/test-plans/phase-1-test-cases.md),
[Phase 2 test plan](../docs/test-plans/phase-2-test-cases.md), and full product
checklist all apply. Automated success does not establish physical-device or
live Endpoint acceptance. Record those results in [acceptance.md](acceptance.md).
Use the [demo walkthrough](demo.md) to present the running client at delivery.

The verification entry point distinguishes automated failures and unavailable
infrastructure from human/device gates. It must not report a missing simulator
as a passing UI suite. Phase 2 physical-device and live Endpoint gates remain
visible in the report without being represented as automated passes.
