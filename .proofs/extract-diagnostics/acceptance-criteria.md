# Acceptance Criteria: extract-diagnostics

## Task

Extract the `debug_snapshot/` module (~1,616 lines) and `ipc/bug_report.rs` (~1,254 lines)
from `parish-core` into a new workspace crate called `parish-diagnostics`. All existing
call sites — `parish-server`, `parish-tauri`, `parish-mcp`, `parish-harness`, and any tests
— continue to compile without import changes, because `parish-core` gains re-export shims
for both modules. The crate count in the workspace grows from 17 to 18. This is a purely
structural, behavior-preserving refactor: no game logic changes, no observable behavior
differences at runtime.

---

## Criteria

### C1 — New crate in workspace

`parish-diagnostics` appears in `parish/Cargo.toml` under `[workspace] members`. The crate
lives at `parish/crates/parish-diagnostics/` with its own `Cargo.toml`, `src/lib.rs`, and
the moved source trees.

**Observable via:** `grep 'parish-diagnostics' parish/Cargo.toml` returns a match; the path
`parish/crates/parish-diagnostics/src/lib.rs` exists.

### C2 — `parish-core` depends on and re-exports `parish-diagnostics`

`parish-core/Cargo.toml` lists `parish-diagnostics` as a dependency. `parish-core/src/lib.rs`
re-exports the two modules under the same public paths consumers already use:

```rust
pub mod debug_snapshot { pub use parish_diagnostics::debug_snapshot::*; }
// and/or
pub use parish_diagnostics as debug_snapshot_re; // whichever form compiles all consumers
```

The exact shape of the shim is up to the implementer, but the invariant is:
`parish_core::debug_snapshot::DebugSnapshot` and `parish_core::ipc::bug_report::*` remain
valid paths without changes to any consumer.

**Observable via:** `cargo test -p parish-core` passes; no consumer crate has modified imports.

### C3 — Source modules removed from `parish-core`; no logic duplicated

Neither `parish/crates/parish-core/src/debug_snapshot/` nor
`parish/crates/parish-core/src/ipc/bug_report.rs` contains original implementation logic
after the refactor — they contain only re-export shims or are removed entirely (with `pub use`
forwarding in `lib.rs` / `ipc/mod.rs`). The `no_orphaned_source_files` architecture-fitness
test must pass, which means any `.rs` file left on disk must be declared as a `mod`.

**Observable via:** `cargo test -p parish-core --test architecture_fitness` passes; `git diff
--stat` shows the source files either deleted or reduced to re-export stubs.

### C4 — `just check` passes (fmt + clippy + all tests)

`just check` completes without errors. This covers:

- `cargo fmt --check`
- `cargo clippy` with `--all-targets -D warnings`
- `cargo test` for all workspace members

**Observable via:** `just check` exits 0.

### C5 — Architecture-fitness tests updated, not silenced

The `BACKEND_AGNOSTIC` constant in
`parish/crates/parish-core/tests/architecture_fitness.rs` is updated to include
`parish-diagnostics` if that crate must remain backend-agnostic (i.e. it must not depend on
`tauri`/`axum`/`tower*`/`wry`/`tao`). No existing fitness assertion is weakened or
`#[allow]`-suppressed. If `parish-diagnostics` becomes a backend-agnostic leaf crate (which
it should be, since `bug_report.rs` only uses `reqwest` — present in `parish-core` already),
it must be added to the list.

**Observable via:** `cargo test -p parish-core --test architecture_fitness` passes and the
constant contains `"parish-diagnostics"`.

### C6 — Behavior parity: verification fixture output identical before/after

Running the fixture `parish/testing/fixtures/play_extract-diagnostics.txt` against a headless
engine built from the refactored code produces output that matches the pre-refactor baseline
for all status/time/location/NPC assertions. The harness-syntax commands `/status`, `/time`,
`/debug clock`, `/debug npcs`, and movement commands must yield identical structured output
(same field names, same NPC count from the same starting state).

**Observable via:** Running
`cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_extract-diagnostics.txt`
produces a transcript where all asserted field patterns appear (see "Expected signals" below).

### C7 — `get_debug_snapshot` returns a populated snapshot from a live entry point

After the refactor, the `get_debug_snapshot` Tauri command (registered in
`parish-tauri/src/command_registry.rs`) and the `/api/debug-snapshot` HTTP route (in
`parish-server/src/lib.rs`) both compile and execute correctly. A live call to
`GET /api/debug-snapshot` on a running headless server returns a JSON object where:

- `clock.game_time` is a non-empty string (e.g. `"08:00 1820-03-20"`)
- `world.location_count` is a positive integer
- `npcs` is a non-null array (may be empty for the testbed mod)
- `inference.provider_name` is a non-empty string

**Observable via:** A live transcript capturing the `/api/debug-snapshot` response from a
running `parish-mcp-backend.sh` server, OR the MCP `tauri_invoke "get_debug_snapshot"` call
— both proven in the live evidence section of `evidence.md`. The headless fixture alone cannot
call the debug-snapshot HTTP endpoint; this criterion is proven separately via a live command
in the proof transcript.

### C8 — Dry-run bug report composes a bundle with required sections (rule 14)

With `PARISH_BUG_REPORT_DRY_RUN=1` set (and no GitHub token), calling the bug-report path
(via `parish_core::ipc::create_bug_report` directly in tests, or via the `/api/submit-bug-report`
HTTP route, or via the `parish_file_bug` MCP tool in dry-run mode) writes a bundle to disk
containing:

1. A `issue.md` file with `## Description`, `## Game state`, `## Recent logs`, and
   `## Diagnostic payload` sections (all four must be present).
2. If a PNG was supplied: a `screenshot.png` file alongside `issue.md` (non-empty bytes).
3. `BugReportResult.created == false` and `bundle_path` is set (rule #14: validate content,
   not just the envelope).

The existing unit test `dry_run_writes_bundle_without_network` already covers (3); the
implementer must ensure it still passes after the move. The `body_has_core_sections` test must
also continue to pass.

**Observable via:** `cargo test -p parish-diagnostics` (the moved tests) plus `cargo test -p
parish-core` both pass. For the live signal: a dry-run `parish_file_bug` MCP call in the
proof transcript shows `created: false` and `bundle_path` set.

### C9 — Documentation updated

- `docs/agent/architecture.md`: the workspace crate table lists `parish-diagnostics` and its role;
  the crate count is updated from 17 to 18.
- `docs/agent/codebase-map.md`: the Parish Crates table gains a row for
  `parish/crates/parish-diagnostics/`.
- `README.md` repository structure / crate list updated if it mentions the crate count.
- `just notices` run and `THIRD_PARTY_NOTICES` updated if any new external dependency was
  introduced. (If `parish-diagnostics` only reshuffles existing deps already in the
  workspace, `just notices` is a no-op — state this explicitly in the PR description.)

**Observable via:** `grep -c 'parish-diagnostics' docs/agent/architecture.md` returns ≥ 1;
`grep -c 'parish-diagnostics' docs/agent/codebase-map.md` returns ≥ 1.

---

## Verification script

Run:

```sh
cargo run --manifest-path parish/Cargo.toml -p parish-engine \
  -- --script parish/testing/fixtures/play_extract-diagnostics.txt
```

Expected signals in output (criteria C6 exercised deterministically):

- A line containing `"kind":"status"` or `"status"` (or equivalent harness output) confirming
  the engine starts and responds to `/status`.
- A line containing `game_time` or `"time"` confirming the clock is running (C6 clock parity).
- `"player_location"` or a location-name field confirming world-graph is loaded (C6 world parity).
- `/debug clock` output containing `game_time` and `season` (C6 debug-clock parity).
- `/debug npcs` output present and parseable — field `tier` or `name` visible (C6 NPC-tier parity).
- Movement command response confirms movement handler still wires `DebugEvent` correctly (C6).

The snapshot/bug-report criteria (C7, C8) are proven separately via a live command in the
proof `evidence.md`:

- Live server call: `curl http://localhost:3030/api/debug-snapshot | jq '.clock.game_time'`
  returns a quoted non-empty string.
- Dry-run bug report: MCP `parish_file_bug` call with `PARISH_BUG_REPORT_DRY_RUN=1` returns
  `created: false` with a `bundle_path`; the file at `bundle_path` contains `## Game state`.

---

## Notes on coupling surprises discovered during research

1. **`build_clock_debug` calls `crate::ipc::handlers::weekday_name`** — a function defined in
   `parish-core/src/ipc/handlers.rs`. After extraction, `parish-diagnostics` will have an
   inward dependency on `parish-core`, OR `weekday_name` must be moved to `parish-types` /
   `parish-world` so the new crate can import it without a circular dep. The circular dep
   `parish-diagnostics` → `parish-core` → `parish-diagnostics` is forbidden by Cargo. The
   implementer must resolve this before any code moves — options:
   - Move `weekday_name` to `parish-types` (preferred, it's a time-formatting utility).
   - Inline the weekday lookup in `debug_snapshot/build.rs` (acceptable for a small function).
   - Export it from `parish-world::time` (also reasonable given it operates on `chrono::Weekday`).

2. **`build.rs` imports `crate::ipc::config::GameConfig`** — used in `build_inference_categories`.
   `GameConfig` is defined in `parish-core/src/ipc/config.rs`. Extracting `debug_snapshot` to
   a new crate while keeping `GameConfig` in `parish-core` would again create a circular dep.
   Resolution: `GameConfig` (or a minimal `InferenceCategoryInput` trait/struct) must be moved
   to `parish-diagnostics` or to a lower-level crate accessible to both.

3. **`bug_report.rs` imports `crate::debug_snapshot::DebugSnapshot` and
   `crate::ipc::types::WorldSnapshot`** — both must be co-located in `parish-diagnostics`
   (or `WorldSnapshot` stays in `parish-core::ipc::types` and `parish-diagnostics` depends on
   `parish-core`). The safest approach is to move both modules together so neither reaches back
   into `parish-core` for implementation types. The implementer must verify there are no
   remaining `crate::` imports in the moved code pointing to `parish-core`-only types.

4. **`reexport.rs` re-exports `crate::inference::InferenceLogEntry`** — defined in
   `parish-inference`. This dependency already exists on the new crate transitively; ensure
   `parish-diagnostics/Cargo.toml` lists `parish-inference = { workspace = true }` directly
   so the re-export is explicit.

5. **`parish-harness` imports `parish_core::ipc::{BugReportRequest, BugReportState,
GitHubBugConfig, create_bug_report}`** via `parish-core`'s re-export in `ipc/mod.rs`.
   These paths must remain valid after the move — the `ipc/mod.rs` re-export block must be
   preserved (or the `pub use` shim added to `parish-core::ipc`).

6. **Architecture-fitness `BACKEND_AGNOSTIC` list** — `parish-diagnostics` must be added to
   the list in `architecture_fitness.rs` because it will be consumed by `parish-server`,
   `parish-tauri`, and `parish-engine` alike and must not pull in backend runtime deps.
   `reqwest` is already in `BACKEND_AGNOSTIC` via `parish-core` (the existing `Cargo.toml`
   lists it), so its presence is not a new violation; verify it is not in
   `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.
