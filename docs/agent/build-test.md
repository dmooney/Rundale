# Build & Test

## Cargo

Most engine commands should be run from the `parish/` directory:

- Build: `cargo build` (builds the default member, `parish-engine`)
- Build everything: `cargo build --workspace`
- Release build: `cargo build --release`
- Run (headless REPL): `cargo run -p parish-engine` (or `cargo run`, default member)
- Run (HTTP client against server): `cargo run -p parish-client`
- Test all: `cargo test --workspace`
- Test one: `cargo test <test_name>`
- Format check: `cargo fmt --check` (apply: `cargo fmt`)
- Lint: `cargo clippy --workspace -- -D warnings`

Alternatively, use the top-level `justfile` proxies from the repository root.

## Game harness

Scripted gameplay fixtures live in `parish/testing/fixtures/`. Run one with:

```sh
# From parish/ directory (local headless runtime — no LLM):
cargo run -p parish-engine -- --script testing/fixtures/test_walkthrough.txt

# Or from root via just:
just game-test
just game-test-one test_movement_errors
just game-test-all

# Against a live server (real LLM, real NPCs):
cargo run -p parish-client -- "look"          # single-shot
cargo run -p parish-client -- --script testing/fixtures/test_walkthrough.txt
just run-client                               # interactive REPL
```

## Demo API Profiling

Profile request volume during a human-paced local-inference demo run:

```sh
just demo-profile                  # 5 minutes, 10s reading pause, macOS vLLM-MLX slots
just demo-profile 300 10 mlx-community/Qwen2.5-14B-Instruct-4bit http://localhost:8000/v1
```

The profiler runs `just demo` through a local OpenAI-compatible proxy, writes
request JSONL plus a Markdown report under `docs/proofs/demo-api-profile/`,
routes intent/reaction to the small vLLM-MLX slot on `localhost:8001` by
default, and can compare against a saved baseline:

```sh
python3 parish/scripts/profile-demo-requests.py --baseline docs/proofs/demo-api-profile/baseline.json
```

## Frontend

```sh
cd parish/apps/ui && npx vitest run    # unit tests
cd parish/apps/ui && npx playwright test    # e2e (auto-starts axum server)
cd parish/apps/ui && npm run test:e2e       # same config-managed e2e path
just ui-test
just ui-e2e
just ui-e2e-update             # update visual baselines after a fresh UI build
just screenshots                 # regenerate docs/screenshots/*.png
```

`just ui-e2e` needs no per-worktree setup. Every managed-server launch first
runs the UI's normal `npm run build`, so direct `npx playwright test`, baseline,
and screenshot commands cannot capture a missing or stale `dist`. Package
scripts (`test:e2e`, `test:e2e:update`) and the `just ui-e2e-update` and
`just screenshots` recipes all enter that same config-managed lifecycle.
`npm`/`npx` supplies the absolute `npm_execpath` used to invoke npm's JavaScript
entry point through the current Node runtime without a shell. A deliberate
direct `node scripts/playwright-worktree-server.js` launch must instead set
`PARISH_PLAYWRIGHT_NPM_EXEC_PATH` to an absolute npm `.js`, `.cjs`, or `.mjs`
entry point. The helper then uses the normal shared Cargo target for dependency
reuse, snapshots the invoking worktree's fresh UI `dist`, embeds a
worktree/snapshot build identity, and publishes a content-addressed server copy
only after validating that identity and its CSP hashes. The server then echoes
the identity through a per-run readiness URL before Playwright proceeds.
Default runs allocate a free loopback port; set `PARISH_TEST_PORT` only when a
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

To unit-test the isolation helper from `parish/apps/ui/`:

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
cd parish/apps/ui && npm run build && cd ../../..
cargo run -p parish-server -- --port            # default port 3001
cargo run -p parish-server -- --port 8080
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

- `/check` — both gate levels: `just check` (fmt + clippy + tests + doc-consistency) and `just verify` (adds the harness walkthrough)
- `/parish-engine prove <feature>` — required after implementing any gameplay feature
- `/parish-engine rubric` — snapshot baselines + structural rubrics (sister to `prove`)
- `/parish-engine harness [script]` — fixture-script harness run
- `just agent-check` — requires proof evidence and a judge verdict for proof-relevant PRs

## Eval baselines

```sh
just baselines       # regenerate gameplay-output snapshots after intentional change
just harness-audit   # cross-reference fixtures, baselines, and roadmap for gaps
```

See [../design/testing.md](../design/testing.md) §Eval baselines for the schema. See reference in `parish/crates/parish-engine/tests/eval_baselines.rs`.

## Coverage

Run `just coverage` to generate the Tarpaulin HTML/JSON report. Run `just coverage-check` to enforce the current Rust coverage ratchet. Raise the ratchet floor as coverage-recovery work lands; the long-term target is 90%.
