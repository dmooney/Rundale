# Frontend IPC types: why manual TS↔Rust sync is kept

Status: decided 2026-06 (epic #1366 §4). Revisit if the IPC surface grows
past roughly double its current size or a second frontend appears.

## The question

`parish/apps/ui/src/lib/types.ts` (~600 lines) hand-mirrors the Rust IPC
structs that cross the Tauri/Axum boundary. The 2026-06 architecture review
asked whether to generate these bindings from Rust (`ts-rs` / `specta`) or
document why manual sync is kept. This note records the decision: **manual
sync stays, backed by the existing two-direction parity sensor.**

## What already guards the seam

Drift is not unguarded today. A three-part contract test gates CI in both
directions (TD-053 / #1202):

- `parish/apps/ui/src/lib/types-manifest.json` — the shared ground-truth
  list of required fields per IPC struct.
- `parish/crates/parish-core/tests/ipc_field_parity.rs` — serializes a
  representative instance of every mirrored Rust struct and asserts the
  JSON keys cover the manifest.
- `parish/apps/ui/src/lib/types.test.ts` — parses `types.ts` source and
  asserts each interface declares the manifest's fields.

A rename is a three-place edit (Rust struct, `types.ts`, manifest), but a
missed place is a CI failure, not a silent `undefined` at runtime.

## Why not ts-rs / specta now

- **The annotation burden lands on every leaf crate.** The mirrored types
  live across `parish-core::ipc`, `parish-diagnostics`, `parish-editor`,
  and `parish-types`. Deriving `TS`/`specta::Type` adds a build-time
  dependency and attribute noise to backend-agnostic leaf crates whose
  dependency surface is deliberately minimal (root AGENTS.md rule 1; the
  architecture-fitness test polices leaf-crate deps).
- **Doc comments are load-bearing in `types.ts`.** The file carries
  frontend-facing contract notes (e.g. `#[serde(default)]` semantics,
  #1164 mid-turn-clear behaviour) that generation would either drop or
  scatter back into Rust doc comments aimed at the wrong audience.
- **The failure mode generation prevents is already caught.** The parity
  sensor turns silent drift into a red CI run; generation would mainly
  save the third edit of a rename, at the cost of a codegen step that
  itself drifts (generated-file freshness checks, build ordering with
  Vite, two ecosystems' toolchains in one loop).
- **One frontend, one transport.** With a single consumer the contract
  surface is small enough to curate; generation pays off when N consumers
  must agree.

## What would change the decision

Any of: a second frontend or external API consumer; the manifest growing
past ~2× its current entry count; or recurring parity-test failures
showing the manual process is actually error-prone in practice. At that
point prefer `specta` (used by Tauri's own ecosystem) over `ts-rs`, and
generate into a separate `types.gen.ts` so the hand-written contract
notes survive alongside.
