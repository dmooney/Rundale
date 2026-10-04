# Swift quality gates

The policy for the native iPhone client's Swift code: which checks run where,
what fails them, and what each one does not prove. The commands live in
[`mobile/scripts/verify.py`](../../mobile/scripts/verify.py), which is the only
runner; its options and report format are in the
[mobile scripts README](../../mobile/scripts/README.md).

## Gate matrix

| Gate                                                                      | Where it runs                                          | Required             |
| ------------------------------------------------------------------------- | ------------------------------------------------------ | -------------------- |
| SwiftLint `--strict`, SwiftFormat `--lint` (pinned binaries)              | CI `Swift quality` job; every `just mobile-verify`     | Yes                  |
| `RundaleKit`, `RundaleBridge`, `LimerickEndpointKit` package tests        | CI `Swift quality` job; `just mobile-verify`           | Yes                  |
| Package line coverage against the baseline                                | CI `Swift quality` job; `just mobile-verify`           | Yes                  |
| Rust FFI tests, XCFramework packaging, FFI dependency graph               | `just mobile-verify --phase 2` and later               | Yes, local           |
| Unsigned device build, app-unit (`RundaleTests`) and simulator UI suites  | `just mobile-verify --phase N`                         | Yes, local           |
| Live Endpoint suite against `limerick-prod`                               | `just mobile-verify --live-endpoint --device <UDID>`   | Manual, opt-in       |
| Soak and performance suites                                               | `just mobile-verify --soak` / `--performance --device` | Manual, opt-in       |
| Physical-iPhone acceptance (interaction, VoiceOver, sessions, App Attest) | A person with the device                               | Yes, not automatable |

The CI job is `python3 mobile/scripts/verify.py --fast`. It runs on GitHub's
hosted `macos-26` runner with Xcode 26.6 for every pull request that changes
`mobile/**`, on pushes to `main`, and on manual dispatch. It is part of the
required `CI gate`. Hosted macOS minutes are free for this public repository.

The Xcode app build and the simulator suites are not in CI. They need the Rust
XCFramework (a full cross-compile of the engine for two iOS targets) and the
private Firebase configuration, and the Phase 2 to 4 UI suites take tens of
minutes on four cloned simulators. Run `just mobile-verify --phase N` locally on
macOS before a mobile pull request merges, and report its `summary.txt`.

The live Endpoint, soak, and performance suites need a physical iPhone, the
private Firebase configuration, and an App Check debug token or App Attest.
They cannot run on a hosted runner, so they are manual rather than scheduled.
Credentials come from the environment and the ignored plist; the runner never
writes them to its logs or reports.

`verify.json` reports `passed`, `failed`, `skipped`, `unavailable`, and
`not_automatable` separately. A required gate that is skipped or unavailable is
blocking, never a pass. See [Evidence and exit status](../../mobile/scripts/README.md#evidence-and-exit-status).

## Lint and format

`mobile/scripts/install-swift-tools.sh` downloads SwiftLint 0.65.1 and
SwiftFormat 0.63.1 from their GitHub releases into `mobile/.build/swift-tools/bin`,
checks each archive's SHA-256 before unpacking it, and fails on a mismatch or a
version that does not match the pin. Homebrew is not used, because it cannot
install an exact past version. `just mobile-verify` runs the installer first and
then checks every Swift file under `mobile/` without modifying it:

```sh
mobile/.build/swift-tools/bin/swiftlint lint --strict --quiet --config mobile/.swiftlint.yml mobile
mobile/.build/swift-tools/bin/swiftformat --lint --config mobile/.swiftformat mobile
```

To apply SwiftFormat's fixes, run the second command without `--lint`.

### SwiftLint exclusions

[`mobile/.swiftlint.yml`](../../mobile/.swiftlint.yml) uses the default rules,
some opt-in rules, and `--strict`, so a warning fails the gate. It disables:

- `cyclomatic_complexity`: the Endpoint stream validator and the session
  reducer are exhaustive `switch` statements over protocol events and actions.
  The metric counts each case. Splitting them would lose the compiler's
  exhaustiveness check.
- `file_length`, `type_body_length`, `function_body_length`: SwiftUI views and
  UI test classes keep one screen or one scenario per type. Size alone is not a
  defect this gate should block.
- `line_length`: long string literals (Endpoint error text, accessibility
  labels, URLs) read better unwrapped, and SwiftFormat owns layout.
- `trailing_comma`: the code has trailing commas in some multiline literals and
  not in others. SwiftFormat's `trailingCommas` is also off, so the code stays
  as it is.

It also allows the short names `id`, `x`, `y`, and the FFI's `ok` flag, and
allows types nested two levels deep for Codable payloads that mirror the FFI's
nested JSON.

### SwiftFormat exclusions

[`mobile/.swiftformat`](../../mobile/.swiftformat) uses the default rules with
options that match the existing code (`--ifdef no-indent`, preserved argument
wrapping, preserved digit grouping). It disables rules whose preferred form
differs from that code. Adopting them would reformat most files and gain
nothing in correctness:

- `wrapIfStatementBodies`, `wrapFunctionBodies`, `wrapPropertyBodies`,
  `wrapLoopBodies`, `wrapMultilineStatementBraces`: the code keeps short
  bodies such as `guard ... else { return }` on one line.
- `redundantReturn`, `conditionalAssignment`: `switch` arms keep an explicit
  `return` rather than expression syntax.
- `redundantSelf`: `self.` stays where it clarifies a closure capture.
- `hoistTry`, `hoistAwait`: `try` and `await` stay at the call they mark.
- `andOperator`: `&&` and condition lists are not interchangeable once a
  condition binds, so the code chooses each by hand.
- `redundantSwiftUIGroup`: removing a `Group` changes which views a modifier
  applies to.
- `noForceUnwrapInTests`, `noForceTryInTests`: a test fixture that fails to
  build should crash the test.
- `spaceAroundOperators`, `sortImports`, `trailingCommas`, `unusedArguments`:
  existing alignment, import order, literal style, and named unused closure
  parameters are kept.

## Compiler warnings and concurrency

Every target compiles in the Swift 6 language mode (`SWIFT_VERSION: "6.0"` in
`mobile/project.yml`, `swift-tools-version: 6.0` in each package), which turns
on complete concurrency checking and makes data-race diagnostics errors. On top
of that:

- The app, `RundaleTests`, and `RundaleUITests` targets set
  `SWIFT_TREAT_WARNINGS_AS_ERRORS: "YES"`, so every simulator and device build
  in `just mobile-verify` fails on a Swift warning.
- The package tests run `swift test -Xswiftc -warnings-as-errors`, so package
  sources and tests must build warning-free too.

The Firebase SDK is a remote package. Its targets build with their own settings
and are not subject to the policy. Any exception for first-party code must be
the narrowest one possible: a single file's diagnostic group, recorded here with
its reason. There are none.

## Coverage

`swift test --enable-code-coverage` measures each package's line coverage over
its own `Sources/` (tests and dependencies excluded). The gate fails when a
package drops more than `tolerance_percentage_points` below its value in
[`mobile/coverage-baseline.json`](../../mobile/coverage-baseline.json). It
fails, too, when a passing run produced no coverage report or the baseline has
no entry for the package.

The tolerance (1 percentage point) absorbs small refactors that delete covered
lines. It does not absorb new untested code of any size. The floor is not a
target: raise a package's `line_percent` in the pull request that adds tests,
and lower it only with a stated reason in that pull request. The baseline is
outside the gate's cache key, so editing it re-checks cached passes without
rerunning the tests.

The Xcode scheme gathers coverage for the `Rundale` app target
(`gatherCoverageData: true`), so each simulator suite's `.xcresult` under
`mobile/.verification/xcresults/` carries app coverage. Read it with
`xcrun xccov view --report <bundle>`. App coverage is recorded in the baseline
file for reference but not gated: each phase's suite runs a different subset of
the app, so a single floor would compare unlike runs.

## Physical-device acceptance

Physical-iPhone checks appear in `verify.json` as required `not_automatable`
gates. A simulator run never turns them into passes. Record each physical
result, with the device model, iOS version, app build number, and what was
observed, in its own section of the pull request's evidence page
(`https://dmooney.github.io/rundale-pages/pr/<number>/`), or in the release
notes for a TestFlight build. Keep it apart from simulator and fixture results.
Never describe a simulator or fixture run as physical-device validation.

## Proving a gate fails

A gate counts only if it can go red. The pull request that introduced these
gates recorded receipts for a deliberately failing `RundaleKit` test, a
SwiftLint violation, and a SwiftFormat violation, each turning `just
mobile-verify` red, then green again once reverted. Repeat that check when
changing a gate's command or its status mapping.
