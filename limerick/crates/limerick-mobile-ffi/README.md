# limerick-mobile-ffi

The callback-free C boundary between the native iPhone client and the shared
Limerick engine. It builds as a static library and links `limerick-core` in
its portable configuration (`default-features = false`, `features = ["mobile"]`),
so building this package never compiles the desktop-only dependencies.

## ABI

The header is `include/limerick_mobile_ffi.h`. `mobile/RundaleBridge` vendors an
identical copy next to its Clang module map; a unit test fails if the two drift.

- `limerick_mobile_open` copies a bounded UTF-8 JSON request and returns an
  opaque session handle plus an owned JSON response.
- `limerick_mobile_dispatch` runs one JSON operation (`{"op": ...}`) on a session.
- `limerick_mobile_close` releases a session handle.
- `limerick_mobile_owned_bytes_free` releases one owned response exactly once.

Responses are envelopes: `{"ok": true, "value": ...}` or
`{"ok": false, "error": {"code": ..., "message": ...}}`. Panics are contained
and reported as `LIMERICK_MOBILE_INTERNAL_ERROR`.

## Status

The boundary is not wired to gameplay yet. The `ios-port` branch's mobile-only
runtime was not carried over ([ADR-025](../../../docs/adr/025-mobile-runtime-on-shared-engine.md)).
Until #2044 connects these entry points to the shared `TurnEngine`:

- `limerick_mobile_open` validates its request, returns a zero handle, and answers
  with the error code `not_wired`. The error also reports the linked engine's
  save format version (`engine.save_format_version`).
- `limerick_mobile_dispatch` validates the operation and answers `not_wired`.
- `limerick_mobile_close` reports every handle as invalid, since none is issued.

Symbol names use the `limerick_mobile_*` / `LIMERICK_MOBILE_*` prefix.

## Build

`mobile/scripts/build-rust-mobile.sh` builds the device and simulator slices
with the toolchain pinned in `rust-toolchain.toml` and packages them as
`mobile/.build/rust-mobile/LimerickMobileFFI.xcframework`.
