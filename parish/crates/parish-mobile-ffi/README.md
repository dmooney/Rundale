# Parish mobile FFI

This crate owns the callback-free C boundary used by the native mobile client.
Each call copies bounded UTF-8 JSON into Rust and returns one owned JSON buffer;
the caller releases that buffer exactly once with
`parish_mobile_owned_bytes_free`. Rust keeps only an opaque session handle in a
registry, and one mutex per session serializes access to the authoritative
`parish_core::mobile::MobileSession`.

The production build enables the `engine-api` feature:

```sh
rustup run 1.98.0 cargo build \
  --manifest-path parish/Cargo.toml \
  -p parish-mobile-ffi --no-default-features --features engine-api
```

The JSON operations are `submit`, `retry`, `stop`, `fail`, `receive_failure`,
`receive_frame`, `receive_candidate`, `read_events`, `snapshot`, and
`pending_endpoint`. The `receive_failure` operation accepts the native
camel-case fields `attemptID`, `baseRevision`, `errorKind`, and `message`;
the bridge also accepts snake-case aliases for compatibility.
Successful responses use `{ "ok": true, "value": ... }`; failures use a
bounded `{ "ok": false, "error": ... }` envelope. The core DTOs define the
payload fields and serialization names, so the bridge does not mirror engine
objects in the C header.
