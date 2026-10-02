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
```

`just mobile-verify` is the app's gate and `just verify` is the engine's; one
does not stand in for the other. Both need macOS for the iOS parts; on Linux,
say which gates you could not run.

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
- **`rundale-intent` v1 can hit its 256-token cap** on vocative speech
  ("Mícheál, how are the cattle?"). The turn still works through dialogue, at
  the cost of a call.
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
- **Product identifiers say Limerick; geography keeps "parish".** Swift, FFI,
  and Endpoints identifiers use `Limerick*`, `limerick_mobile_*`, and
  `@limerick/*` (#2007). Leave Irish geography, ADR text, and historical
  revision URLs alone.
