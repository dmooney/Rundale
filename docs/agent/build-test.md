# Build & Test

## Mobile reset verification

The [product specs](../product-specs/README.md) define the required mobile gates.
The [mobile test plans](../test-plans/README.md) provide the companion case
lists; they are test instructions, not completed verification reports.

`just mobile-verify --phase N` is the phase-selectable entry point the technical
vision requires ("Phase N" is spec Milestone N). It runs on macOS with Xcode;
`--phase all` (the default) runs every implemented phase. It writes
`verify.json`, `verify.junit.xml`, and `summary.txt` under `mobile/.verification/`
and exits nonzero on a blocking gate. Suites that passed with identical inputs are
reused; `--no-cache` reruns them. Options, the gates in each phase, and UI
recording are in the [mobile scripts README](../../mobile/scripts/README.md).
Simulator suites run with Xcode parallel testing on a developer Mac (6
cloned-simulator workers by default; `--parallel-workers 1` is serial). A
GitHub-hosted runner (the nightly gate and the PR smoke gate) runs them with one
worker, and `verify.py` refuses more there (#2301). A full or Phase 4 run puts every
simulator suite that has no cached pass into one `xcodebuild`; a cold full run
takes about 10 minutes on an M5 Pro. While iterating, do not run the full
gate: build once, then run only the affected class with
`xcodebuild test-without-building -only-testing:RundaleUITests/<Class>` (or
`record-ui-test.py`). Run the full `just mobile-verify` once per simulator at the
end, and never edit a worktree while a gate is building from it.
When a simulator test fails, the gate reads the `.xcresult` before the next run
prunes it and keeps the diagnosis under `mobile/.verification/failures/<run>/`:
`index.md`, one `<Class>.<test>/failure.md` per failed test (failure messages with
file:line, the activity timeline leading to the failure, result bundle path), the
test's attachments (failure screenshot, UI hierarchy, a short screen recording;
files over 3 MB are listed, not copied; the first 20 failed tests get a digest and
`index.md` counts the rest), and `diagnostics/` (test runner logs and
any crash reports). `summary.txt` and `verify.json` (`details.failure_evidence`)
name the paths. The newest three failed runs keep their evidence and `.xcresult`.
CI report artifacts include the bounded exported `failures/` tree for diagnosis;
they omit `.xcresult` bundles. The unwatched nightly full gate (`testflight.yml`)
also uploads the `.xcresult` bundles when it fails, and its "Nightly mobile-verify
failed" issue links that artifact. UI test classes
derive from `RundaleUITestCase`, which attaches the app's element hierarchy and a
screenshot at the moment any assertion fails.
The engine's `just verify` below is a different gate; it is not an iPhone
acceptance gate.

### Which layer tests a UI behaviour (#2319)

A XCUITest costs about 30 seconds and asks the accessibility tree whether an
animating view is ready, so it is the slowest and flakiest place to test a rule.
Put a new behaviour test in the lowest layer that can see it:

| The behaviour is                                                                                                                                             | Test it in                                                                                                                                                                  | Run by                               |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ |
| A rule about state: what a row's spoken label says, when Send is enabled, what a completion inserts, whether Retry shows, which row the reading anchor keeps | `RundaleKit` or `RundaleSession` (move the decision out of the view into a plain value type or the presenter), a unit test over the fixture controller (`FixtureRuleTests`) | `swift test`, any host, milliseconds |
| An engine scenario: commands, travel, clarification, stop and retry, fault recovery, relaunch from the save                                                  | `RundaleTests/EngineScenarioTests`: the real engine, save and mock Endpoint with the real presenter, app-hosted                                                             | the simulator, seconds, no UI        |
| Layout and gestures only the real tree has: the keyboard, Dynamic Type, a grown composer, scrolling, rotation                                                | a `RundaleUITests` journey test                                                                                                                                             | the simulator, serial on hosted CI   |
| The process lifecycle: launch, background, force quit, relaunch                                                                                              | a `RundaleUITests` journey test                                                                                                                                             | the simulator                        |

Keep the XCUITest set to journeys (launch, a streamed turn, completion, reading
history, save and restore, accessibility). When you move a rule down, delete the
XCUITest in the same change and name it in the new test's comment, as
`EngineScenarioTests` does; never drop coverage. Do not add a test to
`TIME_SENSITIVE_TESTS` to hide a flake: serial hosted runs are the fix. Only a
test that still fails alone on the hosted runner, with that run linked and an
issue to fix it, belongs there.

`just mobile-verify --fast` runs pinned SwiftLint and SwiftFormat checks and
the three Swift package suites. Swift builds treat warnings as errors, and each
Swift package must hold its line-coverage baseline. CI's `Swift quality` job
runs it on Linux. The phase gates run the package suites too, against Apple's
Foundation, but not the style checks; `--smoke` leaves out both.
The full simulator gate runs on a Mac; without a recorded pass, CI runs
`--smoke` on a PR (the full gate on one labelled `mobile-verify-full`) and the
TestFlight workflow runs the full gate nightly. The
[Swift quality gates](swift-quality-gates.md) give the full matrix, the lint
exclusions, the coverage policy, and where physical-device evidence goes.

Phase 1 must report deterministic fixture/UI checks; Phase 2 adds portable Rust,
persistence fault injection, binding contracts, device/simulator builds, and
Endpoint protocol checks. Reports must distinguish pass, fail, skipped,
unavailable, and human/device gates. Keep real-inference integration opt-in and
separate from deterministic regression tests. Physical-iPhone interaction and
VoiceOver judgment require recorded evidence, even when automated checks pass.

### Rundale CLI frames

`just rundale-cli` plays the app's game from the command line and prints each meaningful
screen state as a Markdown frame; `just rundale-cli-test` runs its golden-frame
tests, which CI's `Rundale CLI tests (Linux)` job runs too
([tools/rundale-cli](../../tools/rundale-cli/README.md)). The host runs the
app's `RundaleSession` controllers and presenter against the real engine,
linked from `liblimerick_mobile_ffi.a` built for Linux, on `mods/rundale`,
with the scripted mock Endpoint, a recorded Endpoint stream, or the fixture
session.

What frames prove: the session logic the app runs, on the engine and world it
ships, produces this transcript, composer state, and choices for this input,
including streaming, Stop, an Endpoint failure and Retry, a clarification, and
a save and resume. What they do not: anything about the app's UI. Frames show
no layout, styling, keyboard, scrolling, or accessibility, and identify the actual host platform (Linux or macOS terminal). Linux runs use
swift-corelibs Foundation; terminal runs on macOS use Foundation without an iOS
GUI. Never present a frame as app UI, simulator, or physical-iPhone evidence,
or as live gameplay unless it was played against a live Endpoint.

## Existing engine and client commands

These procedures remain applicable to the existing runtimes and tooling.
Select checks for the changed surface; do not treat web viewport tests as native
iPhone validation. For documentation changes, use the repository documentation
path checker and the configured Prettier/Markdown lint tools on changed files.

## Gate dependencies

Gates install their own language-managed dependencies, so a fresh clone or
worktree needs no manual install step. `limerick/scripts/ensure-deps.sh` runs
before each gate that needs one:

- Root docs tooling (`just docs-check`, `fmt-docs`, `fmt-docs-check`,
  `lint-docs`, and the pre-push hook) and the UI's `node_modules` (`ui-*`,
  `run`, `demo`) get a frozen `npm ci --prefer-offline`.
- `just lint-py`, `test-py`, and `fmt-py` create or refresh `.venv-dev` from
  `requirements-dev.txt` and always run its pinned tools, never PATH copies.
- `just ui-e2e`, `ui-e2e-update`, and `screenshots` install Playwright's
  Chromium when the installed Playwright version lacks it.

Each install writes a stamp hashing its inputs (lockfile and manifest, or
requirements file, plus the Node module ABI or Python version). A rerun with
unchanged inputs does no install work; changing any input reinstalls on the next
run. Timestamps are not used, because checkouts reset them.

Tools that are not language-managed (shellcheck, shfmt, taplo, osv-scanner, and
Node or Python themselves) are not installed by gates. A gate that needs one
fails with the command that installs it; none passes or skips silently. `just
setup` installs the system-level pieces once.

### Cloud sessions

Claude Code cloud sessions (claude.ai/code) run on Linux behind a proxy that
blocks some download hosts, including `download.swift.org`. The environment's
setup script installs what the gates need there:

- Swift for Linux, pinned to the Swift version of the Xcode that CI selects
  (`.github/workflows/ci.yml`); currently 6.3.3. The proxy blocks
  `download.swift.org`, so take the toolchain from the `swift:6.3.3-noble` image
  on `mirror.gcr.io`. With it, `swift test` runs in `mobile/RundaleKit`.
- Node at the latest `.nvmrc` major (some UI dependencies need a recent minor),
  `just` new enough for the justfile (1.21 cannot parse it), and `osv-scanner`.

Cargo needs only crates.io. Do not add `[patch.crates-io]` git sources: every
cargo command fetches them, including any submodules, and the proxy can block the
submodule host (#2270).

A session's disk allowance is fixed (about 40 GB), and Cargo's default debug
builds fill it: `limerick/target` reached 21 GB in one session. Set
`CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_INCREMENTAL=0` in the environment's
variables, as CI does; `just check` then leaves about 3 GB. When a session still
runs low, delete rebuildable output: `cargo clean` in `limerick/` first, then
`mobile/RundaleKit/.build` and the npm cache (`~/.npm/_cacache`).

## Cargo

Most engine commands should be run from the `limerick/` directory:

- Build: `cargo build` (builds the default member, `limerick-engine`)
- Build everything: `cargo build --workspace`
- Release build: `cargo build --release`
- Run (headless REPL): `cargo run -p limerick-engine` (or `cargo run`, default member)
- Run (HTTP client against server): `cargo run -p limerick-client`
- Test all: `cargo test --workspace`
- Test one: `cargo test <test_name>`
- Format check: `cargo fmt --check` (apply: `cargo fmt`)
- Lint: `cargo clippy --workspace -- -D warnings`
- Portable (mobile) configuration: `cargo clippy -p limerick-core --all-targets --no-default-features --features mobile -- -D warnings`, `cargo test -p limerick-core --no-default-features --features mobile`, and `cargo test -p limerick-mobile-ffi`
  (CI job `rust-mobile-build`; see the `limerick-core` README for what it excludes). `just mobile-verify` does not rerun these on the Mac.

Alternatively, use the top-level `justfile` proxies from the repository root.

### Build outputs per worktree

Each checkout and Git worktree builds into its own `limerick/target/`. The
repository's `.cargo/config.toml` sets `build.target-dir` relative to the
repository root, and that project config overrides a `target-dir` in
`~/.cargo/config.toml`, so a host-wide shared target no longer applies inside
this repository (#1708). Do not point worktrees at one shared target: Cargo's
lock serializes writes but gives no worktree ownership of first-party binaries
and test executables, so one worktree can run another's build.

Host setup: keep `build.rustc-wrapper = "sccache"` (or `RUSTC_WRAPPER=sccache`)
in the user config. sccache is a compiler cache, not an output directory, so
worktrees still reuse compiled dependencies while keeping separate outputs. A
host-level `target-dir` may stay for other projects; to remove it everywhere,
delete it from `~/.cargo/config.toml`. An explicit `CARGO_TARGET_DIR` still
wins; CI and the mobile build scripts use it to pin per-run outputs.

Scripts resolve built artifacts through
`cargo metadata --format-version 1 --no-deps` (`target_directory`), never a
fixed path from another checkout. `limerick-mcp-launch.sh`, `prove_diff.py`, and
the Playwright worktree server already do.

Storage and pruning: `git worktree remove` deletes that worktree's
`limerick/target/` with it. To reclaim space in a worktree you keep, run
`cargo clean` from its `limerick/` directory; it only touches that worktree.
Do not delete another active worktree's target while it builds or runs.

## Game harness

Scripted gameplay fixtures live in `limerick/testing/fixtures/`. Run one with:

```sh
# From limerick/ directory (local headless runtime — no LLM):
cargo run -p limerick-engine -- --script testing/fixtures/test_walkthrough.txt

# Or from root via just:
just game-test
just game-test-one test_movement_errors
just game-test-all

# Against a live server (real LLM, real NPCs):
cargo run -p limerick-client -- "look"          # single-shot
cargo run -p limerick-client -- --script testing/fixtures/test_walkthrough.txt
just run-client                               # interactive REPL
```

## Demo API Profiling

Profile request volume during a human-paced local-inference demo run:

```sh
just demo-profile                  # 5 minutes, 10s reading pause, macOS vLLM-MLX slots
just demo-profile 300 10 mlx-community/Qwen2.5-14B-Instruct-4bit http://localhost:8000/v1
```

The profiler runs `just demo` through a local OpenAI-compatible proxy, writes
request JSONL plus a Markdown report under the ignored local archive at
`docs/proofs/demo-api-profile/`,
routes intent/reaction to the small vLLM-MLX slot on `localhost:8001` by
default, and can compare against a saved baseline:

```sh
python3 limerick/scripts/profile-demo-requests.py --baseline docs/proofs/demo-api-profile/baseline.json
```

## Frontend

```sh
cd limerick/apps/ui && npx vitest run    # unit tests
cd limerick/apps/ui && npx playwright test    # e2e (auto-starts axum server)
cd limerick/apps/ui && npm run test:e2e       # same config-managed e2e path
just ui-test
just ui-e2e
just ui-e2e-update             # update visual baselines after a fresh UI build
just screenshots               # exercise the dedicated Playwright screenshot baselines
just repository-artifacts      # enforce generated-output and large-file policy
```

`just ui-e2e` needs no per-worktree setup. Every managed-server launch first
runs the UI's normal `npm run build`, so direct `npx playwright test`, baseline,
and screenshot commands cannot capture a missing or stale `dist`. Package
scripts (`test:e2e`, `test:e2e:update`) and the `just ui-e2e-update` and
`just screenshots` recipes all enter that same config-managed lifecycle.
`screenshots.spec.ts` writes Playwright baselines under
`limerick/apps/ui/e2e/screenshots/baseline/`; it does not write documentation
images. Curated images under `docs/screenshots/` are promoted deliberately and
must be referenced from tracked source or documentation.

`npm`/`npx` supplies the absolute `npm_execpath` used to invoke npm's JavaScript
entry point through the current Node runtime without a shell. A deliberate
direct `node scripts/playwright-worktree-server.js` launch must instead set
`LIMERICK_PLAYWRIGHT_NPM_EXEC_PATH` to an absolute npm `.js`, `.cjs`, or `.mjs`
entry point. The helper then builds in the worktree's own Cargo target, snapshots the invoking worktree's fresh UI `dist`, embeds a
worktree/snapshot build identity, and publishes a content-addressed server copy
only after validating that identity and its CSP hashes. The server then echoes
the identity through a per-run readiness URL before Playwright proceeds.
Default runs allocate a free loopback port; set `LIMERICK_TEST_PORT` only when a
fixed port is required. A heartbeat lease serializes helper builds. Before
releasing that lock, the helper publishes a second heartbeat lease for the
exact binary and UI snapshot used by the live server. Losing that lease fences
the server; pruning first retires stale or malformed state behind a tombstone
grace and only a later pass can reclaim its artifacts. POSIX group-signal
teardown stops and waits for the child before releasing ownership. Windows
force-tree termination can skip that release, so the bounded retirement path
is required there. Pruning keeps up to three reusable entries per worktree
cache directory and artifact type, removes empty cache directories, and applies
a 24-hour age limit to bound cross-worktree residue. The same helper path runs
locally and on GitHub-hosted/self-hosted CI.

To unit-test the isolation helper from `limerick/apps/ui/`:

```sh
node --test scripts/playwright-worktree-server.test.js
```

To run the slower integration races for shared-Cargo overwrite and live lease
loss fencing (required by the UI PR workflow):

```sh
npm run test:playwright-launcher-integration
```

To update Playwright baselines after intentional UI changes:

```sh
just ui-e2e-update
```

## Web server (browser testing)

```sh
(cd limerick/apps/ui && npm run build)
cd limerick
cargo run -p limerick-server                     # default port 3001
cargo run -p limerick-server -- --port 8080
```

Then open `http://localhost:3001`.

## Tauri desktop

```sh
just tauri-dev      # cargo tauri dev
just tauri-build    # production bundle
```

System packages on Linux: `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`.

## Editor / LSP

Symbol-level navigation needs language servers on PATH. If a session warns that
`svelte-language-server` or `typescript-language-server` are missing, install
them once:

```sh
just setup-lsp        # npm install -g svelte-language-server typescript-language-server typescript
```

Restart the editor/agent session afterwards so the new binaries are picked up.

## Quality gates

- `/check` — both gate levels: `just check` (docs/data Prettier + markdownlint over tracked files, which installs the root tooling itself; `just osv-changed`, which fails on known vulnerabilities that changed lockfiles add, as CI's OSV-Scanner `scan-pr` does, and needs `brew install osv-scanner` and, when a lockfile changed, network access to osv.dev; `just endpoints-changed`, which runs CI's Endpoints job (`pnpm check` in `endpoints/`) when the branch changes an input CI runs that job for (`scripts/endpoints_changed.py` mirrors its path filter); the complete Python quality job using pinned `.venv-dev` tools, including repository-wide `mypy .` and pytest; `just script-tests`, the `limerick/scripts/tests/*.test.sh` suites from CI's "Shell quality" job; then fmt + clippy + tests + doc-consistency) and `just verify` (adds the harness walkthrough)
- `/limerick-engine prove <feature>` — required after implementing any gameplay feature
- `/limerick-engine rubric` — snapshot baselines + structural rubrics (sister to `prove`)
- `/limerick-engine harness [script]` — fixture-script harness run
- `just agent-check` — lints locally; in CI requires the evidence-page link for runtime-shipping PRs

## Eval baselines

```sh
just baselines       # regenerate gameplay-output snapshots after intentional change
just harness-audit   # cross-reference fixtures, baselines, and roadmap for gaps
```

See [../design/testing.md](../design/testing.md) §Eval baselines for the schema. See reference in `limerick/crates/limerick-engine/tests/eval_baselines.rs`.

## Coverage

Run `just coverage` to generate the Tarpaulin HTML/JSON report. Run `just coverage-check` to enforce the current Rust coverage ratchet. Raise the ratchet floor as coverage-recovery work lands; the long-term target is 90%.

The same host supports macOS terminal runs and a managed MCP backend; see
[portable app-session MCP setup](runtime-driving-reference.md#portable-app-session-through-mcp).
The private protocol and real MCP adapter are tested together by
`python3 tools/rundale-cli/test_mcp_session.py` after both binaries are built.
Linux CI runs this deterministic test in addition to the golden frames.
