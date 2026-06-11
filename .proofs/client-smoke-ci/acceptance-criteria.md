# Acceptance Criteria: client-smoke-ci

## Task

Epic #1366 §5, second checkbox (low effort / medium payoff): **`parish-client`
has zero CI coverage.** Today the `parish` binary (the thin synchronous HTTP
client, `parish/crates/parish-client/`) is exercised only by crate-local unit
tests of its pure helpers (`client.rs` wire-shape, `render.rs` formatting,
`session.rs` cookie path, `repl.rs` line filter). The binary's actual
end-to-end path — parse args → build a cookie-jar HTTP client → `POST
/api/command` against a live server → render the response → exit — is **never
run in CI**. Worse, the coverage-ratchet job in `.github/workflows/ci.yml`
(line ~264) passes `--exclude parish-client`, so the crate is invisible to the
coverage floor.

Add the **smallest** end-to-end smoke test the epic sketches: start a real
`parish-server`, run the real `parish "look"` binary against it, assert the
process exits `0` and that its stdout prints a location. This is the epic's
smallest §5 item — **resist gold-plating** (no multi-command scripts, no
dialogue/inference, no REPL coverage, no JSON-mode matrix; those are separate
items if ever wanted).

## Chosen mechanism (prescribed — do not re-decide)

**A single Rust integration test at
`parish/crates/parish-client/tests/smoke.rs` that (1) boots a real
`parish-server` in-process on a pre-picked free port via
`parish_server::run_server`, spawned onto a background Tokio task, then (2)
spawns the real `parish` binary as a child process pointed at that port with
`look`, and asserts exit `0` + a location string in stdout.**

This is option (a) from the task brief — a Rust integration test that runs
inside `cargo test`/`cargo nextest --workspace` everywhere (CI, local,
`just check`), with **no new CI-workflow YAML and no new `just` recipe**.
Rationale for (a) over (b)/(c): the workspace test job
(`cargo nextest run --workspace --all-targets --profile ci`, ci.yml ~line 168)
already runs every crate's `tests/`, so adding the file is the entire wiring —
it cannot drift out of sync with a shell step, runs identically for every
contributor, and is the lowest-surface change. A shell step in `ci.yml` (b) or
a `just` recipe (c) would duplicate readiness/port logic that the Rust test can
express directly and would only run in CI, not under a local `cargo test`.

### Why in-process `run_server`, not a `parish-server` subprocess

The brief floats spawning a `parish-server` binary subprocess. **Reject that**
for the stable toolchain (1.96.0):

- Cargo only exposes `CARGO_BIN_EXE_<name>` for binaries **defined in the same
  package** as the test. `parish-client` defines the `parish` bin (so
  `CARGO_BIN_EXE_parish` **is** available to its tests) but does **not** define
  `parish-server`, so `CARGO_BIN_EXE_parish-server` is **not** set and the
  sibling binary is not guaranteed to be built before the test runs.
- Artifact dependencies (`[dev-dependencies] parish-server = { artifact = "bin" }`,
  which would set `CARGO_BIN_EXE_parish_server`) are still nightly-gated
  (`-Z bindeps`) on 1.96.0 — out of scope.
- Shelling out to `cargo run -p parish-server` from inside `cargo test` nests
  cargo invocations and contends on the build lock — fragile and slow.

So the **server** runs in-process: add `parish-server` to
`parish-client`'s `[dev-dependencies]` (library, test-only) and call its
public `pub async fn run_server(port, data_dir, static_dir, headless_models)`.
The **client** still runs as a real subprocess via `CARGO_BIN_EXE_parish`
(available — same crate), so the binary's real arg-parse/HTTP/render/exit path
is genuinely exercised end-to-end through a real TCP socket. This is the live
signal the proof tier wants (see C7).

### Port strategy (prescribed)

`run_server` takes a **fixed** `port: u16` and binds `0.0.0.0:{port}` itself; it
does **not** return the OS-assigned address, so a bind-`:0`-and-discover trick
is not directly available through its signature. Instead **pre-pick a free
port**: bind `std::net::TcpListener::bind("127.0.0.1:0")`, read
`local_addr().port()`, **drop the listener**, then pass that port to
`run_server`. This carries a tiny TOCTOU window but is the standard pragmatic
choice and is parallel-test-safe in practice (each test process picks its own
port). Point the client at `http://127.0.0.1:{port}` via either the `--server`
flag or the `PARISH_SERVER` env var on the child (prescribe `PARISH_SERVER`
on the child's env — it is the documented mechanism and avoids arg-order
coupling). **Do not** hardcode `3001`/`3030`.

### Readiness (prescribed)

After spawning the server task, **poll `GET /api/health`** (the auth-exempt
health route, returns 200 — `build_router` in `parish-server/src/lib.rs`) using
`parish-client`'s already-present `reqwest` dependency, with a bounded retry
loop (e.g. up to ~30 attempts × 250–500 ms, ≈10–15 s budget) and a clear panic
message on timeout. Only after health is green is the `parish` child spawned.
Mirror the existing CI pattern (`eval-inference.yml`: `for i in $(seq 1 60); do
curl /api/health …`). `look` is a **deterministic** command and needs **no**
LLM inference, so `headless_models = false` (no model bootstrap) is correct and
keeps boot fast and offline.

### Isolation (prescribed)

Set `PARISH_USER_DATA_DIR` (and `PARISH_SAVES_DIR` if needed) to a per-test
`tempfile::tempdir()` on the child's environment so the test never reads or
writes the developer's real persisted `parish_sid` cookie or saves (the client
persists the session cookie under `resolve_user_data_dir(DEFAULT_APP_NAME)` —
see `session.rs`). A single `look` on a fresh cookieless session returns the
**starting** scene, which is sufficient — no multi-command cookie sharing is
required. `tempfile` is already a `parish-client` dev-dependency.

### data_dir / static_dir (prescribed)

`run_server` needs a mod `data_dir` (containing `world.json`) and a
`static_dir`. Resolve `data_dir` to the repo's `mods/rundale` from the test by
walking up from `env!("CARGO_MANIFEST_DIR")`
(`…/parish/crates/parish-client` → repo root → `mods/rundale`). `look` does
**not** serve static files, so `static_dir` may point at the same temp dir or a
non-existent path; prescribe a temp dir to avoid surprising the static-file
layer. (Resolving a fixture path from `CARGO_MANIFEST_DIR` in a test is **not**
a rule-9 violation — rule #9 governs _runtime_ path resolution in handlers, not
test fixtures.)

## Criteria

### C1 — A smoke integration test exists in the client crate

`parish/crates/parish-client/tests/smoke.rs` exists and contains a single
`#[tokio::test]` (multi-thread flavor, e.g.
`#[tokio::test(flavor = "multi_thread")]`, so the in-process server task and the
blocking child-process wait can both make progress). It is the only new test
file.
Observable via: `ls parish/crates/parish-client/tests/smoke.rs` and the test
name appears in `cargo nextest run -p parish-client --all-targets` output.

### C2 — The test boots a real `parish-server` in-process on a free port

The test picks a free port (bind `127.0.0.1:0` → `local_addr().port()` → drop),
spawns `parish_server::run_server(port, data_dir, static_dir, /*headless_models=*/false)`
onto a background Tokio task, and waits for `GET http://127.0.0.1:{port}/api/health`
to return success before proceeding. `parish-server` is a **`[dev-dependencies]`**
of `parish-client` (test-only); it does not appear under `[dependencies]`.
**Declaration form:** `parish-server` is **not** in `[workspace.dependencies]`
(only leaf + `parish-core` crates are; entry-point binaries are not), so use a
**direct path dep** in `parish-client/Cargo.toml`:
`parish-server = { path = "../parish-server" }` under `[dev-dependencies]`. Do
**not** write `{ workspace = true }` (that key does not exist for it). Adding it
to `[workspace.dependencies]` is an acceptable alternative but is broader churn
than this task needs — prefer the direct path dep.
Observable via: reading the diff — `Cargo.toml` gains
`parish-server = { path = "../parish-server" }` under `[dev-dependencies]`
only, and `smoke.rs` shows the free-port pick + `run_server` spawn +
`/api/health` poll.

### C3 — The test runs the real `parish` binary as a subprocess

The `parish` binary is invoked via `std::process::Command::new(env!("CARGO_BIN_EXE_parish"))`
with the argument `"look"`, the child env carrying `PARISH_SERVER=http://127.0.0.1:{port}`
and `PARISH_USER_DATA_DIR=<tempdir>`, stdout captured. The server is **not**
reached by calling library functions directly for the assertion — the actual
compiled `parish` binary's arg-parse → HTTP → render → exit path runs.
Observable via: reading the diff — `Command::new(env!("CARGO_BIN_EXE_parish"))`
with `.arg("look")` and `.env("PARISH_SERVER", …)`.

### C4 — Exit-0 assertion

The test asserts the `parish look` child exited with status success
(`output.status.success()` is `true`, i.e. exit code 0). On failure the panic
message includes the child's captured stderr to make CI triage trivial.
Observable via: the `assert!(output.status.success(), …)` line in `smoke.rs`;
the test passes.

### C5 — "Prints a location" assertion (the output contract)

The test asserts the child's **stdout** contains the player's current location
name. Concretely: `parish "look"` renders via `render::render_response`, whose
**first** output line for a stateful response is the header
`"{location_name} | {time_label} | {season} · {weather}\n"` (see
`parish-client/src/render.rs`). The deterministic assertion is therefore:

- stdout is non-empty, **and**
- stdout's first line contains the `|` header separator **and** at least one
  `·` (season·weather) separator — i.e. the location-header shape — **or**,
  more robustly, stdout contains the **known Rundale starting location name**
  for a fresh `look` (resolve the exact string from the live transcript in C7;
  do not hardcode a guess in the AC — read it from the proof run).

Prescribe the **structural** check (`stdout.lines().next()` contains `" | "`
and the line contains `" · "`) as the primary assertion because it is
mod-agnostic and survives a starting-location rename; optionally add the exact
starting-location-name `contains` as a stronger secondary assertion once the
proof transcript reveals it. The assertion must **not** merely check exit code
— it must inspect stdout for the location header (that is the "prints a
location" contract).
Observable via: the `assert!(first_line.contains(" | ") && first_line.contains(" · "), …)`
(and/or `assert!(stdout.contains("<starting-location>"))`) lines in `smoke.rs`;
the test passes against the live server.

### C6 — The test is hermetic and parallel-safe

The test sets `PARISH_USER_DATA_DIR` to a `tempfile::tempdir()` on the child so
it never touches the developer's real `~/.../Parish/session` cookie or saves,
uses an OS-assigned free port (never a hardcoded port), and tears down cleanly
(the server task is dropped / aborted at test end; no leaked listener on the
fixed port). Running the test twice concurrently (e.g.
`cargo nextest run -p parish-client` which parallelizes) does not flake.
Observable via: reading the diff — `tempdir()` for `PARISH_USER_DATA_DIR`, free
port, no `:3001`/`:3030` literals; and `cargo nextest run -p parish-client`
green (C8).

### C7 — Live proof (proof tier)

This diff touches a runtime-shipping path under the rule-10 live matrix
(`parish-client` is an entry-point binary; `mods/**` is read by the booted
server). The bundle's `evidence.md` header must declare
`Evidence type: live gameplay transcript`, and the live run is a real
`parish` binary against a real `parish-server` — which is itself the new smoke
test. Accepted live signals for this task:

- the **smoke test's own run** captured in the transcript
  (`cargo nextest run -p parish-client smoke` / `cargo test -p parish-client --test smoke`,
  showing the test boot the server and pass), **and/or**
- an out-of-band manual live run:
  `bash parish/scripts/parish-mcp-backend.sh start` (or `just web 3001`) then
  `cargo run -p parish-client -- --server http://127.0.0.1:3030 "look"`
  (a `cargo run -p parish-client` invocation is explicitly in the rule-10
  accepted-signal list), pasting the rendered location-header line.

`evidence.md` maps each criterion to output lines; `judge.md` is written
independently, verifies every criterion, and includes the line
`Acceptance criteria: met`.
Observable via: `evidence.md` + `judge.md` in `.proofs/client-smoke-ci/`,
attached to the PR body via `just attach-proof client-smoke-ci`.

### C8 — `just check` green; no existing test edited

`cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and
`cargo nextest run --workspace --all-targets` all pass with the new test and
the new `[dev-dependencies]` line, no new warnings, no `#[allow]` (rule #5). No
existing test in the workspace is edited.
Observable via: `just check` exits 0; `git diff` shows only `smoke.rs` added
and `parish-client/Cargo.toml`'s `[dev-dependencies]` extended (plus this
proof bundle, which is gitignored).

### C9 — Coverage-exclude reviewed (documented decision, no required change)

The coverage-ratchet step still carries `--exclude parish-client`
(ci.yml ~264). This task's smoke test makes the crate's end-to-end path
**run**, but the test boots `parish-server` in-process, so removing the
exclude would attribute `parish-server` coverage churn and could perturb the
`--fail-under-lines 60.8` floor. **Default: leave the exclude as-is** and note
in the PR body that end-to-end coverage now exists via the smoke test even
though the line-coverage ratchet still skips the crate. Removing the exclude is
explicitly **out of scope** (it risks a floor-number change that is a separate,
larger task). If the implementer removes it, that is a deviation requiring a
re-baselined `--fail-under-lines` value justified in the PR.
Observable via: a one-line note in `evidence.md`/PR body; `git diff` of
`ci.yml` is empty (default) — or, if changed, the floor number is re-baselined
with justification.

## Verification script

```sh
# 1. Run the new smoke test in isolation (boots server in-process, runs the
#    real `parish look` child, asserts exit 0 + location header).
cargo nextest run -p parish-client --test smoke
# or: cargo test -p parish-client --test smoke -- --nocapture

# 2. Optional out-of-band live confirmation of the same contract:
bash parish/scripts/parish-mcp-backend.sh start          # live server on :3030
cargo run -p parish-client -- --server http://127.0.0.1:3030 "look"
#   → first stdout line is "<Location> | <time> | <season> · <weather>"
bash parish/scripts/parish-mcp-backend.sh stop

# 3. Full gate.
just check
```

Expected signals:

- `cargo nextest run -p parish-client --test smoke` reports the smoke test
  PASS; with `--nocapture` the rendered `look` header line is visible.
- The out-of-band `cargo run -p parish-client … "look"` prints a single header
  line of the shape `Location | time | season · weather` (the "prints a
  location" contract).
- `just check` exits 0; the coverage ratchet still excludes `parish-client`
  (unchanged) and the floor is unmoved.

## Coupling surprises

- **`CARGO_BIN_EXE_parish` is available, `CARGO_BIN_EXE_parish-server` is
  not.** Cargo sets the bin-exe env only for binaries in the **same** package
  as the test. The client crate owns the `parish` bin, so its test can spawn it
  directly; it does **not** own `parish-server`, so the server must run
  **in-process** via the `run_server` library export (added as a test-only
  `[dev-dependencies]`), not as a sibling subprocess. Artifact deps that would
  fix this are nightly-gated on 1.96.0.
- **`parish-server` is not a `[workspace.dependencies]` entry.** Only leaf
  crates + `parish-core` are declared there; the four entry-point binaries
  (`parish-server`, `parish-engine`, `parish-tauri`, `parish-client`) are
  workspace _members_ but not workspace _dependency_ entries. So the dev-dep
  must be `parish-server = { path = "../parish-server" }`, not
  `{ workspace = true }`.
- **`run_server` binds a fixed port and never reports it.** Its signature is
  `run_server(port, data_dir, static_dir, headless_models)` and it binds
  `0.0.0.0:{port}` internally with no `local_addr` return. The standard
  in-process integration test (`ws_integration.rs`) binds `127.0.0.1:0` and
  reads `local_addr()` from its **own** listener — but that test builds a bare
  `Router`/`AppState` by hand, it does **not** call `run_server`. So for the
  smoke test, **pre-pick** a free port (bind `:0`, read port, drop listener,
  pass to `run_server`).
- **`look` needs no inference.** `headless_models = false` is correct; the
  command renders world/location state deterministically with no LLM call, so
  the test is offline and fast and needs no ollama/vllm/models. Do **not** set
  `headless_models = true` (it would try to bring up the local Qwen loadout).
- **Per-visitor session isolation + cookie file.** The headless server gives
  each cookieless request a fresh `Arc<AppState>` keyed by the `parish_sid`
  cookie (LEARNINGS.md), and the `parish` client persists that cookie under
  `resolve_user_data_dir(DEFAULT_APP_NAME)`. A single `look` on a fresh session
  is fine, but the test **must** set `PARISH_USER_DATA_DIR` to a tempdir so it
  doesn't read/clobber the developer's real cookie. (A pre-existing cookie
  pointing at a stale session id is harmless for `look` but the isolation is
  still required for hermeticity.)
- **The output contract is the render header, not raw JSON.** `parish "look"`
  (non-`--json`) renders via `render::render_response`; the location appears as
  the **first line** in the form `"{location} | {time} | {season} · {weather}"`.
  Assert on that header shape (`|` + `·`), not on a JSON field. (`--json`
  mode would emit `state.world.location_name`, but the default human path is
  what the binary ships, so test that.)
- **`--exclude parish-client` on the coverage ratchet is intentional and
  out of scope to remove.** The smoke test makes the crate's path _run_ but
  boots `parish-server` in-process; flipping the exclude could move the
  `--fail-under-lines 60.8` floor. Leave it; note the gap closed by the e2e
  test in the PR body.
- **No new fixture file is needed.** The "smoke test IS the verification"
  (per the brief): a single hardcoded `look` arg, no
  `play_client-smoke-ci.txt` script. (A `parish --script` fixture would only
  matter for a multi-command transcript, which is gold-plating here.)
