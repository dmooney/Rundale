# parish-diagnostics

Backend-agnostic **diagnostics** for the Parish engine: the debug-snapshot
builders and the bug-report orchestration, extracted from `parish-core` so the
desktop, web-server, and headless entry points share one implementation.

This is a leaf crate: it has no `tauri` / `axum` / `tower` dependency and is
enforced backend-agnostic by the architecture-fitness test. It depends only on
the lower leaf crates (`parish-types`, `parish-config`, `parish-inference`,
`parish-world`, `parish-npc`).

## Module map

- `debug_snapshot/` — a serializable point-in-time aggregate of all inspectable
  game state (`DebugSnapshot`), built from live world / NPC / inference
  references via `build_debug_snapshot`. Consumed by the TUI debug panel and
  the Tauri/Svelte debug panel via IPC. `build_inference_categories` and
  `build_configured_providers` build the per-role inference table.
- `bug_report` — turns an in-app (or MCP-driven) bug report into a well-formed
  GitHub issue (`create_bug_report`) with screenshots uploaded to the stable
  `bug-evidence` release, or, in dry-run / no-token mode, an offline
  disk bundle (`issue.md` + optional `screenshot.png`). `BugReportState`
  folds a world snapshot + a `DebugSnapshot` into the issue body and validates
  the payload against GitHub's body-size limit before sending.

## Dependency inversion

Two types the diagnostics code reads live in `parish-core` (which depends on
this crate, so it cannot be depended on in return without a cycle). The
coupling is inverted via traits defined here and implemented by `parish-core`:

- `debug_snapshot::InferenceCategoryConfig` — the per-category
  provider/model/base-url view `build_inference_categories` needs from
  `GameConfig`.
- `bug_report::WorldSnapshotFields` — the scalar world-state fields
  `BugReportState::from_snapshots` reads from `WorldSnapshot`.

The `weekday_name` helper both crates need lives in `parish_types::time`.

## Re-export

`parish-core` re-exports this crate so existing consumers keep their import
paths unchanged:

- `parish_core::debug_snapshot::*` ⇒ `parish_diagnostics::debug_snapshot::*`
- `parish_core::ipc::bug_report::*` ⇒ `parish_diagnostics::bug_report::*`
  (the `WorldSnapshotFields` impl for `WorldSnapshot` lives in the
  `parish-core` shim).

## Scoped commands

```sh
cargo test -p parish-diagnostics
cargo doc  -p parish-diagnostics --no-deps --open
```
