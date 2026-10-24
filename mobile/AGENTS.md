# mobile — agent scope

The SwiftUI iPhone app, the primary product client. It embeds the shared engine
through [`limerick-mobile-ffi`](../limerick/crates/limerick-mobile-ffi/) and
calls models only through Limerick Endpoints. Start with [README.md](README.md)
for build, layout, and status, and
[mobile-architecture.md](../docs/agent/mobile-architecture.md) for the rules on
shared-engine work.

## Commands

```sh
bash mobile/scripts/build-rust-mobile.sh   # rebuild the Rust xcframework after engine changes
xcodegen generate --spec mobile/project.yml
just mobile-verify --phase N               # iPhone gate for product spec Milestone N (macOS + Xcode)
just mobile-build                          # unsigned Release build
just testflight-update                      # verify, sign, upload an internal beta
swift test --package-path mobile/RundaleKit
just mobile-verify --fast                  # CI's Swift lane: lint, format, three packages
```

`just mobile-verify` is the app's gate and `just verify` is the engine's; one
does not stand in for the other. Both need macOS for the iOS parts; on Linux,
`just mobile-verify --fast` runs (it is CI's Linux Swift lane), and say which
other gates you could not run.

Iterating: run only the affected class with `xcodebuild test-without-building
-only-testing:RundaleUITests/<Class>`, and the full `just mobile-verify` gate once
per simulator at the end. Never edit a worktree while a gate is building from it.
Demo `hold(_:)` pauses are no-ops unless `TEST_RUNNER_RUNDALE_DEMO_HOLD=<seconds>`
is set (recording only).

## Evidence

- The simulator is not a physical iPhone. Report simulator runs as simulator
  runs, and leave device acceptance open until it happens on a device.
- `--fixture=<name>`, or `--ui-tests` without `--phase2`, runs RundaleKit's
  deterministic presentation fixtures without the engine. A fixture run is not
  live gameplay; say which you ran.
- The live Endpoint suite (`RundaleLiveEndpointUITests`) is opt-in and runs
  against `limerick-prod`; see [endpoint/README.md](endpoint/README.md#live-endpoint-suite).
  Live runs there are pre-authorized (root `AGENTS.md`).
- After a change that gives the owner something new to try, publish an internal
  TestFlight build per [testflight.md](testflight.md).

## Traps

- **Every Swift warning is an error.** The app targets set
  `SWIFT_TREAT_WARNINGS_AS_ERRORS` and the package gate passes
  `-warnings-as-errors`; SwiftLint runs `--strict`. Fix the warning; any
  exclusion goes in [swift-quality-gates.md](../docs/agent/swift-quality-gates.md)
  with its reason.

- **The app pins its Firebase app.** `FirebaseEndpointCredentialConfiguration`
  rejects a `GoogleService-Info.plist` from another project
  (`unexpectedFirebaseConfiguration`) before any request leaves the phone. No
  Cloud Run request log for a live run means the failure is client-side. The
  plist is supplied privately and is never committed.
- **Pick the Endpoint route from `pending_endpoint.stream`.** An Endpoint
  without `inferenceConfig.streaming` answers `/stream` with 502 `MODEL_ERROR`;
  `rundale-intent` v1 does not stream, so post it to the JSON route. The engine
  absorbs an intent failure as unclassified input, so a wrong route looks like a
  flaky model; check the `com.rundale.mobile:endpoint` log.
- **One Endpoint call, one attempt id.** The server keys a stream by request id
  and `X-Attempt-Id`. A line that calls several speakers makes several dialogue
  calls in one engine attempt, so `endpointAttemptID(callID:)` sends each call's
  own id. Reusing the attempt id makes the second stream fail with HTTP 500 and
  no invocation row. A mock-only run cannot show it; the live suite does.
- **Stop records a cancelled invocation only while the dialogue call is open.**
  `rundale-intent` is non-streaming and finishes server-side, and a live
  dialogue reply streams for under a second (the tap itself takes about a
  second to land), so a single fixed Stop delay usually records nothing.
  `test09` in `RundaleLiveEndpointUITests` steps the delay; the app's
  "Interrupted; not applied" row is not a reliable signal that `/cancel` hit an
  open stream. Check `invocations` for `error_code = REQUEST_CANCELLED` on the
  `rundale-dialogue` request id `<request>:<attempt>.<call>` (#2171).
- **The Endpoints database keeps no request or response bodies.** Its
  `invocations` rows hold status, sizes, tokens, and error codes only, so
  `limerick-prod.sh sql` cannot show what `rundale-intent` returned. To see a
  live intent output, log the JSON-route body in `RundaleEngineController` in a
  local build and read it with `xcrun simctl spawn <sim> log show`.
- **Location description templates are the dialogue Endpoint's only time and
  weather cue.** Do not delete "It is {time}." from `world.json` templates to
  tidy the scene card; the phone drops those sentences at render time
  (`render_setting`, #2110).
- **Ship the mod under `Mods/`.** A top-level bundle folder named like the app
  executable (`Rundale.app/rundale`) breaks the macOS link on a
  case-insensitive file system (`errno=21`). `project.yml` uses a `copyFiles`
  subpath.
- **XCUITest cannot see rows scrolled out of the lazy transcript.** Assert on
  what a command produced, not on a command row a long narration pushed
  off-screen.
- **One non-fast mobile-verify runs per machine.** `verify.py` holds a host-wide
  lock (`~/Library/Caches/rundale/mobile-verify.lock`, or
  `RUNDALE_MOBILE_VERIFY_LOCK`) across worktrees and entry points, since runs
  share the simulator and the Cargo target; a second run waits up to six hours,
  then exits 2. `--fast` does not take it. Tests point the variable at a temp file.
- **Stop stalled mobile verification at the parent process.** Killing only its
  `xcodebuild` child can leave `verify.py` running and starting later suites on
  the same simulator.
- **Product identifiers say Limerick; geography keeps "parish".** Swift, FFI,
  and Endpoints identifiers use `Limerick*`, `limerick_mobile_*`, and
  `@limerick/*` (#2007). Leave Irish geography, ADR text, and historical
  revision URLs alone.
- **Build for the iOS Simulator with rustup's cargo, not Homebrew's.** `/opt/homebrew/bin/cargo` comes first on some PATHs and its toolchain has no `aarch64-apple-ios-sim` std (`can't find crate for core`). Use `~/.cargo/bin/cargo` (the pinned `rust-toolchain.toml` channel) and `rustup target add aarch64-apple-ios-sim` for that channel; `just ios-sim-save-lock` does both.
- **`XCTExpectFailure` is a fourth xcresult bucket.** `xcresulttool get test-results summary` reports it as `expectedFailures`, outside `passedTests`/`failedTests`/`skippedTests`; a validator that reconciles only those three against `totalTestCount` fails a green run (#2081's wrapped test did this to `just mobile-verify` in #2045). The runner's Xcode builds and its host Rust test build go to `mobile/.verification/` and its iOS Rust libraries to `mobile/.build/rust-mobile/`, without sccache, so its first run recompiles the engine from scratch.
