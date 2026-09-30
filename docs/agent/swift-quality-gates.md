# Swift quality gates

Tracks the Swift testing, linting, coverage, and CI gaps closed by #2103.
This is not the phase-selectable `./verify` runner (#2045) and not the
physical-device / TestFlight continuity gate (#2046).

## Toolchain

| Tool        | Pin source                     | Install                                          |
| ----------- | ------------------------------ | ------------------------------------------------ |
| SwiftLint   | `mobile/tool-versions.toml`    | `bash mobile/scripts/install-swift-tools.sh`     |
| SwiftFormat | `mobile/tool-versions.toml`    | same                                             |
| Swift / Xcode | Xcode 26.6+ (app README)     | Apple developer tools on macOS                   |
| XcodeGen    | app README                     | `brew install xcodegen`                          |

Configs:

- `mobile/.swiftlint.yml` — style/smell rules; documented exclusions only.
- `mobile/.swiftformat` — non-mutating layout check via `--lint`.
- `mobile/project.yml` — `SWIFT_TREAT_WARNINGS_AS_ERRORS=YES`,
  `SWIFT_STRICT_CONCURRENCY=complete`, `gatherCoverageData: true`.

## Local commands

From the repository root on macOS:

```sh
bash mobile/scripts/install-swift-tools.sh   # once
just swift-quality                           # lint + format + three package suites
just swift-quality-xcode                     # also app build, app-unit, deterministic UI
python3 mobile/scripts/swift_quality.py \
  --opt-in live-endpoint --opt-in performance --opt-in soak --opt-in physical-device
```

On Linux or any host without Swift, `just swift-quality` exits non-zero and
reports `unavailable` for required gates. That is intentional: missing tools
are never recorded as passes.

Unit tests for the gate runner (no Swift required):

```sh
python3 -m unittest discover -s mobile/scripts -p 'test_swift_quality.py'
```

## Gate matrix

| Gate                         | Required fast | Scheduled / manual | Notes |
| ---------------------------- | ------------- | ------------------ | ----- |
| SwiftLint `--strict`         | yes           | —                  | Non-mutating |
| SwiftFormat `--lint`         | yes           | —                  | Non-mutating |
| `RundaleKit` `swift test`    | yes           | —                  | Package suite |
| `LimerickEndpointKit` tests  | yes           | —                  | Package suite |
| `RundaleBridge` tests        | yes           | —                  | Uses FFI test stubs |
| Xcode app `build-for-testing`| yes (mobile CI / `swift-quality-xcode`) | — | Needs Rust FFI + XcodeGen |
| `RundaleTests` app-unit      | yes (same)    | —                  | |
| Deterministic simulator UI   | yes (same)    | —                  | Excludes live/perf/soak classes |
| Coverage collection          | yes when `--coverage` | —            | Baseline starts `unmeasured` |
| Coverage regression floor    | after measured baseline | —          | Do not invent a percentage |
| Live Endpoint UI             | no            | manual / #2046     | Private Firebase + Endpoint config; never log secrets |
| Performance UI               | no            | manual             | XCTest metrics ≠ approved budgets |
| Soak UI                      | no            | manual / opt-in    | Explicit duration env; smoke ≠ soak |
| Physical iPhone acceptance   | no            | human evidence     | Record in `mobile/acceptance.md`; simulator ≠ device |

Every gate result is one of `passed`, `failed`, `skipped`, or `unavailable`.
The JSON report under `mobile/.verification/swift-quality/` keeps those counts
separate. Required `failed` or `unavailable` blocks the run.

## Compiler and concurrency policy

- Swift 6 language mode (`SWIFT_VERSION = 6.0`).
- Complete strict concurrency checking (`SWIFT_STRICT_CONCURRENCY = complete`).
- Warnings are errors for the app and both test bundles
  (`SWIFT_TREAT_WARNINGS_AS_ERRORS = YES`).
- Narrow exceptions belong in reviewable source (`@available`, targeted
  `@preconcurrency import`, or a documented SwiftLint disable with reason).
  Do not weaken the project-wide warning gate to silence one file.

## Coverage policy

`mobile/coverage-baseline.json` ships with `"status": "unmeasured"`. Collection
is enabled in the Xcode scheme. A measured baseline is produced on macOS after a
green Xcode lane; only then does a regression floor apply. Test-method counts
are inventory, not coverage evidence.

## Physical-device evidence

Automated simulator and package results never satisfy physical-iPhone
acceptance. Record device sessions (build, model, iOS version, tester, result,
defects) in `mobile/acceptance.md` when that file is added by #2046 tooling, or
in the PR body until then. VoiceOver meaning and hands-on judgment stay human
gates.

## CI

Pull requests that touch `mobile/**` or the Swift quality workflow run the
`swift-quality` job on `macos-15`. That job:

1. Installs the pinned SwiftLint / SwiftFormat versions when Homebrew provides them.
2. Runs the fast lane (lint, format, three package suites).
3. Builds the Rust mobile FFI, regenerates the Xcode project, and runs app-unit
   plus deterministic simulator UI tests.
4. Uploads logs / `.xcresult` artifacts on failure.

The aggregate `CI gate` requires that job when the mobile path filter selects
it, and requires it to be skipped otherwise.

## Failing-gate demonstration (Linux-safe)

These unittest cases prove required failures cannot look green without a Mac:

```text
test_lint_violation_fails_required_gate
test_failing_package_test_fails_required_gate
test_missing_toolchain_is_unavailable_not_passed
```

On macOS, introduce a temporary SwiftLint violation or flip an `XCTAssert` to
confirm the same exit status from `just swift-quality`.
